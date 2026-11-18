use crate::presets::{self, Action, Cancellation, Instruction, Target};
use serde_json::Value;
use std::{io, time::Duration};

pub struct CustomProgram {
    pub instructions: Vec<Instruction>,
    pub skipped_steps: usize,
    pub uses_held_input: bool,
}

fn mouse(name: &str) -> Option<Target> {
    match name {
        "left" => Some(Target::MouseLeft),
        "right" => Some(Target::MouseRight),
        "middle" => Some(Target::MouseMiddle),
        "mouse_3" | "x1" => Some(Target::MouseX1),
        "mouse_4" | "x2" => Some(Target::MouseX2),
        _ => None,
    }
}

fn key(name: &str) -> Option<Target> {
    let normalized = match name {
        "num_enter" => "enter",
        "num_lock_on" | "num_lock_off" => "num_lock",
        "caps_lock_on" | "caps_lock_off" => "caps_lock",
        "scroll_lock_on" | "scroll_lock_off" => "scroll_lock",
        _ => name,
    };
    if name.chars().count() == 1 {
        return Some(Target::Key(name.to_lowercase()));
    }
    let named_keyboard_target = matches!(
        normalized,
        "left"
            | "right"
            | "num_0"
            | "num_1"
            | "num_2"
            | "num_3"
            | "num_4"
            | "num_5"
            | "num_6"
            | "num_7"
            | "num_8"
            | "num_9"
            | "num_mul"
            | "num_add"
            | "num_sub"
            | "num_dec"
            | "num_div"
            | "break"
            | "ctrl_break"
            | "browser_back"
            | "browser_forward"
            | "browser_refresh"
            | "browser_stop"
            | "browser_search"
            | "browser_favorites"
            | "browser_home"
    );
    if named_keyboard_target {
        return Some(Target::Key(normalized.to_owned()));
    }
    if let Some((encoding, number)) = name.split_once(':') {
        if !matches!(encoding, "sc" | "sce" | "vkc" | "ext") {
            return None;
        }
        let number = number.trim();
        let number = number.strip_prefix('+').unwrap_or(number);
        let (base, digits) = match number.get(..2) {
            Some("0x" | "0X") => (16, &number[2..]),
            Some("0o" | "0O") => (8, &number[2..]),
            Some("0b" | "0B") => (2, &number[2..]),
            _ => (10, number),
        };
        let code = u16::from_str_radix(&digits.replace('_', ""), base).ok()?;
        return (code != 0).then(|| Target::Key(format!("{encoding}:{code}")));
    }
    match crate::bindings::parse_hotkey(normalized) {
        Some(target @ Target::Key(_)) => Some(target),
        _ => None,
    }
}

fn millis(value: Option<&Value>, default: f64, min: f64, max: f64) -> Duration {
    let number = value
        .and_then(|value| match value {
            Value::String(number) => number.trim().parse::<f64>().ok(),
            Value::Bool(true) => Some(1.0),
            Value::Bool(false) => Some(0.0),
            _ => value.as_f64(),
        })
        .filter(|number| number.is_finite())
        .unwrap_or(default);
    Duration::from_secs_f64(number.clamp(min, max) / 1000.0)
}

fn input(step: &Value, kind: &str) -> Option<Instruction> {
    let target = if kind.starts_with("key_") {
        key(step.get("key")?.as_str()?)?
    } else {
        mouse(step.get("button").map_or(Some("left"), Value::as_str)?)?
    };
    Some(match kind {
        "key_down" | "mouse_down" => Instruction::Input(Action::Down(target)),
        "key_up" | "mouse_up" => Instruction::Input(Action::Up(target)),
        "key_tap" | "mouse_click" => interruptible(vec![
            Instruction::Input(Action::Down(target.clone())),
            Instruction::WaitFor(millis(step.get("hold"), 50.0, 1.0, 5_000.0)),
            Instruction::Input(Action::Up(target)),
        ]),
        _ => return None,
    })
}

fn interruptible(body: Vec<Instruction>) -> Instruction {
    Instruction::Block {
        body,
        cancellation: Cancellation::Hold,
        cleanup: false,
    }
}

fn has_timed_step(instruction: &Instruction) -> bool {
    match instruction {
        Instruction::WaitFor(duration) | Instruction::WaitUntil(duration) => !duration.is_zero(),
        Instruction::Block { body, .. }
        | Instruction::Cycle { body, .. }
        | Instruction::Call(body) => body.iter().any(has_timed_step),
        _ => false,
    }
}

pub fn compile(profile: &Value, fps: f64, timings: &Value) -> io::Result<CustomProgram> {
    let sequence = profile
        .get("pythonSequence")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "profile has no pythonSequence array",
            )
        })?;
    let mut instructions = Vec::new();
    let mut loop_start = None;
    let mut skipped_steps = 0;
    let mut uses_held_input = false;
    for step in sequence {
        if let Some(name) = step.as_str() {
            if name == "is_no_key_pressed" {
                instructions.push(Instruction::Checkpoint);
            } else if let Some(preset) = presets::preset(name, fps, timings) {
                uses_held_input |= preset.uses_held_input;
                instructions.push(Instruction::Call(preset.instructions));
            } else {
                skipped_steps += 1;
            }
            continue;
        }
        let Some(kind) = step.get("type").and_then(Value::as_str) else {
            skipped_steps += 1;
            continue;
        };
        match kind {
            "wait" => instructions.push(interruptible(vec![Instruction::WaitFor(millis(
                step.get("ms"),
                50.0,
                0.0,
                60_000.0,
            ))])),
            "loop_start" => loop_start = Some(instructions.len()),
            "loop_end" => {
                if let Some(start) = loop_start {
                    let body = instructions.split_off(start);
                    if !body.iter().any(has_timed_step) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "macro loop has no positive timed step",
                        ));
                    }
                    instructions.push(Instruction::Cycle {
                        body,
                        max_duration: None,
                        cancellation: Cancellation::Hold,
                        cleanup: false,
                    });
                    // The legacy loop can exit only by interrupting the profile.
                    break;
                }
            }
            _ => match input(step, kind) {
                Some(instruction) => instructions.push(instruction),
                None => skipped_steps += 1,
            },
        }
    }
    if instructions.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "macro has no executable steps",
        ));
    }
    Ok(CustomProgram {
        instructions,
        skipped_steps,
        uses_held_input,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unicode_input_and_duration_coercion() -> io::Result<()> {
        let program = compile(
            &json!({"pythonSequence": [
                "unknown", {"type":"key_tap", "key":"Đ", "hold":"0"}, {"type":"wait", "ms": 70000}
            ]}),
            120.0,
            &json!({}),
        )?;
        assert_eq!(program.skipped_steps, 1);
        let Instruction::Block { body, .. } = &program.instructions[0] else {
            panic!("expected tap");
        };
        assert_eq!(
            body,
            &[
                Instruction::Input(Action::Down(Target::Key("đ".into()))),
                Instruction::WaitFor(Duration::from_millis(1)),
                Instruction::Input(Action::Up(Target::Key("đ".into())))
            ]
        );
        let Instruction::Block { body, .. } = &program.instructions[1] else {
            panic!("expected wait");
        };
        assert_eq!(body, &[Instruction::WaitFor(Duration::from_secs(60))]);
        Ok(())
    }

    #[test]
    fn rejects_unknown_key_names_and_preserves_virtual_keys() -> io::Result<()> {
        let program = compile(
            &json!({"pythonSequence": [
                {"type":"key_down", "key":"qqq"}, {"type":"key_down", "key":"left"},
                {"type":"key_up", "key":"vkc:0x1B"}, {"type":"key_up", "key":"sc:xyz"}
            ]}),
            120.0,
            &json!({}),
        )?;
        assert_eq!(program.skipped_steps, 2);
        assert_eq!(
            program.instructions,
            [
                Instruction::Input(Action::Down(Target::Key("left".into()))),
                Instruction::Input(Action::Up(Target::Key("vkc:27".into()))),
            ]
        );
        Ok(())
    }

    #[test]
    fn loops_use_last_start_and_keep_prefix() -> io::Result<()> {
        let config = json!({"pythonSequence": [
            {"type":"key_down", "key":"w"}, {"type":"loop_start"}, {"type":"wait", "ms":30},
            {"type":"loop_start"}, {"type":"mouse_click", "button":"mouse_4"}, {"type":"loop_end"},
            {"type":"key_up", "key":"w"}
        ]});
        let program = compile(&config, 120.0, &json!({}))?;
        assert_eq!(program.instructions.len(), 3);
        assert!(matches!(
            program.instructions[2],
            Instruction::Cycle {
                max_duration: None,
                ..
            }
        ));
        Ok(())
    }
}
