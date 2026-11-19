mod advanced;
mod extras;
mod tray;
mod editor;
mod input;
mod settings;
mod views;

use crate::presets::{Cancellation, Instruction, Target};
use crate::timing::Interrupt;
use crate::worker::{Playback, Worker};
use advanced::{Advanced, AdvancedAction};
use editor::{Editor, EditorAction};
use eframe::egui;
use input::{DesktopOutput, FocusGuard, InputEvent, InputMonitor};
use settings::{Settings, hotkey_name};
use std::collections::{HashMap, HashSet};
use std::io;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Duration;
use crate::unlocker::UnlockerConfig;
use extras::{Announcement, UpdateState};
use views::{Command, Model, Views};
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

const TITLE: &str = "Cryss";

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Mở lần hai thì đưa cửa sổ đang chạy lên trước, như phiên đơn của bản Electron.
    // `CRYSS_ALLOW_MULTI` chỉ dùng khi kiểm thử cạnh một bản đang chạy.
    let _instance = match lock_instance() {
        Ok(handle) => Some(handle),
        Err(_) if std::env::var_os("CRYSS_ALLOW_MULTI").is_some() => None,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if tray::show_window(TITLE) { return Ok(()); }
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    };
    let path = config_path()?;
    let icon = image::load_from_memory(include_bytes!("../../../../src/UI/icon.png"))?.to_rgba8();
    let (width, height) = icon.dimensions();
    let tray_icon = (icon.clone().into_raw(), width, height);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([1280.0, 768.0])
            .with_min_inner_size([840.0, 550.0])
            .with_icon(egui::IconData { rgba: icon.into_raw(), width, height }),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(TITLE, options, Box::new(move |cc| Ok(Box::new(App::new(&cc.egui_ctx, path, tray_icon)))))?;
    Ok(())
}

fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain([0]).collect() }

fn lock_instance() -> io::Result<OwnedHandle> {
    let name = wide("Local\\CUTTOOL.Cryss.Native");
    // Null security attributes use the current session's defaults.
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if mutex.is_null() { return Err(io::Error::last_os_error()); }
    let existing = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    // CreateMutexW transfers one reference. No thread owns the mutex state.
    let mutex = unsafe { OwnedHandle::from_raw_handle(mutex) };
    if existing { return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Cryss đang mở.")); }
    Ok(mutex)
}

pub fn show_error(message: &str) {
    let title = wide(TITLE);
    let message = wide(message);
    // Null parent requests an independent dialog on the current Windows desktop.
    unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR) };
}

fn config_path() -> io::Result<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    match args.next() {
        Some(flag) if flag == "--config" => {
            let path = args.next().map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Thiếu đường dẫn sau --config."))?;
            if args.next().is_some() { return Err(io::Error::new(io::ErrorKind::InvalidInput, "Chỉ nhận --config <đường dẫn JSON>.")); }
            std::path::absolute(path)
        }
        Some(_) => Err(io::Error::new(io::ErrorKind::InvalidInput, "Chỉ nhận --config <đường dẫn JSON>.")),
        None => {
            let exe = std::env::current_exe()?;
            let folder = exe.parent().ok_or_else(|| io::Error::other("Không tìm thấy thư mục ứng dụng."))?;
            let beside_exe = folder.join("config.json");
            if beside_exe.exists() { return Ok(beside_exe); }
            // Development builds live below the repository's ignored build directory.
            if let Some(root) = folder.ancestors().find(|path| path.join(".git").exists()) {
                for candidate in ["build/app/win-unpacked/resources/config.json", "src/macro/config.json"] {
                    let path = root.join(candidate);
                    if path.is_file() { return Ok(path); }
                }
            }
            Ok(beside_exe)
        }
    }
}

struct Active {
    focus: FocusGuard,
    playback: Playback,
    _worker: Worker,
}

struct App {
    views: Views,
    settings: Option<Settings>,
    path: PathBuf,
    status: String,
    enabled: bool,
    monitor: Option<InputMonitor>,
    input: Option<Receiver<InputEvent>>,
    pressed: HashSet<Target>,
    active: HashMap<Target, Active>,
    capture: Option<String>,
    editor: Option<Editor>,
    advanced: Option<Advanced>,
    unlocker: UnlockerConfig,
    version: String,
    announcement: Announcement,
    update: UpdateState,
    update_rx: Option<Receiver<UpdateState>>,
    banner_path: PathBuf,
    started: std::time::Instant,
    auto_checked: bool,
    tray: Option<tray::Tray>,
    elevated: bool,
}

impl App {
    fn new(ctx: &egui::Context, path: PathBuf, tray_icon: (Vec<u8>, u32, u32)) -> Self {
        // Chỉ dùng khi đối chiếu giao diện ở đúng cỡ điểm ảnh của bản gốc trên màn hình scale cao.
        if let Some(zoom) = std::env::var("CRYSS_ZOOM").ok().and_then(|v| v.parse::<f32>().ok()).filter(|z| (0.25..=4.0).contains(z)) {
            ctx.set_zoom_factor(zoom);
        }
        let mut app = Self {
            views: Views::new(ctx), settings: None, path,
            status: String::new(), enabled: false, monitor: None, input: None,
            pressed: HashSet::new(), active: HashMap::new(), capture: None, editor: None, advanced: None,
            unlocker: UnlockerConfig::load(PathBuf::new()), version: app_version(),
            announcement: Announcement::bundled(), update: UpdateState::Idle, update_rx: None, banner_path: PathBuf::new(),
            started: std::time::Instant::now(), auto_checked: false, tray: None, elevated: is_elevated(),
        };
        if !app.elevated {
            app.status = "Chưa chạy bằng quyền quản trị: macro sẽ không nhận phím khi game chạy quyền admin.".into();
        }
        match tray::Tray::new(ctx, TITLE, tray_icon.0, tray_icon.1, tray_icon.2) {
            Ok(tray) => app.tray = Some(tray),
            Err(error) => app.status = format!("Không tạo được biểu tượng khay: {error}"),
        }
        app.unlocker = UnlockerConfig::load(unlocker_dir(&app.path));
        app.banner_path = extras::custom_banner_path(&app.path);
        if let Ok(bytes) = std::fs::read(&app.banner_path) { app.views.set_banner(ctx, Some(&bytes)); }
        app.reload();
        app
    }

    fn ensure_monitor(&mut self, ctx: &egui::Context) -> bool {
        if self.monitor.as_ref().is_some_and(InputMonitor::is_running) { return true; }
        self.monitor.take();
        self.input.take();
        let ctx = ctx.clone();
        match InputMonitor::start(move || ctx.request_repaint()) {
            Ok((monitor, events)) => { self.monitor = Some(monitor); self.input = Some(events); true }
            Err(error) => { self.status = format!("Không thể nhận sự kiện bàn phím và chuột: {error}"); false }
        }
    }

    fn reload(&mut self) {
        self.disarm();
        self.unlocker = UnlockerConfig::load(unlocker_dir(&self.path));
        match Settings::load(&self.path) {
            Ok(settings) => {
                let diagnostics = settings.bindings.diagnostics.len();
                self.settings = Some(settings);
                self.status = if diagnostics == 0 { "Sẵn sàng".to_owned() }
                    else { format!("Có {diagnostics} lưu ý về phím tắt trong cấu hình.") };
            }
            Err(error) => { self.settings = None; self.status = format!("Không đọc được cấu hình: {error}"); }
        }
    }

    fn disarm(&mut self) {
        self.enabled = false;
        self.pressed.clear();
        self.capture = None;
        if let Some(monitor) = &self.monitor {
            monitor.set_bindings(std::iter::empty());
            monitor.set_capture(false);
        }
        for active in self.active.values() { let _ = active.playback.interrupt(Interrupt::Stop); }
    }

    fn enable(&mut self, ctx: &egui::Context) {
        if !self.active.is_empty() && !self.enabled {
            self.status = "Đang kết thúc combo trước. Hãy thử lại sau khi đã dừng.".into();
            return;
        }
        let Some(settings) = &self.settings else { return; };
        let targets: Vec<_> = settings.bindings.active.keys().cloned().collect();
        if targets.is_empty() {
            self.status = "Hãy gán phím cho ít nhất một combo trước khi chạy.".into();
            return;
        }
        if !self.ensure_monitor(ctx) { return; }
        if let Some(monitor) = &self.monitor {
            monitor.set_capture(false);
            monitor.set_bindings(targets);
        }
        self.capture = None;
        self.enabled = true;
        self.status = "Macro đang bật. Giữ phím tắt khi game ở cửa sổ trước.".into();
    }

    fn start(&mut self, target: Target) {
        if !self.enabled || self.active.contains_key(&target) { return; }
        let Some(binding) = self.settings.as_ref().and_then(|s| s.bindings.active.get(&target)) else { return; };
        let Some(focus) = FocusGuard::game() else { return; };
        let instruction = Instruction::Cycle {
            body: binding.instructions.clone(), max_duration: None,
            cancellation: Cancellation::Checkpoint, cleanup: false,
        };
        let result = Worker::new(DesktopOutput::new(focus)).and_then(|mut worker| {
            let playback = worker.start(vec![instruction].into())?;
            Ok(Active { focus, playback, _worker: worker })
        });
        match result {
            Ok(active) => { self.active.insert(target, active); }
            Err(error) => { self.disarm(); self.status = format!("Không thể chạy combo: {error}"); }
        }
    }

    fn event(&mut self, event: InputEvent) {
        match event {
            InputEvent::Press(target) => {
                if self.enabled && self.pressed.insert(target.clone()) { self.start(target); }
            }
            InputEvent::Release(target) => {
                self.pressed.remove(&target);
                if let Some(active) = self.active.get(&target) { let _ = active.playback.interrupt(Interrupt::Release); }
            }
            InputEvent::ForegroundChanged(foreground) => {
                self.pressed.clear();
                for active in self.active.values().filter(|active| !active.focus.matches(foreground)) {
                    let _ = active.playback.interrupt(Interrupt::Stop);
                }
            }
            InputEvent::Capture(target) => {
                // Esc hủy popup như bản gốc; mọi phím khác (và chuột giữa, nút phụ) được gán ngay.
                if target == Target::Key("esc".into()) { self.cancel_capture(); }
                else if let Some(id) = self.capture.take() { self.bind(&id, &hotkey_name(target)); }
            }
        }
    }

    fn reap(&mut self) {
        let mut failure = None;
        self.active.retain(|_, active| match active.playback.wait_timeout(Duration::ZERO) {
            Ok(Some(_)) => false,
            Ok(None) => true,
            Err(error) => { failure = Some(error); false }
        });
        if let Some(error) = failure {
            self.disarm();
            self.status = format!("Macro đã dừng: {error}");
        }
    }

    fn save_change(&mut self, action: impl FnOnce(&mut Settings) -> Result<(), String>, success: &str) -> bool {
        let Some(settings) = &mut self.settings else { return false; };
        match action(settings) {
            Ok(()) => {
                let targets: Vec<_> = settings.bindings.active.keys().cloned().collect();
                if self.enabled && !targets.is_empty() {
                    self.pressed.clear();
                    self.capture = None;
                    for active in self.active.values() { let _ = active.playback.interrupt(Interrupt::Stop); }
                    if let Some(monitor) = &self.monitor {
                        monitor.set_capture(false);
                        monitor.set_bindings(targets);
                    }
                } else { self.disarm(); }
                self.status = success.into();
                true
            }
            Err(error) => { self.status = error; false }
        }
    }

    fn cancel_capture(&mut self) {
        self.capture = None;
        if let Some(monitor) = &self.monitor { monitor.set_capture(false); }
    }

    fn bind(&mut self, id: &str, key: &str) {
        self.capture = None;
        if let Some(monitor) = &self.monitor { monitor.set_capture(false); }
        self.save_change(|settings| settings.set_binding(id, key), "Đã lưu phím tắt.");
    }

    fn open_editor(&mut self, id: Option<String>) {
        let Some(settings) = &self.settings else { return; };
        let fps = f64::from(settings.fps());
        if let Some(id) = id {
            let Some(profile) = settings.custom_profile(&id) else { self.status = "Không tìm thấy combo.".into(); return; };
            if needs_advanced(profile) {
                match Advanced::from_profile(id, profile, fps) {
                    Ok(advanced) => self.advanced = Some(advanced),
                    Err(error) => self.status = error,
                }
                return;
            }
            match Editor::new(id, Some(profile)) { Ok(editor) => self.editor = Some(editor), Err(error) => self.status = error }
        } else {
            match Editor::new(settings.unused_id(), None) { Ok(editor) => self.editor = Some(editor), Err(error) => self.status = error }
        }
    }

    fn advanced_ui(&mut self, ctx: &egui::Context) {
        let fps = f64::from(self.settings.as_ref().map_or(120, Settings::fps));
        let Some(advanced) = self.advanced.as_mut() else { return; };
        advanced.set_fps(fps);
        match advanced.show(ctx) {
            Some(AdvancedAction::Cancel) => self.advanced = None,
            Some(AdvancedAction::Save(profile)) => {
                if self.save_change(|settings| settings.save_profile(profile), "Đã lưu combo.") { self.advanced = None; }
            }
            Some(AdvancedAction::NewCombo) => {
                if let Some(settings) = &self.settings {
                    self.advanced = Some(Advanced::new(settings.unused_id(), fps));
                    self.status = "Sẵn sàng tạo Combo mới.".into();
                }
            }
            Some(AdvancedAction::Basic) => {
                let (functions, dropped) = advanced.block_functions();
                let steps: Vec<serde_json::Value> = functions.iter().map(|f| serde_json::json!(f)).collect();
                let profile = serde_json::json!({
                    "id": advanced.id(), "name": advanced.name(), "hotkey": advanced.hotkey(),
                    "timeline": steps,
                });
                match Editor::new(advanced.id().to_owned(), Some(&profile)) {
                    Ok(editor) => {
                        self.editor = Some(editor);
                        self.advanced = None;
                        ctx.request_repaint();
                        if dropped > 0 { self.status = format!("Editor cơ bản chỉ giữ khối Skirk: đã bỏ {dropped} clip phím / chuột."); }
                    }
                    Err(error) => self.status = error,
                }
            }
            None => {}
        }
    }

    fn check_update(&mut self, ctx: &egui::Context) {
        if matches!(self.update, UpdateState::Checking) { return; }
        self.update = UpdateState::Checking;
        self.update_rx = Some(extras::spawn_check(self.version.clone(), ctx.clone()));
    }

    fn command(&mut self, ctx: &egui::Context, command: Command) {
        match command {
            Command::SetEnabled(true) => self.enable(ctx),
            Command::SetEnabled(false) => { self.disarm(); self.status = "Macro đã tắt.".into(); }
            Command::SetFps(fps) => { self.save_change(|settings| settings.set_fps(fps), "Đã lưu FPS."); }
            Command::Bind { id, hotkey } => self.bind(&id, &hotkey),
            Command::CaptureBinding(id) => {
                if self.ensure_monitor(ctx) {
                    self.capture = Some(id);
                    // Phím được bắt từ sự kiện của chính cửa sổ (popup), không qua hook toàn cục.
                    self.status.clear();
                }
            }
            Command::CancelCapture => self.cancel_capture(),
            Command::Reload => self.reload(),
            Command::NewProfile => self.open_editor(None),
            Command::EditProfile(id) => self.open_editor(Some(id)),
            Command::SaveUnlocker => {
                self.status = match self.unlocker.save() {
                    Ok(()) => "Đã lưu cấu hình Unlocker.".into(),
                    Err(error) => format!("Không lưu được cấu hình Unlocker: {error}"),
                };
            }
            Command::ChangeBanner => {
                let picked = rfd::FileDialog::new().add_filter("Ảnh", &["png", "jpg", "jpeg", "webp"]).pick_file();
                if let Some(path) = picked {
                    match extras::import_banner(&path, &self.banner_path).and_then(|()| std::fs::read(&self.banner_path).map_err(|error| error.to_string())) {
                        Ok(bytes) => { self.views.set_banner(ctx, Some(&bytes)); self.status = "Đã đổi ảnh bìa.".into(); }
                        Err(error) => self.status = error,
                    }
                }
            }
            Command::ResetBanner => {
                let _ = std::fs::remove_file(&self.banner_path);
                self.views.set_banner(ctx, None);
                self.status = "Đã đặt lại ảnh bìa mặc định.".into();
            }
            Command::CheckUpdate => self.check_update(ctx),
            Command::OpenUrl(url) => { if let Err(error) = extras::open_url(&url) { self.status = error; } }
            Command::StartGame => {
                self.status = match self.unlocker.launch_game() {
                    Ok(()) => "Đang mở launcher (cần xác nhận quyền quản trị).".into(),
                    Err(error) => error,
                };
            }
            Command::BrowseGame => {
                let current = Path::new(&self.unlocker.game_path).parent().map(Path::to_path_buf);
                let mut dialog = rfd::FileDialog::new().add_filter("GenshinImpact.exe", &["exe"]);
                if let Some(dir) = current.filter(|d| d.is_dir()) { dialog = dialog.set_directory(dir); }
                if let Some(path) = dialog.pick_file() { self.unlocker.game_path = path.display().to_string(); }
            }
            Command::DeleteProfile(id) => { self.save_change(|settings| settings.delete_profile(&id), "Đã xóa combo."); }
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Nút X ẩn xuống khay thay vì thoát; chỉ "Thoát hoàn toàn" mới đóng hẳn.
        if self.tray.is_some() && !tray::QUITTING.load(std::sync::atomic::Ordering::SeqCst) && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            tray::hide_window(TITLE);
        }
        // Tự kiểm tra cập nhật một lần sau 3 giây, như bản gốc.
        if !self.auto_checked {
            if self.started.elapsed() >= Duration::from_secs(3) { self.auto_checked = true; self.check_update(ctx); }
            else { ctx.request_repaint_after(Duration::from_millis(500)); }
        }
        if let Some(result) = self.update_rx.as_ref().and_then(|rx| rx.try_recv().ok()) { self.update = result; self.update_rx = None; }
        self.reap();
        if self.monitor.as_ref().is_some_and(|monitor| !monitor.is_running()) {
            self.disarm();
            self.monitor.take();
            self.input.take();
            self.status = "Bộ nhận bàn phím và chuột đã dừng.".into();
        }
        let events: Vec<_> = self.input.iter().flat_map(Receiver::try_iter).collect();
        for event in events { self.event(event); }
        if !self.active.is_empty() { ctx.request_repaint_after(Duration::from_millis(32)); }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let profiles = self.settings.as_ref().map(Settings::profiles).unwrap_or_default();
        let model = Model {
            enabled: self.enabled,
            fps: self.settings.as_ref().map_or(120, Settings::fps),
            profiles: &profiles,
            status: &self.status,
            config_path: Path::new(&self.path),
            version: &self.version,
            unlocker: &mut self.unlocker,
            announcement: &self.announcement,
            update: &self.update,
            custom_banner: self.banner_path.is_file(),
            elevated: self.elevated,
            capturing: self.capture.as_deref(),
        };
        let ctx = ui.ctx().clone();
        let editor_was_open = self.editor.is_some();
        let commands = self.views.show(ui, model);
        for command in commands { self.command(&ctx, command); }
        // Editor vừa được mở bởi cú bấm của khung này chỉ vẽ từ khung kế tiếp: nếu không, cú bấm đó
        // rơi lên nút cùng vị trí của editor mới và chuyển ngược lại.
        self.advanced_ui(&ctx);
        if let Some(action) = self.editor.as_mut().filter(|_| editor_was_open).and_then(|editor| editor.show(&ctx)) {
            match action {
                EditorAction::Cancel => self.editor = None,
                EditorAction::NewCombo => {
                    if let Some(settings) = &self.settings {
                        match Editor::new(settings.unused_id(), None) {
                            Ok(editor) => { self.editor = Some(editor); self.status = "Sẵn sàng tạo Combo mới.".into(); }
                            Err(error) => self.status = error,
                        }
                    }
                }
                EditorAction::Advanced { import } => {
                    let fps = f64::from(self.settings.as_ref().map_or(120, Settings::fps));
                    if let Some(editor) = self.editor.take() { self.advanced = Some(editor.to_advanced(fps, import)); ctx.request_repaint(); }
                }
                EditorAction::Save(profile) => {
                    if self.save_change(|settings| settings.save_profile(profile), "Đã lưu combo.") { self.editor = None; }
                }
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.disarm();
        self.monitor.take();
        self.active.clear();
    }
}

fn app_version() -> String {
    serde_json::from_str::<serde_json::Value>(include_str!("../../../../src/UI/version.json")).ok()
        .and_then(|json| json["version"].as_str().map(str::to_owned)).unwrap_or_else(|| "?".into())
}

/// Thư mục `unlocker` của bản gốc: cạnh `config.json` (gói đóng sẵn) hoặc ở `src/unlocker` khi chạy từ mã nguồn.
fn unlocker_dir(config: &Path) -> PathBuf {
    let folder = config.parent().unwrap_or_else(|| Path::new("."));
    let candidates = [folder.join("unlocker"), folder.parent().map(|p| p.join("unlocker")).unwrap_or_default()];
    candidates.iter().find(|dir| dir.is_dir()).cloned().unwrap_or_else(|| folder.join("unlocker"))
}

/// Combo lưu từ editor chuyên sâu (có `clips`) hoặc có bước kiểu đối tượng thì phải mở ở editor chuyên sâu.
fn needs_advanced(profile: &serde_json::Value) -> bool {
    profile["clips"].is_array()
        || ["timeline", "pythonSequence"].iter().any(|key| profile[key].as_array().is_some_and(|steps| steps.iter().any(|s| s.is_object() || s.is_array())))
}

/// Tiến trình có đang chạy với quyền quản trị (token đã nâng quyền) hay không.
fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token = std::ptr::null_mut();
    // Mã thông báo của chính tiến trình này; luôn đóng sau khi đọc.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 { return false; }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut length = 0u32;
    let ok = unsafe { GetTokenInformation(token, TokenElevation, (&raw mut elevation).cast(), std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut length) };
    unsafe { CloseHandle(token); }
    ok != 0 && elevation.TokenIsElevated != 0
}
