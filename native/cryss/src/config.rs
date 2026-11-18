use serde_json::{Map, Value, json};
use std::fmt::Display;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

const BUILTIN_MAVUIKA: &str = include_str!("../../../src/macro/mavuika.json");
static CONFIG_WRITE_LOCK: Mutex<()> = Mutex::new(());
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> io::Result<Value> {
        match read_object(&self.path)? {
            Some(config) => Ok(Value::Object(config)),
            None => Ok(json!({ "comboSignKeys": {}, "FPS": 120, "customCombos": [] })),
        }
    }

    /// Applies the supplied top-level fields to the latest config.
    pub fn save(&self, config: &Value) -> io::Result<()> {
        let fields = config.as_object().ok_or_else(not_an_object)?;
        self.update(|current| {
            for (key, value) in fields {
                current[key] = value.clone();
            }
        })?;
        Ok(())
    }

    /// Serializes read-modify-write operations across stores in this process.
    pub fn update(&self, edit: impl FnOnce(&mut Value)) -> io::Result<Value> {
        let _guard = CONFIG_WRITE_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut config = self.load()?;
        edit(&mut config);
        if !config.is_object() {
            return Err(not_an_object());
        }
        let mut bytes = serde_json::to_vec_pretty(&config).map_err(io::Error::other)?;
        bytes.push(b'\n');
        replace_atomically(&self.path, &bytes)?;
        Ok(config)
    }

    pub fn load_mavuika(&self) -> io::Result<Value> {
        let path = self.path.with_file_name("mavuika.json");
        let Some(profile) = read_object(&path)? else {
            return parse_object(
                Path::new("bundled mavuika.json"),
                BUILTIN_MAVUIKA.as_bytes(),
            )
            .map(Value::Object);
        };

        let mut combos = profile
            .iter()
            .filter(|(name, _)| !name.starts_with('_'))
            .peekable();
        if combos.peek().is_none() {
            return Err(invalid_data(&path, "expected at least one combo"));
        }
        for (name, combo) in combos {
            if combo
                .get("events")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty)
            {
                return Err(invalid_data(&path, format!("combo {name:?} has no events")));
            }
        }
        Ok(Value::Object(profile))
    }
}

fn not_an_object() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "expected a JSON object")
}

fn invalid_data(path: &Path, message: impl Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {message}", path.display()),
    )
}

fn parse_object(path: &Path, contents: &[u8]) -> io::Result<Map<String, Value>> {
    serde_json::from_slice::<Value>(contents)
        .map_err(|error| invalid_data(path, error))
        .and_then(|value| match value {
            Value::Object(fields) => Ok(fields),
            _ => Err(invalid_data(path, "expected a JSON object")),
        })
}

fn read_object(path: &Path) -> io::Result<Option<Map<String, Value>>> {
    match fs::read(path) {
        Ok(contents) => parse_object(path, &contents).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io::Error::new(
            error.kind(),
            format!("{}: {error}", path.display()),
        )),
    }
}

fn replace_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "config path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;

    let (temporary, mut file) = loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".cryss-config-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };

    let result = file.write_all(contents).and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| fs::rename(&temporary, path));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            loop {
                let id = NEXT.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir()
                    .join(format!("cryss-config-test-{}-{id}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("cannot create test directory: {error}"),
                }
            }
        }

        fn store(&self) -> ConfigStore {
            ConfigStore::new(self.0.join("config.json"))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            for name in ["config.json", "mavuika.json"] {
                let _ = fs::remove_file(self.0.join(name));
            }
            let _ = fs::remove_dir(&self.0);
        }
    }

    #[test]
    fn missing_config_loads_shipped_defaults() {
        let fixture = Fixture::new();
        assert_eq!(
            fixture.store().load().unwrap(),
            json!({"comboSignKeys": {}, "FPS": 120, "customCombos": []})
        );
        assert!(!fixture.0.join("config.json").exists());
    }

    #[test]
    fn save_replaces_bindings_and_keeps_omitted_data_in_utf8() {
        let fixture = Fixture::new();
        let path = fixture.0.join("config.json");
        fs::write(&path, r#"{"comboSignKeys":{"old":"F1"},"tùy chọn":{"nhãn":"Đánh thử","số":10000000000000000000001}}"#).unwrap();

        fixture
            .store()
            .save(&json!({"comboSignKeys": {}, "FPS": 144}))
            .unwrap();

        let raw = fs::read(&path).unwrap();
        assert!(!raw.starts_with(&[0xef, 0xbb, 0xbf]));
        let text = std::str::from_utf8(&raw).unwrap();
        assert!(text.contains("Đánh thử"));
        assert!(text.contains("10000000000000000000001"));
        let loaded = fixture.store().load().unwrap();
        assert_eq!(loaded["comboSignKeys"], json!({}));
        assert_eq!(loaded["FPS"], json!(144));
        assert_eq!(loaded["tùy chọn"]["nhãn"], "Đánh thử");
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }

    #[test]
    fn update_preserves_sibling_fields_while_editing_nested_data() {
        let fixture = Fixture::new();
        fs::write(
            fixture.0.join("config.json"),
            r#"{"customCombos":[{"name":"Tổ hợp","future":{"mode":3},"hotkey":"F2"}]}"#,
        )
        .unwrap();

        let updated = fixture
            .store()
            .update(|config| {
                config["customCombos"][0]["hotkey"] = json!("F4");
            })
            .unwrap();

        let first = &updated["customCombos"][0];
        assert_eq!(first["hotkey"], "F4");
        assert_eq!(first["future"]["mode"], 3);
        assert_eq!(first["name"], "Tổ hợp");
        assert_eq!(fixture.store().load().unwrap(), updated);
    }

    #[test]
    fn concurrent_updates_keep_every_change() {
        let fixture = Fixture::new();
        let path = fixture.0.join("config.json");
        fs::write(&path, r#"{"count":0}"#).unwrap();

        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    ConfigStore::new(&path)
                        .update(|config| {
                            let count = config["count"].as_u64().unwrap();
                            config["count"] = json!(count + 1);
                        })
                        .unwrap();
                });
            }
        });

        assert_eq!(fixture.store().load().unwrap()["count"], 8);
    }

    #[test]
    fn invalid_existing_config_is_never_replaced() {
        let fixture = Fixture::new();
        let path = fixture.0.join("config.json");
        for invalid in [b"{incorrect".as_slice(), b"[1, 2]", b"{\"value\":\"\xff\"}"] {
            fs::write(&path, invalid).unwrap();
            assert_eq!(
                fixture.store().load().unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            assert!(fixture.store().save(&json!({"FPS": 144})).is_err());
            assert!(
                fixture
                    .store()
                    .update(|_| panic!("invalid data must not reach edit"))
                    .is_err()
            );
            assert_eq!(fs::read(&path).unwrap(), invalid);
        }
    }

    #[cfg(windows)]
    #[test]
    fn failed_replacement_leaves_the_file_and_cleans_up_temporary_data() {
        use std::os::windows::fs::OpenOptionsExt;

        const SHARE_READ_WRITE: u32 = 0x00000001 | 0x00000002;
        let fixture = Fixture::new();
        let path = fixture.0.join("config.json");
        let original = br#"{"FPS":120}"#;
        fs::write(&path, original).unwrap();
        let _guard = fs::OpenOptions::new()
            .read(true)
            .share_mode(SHARE_READ_WRITE)
            .open(&path)
            .unwrap();

        assert!(fixture.store().save(&json!({"FPS": 144})).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }

    #[test]
    fn missing_profile_loads_bundled_timing() {
        let fixture = Fixture::new();
        let profile = fixture.store().load_mavuika().unwrap();
        assert_eq!(profile["beat_q"]["events"][1], json!([202, "q_up"]));
        assert_eq!(
            profile["C0:  Combo Mavuika CDCDCF (Full Combo)"]["mode"],
            "loop"
        );
        assert!(!fixture.0.join("mavuika.json").exists());
    }

    #[test]
    fn reloading_profile_reads_local_customizations() {
        let fixture = Fixture::new();
        let path = fixture.0.join("mavuika.json");
        let store = fixture.store();
        fs::write(
            &path,
            r#"{"Nhịp thử":{"mode":"once","events":[[5,"end"]]}}"#,
        )
        .unwrap();
        assert_eq!(
            store.load_mavuika().unwrap()["Nhịp thử"]["events"][0],
            json!([5, "end"])
        );
        fs::write(
            &path,
            r#"{"Nhịp thử":{"mode":"once","events":[[15,"end"]]}}"#,
        )
        .unwrap();
        assert_eq!(
            store.load_mavuika().unwrap()["Nhịp thử"]["events"][0],
            json!([15, "end"])
        );
        assert!(!fixture.0.join("config.json").exists());
    }

    #[test]
    fn invalid_profile_surfaces_an_error_without_changing_the_file() {
        let fixture = Fixture::new();
        let path = fixture.0.join("mavuika.json");
        fs::write(&path, b"{}").unwrap();
        assert_eq!(
            fixture.store().load_mavuika().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(&path).unwrap(), b"{}");
    }
}
