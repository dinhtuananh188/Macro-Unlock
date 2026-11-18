use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Key(String),
    MouseLeft,
    MouseRight,
    MouseMiddle,
    MouseX1,
    MouseX2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Down(Target),
    Up(Target),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cancellation {
    /// Check trigger and focus when an explicit checkpoint is reached.
    Checkpoint,
    /// Check trigger and focus while waiting for the next event.
    Hold,
    /// Check focus while waiting for the next event.
    Foreground,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    /// Start an elapsed-time origin in the current block or cycle iteration.
    Origin,
    /// Wait until this deadline relative to the last origin.
    WaitUntil(Duration),
    /// Wait this long from the current scheduler time.
    WaitFor(Duration),
    Input(Action),
    // When elapsed time since Origin exceeds deadline, skip this many following instructions.
    SkipIfPast {
        deadline: Duration,
        instructions: usize,
    },
    Checkpoint,
    Call(Vec<Instruction>),
    Block {
        body: Vec<Instruction>,
        cancellation: Cancellation,
        cleanup: bool,
    },
    Cycle {
        body: Vec<Instruction>,
        max_duration: Option<Duration>,
        cancellation: Cancellation,
        cleanup: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub instructions: Vec<Instruction>,
    pub uses_held_input: bool,
}

const MAVUIKA: &str = "C0:  Combo Mavuika CDCDCF (Full Combo)";
const ARLECCHINO: &str = "C0:  Combo Arlecchino Overload N2W (GI)";
const MAX_LOOP_DURATION: Duration = Duration::from_secs(20);

const TIME_THREE_W: &[(f64, f64)] = &[
    (30.0, 1.5),
    (60.0, 1.366),
    (100.0, 1.34),
    (120.0, 1.32),
    (220.0, 1.285),
];
const FRAMES_THREE_DASH: &[(f64, f64)] =
    &[(60.0, 18.0), (120.0, 20.0), (144.0, 22.0), (240.0, 24.0)];
const TIME_TWO_DASH: &[(f64, f64)] = &[(60.0, 0.44), (120.0, 0.40), (220.0, 0.37)];
const END_TWO_DASH: &[(f64, f64)] = &[(60.0, 0.63), (120.0, 0.57), (220.0, 0.56)];
const TIME_TWO_CHARGE: &[(f64, f64)] = &[(60.0, 0.24), (120.0, 0.22), (220.0, 0.20)];
const END_TWO_CHARGE: &[(f64, f64)] = &[(60.0, 1.16), (120.0, 1.12), (220.0, 1.09)];
const END_TWO_CHARGE_DASH: &[(f64, f64)] = &[(60.0, 0.90), (120.0, 0.86), (220.0, 0.82)];
const TIME_TWO_Q: &[(f64, f64)] = &[(60.0, 0.44), (120.0, 0.39), (220.0, 0.37)];
const END_TWO_Q: &[(f64, f64)] = &[(60.0, 1.17), (120.0, 1.09), (220.0, 1.07)];
const TIME_FIVE: &[(f64, f64)] = &[(60.0, 2.22), (120.0, 2.14), (220.0, 2.12)];
const END_FIVE_DASH: &[(f64, f64)] = &[(60.0, 2.41), (120.0, 2.28), (220.0, 2.24)];

#[derive(Clone, Copy)]
enum Skirk {
    ThreeW,
    ThreeDash,
    TwoDash,
    Two,
    TwoCharge,
    TwoChargeDash,
    TwoChargeDashSlow,
    FiveDash,
    Five,
    TwoQ,
    E,
}

use Skirk::*;

const SKIRK_223: &[Skirk] = &[TwoDash, TwoDash, ThreeW];
const SKIRK_EQA_120: &[Skirk] = &[
    E,
    TwoDash,
    TwoDash,
    TwoQ,
    TwoDash,
    TwoDash,
    ThreeW,
    TwoDash,
    TwoDash,
    ThreeW,
    TwoDash,
    TwoChargeDashSlow,
    TwoDash,
    ThreeW,
    TwoDash,
    TwoDash,
    ThreeW,
];
const SKIRK_QEA_120: &[Skirk] = &[
    E,
    TwoDash,
    TwoDash,
    TwoCharge,
    TwoDash,
    TwoDash,
    ThreeW,
    TwoDash,
    TwoDash,
    ThreeW,
    TwoDash,
    TwoChargeDashSlow,
    TwoDash,
    ThreeW,
    TwoDash,
    TwoDash,
    ThreeW,
];
const SKIRK_EQA_60: &[Skirk] = &[
    E, TwoDash, TwoDash, TwoQ, TwoDash, TwoDash, ThreeW, TwoDash, TwoDash, ThreeW, TwoDash,
    TwoCharge, TwoDash, TwoDash, FiveDash,
];

fn interpolate(table: &[(f64, f64)], fps: f64) -> f64 {
    if fps <= table[0].0 {
        return table[0].1;
    }
    for pair in table.windows(2) {
        let (lower_fps, lower_value) = pair[0];
        let (upper_fps, upper_value) = pair[1];
        if fps <= upper_fps {
            let weight = (fps - lower_fps) / (upper_fps - lower_fps);
            return lower_value + (upper_value - lower_value) * weight;
        }
    }
    table[table.len() - 1].1
}

fn key(name: &str) -> Target {
    Target::Key(name.to_owned())
}

fn down(target: Target) -> Instruction {
    Instruction::Input(Action::Down(target))
}

fn up(target: Target) -> Instruction {
    Instruction::Input(Action::Up(target))
}

fn wait(seconds: f64) -> Instruction {
    Instruction::WaitFor(Duration::from_secs_f64(seconds))
}

fn deadline(seconds: f64) -> Instruction {
    Instruction::WaitUntil(Duration::from_secs_f64(seconds))
}

fn tap(outputs: &mut Vec<Instruction>, target: Target, hold: f64) {
    outputs.push(down(target.clone()));
    if hold > 0.0 {
        outputs.push(wait(hold));
    }
    outputs.push(up(target));
}

fn spam(
    outputs: &mut Vec<Instruction>,
    target: Target,
    iterations: usize,
    cutoff: f64,
    hold: f64,
    interval: f64,
) {
    let mut body = Vec::new();
    tap(&mut body, target, hold);
    body.push(wait(interval));

    // The Python loop measures the actual elapsed time before each iteration.
    // A delayed event can shorten this burst without moving the primitive's next anchor.
    let stride = body.len() + 1;
    let deadline = Duration::from_secs_f64(cutoff);
    for remaining in (1..=iterations).rev() {
        outputs.push(Instruction::SkipIfPast {
            deadline,
            instructions: remaining * stride - 1,
        });
        outputs.extend_from_slice(&body);
    }
}

fn right_then_w(outputs: &mut Vec<Instruction>, frame: f64) {
    tap(outputs, Target::MouseRight, frame);
    outputs.push(wait(frame));
    tap(outputs, key("w"), frame);
}

fn skirk(outputs: &mut Vec<Instruction>, step: Skirk, fps: f64) {
    let step = match step {
        TwoChargeDashSlow if fps < 105.0 => TwoChargeDash,
        other => other,
    };
    outputs.push(Instruction::Origin);
    let frame = 1.0 / fps;
    let time = |table| interpolate(table, fps);
    match step {
        ThreeW => {
            spam(
                outputs,
                Target::MouseLeft,
                (0.6 * fps) as usize,
                0.6,
                0.0,
                2.0 * frame,
            );
            outputs.push(deadline(time(TIME_THREE_W)));
            tap(outputs, key("w"), frame);
        }
        ThreeDash => {
            spam(
                outputs,
                Target::MouseLeft,
                (0.6 * fps) as usize,
                0.6,
                frame,
                2.0 * frame,
            );
            outputs.push(wait(time(FRAMES_THREE_DASH) * frame));
            tap(outputs, Target::MouseRight, 0.0);
        }
        Two | TwoDash | TwoQ => {
            spam(
                outputs,
                Target::MouseLeft,
                (0.26 * fps) as usize,
                0.32,
                0.0,
                2.0 * frame,
            );
            match step {
                Two => outputs.push(deadline(time(TIME_TWO_DASH))),
                TwoDash => {
                    outputs.push(deadline(time(TIME_TWO_DASH)));
                    right_then_w(outputs, frame);
                    outputs.push(deadline(time(END_TWO_DASH)));
                }
                TwoQ => {
                    outputs.push(deadline(time(TIME_TWO_Q)));
                    tap(outputs, key("q"), frame);
                    outputs.extend([wait(frame), deadline(time(END_TWO_Q))]);
                }
                _ => unreachable!(),
            }
        }
        TwoCharge | TwoChargeDash | TwoChargeDashSlow => {
            let start = time(TIME_TWO_CHARGE);
            let slow = matches!(step, TwoChargeDashSlow);
            let cutoff = if slow { 0.26 } else { start };
            let charge = if slow { 0.28 } else { start + 2.0 * frame };
            spam(
                outputs,
                Target::MouseLeft,
                (0.2 * fps) as usize,
                cutoff,
                0.0,
                2.0 * frame,
            );
            outputs.push(deadline(charge));
            tap(outputs, Target::MouseLeft, 0.44);

            if matches!(step, TwoCharge) {
                outputs.push(deadline(time(END_TWO_CHARGE)));
            } else {
                outputs.push(wait(frame));
                if slow {
                    outputs.push(deadline(0.725));
                }
                right_then_w(outputs, frame);
                let end = if slow {
                    0.87
                } else {
                    time(END_TWO_CHARGE_DASH)
                };
                outputs.extend([wait(frame), deadline(end)]);
            }
        }
        Five | FiveDash => {
            spam(
                outputs,
                Target::MouseLeft,
                (2.0 * fps) as usize,
                2.1,
                0.0,
                2.0 * frame,
            );
            outputs.push(deadline(time(TIME_FIVE) + 2.0 * frame));
            if matches!(step, FiveDash) {
                tap(outputs, Target::MouseRight, frame);
                outputs.extend([wait(frame), deadline(time(END_FIVE_DASH) + 2.0 * frame)]);
            }
        }
        E => {
            spam(outputs, key("e"), (0.1 * fps) as usize, 0.1, frame, frame);
            outputs.push(wait(0.19));
        }
    }
}

fn skirk_sequence(sequence: &[Skirk], fps: f64, check_between: bool) -> Vec<Instruction> {
    let mut instructions = Vec::new();
    for (index, &step) in sequence.iter().enumerate() {
        if check_between && index != 0 {
            instructions.push(Instruction::Checkpoint);
        }
        skirk(&mut instructions, step, fps);
    }
    instructions
}

fn skirk_preset(name: &str, fps: f64) -> Option<Vec<Instruction>> {
    // The dashboard accepts 1–1,000 FPS. Bound the expanded spam instructions.
    if !(1.0..=1000.0).contains(&fps) {
        return None;
    }
    let step = match name {
        "skk3aw" => ThreeW,
        "skk3as" => ThreeDash,
        "skk2as" => TwoDash,
        "skk2a" => Two,
        "skk2az" => TwoCharge,
        "skk2azs" => TwoChargeDash,
        "skk2azs_slow" => TwoChargeDashSlow,
        "skk5as" => FiveDash,
        "skk5a" => Five,
        "skk2aq" => TwoQ,
        "skke" => E,
        "C0:  Combo Skirk C0 EQA 120fps" => return Some(skirk_sequence(SKIRK_EQA_120, fps, true)),
        "C0:  Combo Skirk C0 EA 120fps" => return Some(skirk_sequence(SKIRK_QEA_120, fps, true)),
        "C0:  Combo Skirk C0 EQA 60fps" => return Some(skirk_sequence(SKIRK_EQA_60, fps, true)),
        "skk223_loop" => {
            let mut body = skirk_sequence(SKIRK_223, fps, false);
            body.push(Instruction::Checkpoint);
            return Some(vec![Instruction::Cycle {
                body,
                max_duration: Some(MAX_LOOP_DURATION),
                cancellation: Cancellation::Checkpoint,
                cleanup: false,
            }]);
        }
        _ => return None,
    };
    let mut instructions = Vec::new();
    skirk(&mut instructions, step, fps);
    Some(instructions)
}

fn timeline_action(action: &str) -> Option<Instruction> {
    let (input, direction) = action.rsplit_once('_')?;
    let target = match input {
        "c" => Target::MouseLeft,
        "r" => Target::MouseRight,
        "d" => key("shift"),
        "q" => key("q"),
        "k1" => key("1"),
        "e" => key("e"),
        "w" => key("w"),
        "ka" => key("a"),
        "kd" => key("d"),
        _ => return None,
    };
    match direction {
        "down" => Some(down(target)),
        "up" => Some(up(target)),
        _ => None,
    }
}

fn timeline(events: &[(f64, &str)], loop_from_ms: Option<f64>) -> Option<Vec<Instruction>> {
    let mut instructions = vec![Instruction::Origin];
    for &(time_ms, action) in events {
        if loop_from_ms.is_some_and(|origin_ms| time_ms < origin_ms) {
            continue;
        }
        let offset = ((time_ms - loop_from_ms.unwrap_or_default()) / 1000.0).max(0.0);
        instructions.push(Instruction::WaitUntil(
            Duration::try_from_secs_f64(offset).ok()?,
        ));
        // "end" and unrecognized actions still consume their configured time.
        instructions.extend(timeline_action(action));
    }
    Some(instructions)
}

fn timeline_preset(name: &str, timing: &Value) -> Option<Vec<Instruction>> {
    let timeline_name = match name {
        MAVUIKA | "mav_cdcdcf" => MAVUIKA,
        ARLECCHINO => ARLECCHINO,
        "mav_b_q" => "beat_q",
        "mav_b_c" => "beat_c",
        "mav_b_d" => "beat_d",
        "mav_b_cd" => "beat_cd",
        "mav_b_cdf" => "beat_cdf",
        "mav_b_click" => "beat_click",
        "mav_b_click2" => "beat_click2",
        _ => return None,
    };
    let data = timing.get(timeline_name)?;
    let events: Vec<_> = data["events"]
        .as_array()?
        .iter()
        .map(|event| Some((event.get(0)?.as_f64()?, event.get(1)?.as_str()?)))
        .collect::<Option<_>>()?;
    if events.is_empty() {
        return None;
    }

    let body = timeline(&events, None)?;
    match data["mode"].as_str() {
        Some("once") => {
            let mut steps = vec![Instruction::Block {
                body,
                cancellation: Cancellation::Hold,
                cleanup: true,
            }];
            if let Some(origin_ms) = data["loop_from_ms"].as_f64() {
                let body = timeline(&events, Some(origin_ms))?;
                if body.len() > 1 {
                    steps.push(Instruction::Cycle {
                        body,
                        max_duration: None,
                        cancellation: Cancellation::Hold,
                        cleanup: true,
                    });
                }
            }
            Some(steps)
        }
        Some("once_locked") => Some(vec![Instruction::Block {
            body,
            cancellation: Cancellation::Foreground,
            cleanup: true,
        }]),
        _ => Some(vec![Instruction::Cycle {
            body,
            max_duration: Some(MAX_LOOP_DURATION),
            cancellation: Cancellation::Hold,
            cleanup: true,
        }]),
    }
}

pub fn preset(name: &str, fps: f64, mavuika: &Value) -> Option<Preset> {
    if let Some(instructions) = skirk_preset(name, fps) {
        return Some(Preset {
            instructions,
            uses_held_input: false,
        });
    }
    if let Some(instructions) = timeline_preset(name, mavuika) {
        return Some(Preset {
            instructions,
            uses_held_input: true,
        });
    }
    let duration_ms = match name {
        "wait_50" => Some(50),
        "wait_100" => Some(100),
        "wait_150" => Some(150),
        "wait_200" => Some(200),
        "wait_300" => Some(300),
        "wait_500" => Some(500),
        "wait_1000" => Some(1000),
        _ => None,
    };
    if let Some(duration_ms) = duration_ms {
        return Some(Preset {
            instructions: vec![Instruction::Block {
                body: vec![Instruction::WaitFor(Duration::from_millis(duration_ms))],
                cancellation: Cancellation::Hold,
                cleanup: false,
            }],
            uses_held_input: false,
        });
    }

    let mut instructions = Vec::new();
    match name {
        "mav_c_hold" => instructions.push(down(Target::MouseLeft)),
        "mav_c_rel" => instructions.push(up(Target::MouseLeft)),
        "mav_d" => tap(&mut instructions, key("shift"), 0.091),
        "mav_q" => tap(&mut instructions, key("q"), 0.202),
        _ => return None,
    }
    Some(Preset {
        instructions,
        uses_held_input: true,
    })
}

pub fn plan(name: &str, fps: f64, mavuika: &Value) -> Option<Vec<Instruction>> {
    preset(name, fps, mavuika).map(|preset| preset.instructions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn skirk_two_dash_keeps_anchored_deadlines_and_right_then_w() {
        let instructions = plan("skk2as", 90.0, &Value::Null).unwrap();
        let first = instructions
            .iter()
            .position(|step| *step == Instruction::WaitUntil(Duration::from_millis(420)))
            .unwrap();
        assert_eq!(
            instructions[first..]
                .iter()
                .filter_map(|step| match step {
                    Instruction::Input(action) => Some(action.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [
                Action::Down(Target::MouseRight),
                Action::Up(Target::MouseRight),
                Action::Down(Target::Key("w".into())),
                Action::Up(Target::Key("w".into())),
            ],
        );
        assert_eq!(
            instructions.last(),
            Some(&Instruction::WaitUntil(Duration::from_millis(600))),
        );
        let Instruction::SkipIfPast {
            deadline,
            instructions: count,
        } = instructions[1]
        else {
            panic!("the spam loop must check elapsed time");
        };
        assert_eq!(deadline, Duration::from_millis(320));
        assert_eq!(2 + count, first);
    }

    #[test]
    fn skirk_three_dash_interpolates_frames_and_slow_switches_at_105() {
        let three_dash = plan("skk3as", 132.0, &Value::Null).unwrap();
        assert_eq!(
            three_dash[three_dash.len() - 3],
            Instruction::WaitFor(Duration::from_secs_f64(21.0 / 132.0)),
        );
        assert_eq!(
            plan("skk2azs_slow", 104.0, &Value::Null),
            plan("skk2azs", 104.0, &Value::Null),
        );
        assert_eq!(
            plan("skk2azs_slow", 105.0, &Value::Null).unwrap().last(),
            Some(&Instruction::WaitUntil(Duration::from_millis(870))),
        );
    }

    #[test]
    fn skirk_named_sequences_have_python_checkpoint_positions() {
        for (name, count) in [
            ("C0:  Combo Skirk C0 EQA 120fps", 16),
            ("C0:  Combo Skirk C0 EA 120fps", 16),
            ("C0:  Combo Skirk C0 EQA 60fps", 14),
        ] {
            let instructions = plan(name, 120.0, &Value::Null).unwrap();
            assert_eq!(
                instructions
                    .iter()
                    .filter(|step| **step == Instruction::Checkpoint)
                    .count(),
                count,
            );
            assert_ne!(instructions.last(), Some(&Instruction::Checkpoint));
        }
    }

    #[test]
    fn mavuika_mode_controls_cancellation_cleanup_and_loop_origin() {
        let timelines = json!({
            "beat_q": {
                "mode": "once",
                "loop_from_ms": 100,
                "events": [[0, "q_down"], [100, "q_up"], [250, "end"]]
            },
            "beat_click": {
                "mode": "once_locked",
                "events": [[0, "c_down"], [125, "c_up"]]
            },
            "C0:  Combo Arlecchino Overload N2W (GI)": {
                "events": [[0, "ka_down"], [40, "ka_up"], [960, "end"]]
            }
        });
        let first = plan("mav_b_q", 60.0, &timelines).unwrap();
        assert_eq!(first, plan("mav_b_q", 220.0, &timelines).unwrap());
        assert!(matches!(
            first[0],
            Instruction::Block {
                cancellation: Cancellation::Hold,
                cleanup: true,
                ..
            }
        ));
        let Instruction::Cycle {
            body,
            max_duration: None,
            cleanup: true,
            ..
        } = &first[1]
        else {
            panic!("once mode must run the configured tail from a fresh origin");
        };
        assert_eq!(
            body,
            &[
                Instruction::Origin,
                Instruction::WaitUntil(Duration::ZERO),
                Instruction::Input(Action::Up(Target::Key("q".into()))),
                Instruction::WaitUntil(Duration::from_millis(150)),
            ],
        );
        assert!(matches!(
            plan("mav_b_click", 120.0, &timelines).unwrap()[0],
            Instruction::Block {
                cancellation: Cancellation::Foreground,
                cleanup: true,
                ..
            }
        ));
        assert!(matches!(
            plan("C0:  Combo Arlecchino Overload N2W (GI)", 120.0, &timelines).unwrap()[0],
            Instruction::Cycle { max_duration: Some(d), cancellation: Cancellation::Hold, .. }
                if d == Duration::from_secs(20)
        ));
    }

    #[test]
    fn accepted_custom_names_preserve_standalone_held_steps_and_waits() {
        assert_eq!(
            plan("mav_c_hold", 120.0, &Value::Null).unwrap(),
            [Instruction::Input(Action::Down(Target::MouseLeft))],
        );
        assert_eq!(
            plan("mav_c_rel", 120.0, &Value::Null).unwrap(),
            [Instruction::Input(Action::Up(Target::MouseLeft))],
        );
        let wait = plan("wait_1000", 120.0, &Value::Null).unwrap();
        assert_eq!(
            wait,
            [Instruction::Block {
                body: vec![Instruction::WaitFor(Duration::from_secs(1))],
                cancellation: Cancellation::Hold,
                cleanup: false,
            }],
        );
        assert!(plan("skk_typo", 120.0, &Value::Null).is_none());
    }
}
