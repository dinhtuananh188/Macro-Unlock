#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

#[cfg(windows)]
use cryss_native::{bindings, clips, config, engine, presets, timing, unlocker, worker};

#[cfg(windows)]
mod desktop;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    match desktop::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            desktop::show_error(&error.to_string());
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Ứng dụng desktop Cryss cần Windows.");
    std::process::exit(1);
}
