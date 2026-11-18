//! Interruptible, monotonic deadlines for a Rust execution worker.

use std::io;
use std::time::Instant;

#[derive(Clone, Copy)]
pub enum Interrupt {
    Stop,
    Focus,
    Release,
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::ptr::null;
    use windows_sys::Win32::Foundation::{HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateEventW, CreateWaitableTimerExW, INFINITE,
        SetEvent, SetWaitableTimerEx, TIMER_ALL_ACCESS, WaitForMultipleObjects,
    };

    fn owned(raw: HANDLE) -> io::Result<OwnedHandle> {
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        // The caller created this unnamed handle and transfers ownership once.
        Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
    }

    fn event() -> io::Result<OwnedHandle> {
        // Manual-reset, initially clear, unnamed and non-inheritable.
        owned(unsafe { CreateEventW(null(), 1, 0, null()) })
    }

    pub struct Signals {
        stop: OwnedHandle,
        focus: OwnedHandle,
        release: OwnedHandle,
    }

    impl Signals {
        pub fn new() -> io::Result<Self> {
            Ok(Self {
                stop: event()?,
                focus: event()?,
                release: event()?,
            })
        }

        pub fn signal(&self, interrupt: Interrupt) -> io::Result<()> {
            let handle = match interrupt {
                Interrupt::Stop => &self.stop,
                Interrupt::Focus => &self.focus,
                Interrupt::Release => &self.release,
            };
            // The owned handle remains valid until after the API call.
            if unsafe { SetEvent(handle.as_raw_handle()) } != 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }

        fn handles(&self, interrupt: Interrupt) -> ([HANDLE; 4], usize) {
            let handles = [
                self.stop.as_raw_handle(),
                self.focus.as_raw_handle(),
                self.release.as_raw_handle(),
                std::ptr::null_mut(),
            ];
            let count = match interrupt {
                Interrupt::Stop => 1,
                Interrupt::Focus => 2,
                Interrupt::Release => 3,
            };
            (handles, count)
        }

        pub fn interrupted(&self, interrupt: Interrupt) -> io::Result<bool> {
            let (handles, count) = self.handles(interrupt);
            Ok(wait(&handles[..count], 0)?.is_some())
        }
    }

    fn wait(handles: &[HANDLE], milliseconds: u32) -> io::Result<Option<usize>> {
        // The caller owns each handle for the duration of the wait and the slice is nonempty.
        let result = unsafe {
            WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, milliseconds)
        };
        match result {
            WAIT_FAILED => Err(io::Error::last_os_error()),
            WAIT_TIMEOUT => Ok(None),
            other => {
                let offset = other.wrapping_sub(WAIT_OBJECT_0) as usize;
                if offset < handles.len() {
                    Ok(Some(offset))
                } else {
                    Err(io::Error::other(format!("unexpected wait result {other}")))
                }
            }
        }
    }

    pub struct Timer(OwnedHandle);

    impl Timer {
        pub fn new() -> io::Result<Self> {
            // Request an unnamed one-shot synchronization timer. Older Windows builds
            // may only accept the normal-resolution flag.
            let high = unsafe {
                CreateWaitableTimerExW(
                    null(),
                    null(),
                    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                    TIMER_ALL_ACCESS,
                )
            };
            let raw = if high.is_null() {
                unsafe { CreateWaitableTimerExW(null(), null(), 0, TIMER_ALL_ACCESS) }
            } else {
                high
            };
            owned(raw).map(Self)
        }

        pub fn wait_until(
            &mut self,
            deadline: Instant,
            signals: &Signals,
            interrupt: Interrupt,
        ) -> io::Result<bool> {
            let (mut handles, count) = signals.handles(interrupt);
            handles[count] = self.0.as_raw_handle();
            let available = deadline.saturating_duration_since(Instant::now());
            let spin_window = (available / 2).min(std::time::Duration::from_micros(1_250));
            let spin_start = deadline - spin_window;
            loop {
                let now = Instant::now();
                if now >= deadline {
                    return signals.interrupted(interrupt);
                }
                let remaining = spin_start.saturating_duration_since(now);
                if remaining.is_zero() {
                    while Instant::now() < deadline {
                        if signals.interrupted(interrupt)? {
                            return Ok(true);
                        }
                        for _ in 0..32 {
                            std::hint::spin_loop();
                        }
                    }
                    return signals.interrupted(interrupt);
                }
                // A negative deadline is a relative interval in 100 ns units.
                // Round upward so the kernel never expires before our deadline.
                let ticks = remaining.as_nanos().div_ceil(100).min(i64::MAX as u128) as i64;
                let due = -ticks;
                // All pointers are live for the call. No callback or asynchronous buffer.
                if unsafe {
                    SetWaitableTimerEx(self.0.as_raw_handle(), &due, 0, None, null(), null(), 0)
                } == 0
                {
                    return Err(io::Error::last_os_error());
                }
                if wait(&handles[..=count], INFINITE)?.is_some_and(|index| index < count) {
                    return Ok(true);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    use std::sync::{Condvar, Mutex};

    pub struct Signals {
        value: Mutex<u8>,
        wake: Condvar,
    }

    impl Interrupt {
        fn bit(self) -> u8 {
            match self {
                Self::Stop => 1,
                Self::Focus => 2,
                Self::Release => 4,
            }
        }
        fn mask(self) -> u8 {
            (self.bit() * 2) - 1
        }
    }

    impl Signals {
        pub fn new() -> io::Result<Self> {
            Ok(Self {
                value: Mutex::new(0),
                wake: Condvar::new(),
            })
        }

        pub fn signal(&self, interrupt: Interrupt) -> io::Result<()> {
            *self
                .value
                .lock()
                .map_err(|_| io::Error::other("signal lock poisoned"))? |= interrupt.bit();
            self.wake.notify_all();
            Ok(())
        }

        pub fn interrupted(&self, interrupt: Interrupt) -> io::Result<bool> {
            Ok(*self
                .value
                .lock()
                .map_err(|_| io::Error::other("signal lock poisoned"))?
                & interrupt.mask()
                != 0)
        }
    }

    pub struct Timer;

    impl Timer {
        pub fn new() -> io::Result<Self> {
            Ok(Self)
        }

        pub fn wait_until(
            &mut self,
            deadline: Instant,
            signals: &Signals,
            interrupt: Interrupt,
        ) -> io::Result<bool> {
            let mut value = signals
                .value
                .lock()
                .map_err(|_| io::Error::other("signal lock poisoned"))?;
            while *value & interrupt.mask() == 0 {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Ok(false);
                }
                (value, _) = signals
                    .wake
                    .wait_timeout(value, remaining)
                    .map_err(|_| io::Error::other("signal lock poisoned"))?;
            }
            Ok(true)
        }
    }
}

pub use platform::{Signals, Timer};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn waits_until_deadline_and_honors_interrupt_masks() -> io::Result<()> {
        let signals = Signals::new()?;
        let mut timer = Timer::new()?;
        signals.signal(Interrupt::Release)?;
        assert!(!signals.interrupted(Interrupt::Focus)?);
        assert!(signals.interrupted(Interrupt::Release)?);
        let deadline = Instant::now() + Duration::from_millis(8);
        assert!(!timer.wait_until(deadline, &signals, Interrupt::Stop)?);
        assert!(Instant::now() >= deadline);
        Ok(())
    }

    #[test]
    fn stop_wakes_a_sleeping_worker() -> io::Result<()> {
        let signals = Signals::new()?;
        let mut timer = Timer::new()?;
        let started = Instant::now();
        std::thread::scope(|scope| -> io::Result<()> {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(10));
                signals.signal(Interrupt::Stop).unwrap();
            });
            assert!(timer.wait_until(
                started + Duration::from_secs(2),
                &signals,
                Interrupt::Stop
            )?);
            Ok(())
        })?;
        assert!(started.elapsed() < Duration::from_secs(1));
        Ok(())
    }
}
