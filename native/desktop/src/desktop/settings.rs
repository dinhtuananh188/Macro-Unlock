use crate::bindings::{self, BindingIssue, Bindings};
use crate::config::ConfigStore;
use crate::presets::Target;
use serde_json::{Map, Value, json};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::views::Profile;

// These identifiers are also the keys saved by the Electron and Python versions.
pub const BUILTIN_NAMES: [&str; 5] = [
    "C0:  Combo Skirk C0 EQA 120fps",
    "C0:  Combo Skirk C0 EA 120fps",
    "C0:  Combo Skirk C0 EQA 60fps",
    "C0:  Combo Mavuika CDCDCF (Full Combo)",
    "C0:  Combo Arlecchino Overload N2W (GI)",
];

pub struct Settings {
    store: ConfigStore,
    config: Value,
    pub bindings: Bindings,
}

impl Settings {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let store = ConfigStore::new(path);
        let config = store.load()?;
        let timings = store.load_mavuika()?;
        let bindings = bindings::compile(&config, f64::from(fps(&config)), &timings);
        Ok(Self { store, config, bindings })
    }

    pub fn fps(&self) -> i32 { fps(&self.config) }

    pub fn profiles(&self) -> Vec<Profile> {
        let mut items: Vec<_> = BUILTIN_NAMES.iter().map(|&name| Profile {
            id: name.to_owned(), name: name.to_owned(),
            hotkey: self.config["comboSignKeys"][name].as_str().unwrap_or_default().to_owned(), custom: false,
        }).collect();
        if let Some(profiles) = self.config["customCombos"].as_array() {
            items.extend(profiles.iter().filter_map(|profile| Some(Profile {
                id: profile["id"].as_str()?.to_owned(),
                name: profile["name"].as_str()?.to_owned(),
                hotkey: profile["hotkey"].as_str().unwrap_or_default().to_owned(), custom: true,
            })));
        }
        items
    }

    pub fn custom_profile(&self, id: &str) -> Option<&Value> {
        self.config["customCombos"].as_array()?.iter().find(|p| p["id"].as_str() == Some(id))
    }

    pub fn unused_id(&self) -> String {
        let epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let base = format!("cryss-{}-{epoch}", std::process::id());
        (0u64..).map(|retry| format!("{base}-{retry}")).find(|id| self.custom_profile(id).is_none()).unwrap()
    }

    pub fn set_fps(&mut self, fps: i32) -> Result<(), String> {
        if !(1..=1000).contains(&fps) { return Err("FPS phải từ 1 đến 1000.".into()); }
        self.edit(|config| { config["FPS"] = fps.into(); Ok(()) }, None)
    }

    pub fn set_binding(&mut self, id: &str, hotkey: &str) -> Result<(), String> {
        let hotkey = hotkey.trim();
        if !hotkey.is_empty() && bindings::parse_hotkey(hotkey).is_none() {
            return Err(format!("Phím tắt không hợp lệ: {hotkey}"));
        }
        self.edit(|config| {
            if BUILTIN_NAMES.contains(&id) {
                let map = object_field(config, "comboSignKeys")?;
                if hotkey.is_empty() { map.remove(id); }
                else { map.insert(id.to_owned(), hotkey.into()); }
                return Ok(());
            }
            let profile = custom_profiles(config)?.iter_mut().find(|p| p["id"].as_str() == Some(id))
                .ok_or_else(|| "Không tìm thấy combo.".to_owned())?;
            profile["hotkey"] = if hotkey.is_empty() { Value::Null } else { hotkey.into() };
            Ok(())
        }, Some(id))
    }

    pub fn save_profile(&mut self, profile: Value) -> Result<(), String> {
        let id = profile["id"].as_str().ok_or_else(|| "Thiếu mã combo.".to_owned())?.to_owned();
        self.edit(|config| {
            let profiles = custom_profiles(config)?;
            if let Some(existing) = profiles.iter_mut().find(|p| p["id"].as_str() == Some(&id)) {
                let updated = profile.as_object().ok_or_else(|| "Combo không hợp lệ.".to_owned())?;
                for (field, value) in updated { existing[field] = value.clone(); }
            } else {
                profiles.push(profile);
            }
            Ok(())
        }, Some(&id))
    }

    pub fn delete_profile(&mut self, id: &str) -> Result<(), String> {
        self.edit(|config| {
            let profiles = custom_profiles(config)?;
            let index = profiles.iter().position(|p| p["id"].as_str() == Some(id))
                .ok_or_else(|| "Không tìm thấy combo.".to_owned())?;
            profiles.remove(index);
            Ok(())
        }, None)
    }

    fn edit(&mut self, edit: impl FnOnce(&mut Value) -> Result<(), String>, validate_id: Option<&str>) -> Result<(), String> {
        // Read the latest copy so an edit of one field preserves settings saved elsewhere.
        let mut config = self.store.load().map_err(|error| error.to_string())?;
        let timings = self.store.load_mavuika().map_err(|error| error.to_string())?;
        edit(&mut config)?;
        let bindings = bindings::compile(&config, f64::from(fps(&config)), &timings);
        if let Some(id) = validate_id {
            let (name, hotkey) = if BUILTIN_NAMES.contains(&id) {
                (id, config["comboSignKeys"][id].as_str().unwrap_or_default())
            } else {
                let profile = config["customCombos"].as_array().into_iter().flatten()
                    .find(|p| p["id"].as_str() == Some(id)).ok_or_else(|| "Không tìm thấy combo.".to_owned())?;
                (profile["name"].as_str().unwrap_or_default(), profile["hotkey"].as_str().unwrap_or_default())
            };
            if !hotkey.is_empty() {
                let invalid = bindings.diagnostics.iter().find(|d| d.name == name && d.hotkey == hotkey
                    && !matches!(d.issue, BindingIssue::Overridden { .. } | BindingIssue::SkippedSteps(_)));
                if let Some(diagnostic) = invalid { return Err(issue(&diagnostic.issue)); }
            }
        }
        self.store.save(&config).map_err(|error| error.to_string())?;
        self.config = config;
        self.bindings = bindings;
        Ok(())
    }
}

fn fps(config: &Value) -> i32 {
    config["FPS"].as_f64().or_else(|| config["FPS"].as_str()?.trim().parse().ok())
        .filter(|fps| fps.is_finite() && (1.0..=1000.0).contains(fps)).map_or(120, |fps| fps as i32)
}

fn object_field<'a>(config: &'a mut Value, key: &str) -> Result<&'a mut Map<String, Value>, String> {
    if config.get(key).is_none() { config[key] = json!({}); }
    config[key].as_object_mut().ok_or_else(|| format!("Trường {key} không hợp lệ."))
}

fn custom_profiles(config: &mut Value) -> Result<&mut Vec<Value>, String> {
    if config.get("customCombos").is_none() { config["customCombos"] = json!([]); }
    config["customCombos"].as_array_mut().ok_or_else(|| "Danh sách combo trong cấu hình không hợp lệ.".into())
}

fn issue(issue: &BindingIssue) -> String {
    match issue {
        BindingIssue::HeldInputHotkey => "Phím này trùng với thao tác giữ của combo.",
        BindingIssue::SelfTrigger => "Combo chứa thao tác bấm lại chính phím tắt.",
        BindingIssue::InvalidHotkey => "Phím tắt không hợp lệ.",
        BindingIssue::InvalidCustomProgram(_) => "Các bước của combo không hợp lệ.",
        _ => "Combo này chưa thể gán với phím đã chọn.",
    }.into()
}

pub fn hotkey_name(target: Target) -> String {
    match target {
        Target::Key(key) => key,
        Target::MouseLeft => "left".into(), Target::MouseRight => "right".into(),
        Target::MouseMiddle => "middle".into(), Target::MouseX1 => "mouse_3".into(), Target::MouseX2 => "mouse_4".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_legacy_config_without_touching_unrelated_data() -> Result<(), Box<dyn std::error::Error>> {
        let file = std::env::temp_dir().join(format!("cryss-desktop-config-{}.json", std::process::id()));
        struct RemoveOnDrop(std::path::PathBuf);
        impl Drop for RemoveOnDrop {
            fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
        }
        let _guard = RemoveOnDrop(file.clone());
        let store = ConfigStore::new(&file);
        store.save(&json!({"unknown":{"caption":"Tiếng Việt"}, "FPS": 60}))?;
        let mut settings = Settings::load(&file)?;
        settings.set_fps(100)?;
        settings.set_binding(BUILTIN_NAMES[0], "f6")?;
        assert!(settings.bindings.active.contains_key(&Target::Key("f6".into())));
        settings.set_binding(BUILTIN_NAMES[0], "")?;
        assert!(!settings.bindings.active.contains_key(&Target::Key("f6".into())));
        assert!(settings.set_binding(BUILTIN_NAMES[3], "left").is_err());
        let config = store.load()?;
        assert_eq!(config["FPS"], 100);
        assert_eq!(config["unknown"]["caption"], "Tiếng Việt");
        assert!(config["comboSignKeys"].as_object().unwrap().is_empty());
        Ok(())
    }
}
