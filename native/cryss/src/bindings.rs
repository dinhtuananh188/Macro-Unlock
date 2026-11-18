use crate::presets::{self, Action, Instruction, Target};
use crate::program;
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub name: String,
    pub instructions: Vec<Instruction>,
    pub uses_held_input: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingIssue {
    UnknownPreset,
    UnavailablePreset,
    InvalidHotkey,
    InvalidCustomProgram(String),
    HeldInputHotkey,
    SelfTrigger,
    SkippedSteps(usize),
    Overridden { previous: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingDiagnostic {
    pub name: String,
    pub hotkey: String,
    pub issue: BindingIssue,
}

#[derive(Debug, Default)]
pub struct Bindings {
    pub active: HashMap<Target, Binding>,
    pub diagnostics: Vec<BindingDiagnostic>,
    order: Vec<Target>,
}

impl Bindings {
    pub fn iter(&self) -> impl Iterator<Item = (&Target, &Binding)> {
        self.order
            .iter()
            .filter_map(|target| self.active.get(target).map(|binding| (target, binding)))
    }

    fn report(&mut self, name: &str, hotkey: &str, issue: BindingIssue) {
        self.diagnostics.push(BindingDiagnostic {
            name: name.into(),
            hotkey: hotkey.into(),
            issue,
        });
    }

    fn insert(&mut self, target: Target, hotkey: &str, binding: Binding) {
        let name = binding.name.clone();
        if let Some(previous) = self.active.insert(target.clone(), binding) {
            self.report(
                &name,
                hotkey,
                BindingIssue::Overridden {
                    previous: previous.name,
                },
            );
        } else {
            self.order.push(target);
        }
    }
}

const BUILT_INS: &[&str] = &[
    "C0:  Combo Skirk C0 EQA 120fps",
    "C0:  Combo Skirk C0 EA 120fps",
    "C0:  Combo Skirk C0 EQA 60fps",
    "C0:  Combo Mavuika CDCDCF (Full Combo)",
    "C0:  Combo Arlecchino Overload N2W (GI)",
];

fn canonical_key(name: &str) -> &str {
    match name {
        "shift_l" => "shift",
        "cmd_l" => "cmd",
        _ => name,
    }
}

fn function_key(name: &str) -> bool {
    name.strip_prefix('f').is_some_and(|number| {
        !number.starts_with('0')
            && number.bytes().all(|byte| byte.is_ascii_digit())
            && number
                .parse::<u8>()
                .is_ok_and(|number| (1..=24).contains(&number))
    })
}

pub fn parse_hotkey(name: &str) -> Option<Target> {
    let key = match name {
        // Hotkey parsing has always preferred mouse button names; action blocks
        // still interpret these names in the namespace of the stated device.
        "left" => Target::MouseLeft,
        "right" => Target::MouseRight,
        "middle" => Target::MouseMiddle,
        "mouse_3" | "x1" => Target::MouseX1,
        "mouse_4" | "x2" => Target::MouseX2,
        "alt" | "alt_l" | "alt_r" | "alt_gr" | "backspace" | "caps_lock" | "cmd" | "cmd_l"
        | "cmd_r" | "ctrl" | "ctrl_l" | "ctrl_r" | "delete" | "down" | "end" | "enter" | "esc"
        | "home" | "page_down" | "page_up" | "shift" | "shift_l" | "shift_r" | "space" | "tab"
        | "up" | "media_play_pause" | "media_stop" | "media_volume_mute" | "media_volume_down"
        | "media_volume_up" | "media_previous" | "media_next" | "insert" | "menu" | "num_lock"
        | "pause" | "print_screen" | "scroll_lock" => Target::Key(canonical_key(name).into()),
        name if function_key(name) => Target::Key(name.into()),
        character if character.chars().count() == 1 => Target::Key(character.to_lowercase()),
        _ => return None,
    };
    Some(key)
}

fn blocked_held_input(target: &Target) -> bool {
    matches!(target, Target::MouseLeft | Target::MouseRight)
        || matches!(target, Target::Key(key) if key == "shift" || key == "shift_r")
}

fn same_input(output: &Target, hotkey: &Target) -> bool {
    match (output, hotkey) {
        (Target::Key(output), Target::Key(hotkey)) => {
            canonical_key(output) == canonical_key(hotkey)
        }
        _ => output == hotkey,
    }
}

fn emits(instructions: &[Instruction], hotkey: &Target) -> bool {
    instructions.iter().any(|instruction| match instruction {
        Instruction::Input(Action::Down(output) | Action::Up(output)) => same_input(output, hotkey),
        Instruction::Block { body, .. } | Instruction::Cycle { body, .. } => emits(body, hotkey),
        _ => false,
    })
}

fn emits_explicit_action(profile: &Value, fps: f64, timings: &Value, hotkey: &Target) -> bool {
    // Legacy self-trigger checks cover authored key/mouse blocks, including
    // actions written after a loop. Reuse the program parser for their targets.
    let explicit_steps: Vec<_> = profile["pythonSequence"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|step| {
            matches!(
                step.get("type").and_then(Value::as_str),
                Some("key_tap" | "key_down" | "key_up" | "mouse_click" | "mouse_down" | "mouse_up")
            )
        })
        .collect();
    if explicit_steps.is_empty() {
        return false;
    }
    program::compile(&json!({ "pythonSequence": explicit_steps }), fps, timings)
        .is_ok_and(|compiled| emits(&compiled.instructions, hotkey))
}

pub fn compile(config: &Value, fps: f64, timings: &Value) -> Bindings {
    let mut bindings = Bindings::default();
    for (name, value) in config["comboSignKeys"].as_object().into_iter().flatten() {
        let hotkey = value.as_str().unwrap_or_default();
        if !BUILT_INS.contains(&name.as_str()) {
            bindings.report(name, hotkey, BindingIssue::UnknownPreset);
            continue;
        }
        let Some(target) = parse_hotkey(hotkey) else {
            bindings.report(name, hotkey, BindingIssue::InvalidHotkey);
            continue;
        };
        let Some(preset) = presets::preset(name, fps, timings) else {
            bindings.report(name, hotkey, BindingIssue::UnavailablePreset);
            continue;
        };
        if preset.uses_held_input && blocked_held_input(&target) {
            bindings.report(name, hotkey, BindingIssue::HeldInputHotkey);
            continue;
        }
        bindings.insert(
            target,
            hotkey,
            Binding {
                name: name.clone(),
                instructions: preset.instructions,
                uses_held_input: preset.uses_held_input,
            },
        );
    }

    for profile in config["customCombos"].as_array().into_iter().flatten() {
        let hotkey = &profile["hotkey"];
        if hotkey.is_null()
            || hotkey.as_str() == Some("")
            || profile["pythonSequence"].is_null()
            || profile["pythonSequence"]
                .as_array()
                .is_some_and(Vec::is_empty)
        {
            continue;
        }
        let name = profile["name"].as_str().unwrap_or("custom");
        let hotkey = hotkey.as_str().unwrap_or_default();
        let Some(target) = parse_hotkey(hotkey) else {
            bindings.report(name, hotkey, BindingIssue::InvalidHotkey);
            continue;
        };
        let compiled = match program::compile(profile, fps, timings) {
            Ok(compiled) => compiled,
            Err(error) => {
                bindings.report(
                    name,
                    hotkey,
                    BindingIssue::InvalidCustomProgram(error.to_string()),
                );
                continue;
            }
        };
        if compiled.uses_held_input && blocked_held_input(&target) {
            bindings.report(name, hotkey, BindingIssue::HeldInputHotkey);
            continue;
        }
        if emits_explicit_action(profile, fps, timings, &target) {
            bindings.report(name, hotkey, BindingIssue::SelfTrigger);
            continue;
        }
        if compiled.skipped_steps != 0 {
            bindings.report(
                name,
                hotkey,
                BindingIssue::SkippedSteps(compiled.skipped_steps),
            );
        }
        bindings.insert(
            target,
            hotkey,
            Binding {
                name: name.into(),
                instructions: compiled.instructions,
                uses_held_input: compiled.uses_held_input,
            },
        );
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn understands_legacy_mouse_aliases_and_windows_key_names() {
        assert_eq!(parse_hotkey("mouse_3"), parse_hotkey("x1"));
        assert_eq!(parse_hotkey("mouse_4"), parse_hotkey("x2"));
        assert_eq!(parse_hotkey("left"), Some(Target::MouseLeft));
        assert_eq!(parse_hotkey("right"), Some(Target::MouseRight));
        assert_eq!(parse_hotkey("shift_l"), parse_hotkey("shift"));
        assert_eq!(parse_hotkey("cmd_l"), parse_hotkey("cmd"));
        assert_eq!(parse_hotkey("Đ"), Some(Target::Key("đ".into())));
        assert_eq!(parse_hotkey("f24"), Some(Target::Key("f24".into())));
        for unknown in ["", "f01", "f25", "numpad_0", "normal", "mouse_5"] {
            assert_eq!(parse_hotkey(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn successful_bindings_win_in_json_order_with_custom_last() {
        let config: Value = serde_json::from_str(
            r#"{
            "comboSignKeys": {
                "C0:  Combo Skirk C0 EQA 120fps": "s",
                "C0:  Combo Skirk C0 EA 120fps": "S",
                "C0:  Combo Skirk C0 EQA 60fps": "x1"
            },
            "customCombos": [
                {"name":"Đầu", "hotkey":"mouse_3", "pythonSequence":[{"type":"key_tap","key":"z"}]},
                {"name":"Cuối", "hotkey":"x1", "pythonSequence":[{"type":"key_tap","key":"v"}]}
            ]
        }"#,
        )
        .unwrap();
        let bindings = compile(&config, 120.0, &Value::Null);
        assert_eq!(
            bindings.active[&Target::Key("s".into())].name,
            "C0:  Combo Skirk C0 EA 120fps"
        );
        assert_eq!(bindings.active[&Target::MouseX1].name, "Cuối");
        assert_eq!(bindings.active.len(), 2);
        assert_eq!(
            bindings
                .iter()
                .map(|(target, _)| target.clone())
                .collect::<Vec<_>>(),
            [Target::Key("s".into()), Target::MouseX1],
        );
        assert_eq!(
            bindings
                .diagnostics
                .iter()
                .filter(|d| matches!(d.issue, BindingIssue::Overridden { .. }))
                .count(),
            3
        );
    }

    #[test]
    fn self_trigger_scope_matches_legacy_custom_programs() {
        let config = json!({
            "customCombos": [
                {"name":"primitive", "hotkey":"w", "pythonSequence":["skk3aw"]},
                {"name":"after loop", "hotkey":"a", "pythonSequence":[
                    {"type":"loop_start"}, {"type":"wait"}, {"type":"loop_end"},
                    {"type":"key_up","key":"a"}
                ]}
            ]
        });
        let bindings = compile(&config, 120.0, &Value::Null);
        assert!(bindings.active.contains_key(&Target::Key("w".into())));
        assert!(!bindings.active.contains_key(&Target::Key("a".into())));
        assert_eq!(bindings.diagnostics[0].issue, BindingIssue::SelfTrigger);
    }

    #[test]
    fn blocked_outputs_leave_the_previous_binding_active() {
        let profile = "C0:  Combo Arlecchino Overload N2W (GI)";
        let config = json!({
            "comboSignKeys": {
                "C0:  Combo Skirk C0 EA 120fps": "s",
                "C0:  Combo Arlecchino Overload N2W (GI)": "shift_r"
            },
            "customCombos": [
                {"name": "key", "hotkey":"S", "pythonSequence":[{"type":"key_tap","key":"s"}]},
                {"name": "side", "hotkey":"x1", "pythonSequence":[{"type":"mouse_click","button":"mouse_3"}]},
                {"name": "arrow", "hotkey":"left", "pythonSequence":[{"type":"key_tap","key":"left"}]},
                {"name": "legacy hold", "hotkey":"shift_l", "pythonSequence":["mav_c_hold"]}
            ]
        });
        let timings = json!({(profile): {"events": [[0,"w_down"],[40,"w_up"],[80,"end"]]}});
        let bindings = compile(&config, 120.0, &timings);
        assert_eq!(
            bindings.active[&Target::Key("s".into())].name,
            "C0:  Combo Skirk C0 EA 120fps"
        );
        assert_eq!(bindings.active[&Target::MouseLeft].name, "arrow");
        assert_eq!(bindings.active.len(), 2);
        assert_eq!(
            bindings
                .diagnostics
                .iter()
                .filter(|d| d.issue == BindingIssue::SelfTrigger)
                .count(),
            2
        );
        assert_eq!(
            bindings
                .diagnostics
                .iter()
                .filter(|d| d.issue == BindingIssue::HeldInputHotkey)
                .count(),
            2
        );
    }

    #[test]
    fn reports_invalid_or_unavailable_entries_and_skips_unbound_profiles() {
        let config = json!({
            "comboSignKeys": {
                "skk2a":"q",
                "C0:  Combo Mavuika CDCDCF (Full Combo)":"t",
                "C0:  Combo Skirk C0 EA 120fps":"unknown"
            },
            "customCombos": [
                {"name":"Unbound", "hotkey":"", "pythonSequence":[{"type":"wait"}]},
                {"name":"Bound", "hotkey":"w", "pythonSequence":["bad", {"type":"wait"}]}
            ]
        });
        let issues: Vec<_> = compile(&config, 120.0, &Value::Null)
            .diagnostics
            .into_iter()
            .map(|item| item.issue)
            .collect();
        assert!(issues.contains(&BindingIssue::UnknownPreset));
        assert!(issues.contains(&BindingIssue::UnavailablePreset));
        assert!(issues.contains(&BindingIssue::InvalidHotkey));
        assert!(issues.contains(&BindingIssue::SkippedSteps(1)));
        assert_eq!(issues.len(), 4);
    }
}
