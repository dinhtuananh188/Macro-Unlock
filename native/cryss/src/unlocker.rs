//! Cấu hình Unlocker của bản Electron: `Plugins/UnlockerIsland/config.ini` và `gamepath.json`.
//!
//! Chỉ đọc và ghi cấu hình. Không chạy launcher hay DLL.
use serde_json::{Value, json};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_GAME_PATH: &str = "C:\\Program Files\\HoYoPlay\\games\\Genshin Impact game\\GenshinImpact.exe";
const DEFAULT_FILE: &str = "CUTTOOL.UnlockerIsland.dll";

/// Khóa và giá trị mặc định mà trang Tùy chỉnh của bản gốc dùng, theo thứ tự ghi của `unlocker.js`.
const DEFAULTS: [(&str, i64); 17] = [
    ("Vsync", 0), ("FpsUnlock", 0), ("TargetFps", 120), ("FovUnlock", 0), ("FovValue", 60),
    ("HideUID", 0), ("DisableCameraMove", 0), ("DisableFog", 0), ("RemoveTeamAnim", 0),
    ("DisableBurstBlackscreen", 0), ("ShowFPS", 0), ("BlockNetwork", 0), ("EnableNetworkToggle", 0),
    ("NetworkToggleKey", 122), ("ToggleKey", 36), ("DumpOffsets", 0), ("DebugConsole", 0),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnlockerConfig {
    dir: PathBuf,
    file: String,
    /// Giữ nguyên thứ tự và cách viết hoa thường của từng section, kể cả `[VSync]`, `[ShowFps]`.
    sections: Vec<(String, String)>,
    pub game_path: String,
}

impl UnlockerConfig {
    /// `dir` là thư mục `unlocker` (chứa `gamepath.json` và `Plugins/UnlockerIsland`).
    pub fn load(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let mut file = DEFAULT_FILE.to_owned();
        let mut sections: Vec<(String, String)> = Vec::new();
        if let Ok(text) = fs::read_to_string(ini_path(&dir)) {
            let mut current: Option<String> = None;
            for line in text.trim_start_matches('\u{feff}').lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with(';') || line.starts_with('#') { continue; }
                if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
                    current = Some(name.trim().to_owned());
                } else if let Some((key, value)) = line.split_once('=') {
                    let (key, value) = (key.trim(), value.trim());
                    match &current {
                        None if key == "File" => file = value.to_owned(),
                        // GamePath nằm riêng trong gamepath.json.
                        Some(name) if name == "GamePath" => {}
                        Some(name) if key.eq_ignore_ascii_case("value") => match sections.iter_mut().find(|(n, _)| n == name) {
                            Some(slot) => slot.1 = value.to_owned(),
                            None => sections.push((name.clone(), value.to_owned())),
                        },
                        _ => {}
                    }
                }
            }
        }
        let game_path = fs::read_to_string(dir.join("gamepath.json")).ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|json| json["GamePath"].as_str().filter(|p| !p.is_empty()).map(str::to_owned))
            .unwrap_or_else(|| DEFAULT_GAME_PATH.to_owned());
        Self { dir, file, sections, game_path }
    }

    /// Giá trị số của một khóa; thiếu hoặc không phải số thì dùng mặc định của bản gốc.
    pub fn get(&self, key: &str) -> i64 {
        self.sections.iter().find(|(name, _)| name == key).and_then(|(_, v)| v.parse().ok())
            .or_else(|| DEFAULTS.iter().find(|(name, _)| *name == key).map(|(_, v)| *v)).unwrap_or(0)
    }

    pub fn flag(&self, key: &str) -> bool { self.get(key) != 0 }

    pub fn set(&mut self, key: &str, value: i64) {
        let value = value.to_string();
        match self.sections.iter_mut().find(|(name, _)| name == key) {
            Some(slot) => slot.1 = value,
            None => self.sections.push((key.to_owned(), value)),
        }
    }

    pub fn dir(&self) -> &Path { &self.dir }

    pub fn ini_path(&self) -> PathBuf { ini_path(&self.dir) }

    fn render(&self) -> String {
        let mut out = format!("File={}\r\n\r\n", self.file);
        // Khóa chuẩn trước, theo thứ tự của bản gốc; phần còn lại giữ thứ tự đọc được.
        let mut written = vec![false; self.sections.len()];
        let mut push = |name: &str, value: &str| out.push_str(&format!("[{name}]\r\nValue={value}\r\n\r\n"));
        for (key, default) in DEFAULTS {
            if let Some(index) = self.sections.iter().position(|(name, _)| name == key) {
                written[index] = true;
                push(key, &self.sections[index].1);
            } else if matches!(key, "DumpOffsets" | "DebugConsole") {
                continue;
            } else {
                push(key, &default.to_string());
            }
        }
        for (index, (name, value)) in self.sections.iter().enumerate() {
            if !written[index] { push(name, value); }
        }
        out
    }

    pub fn save(&self) -> io::Result<()> {
        let ini = ini_path(&self.dir);
        if let Some(parent) = ini.parent() { fs::create_dir_all(parent)?; }
        write_atomically(&ini, self.render().as_bytes())?;
        let game = serde_json::to_string_pretty(&json!({ "GamePath": self.game_path }))
            .map_err(io::Error::other)?;
        write_atomically(&self.dir.join("gamepath.json"), game.as_bytes())
    }
}

/// Kết quả chuẩn bị chạy game: đường dẫn launcher và file game đã kiểm tra.
#[derive(Debug, PartialEq, Eq)]
pub struct LaunchPlan {
    pub launcher: PathBuf,
    pub game: PathBuf,
    pub working_dir: PathBuf,
}

impl UnlockerConfig {
    /// Kiểm tra file và chuẩn bị thư mục plugin như `launchGame` của bản gốc, chưa chạy gì.
    pub fn prepare_launch(&self) -> Result<LaunchPlan, String> {
        let game = PathBuf::from(&self.game_path);
        if self.game_path.trim().is_empty() || !game.is_file() {
            return Err(format!("Không tìm thấy file game tại: \"{}\". Hãy vào trang Tùy chỉnh và chọn lại đường dẫn GenshinImpact.exe.", self.game_path));
        }
        let launcher = self.dir.join("Launcher_2.exe");
        if !launcher.is_file() {
            return Err(format!("Không tìm thấy Launcher_2.exe tại: \"{}\". Hãy kiểm tra thư mục unlocker.", launcher.display()));
        }
        let plugins = self.dir.join("Plugins").join("UnlockerIsland");
        fs::create_dir_all(&plugins).map_err(|error| format!("Không tạo được thư mục plugin: {error}"))?;
        let source = self.dir.join(&self.file);
        let target = plugins.join(&self.file);
        if source.is_file() {
            let stale = match (fs::metadata(&source), fs::metadata(&target)) {
                (Ok(from), Ok(to)) => from.len() != to.len() || from.modified().ok() > to.modified().ok(),
                (Ok(_), Err(_)) => true,
                _ => false,
            };
            if stale { fs::copy(&source, &target).map_err(|error| format!("Không chép được DLL: {error}"))?; }
        }
        if !ini_path(&self.dir).is_file() { self.save().map_err(|error| format!("Không tạo được config.ini: {error}"))?; }
        Ok(LaunchPlan { launcher, game, working_dir: self.dir.clone() })
    }

    /// Chạy launcher với quyền quản trị (UAC) và đường dẫn game. Không chờ launcher kết thúc.
    pub fn launch_game(&self) -> Result<(), String> {
        let plan = self.prepare_launch()?;
        // Chuỗi đơn trong PowerShell: nhân đôi dấu ' để đường dẫn không thoát được khỏi chuỗi.
        let quote = |path: &Path| format!("'{}'", path.display().to_string().replace('\'', "''"));
        let command = format!(
            "Start-Process -FilePath {} -ArgumentList ('\"' + {} + '\"') -WorkingDirectory {} -Verb RunAs",
            quote(&plan.launcher), quote(&plan.game), quote(&plan.working_dir)
        );
        let mut process = std::process::Command::new("powershell.exe");
        process.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &command]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            process.creation_flags(0x0800_0000);
        }
        process.spawn().map(|_| ()).map_err(|error| format!("Không chạy được PowerShell: {error}"))
    }
}

fn ini_path(dir: &Path) -> PathBuf { dir.join("Plugins").join("UnlockerIsland").join("config.ini") }

fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension(format!("tmp{}", std::process::id()));
    let result = fs::write(&temporary, bytes).and_then(|()| fs::rename(&temporary, path));
    if result.is_err() { let _ = fs::remove_file(&temporary); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cryss-unlocker-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn missing_files_give_the_defaults_of_the_original_app() {
        let dir = scratch("defaults");
        let config = UnlockerConfig::load(&dir);
        assert_eq!(config.get("TargetFps"), 120);
        assert_eq!(config.get("FovValue"), 60);
        assert_eq!(config.get("NetworkToggleKey"), 122);
        assert!(!config.flag("Vsync"));
        assert_eq!(config.game_path, DEFAULT_GAME_PATH);
    }

    #[test]
    fn keeps_case_variant_sections_and_unknown_keys_across_a_save() -> io::Result<()> {
        let dir = scratch("roundtrip");
        let ini = ini_path(&dir);
        fs::create_dir_all(ini.parent().unwrap())?;
        fs::write(&ini, "File=CUTTOOL.UnlockerIsland.dll\r\n\r\n[Vsync]\r\nValue=1\r\n\r\n[VSync]\r\nValue=1\r\n\r\n[ShowFps]\r\nValue=1\r\n\r\n[Mới]\r\nValue=7\r\n")?;
        fs::write(dir.join("gamepath.json"), r#"{"GamePath":"D:\\Genshin Impact game\\GenshinImpact.exe"}"#)?;
        let mut config = UnlockerConfig::load(&dir);
        assert_eq!(config.game_path, "D:\\Genshin Impact game\\GenshinImpact.exe");
        config.set("TargetFps", 144);
        config.save()?;
        let text = fs::read_to_string(&ini)?;
        for expected in ["[Vsync]\r\nValue=1", "[VSync]\r\nValue=1", "[ShowFps]\r\nValue=1", "[Mới]\r\nValue=7", "[TargetFps]\r\nValue=144"] {
            assert!(text.contains(expected), "thiếu {expected:?} trong {text:?}");
        }
        assert!(text.starts_with("File=CUTTOOL.UnlockerIsland.dll\r\n"));
        assert_eq!(UnlockerConfig::load(&dir).get("TargetFps"), 144);
        assert!(fs::read_to_string(dir.join("gamepath.json"))?.contains("Genshin Impact game"));
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn launch_preparation_reports_missing_files_and_copies_the_dll_without_running_anything() -> io::Result<()> {
        let dir = scratch("launch");
        fs::create_dir_all(&dir)?;
        let mut config = UnlockerConfig::load(&dir);
        assert!(config.prepare_launch().unwrap_err().contains("Không tìm thấy file game"));
        let game = dir.join("GenshinImpact.exe");
        fs::write(&game, b"game")?;
        config.game_path = game.display().to_string();
        assert!(config.prepare_launch().unwrap_err().contains("Launcher_2.exe"));
        fs::write(dir.join("Launcher_2.exe"), b"launcher")?;
        fs::write(dir.join(DEFAULT_FILE), b"dll")?;
        let plan = config.prepare_launch().unwrap();
        assert_eq!(plan.game, game);
        assert_eq!(fs::read(dir.join("Plugins").join("UnlockerIsland").join(DEFAULT_FILE))?, b"dll");
        assert!(ini_path(&dir).is_file());
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
