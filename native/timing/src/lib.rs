use std::hint::spin_loop;
use std::thread;
use std::time::{Duration, Instant};

const SPIN_WINDOW: Duration = Duration::from_micros(1_750);
const MAX_SLEEP: Duration = Duration::from_millis(1);

// This versioned C ABI accepts only a number and owns all of its state.
#[unsafe(no_mangle)]
pub extern "C" fn cryss_wait_seconds_v1(seconds: f64) -> u8 {
    let started = Instant::now();
    let Ok(duration) = Duration::try_from_secs_f64(seconds) else {
        return 0;
    };
    let Some(deadline) = started.checked_add(duration) else {
        return 0;
    };

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return 1;
        }
        if remaining > SPIN_WINDOW {
            thread::sleep((remaining - SPIN_WINDOW).min(MAX_SLEEP));
        } else {
            spin_loop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unrepresentable_durations() {
        for seconds in [-1.0, f64::NAN, f64::INFINITY, f64::MAX] {
            assert_eq!(cryss_wait_seconds_v1(seconds), 0);
        }
    }

    #[test]
    fn honors_supported_durations() {
        for duration in [Duration::ZERO, Duration::from_millis(2)] {
            let started = Instant::now();
            assert_eq!(cryss_wait_seconds_v1(duration.as_secs_f64()), 1);
            assert!(started.elapsed() >= duration);
        }
    }
}
