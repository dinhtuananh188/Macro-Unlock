#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_modifier_bindings_accept_their_physical_variants() {
        assert_eq!(named_key("shift_l"), Some(VK_LSHIFT));
        assert_eq!(named_key("cmd"), named_key("cmd_l"));
        assert_eq!(named_key("alt_gr"), Some(VK_RMENU));
        assert_eq!(
            binding_key_variants("ctrl", VK_LCONTROL),
            &[VK_LCONTROL, VK_RCONTROL]
        );
        assert_eq!(binding_key_variants("ctrl_l", VK_LCONTROL), &[VK_LCONTROL]);
    }

    #[test]
    fn keeps_extended_and_raw_scan_codes_distinct() {
        assert_eq!(
            explicit_key("sc:29"),
            Some(KeyStroke::Scan {
                code: 29,
                extended: false
            })
        );
        assert_eq!(
            explicit_key("sce:29"),
            Some(KeyStroke::Scan {
                code: 29,
                extended: true
            })
        );
        assert_eq!(explicit_key("sc:57373"), explicit_key("sce:29"));
        assert_eq!(
            explicit_key("ext:163"),
            Some(KeyStroke::Virtual {
                key: VK_RCONTROL,
                extended: Some(true)
            })
        );
        assert_eq!(
            explicit_key("vkc:84"),
            Some(KeyStroke::Virtual {
                key: b'T' as u16,
                extended: None
            })
        );
        assert_eq!(named_key("left"), Some(VK_LEFT));
        assert_eq!(capture_named_key(VK_LEFT), None);
    }

    #[test]
    fn matches_side_button_input_on_down_and_up() {
        assert_eq!(
            mouse_button(WM_XBUTTONDOWN, XBUTTON1),
            Some((Target::MouseX1, true))
        );
        assert_eq!(
            mouse_button(WM_XBUTTONUP, XBUTTON2),
            Some((Target::MouseX2, false))
        );
        assert_eq!(mouse_button(WM_XBUTTONUP, 0), None);
        assert_eq!(mouse_button(WM_MOUSEMOVE, 0), None);
    }

    #[test]
    fn recognizes_both_game_image_names_case_insensitively() {
        for path in [r"D:\Games\GenshinImpact.exe", r"D:\Games\yuanShen.EXE"] {
            assert!(is_game_image(&path.encode_utf16().collect::<Vec<_>>()));
        }
        for path in [
            r"D:\GenshinImpact.exe\launcher.exe",
            "GenshinImpact.exe.bak",
        ] {
            assert!(!is_game_image(&path.encode_utf16().collect::<Vec<_>>()));
        }
    }
}
use crate::engine::Output;
use crate::presets::{Action, Target};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, LPARAM, LRESULT, SetLastError, WAIT_FAILED, WAIT_OBJECT_0, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcessId, INFINITE, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW, SetEvent,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const NAMED_KEYS: &[(u16, &[&str])] = &[
    (VK_BACK, &["backspace"]),
    (VK_TAB, &["tab"]),
    (VK_RETURN, &["enter", "num_enter"]),
    (VK_PAUSE, &["pause"]),
    (VK_CAPITAL, &["caps_lock", "caps_lock_on", "caps_lock_off"]),
    (VK_ESCAPE, &["esc"]),
    (VK_SPACE, &["space"]),
    (VK_PRIOR, &["page_up"]),
    (VK_NEXT, &["page_down"]),
    (VK_END, &["end"]),
    (VK_HOME, &["home"]),
    (VK_LEFT, &["left"]),
    (VK_UP, &["up"]),
    (VK_RIGHT, &["right"]),
    (VK_DOWN, &["down"]),
    (VK_SNAPSHOT, &["print_screen"]),
    (VK_INSERT, &["insert"]),
    (VK_DELETE, &["delete"]),
    (VK_LWIN, &["cmd", "cmd_l"]),
    (VK_RWIN, &["cmd_r"]),
    (VK_APPS, &["menu"]),
    (VK_NUMLOCK, &["num_lock", "num_lock_on", "num_lock_off"]),
    (
        VK_SCROLL,
        &["scroll_lock", "scroll_lock_on", "scroll_lock_off"],
    ),
    (VK_LSHIFT, &["shift", "shift_l"]),
    (VK_RSHIFT, &["shift_r"]),
    (VK_LCONTROL, &["ctrl_l", "ctrl"]),
    (VK_RCONTROL, &["ctrl_r"]),
    (VK_LMENU, &["alt_l", "alt"]),
    (VK_RMENU, &["alt_r", "alt_gr"]),
    (VK_VOLUME_MUTE, &["media_volume_mute"]),
    (VK_VOLUME_DOWN, &["media_volume_down"]),
    (VK_VOLUME_UP, &["media_volume_up"]),
    (VK_MEDIA_NEXT_TRACK, &["media_next"]),
    (VK_MEDIA_PREV_TRACK, &["media_previous"]),
    (VK_MEDIA_STOP, &["media_stop"]),
    (VK_MEDIA_PLAY_PAUSE, &["media_play_pause"]),
    (VK_NUMPAD0, &["num_0"]),
    (VK_NUMPAD1, &["num_1"]),
    (VK_NUMPAD2, &["num_2"]),
    (VK_NUMPAD3, &["num_3"]),
    (VK_NUMPAD4, &["num_4"]),
    (VK_NUMPAD5, &["num_5"]),
    (VK_NUMPAD6, &["num_6"]),
    (VK_NUMPAD7, &["num_7"]),
    (VK_NUMPAD8, &["num_8"]),
    (VK_NUMPAD9, &["num_9"]),
    (VK_MULTIPLY, &["num_mul"]),
    (VK_ADD, &["num_add"]),
    (VK_SUBTRACT, &["num_sub"]),
    (VK_DECIMAL, &["num_dec"]),
    (VK_DIVIDE, &["num_div"]),
    (VK_CANCEL, &["break", "ctrl_break"]),
    (VK_BROWSER_BACK, &["browser_back"]),
    (VK_BROWSER_FORWARD, &["browser_forward"]),
    (VK_BROWSER_REFRESH, &["browser_refresh"]),
    (VK_BROWSER_STOP, &["browser_stop"]),
    (VK_BROWSER_SEARCH, &["browser_search"]),
    (VK_BROWSER_FAVORITES, &["browser_favorites"]),
    (VK_BROWSER_HOME, &["browser_home"]),
];

fn named_key(name: &str) -> Option<u16> {
    if let Some(number) = name
        .strip_prefix('f')
        .and_then(|number| number.parse::<u16>().ok())
    {
        return (1..=24).contains(&number).then(|| VK_F1 + number - 1);
    }
    NAMED_KEYS
        .iter()
        .find_map(|(key, names)| names.contains(&name).then_some(*key))
}

fn capture_named_key(key: u16) -> Option<Target> {
    if (VK_F1..=VK_F24).contains(&key) {
        return Some(Target::Key(format!("f{}", key - VK_F1 + 1)));
    }
    // The stored binding namespace uses `left` and `right` for mouse buttons.
    let (_, names) = NAMED_KEYS.iter().find(|(code, _)| *code == key)?;
    match crate::bindings::parse_hotkey(names[0]) {
        Some(target @ Target::Key(_)) => Some(target),
        _ => None,
    }
}

fn binding_key_variants(name: &str, key: u16) -> Vec<u16> {
    match name {
        "ctrl" => vec![VK_LCONTROL, VK_RCONTROL],
        "alt" => vec![VK_LMENU, VK_RMENU],
        _ => vec![key],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyStroke {
    Virtual { key: u16, extended: Option<bool> },
    Scan { code: u16, extended: bool },
    Unicode(u16),
}

fn explicit_key(name: &str) -> Option<KeyStroke> {
    let (encoding, number) = name.split_once(':')?;
    let code = number.parse::<u16>().ok()?;
    match encoding {
        "vkc" | "ext" if (1..=0xfe).contains(&code) => Some(KeyStroke::Virtual {
            key: code,
            extended: (encoding == "ext").then_some(true),
        }),
        "sc" | "sce" if code & 0xff != 0 && matches!(code >> 8, 0 | 0xe0) => {
            Some(KeyStroke::Scan {
                code: code & 0xff,
                extended: encoding == "sce" || code >> 8 == 0xe0,
            })
        }
        _ => None,
    }
}

fn single_character(name: &str) -> Option<u16> {
    let mut characters = name.chars();
    let value = u16::try_from(u32::from(characters.next()?)).ok()?;
    characters.next().is_none().then_some(value)
}

fn character_key(value: u16, layout: HKL) -> Option<(u16, u8)> {
    let mapped = unsafe { VkKeyScanExW(value, layout) };
    (mapped != -1).then_some((mapped as u16 & 0xff, (mapped as u16 >> 8) as u8))
}

fn output_key(name: &str, layout: HKL) -> Option<KeyStroke> {
    if let Some(stroke) = explicit_key(name) {
        return Some(stroke);
    }
    if let Some(key) = named_key(name) {
        return Some(KeyStroke::Virtual {
            key,
            extended: None,
        });
    }
    let value = single_character(name)?;
    Some(match character_key(value, layout) {
        Some((key, 0)) => KeyStroke::Virtual {
            key,
            extended: None,
        },
        _ => KeyStroke::Unicode(value),
    })
}

fn mouse_button(message: u32, side: u16) -> Option<(Target, bool)> {
    Some(match message {
        WM_LBUTTONDOWN | WM_LBUTTONUP => (Target::MouseLeft, message == WM_LBUTTONDOWN),
        WM_RBUTTONDOWN | WM_RBUTTONUP => (Target::MouseRight, message == WM_RBUTTONDOWN),
        WM_MBUTTONDOWN | WM_MBUTTONUP => (Target::MouseMiddle, message == WM_MBUTTONDOWN),
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let target = match side {
                XBUTTON1 => Target::MouseX1,
                XBUTTON2 => Target::MouseX2,
                _ => return None,
            };
            (target, message == WM_XBUTTONDOWN)
        }
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForegroundWindow {
    window: usize,
    process: u32,
}

impl ForegroundWindow {
    fn from_window(window: HWND) -> Self {
        let mut process = 0;
        if !window.is_null() {
            unsafe { GetWindowThreadProcessId(window, &mut process) };
        }
        Self {
            window: window as usize,
            process,
        }
    }

    fn current() -> Self {
        Self::from_window(unsafe { GetForegroundWindow() })
    }

    fn layout(self) -> HKL {
        let thread = unsafe { GetWindowThreadProcessId(self.window as HWND, null_mut()) };
        unsafe { GetKeyboardLayout(thread) }
    }

    fn is_game(self) -> bool {
        if self.process == 0 {
            return false;
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, self.process) };
        if process.is_null() {
            return false;
        }
        let process = OwnedHandle(process);
        let mut image = [0u16; 32768];
        let mut length = image.len() as u32;
        let queried =
            unsafe { QueryFullProcessImageNameW(process.0, 0, image.as_mut_ptr(), &mut length) };
        queried != 0 && is_game_image(&image[..length as usize])
    }
}

fn is_game_image(path: &[u16]) -> bool {
    let filename = path
        .rsplit(|&unit| unit == u16::from(b'\\') || unit == u16::from(b'/'))
        .next()
        .unwrap_or_default();
    ["GenshinImpact.exe", "YuanShen.exe"].iter().any(|name| {
        name.len() == filename.len()
            && name.bytes().zip(filename).all(|(expected, &actual)| {
                u8::try_from(actual).is_ok_and(|actual| actual.eq_ignore_ascii_case(&expected))
            })
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusGuard(ForegroundWindow);

impl FocusGuard {
    pub fn game() -> Option<Self> {
        let window = ForegroundWindow::current();
        (window.is_game() && window == ForegroundWindow::current()).then_some(Self(window))
    }

    pub fn is_current(&self) -> bool {
        self.matches(ForegroundWindow::current())
    }

    pub fn matches(&self, foreground: ForegroundWindow) -> bool {
        self.0 == foreground
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Press(Target),
    Release(Target),
    Capture(Target),
    ForegroundChanged(ForegroundWindow),
}

#[derive(Default)]
struct Settings {
    bindings: Vec<Target>,
    generation: u64,
    capture: bool,
}

pub struct InputMonitor {
    settings: Arc<Mutex<Settings>>,
    stop: OwnedHandle,
    thread: Option<JoinHandle<()>>,
}

impl InputMonitor {
    pub fn start(
        wake: impl Fn() + Send + 'static,
    ) -> io::Result<(Self, mpsc::Receiver<InputEvent>)> {
        let settings = Arc::new(Mutex::new(Settings::default()));
        let hook_settings = Arc::clone(&settings);
        let stop = unsafe { CreateEventW(null(), 1, 0, null()) };
        if stop.is_null() {
            return Err(io::Error::last_os_error());
        }
        let stop = OwnedHandle(stop);
        // The monitor owns the handle and joins the hook thread before closing it.
        let stop_address = stop.0 as usize;
        let (sender, events) = mpsc::channel();
        let (ready, started) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("cryss-input".into())
            .spawn(move || {
                run_monitor(hook_settings, sender, stop_address as HANDLE, ready, wake);
            })?;
        let monitor = Self {
            settings,
            stop,
            thread: Some(thread),
        };
        started.recv().map_err(io::Error::other)??;
        Ok((monitor, events))
    }

    pub fn set_bindings(&self, bindings: impl IntoIterator<Item = Target>) {
        let bindings = bindings.into_iter().collect::<Vec<_>>();
        let mut settings = self
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if settings.bindings != bindings {
            settings.bindings = bindings;
            settings.generation = settings.generation.wrapping_add(1);
        }
    }

    pub fn set_capture(&self, capture: bool) {
        self.settings
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .capture = capture;
    }

    pub fn is_running(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
    }
}

impl Drop for InputMonitor {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            unsafe { SetEvent(self.stop.0) };
            let _ = thread.join();
        }
    }
}

struct InputHook(HHOOK);

impl Drop for InputHook {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.0) };
    }
}

struct ForegroundHook(HWINEVENTHOOK);

impl Drop for ForegroundHook {
    fn drop(&mut self) {
        unsafe { UnhookWinEvent(self.0) };
    }
}

fn install_hooks() -> io::Result<(InputHook, InputHook, ForegroundHook)> {
    let module = unsafe { GetModuleHandleW(null()) };
    if module.is_null() {
        return Err(io::Error::last_os_error());
    }
    let keyboard = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), module, 0) };
    if keyboard.is_null() {
        return Err(io::Error::last_os_error());
    }
    let keyboard = InputHook(keyboard);
    let mouse = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), module, 0) };
    if mouse.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mouse = InputHook(mouse);
    let foreground = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            null_mut(),
            Some(foreground_hook),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if foreground.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok((keyboard, mouse, ForegroundHook(foreground)))
}

fn run_monitor(
    settings: Arc<Mutex<Settings>>,
    events: mpsc::Sender<InputEvent>,
    stop: HANDLE,
    ready: mpsc::SyncSender<io::Result<()>>,
    wake: impl Fn(),
) {
    let mut message = MSG::default();
    unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_NOREMOVE) };
    let foreground = ForegroundWindow::current();
    HOOK_STATE.with(|state| {
        *state.borrow_mut() = Some(HookState {
            foreground,
            game_focused: foreground.is_game(),
            own_process: unsafe { GetCurrentProcessId() },
            settings,
            events,
            pending_wake: false,
            compiled: BindingMap::default(),
            generation: None,
            layout: null_mut(),
            pressed: HashMap::new(),
        });
    });
    let _hooks = match install_hooks() {
        Ok(hooks) => hooks,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    with_hook_state(|state| state.foreground_changed(ForegroundWindow::current()));
    if ready.send(Ok(())).is_err() {
        return;
    }
    loop {
        let result = unsafe {
            MsgWaitForMultipleObjectsEx(1, &stop, INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if matches!(result, WAIT_OBJECT_0 | WAIT_FAILED) {
            return;
        }
        // PeekMessage dispatches the sent hook callbacks even when it returns zero.
        while unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if message.message == WM_QUIT {
                return;
            }
            unsafe { DispatchMessageW(&message) };
        }
        let mut notify = false;
        with_hook_state(|state| notify = std::mem::take(&mut state.pending_wake));
        if notify {
            wake();
        }
    }
}

#[derive(Default)]
struct BindingMap {
    keyboard: HashMap<(u16, Option<u8>), Target>,
    mouse: HashSet<Target>,
}

impl BindingMap {
    fn new(bindings: &[Target], layout: HKL) -> Self {
        let mut map = Self::default();
        for target in bindings {
            let Target::Key(name) = target else {
                map.mouse.insert(target.clone());
                continue;
            };
            if let Some(key) = named_key(name) {
                for variant in binding_key_variants(name, key) {
                    let slot = map.keyboard.entry((variant, None));
                    if matches!(name.as_str(), "ctrl" | "alt") {
                        slot.or_insert_with(|| target.clone());
                    } else {
                        slot.insert_entry(target.clone());
                    }
                }
            } else if let Some((key, modifiers)) =
                single_character(name).and_then(|value| character_key(value, layout))
            {
                let exact = (!name.bytes().all(|byte| byte.is_ascii_alphanumeric())
                    || modifiers != 0)
                    .then_some(modifiers);
                map.keyboard.insert((key, exact), target.clone());
            }
        }
        map
    }

    fn get(&self, input: &PhysicalInput) -> Option<Target> {
        match input {
            PhysicalInput::Key { key, .. } => self
                .keyboard
                .get(&(*key, Some(current_modifiers())))
                .or_else(|| self.keyboard.get(&(*key, None)))
                .cloned(),
            PhysicalInput::Mouse(button) => self.mouse.get(button).cloned(),
        }
    }
}

fn current_modifiers() -> u8 {
    [(VK_SHIFT, 1), (VK_CONTROL, 2), (VK_MENU, 4)]
        .into_iter()
        .fold(0, |active, (key, mask)| {
            if unsafe { GetAsyncKeyState(i32::from(key)) } < 0 {
                active | mask
            } else {
                active
            }
        })
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum PhysicalInput {
    Key { key: u16, extended: bool },
    Mouse(Target),
}

impl PhysicalInput {
    fn capture_target(&self, layout: HKL) -> Option<Target> {
        match self {
            Self::Key { key, .. } => capture_named_key(*key).or_else(|| {
                let mapped = unsafe { MapVirtualKeyExW(u32::from(*key), MAPVK_VK_TO_CHAR, layout) };
                let character = char::from_u32(mapped)?;
                (!character.is_control()).then(|| Target::Key(character.to_lowercase().collect()))
            }),
            Self::Mouse(button) => Some(button.clone()),
        }
    }
}

struct HookState {
    foreground: ForegroundWindow,
    game_focused: bool,
    own_process: u32,
    settings: Arc<Mutex<Settings>>,
    events: mpsc::Sender<InputEvent>,
    pending_wake: bool,
    compiled: BindingMap,
    generation: Option<u64>,
    layout: HKL,
    pressed: HashMap<PhysicalInput, Target>,
}

impl HookState {
    fn send(&mut self, event: InputEvent) {
        self.pending_wake |= self.events.send(event).is_ok();
    }

    fn foreground_changed(&mut self, foreground: ForegroundWindow) {
        if self.foreground == foreground {
            return;
        }
        self.foreground = foreground;
        self.game_focused = foreground.is_game();
        self.pressed.clear();
        self.send(InputEvent::ForegroundChanged(foreground));
    }

    fn physical_input(&mut self, input: PhysicalInput, down: bool) {
        if !(self.game_focused || self.foreground.process == self.own_process)
            || unsafe { GetForegroundWindow() } as usize != self.foreground.window
        {
            return;
        }
        if !down {
            if let Some(target) = self.pressed.remove(&input)
                && !self.pressed.values().any(|other| *other == target)
            {
                self.send(InputEvent::Release(target));
            }
            return;
        }
        if self.pressed.contains_key(&input) {
            return;
        }
        let target = {
            let mut settings = self
                .settings
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if self.foreground.process == self.own_process && settings.capture {
                let event = input.capture_target(self.foreground.layout());
                if event.is_some() {
                    settings.capture = false;
                }
                drop(settings);
                if let Some(target) = event {
                    self.send(InputEvent::Capture(target));
                }
                return;
            }
            if !self.game_focused || settings.bindings.is_empty() {
                return;
            }
            let layout = self.foreground.layout();
            if self.generation != Some(settings.generation) || self.layout != layout {
                self.compiled = BindingMap::new(&settings.bindings, layout);
                self.generation = Some(settings.generation);
                self.layout = layout;
            }
            self.compiled.get(&input)
        };
        if let Some(target) = target {
            let already_pressed = self.pressed.values().any(|other| *other == target);
            self.pressed.insert(input, target.clone());
            if !already_pressed {
                self.send(InputEvent::Press(target));
            }
        }
    }
}

thread_local! {
    static HOOK_STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

fn with_hook_state(callback: impl FnOnce(&mut HookState)) {
    let _ = HOOK_STATE.try_with(|state| {
        if let Ok(mut state) = state.try_borrow_mut()
            && let Some(state) = state.as_mut()
        {
            callback(state);
        }
    });
}

unsafe extern "system" fn keyboard_hook(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && data != 0 {
        let event = unsafe { &*(data as *const KBDLLHOOKSTRUCT) };
        let down = match message as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
            WM_KEYUP | WM_SYSKEYUP => Some(false),
            _ => None,
        };
        if let Some(down) = down
            && event.flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) == 0
            && event.vkCode <= 0xff
        {
            let input = PhysicalInput::Key {
                key: event.vkCode as u16,
                extended: event.flags & LLKHF_EXTENDED != 0,
            };
            with_hook_state(|state| state.physical_input(input, down));
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, message, data) }
}

unsafe extern "system" fn mouse_hook(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && data != 0 {
        let event = unsafe { &*(data as *const MSLLHOOKSTRUCT) };
        if event.flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) == 0
            && let Some((button, down)) =
                mouse_button(message as u32, (event.mouseData >> 16) as u16)
        {
            with_hook_state(|state| state.physical_input(PhysicalInput::Mouse(button), down));
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, message, data) }
}

unsafe extern "system" fn foreground_hook(
    _hook: HWINEVENTHOOK,
    _event: u32,
    window: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let foreground = ForegroundWindow::from_window(window);
    with_hook_state(|state| state.foreground_changed(foreground));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputStroke {
    Keyboard { key: u16, scan: u16, flags: u32 },
    Mouse { down: u32, up: u32, data: u32 },
}

impl InputStroke {
    fn new(target: &Target, layout: HKL) -> io::Result<Self> {
        let (down, up, data) = match target {
            Target::MouseLeft => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
            Target::MouseRight => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
            Target::MouseMiddle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
            Target::MouseX1 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, u32::from(XBUTTON1)),
            Target::MouseX2 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, u32::from(XBUTTON2)),
            Target::Key(name) => {
                let stroke = output_key(name, layout).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unsupported keyboard input: {name}"),
                    )
                })?;
                return Ok(Self::keyboard(stroke, layout));
            }
        };
        Ok(Self::Mouse { down, up, data })
    }

    fn keyboard(stroke: KeyStroke, layout: HKL) -> Self {
        let (key, scan, flags) = match stroke {
            KeyStroke::Virtual { key, extended } => {
                let scan =
                    unsafe { MapVirtualKeyExW(u32::from(key), MAPVK_VK_TO_VSC_EX, layout) } as u16;
                let flags = if extended.unwrap_or(scan >> 8 == 0xe0) {
                    KEYEVENTF_EXTENDEDKEY
                } else {
                    0
                };
                (key, scan & 0xff, flags)
            }
            KeyStroke::Scan { code, extended } => {
                let flags = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
                (0, code, flags | KEYEVENTF_SCANCODE)
            }
            KeyStroke::Unicode(value) => (0, value, KEYEVENTF_UNICODE),
        };
        Self::Keyboard { key, scan, flags }
    }

    fn send(self, pressed: bool) -> io::Result<()> {
        let input = match self {
            Self::Keyboard { key, scan, flags } => INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: key,
                        wScan: scan,
                        dwFlags: flags | if pressed { 0 } else { KEYEVENTF_KEYUP },
                        ..KEYBDINPUT::default()
                    },
                },
            },
            Self::Mouse { down, up, data } => INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        mouseData: data,
                        dwFlags: if pressed { down } else { up },
                        ..MOUSEINPUT::default()
                    },
                },
            },
        };
        unsafe { SetLastError(0) };
        if unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) } == 1 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(0) {
            return Err(io::Error::other("Windows did not accept the input event"));
        }
        Err(error)
    }
}

pub struct DesktopOutput {
    focus: FocusGuard,
    held: Vec<(Target, InputStroke)>,
}

impl DesktopOutput {
    pub fn new(focus: FocusGuard) -> Self {
        Self {
            focus,
            held: Vec::new(),
        }
    }
}

impl Output for DesktopOutput {
    fn emit(&mut self, action: &Action) -> io::Result<()> {
        let (target, pressed) = match action {
            Action::Down(target) => (target, true),
            Action::Up(target) => (target, false),
        };
        let previous = self
            .held
            .iter()
            .find(|(held, _)| held == target)
            .map(|(_, stroke)| *stroke);
        let stroke = match previous {
            // Cleanup uses exactly what this adapter previously pressed, even after focus moves.
            Some(stroke) if !pressed => stroke,
            _ => {
                if !self.focus.is_current() {
                    return if pressed {
                        Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "game window lost foreground focus",
                        ))
                    } else {
                        Ok(())
                    };
                }
                match previous {
                    Some(stroke) => stroke,
                    None => InputStroke::new(target, self.focus.0.layout())?,
                }
            }
        };
        if pressed && !self.focus.is_current() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "game window lost foreground focus",
            ));
        }
        stroke.send(pressed)?;
        if pressed {
            if previous.is_none() {
                self.held.push((target.clone(), stroke));
            }
        } else {
            self.held.retain(|(_, held)| *held != stroke);
        }
        Ok(())
    }
}

impl Drop for DesktopOutput {
    fn drop(&mut self) {
        for (_, stroke) in self.held.drain(..).rev() {
            let _ = stroke.send(false);
        }
    }
}
