use cryss_native::bindings::{self, BindingIssue};
use cryss_native::config::ConfigStore;
use cryss_native::engine::Output;
use cryss_native::presets::{self, Action, Instruction};
use cryss_native::program;
use cryss_native::timing::{Interrupt, Signals, Timer};
use cryss_native::worker::Worker;
use serde_json::{Value, json};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn checked_config(store: &ConfigStore) -> io::Result<(Value, bool)> {
    let config = store.load()?;
    let timings = store.load_mavuika()?;
    let fps = fps(&config);
    let bindings = bindings::compile(&config, fps, &timings);
    let mut invalid = false;
    let mut issues = Vec::new();
    for (field, valid) in [
        ("comboSignKeys", Value::is_object as fn(&Value) -> bool),
        ("customCombos", Value::is_array),
    ] {
        if config.get(field).is_some_and(|value| !valid(value)) {
            invalid = true;
            issues.push(json!({"name": field, "issue": "invalid field type"}));
        }
    }
    for diagnostic in bindings.diagnostics {
        invalid |= !matches!(diagnostic.issue, BindingIssue::Overridden { .. });
        issues.push(json!({"name": diagnostic.name, "hotkey": diagnostic.hotkey,
            "issue": format!("{:?}", diagnostic.issue)}));
    }
    for profile in config
        .get("customCombos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if profile
            .get("pythonSequence")
            .and_then(Value::as_array)
            .is_some_and(|sequence| !sequence.is_empty())
            && profile
                .get("hotkey")
                .and_then(Value::as_str)
                .is_some_and(|key| !key.is_empty())
        {
            continue;
        }
        let name = profile
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("(unnamed)");
        let issue = match program::compile(profile, fps, &timings) {
            Ok(compiled) if compiled.skipped_steps != 0 => {
                Some(format!("{} unsupported steps", compiled.skipped_steps))
            }
            Err(error) => Some(error.to_string()),
            _ => None,
        };
        if let Some(issue) = issue {
            invalid = true;
            issues.push(json!({"name": name, "issue": issue}));
        }
    }
    Ok((
        json!({"active_bindings": bindings.active.len(), "issues": issues}),
        invalid,
    ))
}

fn fps(config: &Value) -> f64 {
    config
        .get("FPS")
        .and_then(|fps| fps.as_f64().or_else(|| fps.as_str()?.trim().parse().ok()))
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(120.0)
}

const MAX_TRACE_EVENTS: usize = 100_000;

type TraceLog = Vec<(Duration, Action)>;

struct Trace {
    events: Arc<Mutex<TraceLog>>,
    started: Instant,
}

impl Output for Trace {
    fn emit(&mut self, action: &Action) -> io::Result<()> {
        let elapsed = self.started.elapsed();
        let mut events = self
            .events
            .lock()
            .map_err(|_| io::Error::other("trace lock poisoned"))?;
        if events.len() == MAX_TRACE_EVENTS {
            return Err(io::Error::other("trace event limit exceeded"));
        }
        events.push((elapsed, action.clone()));
        Ok(())
    }
}

fn trace(store: &ConfigStore, name: &str, limit: Duration) -> io::Result<()> {
    let config = store.load()?;
    let timings = store.load_mavuika()?;
    let fps = fps(&config);
    let instructions: Vec<Instruction> = if let Some(profile) = config
        .get("customCombos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|profile| {
            ["id", "name"]
                .iter()
                .any(|key| profile.get(key).and_then(Value::as_str) == Some(name))
        }) {
        program::compile(profile, fps, &timings)?.instructions
    } else {
        presets::plan(name, fps, &timings).ok_or_else(|| invalid("unknown macro profile"))?
    };

    let events = Arc::new(Mutex::new(Vec::new()));
    let output = Trace {
        events: Arc::clone(&events),
        started: Instant::now(),
    };
    let mut worker = Worker::new(output)?;
    let playback = worker.start(instructions.into())?;
    let completion = match playback.wait_timeout(limit)? {
        Some(completion) => completion,
        None => {
            playback.interrupt(Interrupt::Stop)?;
            playback.wait()?
        }
    };
    worker.close()?;
    let events = Arc::try_unwrap(events)
        .map_err(|_| io::Error::other("worker still owns the trace"))?
        .into_inner()
        .map_err(io::Error::other)?;
    let mut stream = BufWriter::new(io::stdout().lock());
    for (elapsed, action) in events {
        let event = json!({"elapsed_ms": milliseconds(elapsed), "action": format!("{action:?}")});
        writeln!(stream, "{event}")?;
    }
    writeln!(stream, "{}", json!({"outcome": format!("{completion:?}")}))?;
    stream.flush()
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    sorted[((sorted.len() as f64 * fraction).ceil() as usize).saturating_sub(1)]
}

fn milliseconds(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1_000.0
}

#[cfg(windows)]
fn process_stats() -> io::Result<(Duration, usize, u32)> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessHandleCount, GetProcessTimes,
    };
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let mut handles = 0;
    let mut memory = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // The pseudo-handle needs no CloseHandle. All output buffers have the documented size.
    unsafe {
        let process = GetCurrentProcess();
        if GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) == 0
            || GetProcessHandleCount(process, &mut handles) == 0
            || K32GetProcessMemoryInfo(process, &mut memory, memory.cb) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    let ticks =
        |part: FILETIME| (u64::from(part.dwHighDateTime) << 32) | u64::from(part.dwLowDateTime);
    let cpu = Duration::from_nanos((ticks(kernel) + ticks(user)).saturating_mul(100));
    Ok((cpu, memory.WorkingSetSize, handles))
}

#[cfg(not(windows))]
fn process_stats() -> io::Result<(Duration, usize, u32)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "process benchmark requires Windows",
    ))
}

fn benchmark() -> io::Result<Value> {
    let original_stats = process_stats()?;
    let mut timer = Timer::new()?;
    let signals = Signals::new()?;
    for _ in 0..24 {
        timer.wait_until(
            Instant::now() + Duration::from_millis(2),
            &signals,
            Interrupt::Stop,
        )?;
    }
    let mut samples = Vec::with_capacity(360);
    let before = process_stats()?;
    let wall = Instant::now();
    for _ in 0..120 {
        for duration in [2, 8, 16] {
            let due = Instant::now() + Duration::from_millis(duration);
            timer.wait_until(due, &signals, Interrupt::Stop)?;
            samples.push(milliseconds(due.elapsed()));
        }
    }
    let wall_ms = milliseconds(wall.elapsed());
    let after = process_stats()?;
    samples.sort_unstable_by(f64::total_cmp);

    let epoch = Instant::now();
    let mut max_absolute_jitter = 0.0f64;
    for index in 1..=5_000 {
        let due = epoch + Duration::from_millis(index);
        timer.wait_until(due, &signals, Interrupt::Stop)?;
        max_absolute_jitter = max_absolute_jitter.max(milliseconds(due.elapsed()));
    }
    let drift = milliseconds((epoch + Duration::from_secs(5)).elapsed());

    let mut cancellation = Vec::with_capacity(32);
    for _ in 0..32 {
        let signals = Signals::new()?;
        let signaled_at = std::thread::scope(|scope| {
            let sender = scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(2));
                let now = Instant::now();
                signals.signal(Interrupt::Stop)?;
                Ok::<_, io::Error>(now)
            });
            let woke = timer.wait_until(
                Instant::now() + Duration::from_secs(2),
                &signals,
                Interrupt::Stop,
            )?;
            let woke_at = Instant::now();
            let sent_at = sender
                .join()
                .map_err(|_| io::Error::other("cancellation task panicked"))??;
            if !woke {
                return Err(io::Error::other("cancel signal did not interrupt wait"));
            }
            Ok(woke_at.saturating_duration_since(sent_at))
        })?;
        cancellation.push(milliseconds(signaled_at));
    }
    cancellation.sort_unstable_by(f64::total_cmp);

    drop(signals);
    drop(timer);
    let final_stats = process_stats()?;
    Ok(json!({
        "samples": samples.len(), "cpu_ms": milliseconds(after.0 - before.0), "wall_ms": wall_ms,
        "p50_ms": percentile(&samples, 0.50), "p95_ms": percentile(&samples, 0.95),
        "p99_ms": percentile(&samples, 0.99), "max_ms": samples.last(),
        "absolute_5000_tick_final_error_ms": drift,
        "absolute_5000_tick_max_error_ms": max_absolute_jitter,
        "cancel_p50_ms": percentile(&cancellation, 0.50), "cancel_p99_ms": percentile(&cancellation, 0.99),
        "working_set_before_bytes": before.1, "working_set_after_bytes": final_stats.1,
        "process_handles_at_start": original_stats.2, "process_handles_after_cleanup": final_stats.2,
    }))
}

fn run() -> io::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let command = args.next().unwrap_or_default();
    match command.to_str() {
        Some("benchmark") if args.next().is_none() => println!("{}", benchmark()?),
        Some("check-config") | Some("trace") => {
            let path = args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| invalid("expected config JSON path"))?;
            if !std::fs::metadata(&path)?.is_file() {
                return Err(invalid("config path is not a file"));
            }
            let store = ConfigStore::new(path);
            if command == "check-config" {
                if args.next().is_some() {
                    return Err(invalid("unexpected argument"));
                }
                let (report, has_invalid_entries) = checked_config(&store)?;
                println!("{report}");
                if has_invalid_entries {
                    return Err(invalid("config has unsupported entries"));
                }
            } else {
                let name = args
                    .next()
                    .and_then(|name| name.into_string().ok())
                    .ok_or_else(|| invalid("expected profile name"))?;
                let limit = args
                    .next()
                    .map(|limit| {
                        limit
                            .to_str()
                            .and_then(|limit| limit.parse::<u64>().ok())
                            .filter(|limit| (1..=120_000).contains(limit))
                            .ok_or_else(|| invalid("limit must be 1–120000 ms"))
                    })
                    .transpose()?
                    .unwrap_or(2_000);
                if args.next().is_some() {
                    return Err(invalid("unexpected argument"));
                }
                trace(&store, &name, Duration::from_millis(limit))?;
            }
        }
        Some("--help" | "-h" | "") => println!(
            "cryss-native\n  benchmark\n  check-config <config.json>\n  trace <config.json> <profile> [limit-ms]"
        ),
        _ => return Err(invalid("unknown command, see --help")),
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
