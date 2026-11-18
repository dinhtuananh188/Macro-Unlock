use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const CRYSS: &str = env!("CARGO_BIN_EXE_cryss-native");

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("cryss-cli-fixture-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0.join("config.json"));
        let _ = fs::remove_dir(&self.0);
    }
}

#[test]
fn checks_legacy_config_and_runs_an_isolated_timed_profile() {
    let fixture = Fixture::new();
    let path = fixture.0.join("config.json");
    let original = json!({"FPS":120,"future":{"banner":"Việt Nam"},"customCombos":[{
        "id":"fixture", "name":"Combo kiểm thử", "hotkey":"f8", "timeline":[],
        "pythonSequence":[{"type":"key_tap","key":"q","hold":4},{"type":"mouse_click","button":"right","hold":4}]
    }]}).to_string();
    fs::write(&path, &original).unwrap();
    let check = Command::new(CRYSS)
        .arg("check-config")
        .arg(&path)
        .output()
        .unwrap();
    assert!(check.status.success(), "{:?}", check.stderr);
    assert_eq!(
        serde_json::from_slice::<Value>(&check.stdout).unwrap(),
        json!({"active_bindings":1,"issues":[]})
    );

    let trace = Command::new(CRYSS)
        .arg("trace")
        .arg(&path)
        .args(["fixture", "1000"])
        .output()
        .unwrap();
    assert!(trace.status.success(), "{:?}", trace.stderr);
    let events: Vec<Value> = String::from_utf8(trace.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.len(), 5);
    let actions: Vec<_> = events[..4]
        .iter()
        .map(|event| event["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        [
            r#"Down(Key("q"))"#,
            r#"Up(Key("q"))"#,
            "Down(MouseRight)",
            "Up(MouseRight)"
        ]
    );
    assert_eq!(events[4], json!({"outcome":"Finished"}));
    for pair in events[..4].chunks_exact(2) {
        let down = pair[0]["elapsed_ms"].as_f64().unwrap();
        let up = pair[1]["elapsed_ms"].as_f64().unwrap();
        assert!(up - down >= 4.0, "tap expired before its hold deadline");
    }

    let timeout = Command::new(CRYSS)
        .arg("trace")
        .arg(&path)
        .args(["skk3aw", "2"])
        .output()
        .unwrap();
    assert!(timeout.status.success());
    let stdout = String::from_utf8(timeout.stdout).unwrap();
    assert!(stdout.contains(r#""outcome":"Interrupted""#));
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}
