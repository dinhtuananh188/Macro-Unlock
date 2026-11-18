//! Khay hệ thống như `main.js` của bản Electron: nút X ẩn xuống khay, bấm khay để mở lại, "Thoát hoàn toàn" để đóng hẳn.
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SW_HIDE, SW_RESTORE, SW_SHOWMINNOACTIVE, SetForegroundWindow, ShowWindow};

/// Đặt khi người dùng chọn "Thoát hoàn toàn": yêu cầu đóng sau đó không bị ẩn xuống khay nữa.
pub static QUITTING: AtomicBool = AtomicBool::new(false);

fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain([0]).collect() }

fn find_window(title: &str) -> Option<HWND> {
    let title = wide(title);
    // Tìm cửa sổ cấp cao nhất theo tiêu đề, kể cả khi đang ẩn.
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    (!hwnd.is_null()).then_some(hwnd)
}

/// Hiện, khôi phục và đưa cửa sổ lên trước. Trả về `false` nếu không có cửa sổ.
pub fn show_window(title: &str) -> bool {
    let Some(hwnd) = find_window(title) else { return false };
    // Cửa sổ do chính tiến trình này hoặc tiến trình cùng phiên tạo ra; không cần quyền đặc biệt.
    unsafe { ShowWindow(hwnd, SW_RESTORE); SetForegroundWindow(hwnd); }
    true
}

pub fn hide_window(title: &str) {
    if let Some(hwnd) = find_window(title) { unsafe { ShowWindow(hwnd, SW_HIDE); } }
}

pub struct Tray {
    _icon: TrayIcon,
}

impl Tray {
    /// Tạo biểu tượng khay. Sự kiện xử lý ngay ở luồng của khay vì cửa sổ ẩn có thể không vẽ lại khi được đánh thức.
    pub fn new(ctx: &egui::Context, title: &'static str, rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, String> {
        let open = MenuItem::new("Mở ứng dụng", true, None);
        let quit = MenuItem::new("Thoát hoàn toàn", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]).map_err(|error| error.to_string())?;
        let icon = Icon::from_rgba(rgba, width, height).map_err(|error| error.to_string())?;
        let tray = TrayIconBuilder::new().with_menu(Box::new(menu)).with_menu_on_left_click(false).with_tooltip(title).with_icon(icon)
            .build().map_err(|error| error.to_string())?;
        let (open_id, quit_id): (MenuId, MenuId) = (open.id().clone(), quit.id().clone());
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if matches!(event, TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }) {
                show_window(title);
            }
        }));
        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == open_id {
                show_window(title);
            } else if event.id == quit_id {
                QUITTING.store(true, Ordering::SeqCst);
                // Cửa sổ ẩn có thể không xử lý lệnh đóng: hiện nó ở dạng thu nhỏ rồi mới yêu cầu đóng, để `Drop` nhả hết đầu vào.
                if let Some(hwnd) = find_window(title) { unsafe { ShowWindow(hwnd, SW_SHOWMINNOACTIVE); } }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }));
        Ok(Self { _icon: tray })
    }
}
