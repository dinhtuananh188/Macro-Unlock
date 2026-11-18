use crate::presets::{Action, Cancellation, Instruction, Target};
use crate::timing::{Interrupt, Signals, Timer};
use std::io;
use std::time::{Duration, Instant};

pub trait Output {
    fn emit(&mut self, action: &Action) -> io::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completion {
    Finished,
    Interrupted,
}

pub struct Engine {
    timer: Timer,
}

impl Engine {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            timer: Timer::new()?,
        })
    }

    pub fn execute(
        &mut self,
        instructions: &[Instruction],
        signals: &Signals,
        output: &mut impl Output,
    ) -> io::Result<Completion> {
        let mut playback = Playback {
            engine: self,
            signals,
            output,
            held: Vec::new(),
        };
        let result = playback.run(instructions, Cancellation::Checkpoint);
        if let Err(first) = playback.release_except(&[]) {
            if let Err(retry) = playback.release_except(&[]) {
                let execution = result.err();
                return Err(io::Error::other(OutputCleanupError {
                    first,
                    retry,
                    execution,
                }));
            }
        }
        result
    }
}

#[derive(Debug)]
pub(crate) struct OutputCleanupError {
    first: io::Error,
    retry: io::Error,
    execution: Option<io::Error>,
}

impl std::fmt::Display for OutputCleanupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(execution) = &self.execution {
            write!(formatter, "macro failed ({execution}). ")?;
        }
        write!(
            formatter,
            "input release failed ({}) and retry failed ({})",
            self.first, self.retry
        )
    }
}

impl std::error::Error for OutputCleanupError {}

struct Playback<'a, O> {
    engine: &'a mut Engine,
    signals: &'a Signals,
    output: &'a mut O,
    held: Vec<Target>,
}

impl<O: Output> Playback<'_, O> {
    fn emit(&mut self, action: &Action) -> io::Result<()> {
        self.output.emit(action)?;
        match action {
            Action::Down(target) if !self.held.contains(target) => self.held.push(target.clone()),
            Action::Up(target) => self.held.retain(|held| held != target),
            _ => {}
        }
        Ok(())
    }

    fn release_except(&mut self, keep: &[Target]) -> io::Result<()> {
        let mut error = None;
        for index in (0..self.held.len()).rev() {
            if keep.contains(&self.held[index]) {
                continue;
            }
            let target = self.held[index].clone();
            match self.output.emit(&Action::Up(target)) {
                Ok(()) => {
                    self.held.remove(index);
                }
                Err(failure) => {
                    error.get_or_insert(failure);
                }
            }
        }
        error.map_or(Ok(()), Err)
    }

    fn run_scoped(
        &mut self,
        body: &[Instruction],
        cancellation: Cancellation,
        cleanup: bool,
    ) -> io::Result<Completion> {
        if !cleanup {
            return self.run(body, cancellation);
        }
        // The legacy Mavuika timeline also releases its keys when a preceding
        // custom step held one. Keep other previously held targets intact.
        let before: Vec<_> = self
            .held
            .iter()
            .filter(|target| !mavuika_target(target))
            .cloned()
            .collect();
        let result = self.run(body, cancellation);
        let release = self.release_except(&before);
        result.and_then(|completion| release.map(|()| completion))
    }

    fn wait(
        &mut self,
        timestamp: Instant,
        duration: Duration,
        interrupt: Interrupt,
    ) -> io::Result<bool> {
        let deadline = timestamp.checked_add(duration).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "macro deadline overflow")
        })?;
        self.engine
            .timer
            .wait_until(deadline, self.signals, interrupt)
    }

    fn run(
        &mut self,
        instructions: &[Instruction],
        cancellation: Cancellation,
    ) -> io::Result<Completion> {
        let interrupt = match cancellation {
            Cancellation::Checkpoint => Interrupt::Stop,
            Cancellation::Hold => Interrupt::Release,
            Cancellation::Foreground => Interrupt::Focus,
        };
        let mut origin = Instant::now();
        let mut pc = 0usize;
        while let Some(instruction) = instructions.get(pc) {
            if self.signals.interrupted(interrupt)? {
                return Ok(Completion::Interrupted);
            }
            pc += 1;
            let stopped = match instruction {
                Instruction::Origin => {
                    origin = Instant::now();
                    false
                }
                Instruction::WaitUntil(duration) => self.wait(origin, *duration, interrupt)?,
                Instruction::WaitFor(duration) => {
                    self.wait(Instant::now(), *duration, interrupt)?
                }
                Instruction::Input(action) => {
                    self.emit(action)?;
                    false
                }
                Instruction::Checkpoint => self.signals.interrupted(Interrupt::Release)?,
                Instruction::Call(body) => {
                    self.run(body, Cancellation::Checkpoint)?;
                    self.signals.interrupted(Interrupt::Stop)?
                }
                Instruction::SkipIfPast {
                    deadline,
                    instructions,
                } => {
                    if origin.elapsed() > *deadline {
                        pc = pc.saturating_add(*instructions);
                    }
                    false
                }
                Instruction::Block {
                    body,
                    cancellation,
                    cleanup,
                } => self.run_scoped(body, *cancellation, *cleanup)? == Completion::Interrupted,
                Instruction::Cycle {
                    body,
                    max_duration,
                    cancellation,
                    cleanup,
                } => {
                    let cycle_start = Instant::now();
                    let mut stopped = false;
                    while !body.is_empty()
                        && max_duration.is_none_or(|max| cycle_start.elapsed() < max)
                    {
                        if self.signals.interrupted(Interrupt::Release)?
                            || self.run_scoped(body, *cancellation, *cleanup)?
                                == Completion::Interrupted
                        {
                            stopped = true;
                            break;
                        }
                    }
                    stopped
                }
            };
            if stopped {
                return Ok(Completion::Interrupted);
            }
        }
        Ok(Completion::Finished)
    }
}

fn mavuika_target(target: &Target) -> bool {
    matches!(target, Target::MouseLeft | Target::MouseRight)
        || matches!(target, Target::Key(key) if matches!(key.as_str(), "shift" | "q" | "1" | "e" | "w" | "a" | "d"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Trace(Vec<Action>);

    impl Output for Trace {
        fn emit(&mut self, action: &Action) -> io::Result<()> {
            self.0.push(action.clone());
            Ok(())
        }
    }

    #[test]
    fn interrupted_tap_releases_it_once() -> io::Result<()> {
        let mut engine = Engine::new()?;
        let signals = Signals::new()?;
        let target = Target::Key("q".into());
        let down = Action::Down(target.clone());
        let up = Action::Up(target);
        struct StopOnDown<'a>(&'a Signals, Trace);
        impl Output for StopOnDown<'_> {
            fn emit(&mut self, action: &Action) -> io::Result<()> {
                self.1.emit(action)?;
                if matches!(action, Action::Down(_)) {
                    self.0.signal(Interrupt::Release)?;
                }
                Ok(())
            }
        }
        let mut output = StopOnDown(&signals, Trace::default());
        let instructions = [Instruction::Block {
            body: vec![
                Instruction::Input(down.clone()),
                Instruction::WaitFor(Duration::from_secs(2)),
                Instruction::Input(up.clone()),
            ],
            cancellation: Cancellation::Hold,
            cleanup: false,
        }];
        assert_eq!(
            engine.execute(&instructions, &signals, &mut output)?,
            Completion::Interrupted
        );
        assert_eq!(output.1.0, vec![down, up]);
        Ok(())
    }

    #[test]
    fn timeline_releases_mavuika_input_held_by_an_earlier_step() -> io::Result<()> {
        let button = Target::MouseLeft;
        let unrelated = Target::Key("f4".into());
        let instructions = [
            Instruction::Input(Action::Down(button.clone())),
            Instruction::Input(Action::Down(unrelated.clone())),
            Instruction::Block {
                body: vec![],
                cancellation: Cancellation::Hold,
                cleanup: true,
            },
            Instruction::Input(Action::Up(unrelated.clone())),
        ];
        let mut trace = Trace::default();
        Engine::new()?.execute(&instructions, &Signals::new()?, &mut trace)?;
        assert_eq!(
            trace.0,
            [
                Action::Down(button.clone()),
                Action::Down(unrelated.clone()),
                Action::Up(button),
                Action::Up(unrelated)
            ]
        );
        Ok(())
    }

    #[test]
    fn foreground_block_ignores_release_but_checks_stop() -> io::Result<()> {
        let mut engine = Engine::new()?;
        let signals = Signals::new()?;
        signals.signal(Interrupt::Release)?;
        let instructions = [Instruction::Block {
            body: vec![Instruction::WaitFor(Duration::from_millis(2))],
            cancellation: Cancellation::Foreground,
            cleanup: true,
        }];
        assert_eq!(
            engine.execute(&instructions, &signals, &mut Trace::default())?,
            Completion::Finished
        );
        signals.signal(Interrupt::Stop)?;
        assert_eq!(
            engine.execute(&instructions, &signals, &mut Trace::default())?,
            Completion::Interrupted
        );
        Ok(())
    }
}
