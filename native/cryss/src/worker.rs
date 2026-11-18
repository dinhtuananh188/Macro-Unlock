use crate::engine::{Completion, Engine, Output, OutputCleanupError};
use crate::presets::Instruction;
use crate::timing::{Interrupt, Signals};
use std::io;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const IDLE: u8 = 0;
const RUNNING: u8 = 1;
const FAILED: u8 = 2;

struct Job {
    instructions: Arc<[Instruction]>,
    signals: Arc<Signals>,
    result: mpsc::SyncSender<io::Result<Completion>>,
}

pub struct Playback {
    signals: Arc<Signals>,
    result: mpsc::Receiver<io::Result<Completion>>,
}

impl Playback {
    pub fn interrupt(&self, interrupt: Interrupt) -> io::Result<()> {
        self.signals.signal(interrupt)
    }

    pub fn wait_timeout(&self, timeout: Duration) -> io::Result<Option<Completion>> {
        match self.result.recv_timeout(timeout) {
            Ok(result) => result.map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(worker_exited()),
        }
    }

    pub fn wait(self) -> io::Result<Completion> {
        self.result.recv().map_err(|_| worker_exited())?
    }
}

pub struct Worker {
    queue: Option<mpsc::SyncSender<Job>>,
    thread: Option<JoinHandle<()>>,
    status: Arc<AtomicU8>,
    latest_signals: Option<Arc<Signals>>,
}

impl Worker {
    pub fn new(mut output: impl Output + Send + 'static) -> io::Result<Self> {
        let mut engine = Engine::new()?;
        let status = Arc::new(AtomicU8::new(IDLE));
        let work_status = Arc::clone(&status);
        let (queue, jobs) = mpsc::sync_channel::<Job>(1);
        let thread = thread::Builder::new()
            .name("cryss-macro".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let result = engine.execute(&job.instructions, &job.signals, &mut output);
                    let cleanup_failed = result
                        .as_ref()
                        .err()
                        .and_then(io::Error::get_ref)
                        .is_some_and(|cause| cause.is::<OutputCleanupError>());
                    work_status.store(
                        if cleanup_failed { FAILED } else { IDLE },
                        Ordering::Release,
                    );
                    let _ = job.result.send(result);
                    if cleanup_failed {
                        break;
                    }
                }
            })?;
        Ok(Self {
            queue: Some(queue),
            thread: Some(thread),
            status,
            latest_signals: None,
        })
    }

    pub fn start(&mut self, instructions: Arc<[Instruction]>) -> io::Result<Playback> {
        let queue = self.queue.as_ref().ok_or_else(worker_exited)?;
        let signals = Arc::new(Signals::new()?);
        let (send, result) = mpsc::sync_channel(1);
        match self
            .status
            .compare_exchange(IDLE, RUNNING, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {}
            Err(FAILED) => return Err(worker_exited()),
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "macro worker is running",
                ));
            }
        }
        if queue
            .try_send(Job {
                instructions,
                signals: Arc::clone(&signals),
                result: send,
            })
            .is_err()
        {
            self.status.store(FAILED, Ordering::Release);
            return Err(worker_exited());
        }
        self.latest_signals = Some(Arc::clone(&signals));
        Ok(Playback { signals, result })
    }

    pub fn close(mut self) -> io::Result<()> {
        self.shutdown()
    }

    fn shutdown(&mut self) -> io::Result<()> {
        let cancelled = self
            .latest_signals
            .take()
            .map_or(Ok(()), |signals| signals.signal(Interrupt::Stop));
        self.queue.take();
        let joined = self
            .thread
            .take()
            .map_or(Ok(()), |thread| thread.join().map_err(|_| worker_exited()));
        cancelled.and(joined)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn worker_exited() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "macro worker exited")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{Action, Target};

    struct Recording(mpsc::Sender<Action>);

    impl Output for Recording {
        fn emit(&mut self, action: &Action) -> io::Result<()> {
            self.0.send(action.clone()).map_err(io::Error::other)
        }
    }

    #[test]
    fn supports_repeated_runs_on_one_worker_and_releases_on_cancel() -> io::Result<()> {
        let target = Target::Key("q".into());
        let (sender, events) = mpsc::channel();
        let mut worker = Worker::new(Recording(sender))?;
        let program: Arc<[Instruction]> = vec![
            Instruction::Input(Action::Down(target.clone())),
            Instruction::WaitFor(Duration::from_secs(2)),
        ]
        .into();
        for _ in 0..100 {
            let playback = worker.start(Arc::clone(&program))?;
            assert_eq!(
                events.recv_timeout(Duration::from_secs(1)).unwrap(),
                Action::Down(target.clone())
            );
            playback.interrupt(Interrupt::Stop)?;
            assert_eq!(playback.wait()?, Completion::Interrupted);
            assert_eq!(events.try_recv().unwrap(), Action::Up(target.clone()));
        }
        worker.close()
    }

    #[test]
    fn worker_stops_accepting_programs_when_input_cannot_be_released() -> io::Result<()> {
        struct FailingOutput;
        impl Output for FailingOutput {
            fn emit(&mut self, action: &Action) -> io::Result<()> {
                match action {
                    Action::Down(_) => Ok(()),
                    Action::Up(_) => Err(io::Error::other("release failed")),
                }
            }
        }
        let mut worker = Worker::new(FailingOutput)?;
        let program: Arc<[Instruction]> =
            vec![Instruction::Input(Action::Down(Target::Key("q".into())))].into();
        assert!(
            worker
                .start(Arc::clone(&program))?
                .wait()
                .unwrap_err()
                .to_string()
                .contains("retry failed")
        );
        assert!(
            matches!(worker.start(program), Err(error) if error.kind() == io::ErrorKind::BrokenPipe)
        );
        worker.close()
    }

    #[test]
    fn close_cancels_active_work_without_waiting_for_deadline() -> io::Result<()> {
        let (send, _receive) = mpsc::channel();
        let mut worker = Worker::new(Recording(send))?;
        let playback = worker.start(vec![Instruction::WaitFor(Duration::from_secs(2))].into())?;
        let now = std::time::Instant::now();
        worker.close()?;
        assert_eq!(playback.wait()?, Completion::Interrupted);
        assert!(now.elapsed() < Duration::from_secs(1));
        Ok(())
    }
}
