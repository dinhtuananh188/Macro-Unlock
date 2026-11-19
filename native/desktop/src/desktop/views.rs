//! Giao diện chính, dựng theo số đo của bản Electron (`docs/ui-original-spec.md`).
use super::extras::{Announcement, UpdateState, format_bytes};
use crate::unlocker::UnlockerConfig;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Mesh, Painter, Pos2, Rect,
    RichText, ScrollArea, Sense, Shape, Stroke, StrokeKind, TextEdit, TextureHandle, TextureOptions, Ui, UiBuilder, pos2, vec2,
};
use std::path::Path;
use std::sync::Arc;

const BG: Color32 = Color32::from_rgb(9, 13, 22);
const SIDEBAR: Color32 = Color32::from_rgb(7, 11, 18);
const CARD: Color32 = Color32::from_rgb(19, 27, 43);
const ACTIVE: Color32 = Color32::from_rgb(26, 36, 54);
const TEAL: Color32 = Color32::from_rgb(45, 212, 191);
const TEAL_BTN: Color32 = Color32::from_rgb(94, 208, 190);
const PURPLE: Color32 = Color32::from_rgb(167, 139, 250);
const TEXT: Color32 = Color32::from_rgb(231, 237, 248);
const MUTED: Color32 = Color32::from_rgb(143, 160, 186);
const BLUE: Color32 = Color32::from_rgb(96, 165, 250);
const DANGER: Color32 = Color32::from_rgb(253, 164, 175);
const SIDEBAR_WIDTH: f32 = 280.0;
const DEFAULT_BANNER: &[u8] = include_bytes!("../../../../src/UI/assets/defbanner.png");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub hotkey: String,
    pub custom: bool,
}

pub struct Model<'a> {
    pub enabled: bool,
    pub fps: i32,
    pub profiles: &'a [Profile],
    pub status: &'a str,
    pub config_path: &'a Path,
    pub version: &'a str,
    pub unlocker: &'a mut UnlockerConfig,
    pub announcement: &'a Announcement,
    pub update: &'a UpdateState,
    pub custom_banner: bool,
    pub elevated: bool,
    /// Id của combo đang chờ người dùng bấm phím (popup Gán phím đang mở).
    pub capturing: Option<&'a str>,
}

pub enum Command {
    SetEnabled(bool),
    SetFps(i32),
    Bind { id: String, hotkey: String },
    CaptureBinding(String),
    CancelCapture,
    Reload,
    NewProfile,
    EditProfile(String),
    DeleteProfile(String),
    SaveUnlocker,
    ChangeBanner,
    ResetBanner,
    CheckUpdate,
    OpenUrl(String),
    StartGame,
    BrowseGame,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab { Home, Macros, Settings }

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mini { Combo, Status, Fps, Game }

pub struct Views {
    tab: Tab,
    mini: Option<Mini>,
    fps_text: String,
    fps_synced: i32,
    confirm_delete: Option<String>,
    capture_network_key: bool,
    feed_open: bool,
    update_open: bool,
    update_image: TextureHandle,
    logo: TextureHandle,
    banner: TextureHandle,
}

fn family(weight: &str) -> FontFamily { FontFamily::Name(weight.into()) }
fn font(size: f32, weight: &str) -> FontId { FontId::new(size, family(weight)) }
fn radius(value: f32) -> CornerRadius { CornerRadius::same(value as u8) }
fn alpha(color: Color32, a: f32) -> Color32 { Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (a * 255.0) as u8) }
fn line_color() -> Color32 { Color32::from_rgba_unmultiplied(148, 163, 184, 56) }

fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let image = image::load_from_memory(bytes).map(|i| i.to_rgba8()).unwrap_or_else(|_| image::RgbaImage::new(1, 1));
    let size = [image.width() as usize, image.height() as usize];
    ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), TextureOptions::LINEAR)
}

impl Views {
    pub fn new(ctx: &egui::Context) -> Self {
        let mut fonts = FontDefinitions::default();
        let sources: [(&str, &[u8]); 6] = [
            ("h400", include_bytes!("../../../../src/UI/assets/vendor/hanken-400.ttf")),
            ("h500", include_bytes!("../../../../src/UI/assets/vendor/hanken-500.ttf")),
            ("h600", include_bytes!("../../../../src/UI/assets/vendor/hanken-600.ttf")),
            ("h700", include_bytes!("../../../../src/UI/assets/vendor/hanken-700.ttf")),
            ("mont", include_bytes!("../../../../src/UI/assets/Monsterat.ttf")),
            ("mono", include_bytes!("../../../../src/UI/assets/Monsterat.ttf")),
        ];
        // Hanken Grotesk đóng gói thiếu vài chữ Việt có dấu, nên mỗi họ chữ có thêm Montserrat và font hệ thống phía sau.
        let system = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf").ok();
        if let Some(bytes) = system { fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(bytes))); }
        for (name, bytes) in sources {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
        }
        for weight in ["h400", "h500", "h600", "h700"] {
            let mut chain = vec![weight.to_owned(), "mont".to_owned()];
            if fonts.font_data.contains_key("segoe") { chain.push("segoe".into()); }
            fonts.families.insert(family(weight), chain);
        }
        let mut body = vec!["h500".to_owned(), "mont".to_owned()];
        if fonts.font_data.contains_key("segoe") { body.push("segoe".into()); }
        fonts.families.insert(family("mont"), body.clone());
        fonts.families.insert(FontFamily::Proportional, body);
        ctx.set_fonts(fonts);

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.override_text_color = Some(TEXT);
        visuals.selection.bg_fill = alpha(TEAL, 0.4);
        ctx.set_visuals(visuals);

        Self {
            tab: Tab::Home,
            mini: None,
            fps_text: String::new(),
            fps_synced: 0,
            confirm_delete: None,
            capture_network_key: false,
            feed_open: false,
            update_open: false,
            update_image: load_texture(ctx, "update", include_bytes!("../../../../src/UI/assets/update.png")),
            logo: load_texture(ctx, "logo", include_bytes!("../../../../src/UI/assets/logo.png")),
            banner: load_texture(ctx, "banner", DEFAULT_BANNER),
        }
    }

    /// Đổi ảnh bìa; `None` trả về ảnh mặc định.
    pub fn set_banner(&mut self, ctx: &egui::Context, bytes: Option<&[u8]>) {
        self.banner = load_texture(ctx, "banner", bytes.unwrap_or(DEFAULT_BANNER));
    }

    pub fn show(&mut self, ui: &mut Ui, mut model: Model<'_>) -> Vec<Command> {
        let mut commands = Vec::new();
        let window = ui.max_rect();
        let painter = ui.painter().clone();
        painter.rect_filled(window, 0.0, BG);
        let side = Rect::from_min_size(window.min, vec2(SIDEBAR_WIDTH, window.height()));
        let content = Rect::from_min_max(pos2(window.left() + SIDEBAR_WIDTH, window.top()), window.max);
        self.sidebar(ui, side, &model, &mut commands);
        match self.tab {
            Tab::Home => self.home(ui, content, &mut model, &mut commands),
            Tab::Macros => self.macros(ui, content, &model, &mut commands),
            Tab::Settings => self.settings(ui, content, &mut model, &mut commands),
        }
        if !model.status.is_empty() && model.status != "Sẵn sàng" {
            // Thông báo dài tự xuống dòng thay vì bị ẩn.
            let max_width = (content.width() - 80.0 - 28.0).max(120.0);
            let text = painter.layout(model.status.to_owned(), font(12.0, "h500"), TEXT, max_width);
            let rect = Rect::from_min_size(
                pos2(content.left() + 40.0, window.bottom() - 14.0 - text.size().y - 12.0),
                text.size() + vec2(28.0, 12.0),
            );
            painter.rect_filled(rect, radius(10.0), alpha(Color32::BLACK, 0.82));
            painter.rect_stroke(rect, radius(10.0), Stroke::new(1.0, line_color()), StrokeKind::Inside);
            painter.galley(rect.min + vec2(14.0, 6.0), text, TEXT);
        }
        if let Some(id) = model.capturing { self.bind_modal(ui, window, id, &model, &mut commands); }
        commands
    }

    fn sidebar(&mut self, ui: &mut Ui, rect: Rect, model: &Model<'_>, commands: &mut Vec<Command>) {
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, SIDEBAR);
        painter.line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(1.0, line_color()));
        let logo = Rect::from_center_size(pos2(rect.center().x, rect.top() + 96.0), vec2(144.0, 144.0));
        painter.image(self.logo.id(), logo, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        painter.text(pos2(rect.center().x, rect.top() + 192.0), Align2::CENTER_CENTER, "Cứt tool", font(14.0, "h600"), TEAL);

        let items = [(Tab::Home, "Trang chính", Icon::Home), (Tab::Macros, "Macros", Icon::Grid), (Tab::Settings, "Tùy chỉnh", Icon::Tune)];
        for (index, (tab, label, icon)) in items.into_iter().enumerate() {
            let item = Rect::from_min_size(pos2(rect.left() + 8.0, rect.top() + 228.0 + index as f32 * 52.0), vec2(263.0, 48.0));
            let response = ui.interact(item, egui::Id::NULL.with(("nav", index)), Sense::click());
            let active = self.tab == tab;
            if active {
                painter.rect_filled(item, 0.0, ACTIVE);
                painter.rect_filled(Rect::from_min_size(item.min, vec2(3.0, item.height())), 0.0, TEAL);
            } else if response.hovered() {
                painter.rect_filled(item, 0.0, alpha(ACTIVE, 0.6));
            }
            let color = if active { TEAL } else { MUTED };
            draw_icon(painter, icon, pos2(item.left() + 28.0, item.center().y), 20.0, color);
            painter.text(pos2(item.left() + 48.0, item.center().y), Align2::LEFT_CENTER, label, font(16.0, "h500"), if active { TEAL } else { MUTED });
            if response.clicked() { self.tab = tab; self.mini = None; }
        }

        let run = Rect::from_min_size(pos2(rect.left() + 16.0, rect.bottom() - 116.0), vec2(247.0, 44.0));
        let start = Rect::from_min_size(pos2(rect.left() + 16.0, rect.bottom() - 64.0), vec2(247.0, 44.0));
        let (run_label, run_color) = if model.enabled { ("STOP Macro", DANGER) } else { ("RUN Macro", TEAL) };
        if outlined_button(ui, run, "run", run_label, Icon::Play, run_color, run_color).clicked() {
            commands.push(Command::SetEnabled(!model.enabled));
        }
        let purple = Color32::from_rgb(124, 58, 237);
        if outlined_button(ui, start, "start", "START Game", Icon::Gamepad, PURPLE, purple).clicked() {
            commands.push(Command::StartGame);
        }
    }

    fn home(&mut self, ui: &mut Ui, rect: Rect, model: &mut Model<'_>, commands: &mut Vec<Command>) {
        let painter = ui.painter().with_clip_rect(rect);
        // Ảnh bìa phủ kín vùng nội dung (cover), neo giữa.
        let image_aspect = 2048.0 / 1608.0;
        let area_aspect = rect.width() / rect.height();
        let uv = if area_aspect > image_aspect {
            let v = image_aspect / area_aspect;
            Rect::from_min_max(pos2(0.0, (1.0 - v) / 2.0), pos2(1.0, (1.0 + v) / 2.0))
        } else {
            let u = area_aspect / image_aspect;
            Rect::from_min_max(pos2((1.0 - u) / 2.0, 0.0), pos2((1.0 + u) / 2.0, 1.0))
        };
        painter.image(self.banner.id(), rect, uv, Color32::WHITE);
        painter.rect_filled(rect, 0.0, alpha(BG, 0.28));
        vertical_gradient(&painter, Rect::from_min_max(pos2(rect.left(), rect.top()), pos2(rect.right(), rect.top() + 160.0)), alpha(BG, 0.55), Color32::TRANSPARENT);
        vertical_gradient(&painter, Rect::from_min_max(pos2(rect.left(), rect.bottom() - 380.0), rect.max), Color32::TRANSPARENT, alpha(BG, 0.94));

        painter.text(pos2(rect.left() + 32.0, rect.top() + 190.0), Align2::LEFT_CENTER, "CutTool", font(64.0, "h700"), Color32::WHITE);
        let badge = Rect::from_min_size(pos2(rect.left() + 34.0, rect.top() + 232.0), vec2(62.0, 24.0));
        painter.rect_filled(badge, radius(8.0), alpha(TEAL, 0.85));
        painter.text(badge.center(), Align2::CENTER_CENTER, format!("v{}", model.version), font(12.0, "h700"), Color32::BLACK);

        // Chuông cập nhật, đổi ảnh bìa và đặt lại ảnh bìa.
        let bell = Rect::from_min_size(pos2(rect.right() - 158.0, rect.top() + 24.0), vec2(34.0, 34.0));
        let cover = Rect::from_min_size(pos2(rect.right() - 114.0, rect.top() + 24.0), vec2(104.0, 34.0));
        let reset = Rect::from_min_size(pos2(rect.right() - 158.0 - 96.0, rect.top() + 24.0), vec2(86.0, 34.0));
        let mut buttons = vec![(bell, "", Icon::Bell, 0), (cover, "Đổi ảnh bìa", Icon::Camera, 1)];
        if model.custom_banner { buttons.push((reset, "Mặc định", Icon::Refresh, 2)); }
        for (button, label, icon, index) in buttons {
            let response = ui.interact(button, egui::Id::NULL.with(("hero", index)), Sense::click());
            painter.rect_filled(button, radius(10.0), alpha(Color32::BLACK, if response.hovered() { 0.65 } else { 0.5 }));
            painter.rect_stroke(button, radius(10.0), Stroke::new(1.0, alpha(Color32::WHITE, 0.12)), StrokeKind::Inside);
            if label.is_empty() {
                draw_icon(&painter, icon, button.center(), 16.0, MUTED);
                if matches!(model.update, UpdateState::Available(_)) {
                    painter.circle_filled(button.right_top() + vec2(-6.0, 6.0), 4.5, Color32::from_rgb(244, 63, 94));
                }
            } else {
                draw_icon(&painter, icon, pos2(button.left() + 16.0, button.center().y), 14.0, TEXT);
                painter.text(pos2(button.left() + 29.0, button.center().y), Align2::LEFT_CENTER, label, font(11.0, "h600"), TEXT);
            }
            if response.clicked() {
                match index {
                    0 => { self.update_open = true; if !matches!(model.update, UpdateState::Checking) { commands.push(Command::CheckUpdate); } }
                    1 => commands.push(Command::ChangeBanner),
                    _ => commands.push(Command::ResetBanner),
                }
            }
        }

        // Thẻ tin cập nhật: bấm để đọc đầy đủ.
        let news = Rect::from_min_size(pos2(rect.left() + 22.0, rect.bottom() - 232.0), vec2(438.0, 133.0));
        let news_response = ui.interact(news, egui::Id::NULL.with("news"), Sense::click());
        painter.rect_filled(news, radius(14.0), alpha(Color32::BLACK, if news_response.hovered() { 0.68 } else { 0.55 }));
        painter.rect_stroke(news, radius(14.0), Stroke::new(1.0, alpha(if news_response.hovered() { TEAL } else { Color32::WHITE }, if news_response.hovered() { 0.4 } else { 0.12 })), StrokeKind::Inside);
        let tag_text = painter.layout_no_wrap(model.announcement.tag.clone(), font(10.0, "h700"), TEAL);
        let tag = Rect::from_min_size(news.min + vec2(18.0, 16.0), vec2((tag_text.size().x + 20.0).min(news.width() - 130.0), 22.0));
        painter.rect_filled(tag, radius(6.0), alpha(TEAL, 0.18));
        painter.with_clip_rect(tag).galley(tag.min + vec2(10.0, (22.0 - tag_text.size().y) / 2.0), tag_text, TEAL);
        painter.text(pos2(news.right() - 18.0, tag.center().y), Align2::RIGHT_CENTER, &model.announcement.date, font(11.0, "h500"), MUTED);
        let clip = painter.with_clip_rect(news.shrink2(vec2(18.0, 0.0)));
        clip.text(news.min + vec2(18.0, 62.0), Align2::LEFT_CENTER, &model.announcement.title, font(20.0, "h700"), Color32::WHITE);
        clip.text(news.min + vec2(18.0, 100.0), Align2::LEFT_CENTER, &model.announcement.summary, font(12.0, "h400"), MUTED);
        if news_response.clicked() { self.feed_open = true; }

        // Bốn nút nhỏ mở panel nội dung.
        let buttons = [(Mini::Combo, "Combo", Icon::Keyboard, 107.0), (Mini::Status, "Trạng thái", Icon::Tune, 130.0), (Mini::Fps, "FPS", Icon::Speed, 86.0), (Mini::Game, "Game", Icon::Gamepad, 96.0)];
        let mut x = rect.right() - 8.0 - buttons.iter().map(|b| b.3 + 8.0).sum::<f32>() + 8.0 - 32.0;
        let y = rect.bottom() - 62.0;
        for (kind, label, icon, width) in buttons {
            let button = Rect::from_min_size(pos2(x, y), vec2(width, 38.0));
            x += width + 8.0;
            let response = ui.interact(button, egui::Id::NULL.with(("mini", label)), Sense::click());
            let on = self.mini == Some(kind);
            painter.rect_filled(button, radius(12.0), alpha(Color32::BLACK, if on { 0.8 } else { 0.55 }));
            painter.rect_stroke(button, radius(12.0), Stroke::new(1.0, if on { alpha(TEAL, 0.6) } else { alpha(Color32::WHITE, 0.14) }), StrokeKind::Inside);
            draw_icon(&painter, icon, pos2(button.left() + 20.0, button.center().y), 15.0, if on { TEAL } else { MUTED });
            painter.text(pos2(button.left() + 36.0, button.center().y), Align2::LEFT_CENTER, label, font(13.0, "h600"), TEXT);
            if response.clicked() { self.mini = if on { None } else { Some(kind) }; }
        }
        if let Some(kind) = self.mini { self.mini_panel(ui, rect, kind, model, commands); }
        if self.feed_open { self.feed_modal(ui, rect, model); }
        if self.update_open { self.update_modal(ui, rect, model, commands); }
    }

    fn mini_panel(&mut self, ui: &mut Ui, rect: Rect, kind: Mini, model: &mut Model<'_>, commands: &mut Vec<Command>) {
        let painter = ui.painter().clone();
        let panel = Rect::from_min_size(pos2(rect.right() - 396.0, rect.bottom() - 62.0 - 12.0 - 300.0), vec2(364.0, 300.0));
        painter.rect_filled(panel, radius(14.0), alpha(Color32::from_rgb(9, 13, 22), 0.94));
        painter.rect_stroke(panel, radius(14.0), Stroke::new(1.0, line_color()), StrokeKind::Inside);
        let (title, icon) = match kind {
            Mini::Combo => ("Combo đã gán phím", Icon::Keyboard),
            Mini::Status => ("Trạng thái Macro", Icon::Tune),
            Mini::Fps => ("FPS mục tiêu", Icon::Speed),
            Mini::Game => ("Game & Unlocker", Icon::Gamepad),
        };
        draw_icon(&painter, icon, panel.min + vec2(28.0, 28.0), 18.0, TEAL);
        painter.text(panel.min + vec2(46.0, 28.0), Align2::LEFT_CENTER, title, font(15.0, "h700"), TEXT);
        let body = Rect::from_min_max(panel.min + vec2(20.0, 54.0), panel.max - vec2(20.0, 16.0));
        match kind {
            Mini::Combo => {
                let bound: Vec<_> = model.profiles.iter().filter(|p| !p.hotkey.is_empty()).collect();
                if bound.is_empty() {
                    painter.text(body.center(), Align2::CENTER_CENTER, "Chưa có combo nào được gán phím.", font(13.0, "h400"), MUTED);
                }
                let mut child = ui.new_child(UiBuilder::new().max_rect(body));
                ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
                    for profile in bound {
                        let (row, _) = ui.allocate_exact_size(vec2(body.width() - 8.0, 34.0), Sense::hover());
                        ui.painter().text(row.left_center(), Align2::LEFT_CENTER, &profile.name, font(12.0, "h500"), TEXT);
                        key_pill(ui.painter(), pos2(row.right(), row.center().y), &profile.hotkey);
                    }
                });
            }
            Mini::Status => {
                let config = model.config_path.display().to_string();
                let rows = [
                    ("Macro", if model.enabled { "Đang bật" } else { "Đang tắt" }, if model.enabled { TEAL } else { MUTED }),
                    ("Lõi", "Rust native, chạy trong tiến trình", TEAL),
                    ("Quyền", if model.elevated { "Quản trị viên" } else { "Thường: game chạy quyền admin sẽ không nhận phím" }, if model.elevated { TEAL } else { DANGER }),
                    ("Cấu hình", config.as_str(), MUTED),
                ];
                for (index, (label, value, color)) in rows.into_iter().enumerate() {
                    let y = body.top() + 14.0 + index as f32 * 46.0;
                    painter.text(pos2(body.left(), y), Align2::LEFT_CENTER, label, font(12.0, "h500"), MUTED);
                    let galley = painter.layout(value.to_owned(), font(12.0, "h600"), color, body.width());
                    painter.galley(pos2(body.left(), y + 10.0), galley, color);
                }
            }
            Mini::Fps => {
                self.sync_fps(model.fps);
                let input = Rect::from_min_size(body.min + vec2(0.0, 6.0), vec2(110.0, 38.0));
                ui.put(input, TextEdit::singleline(&mut self.fps_text).font(font(15.0, "h600")).margin(egui::Margin::symmetric(12, 9)));
                let save = Rect::from_min_size(body.min + vec2(122.0, 6.0), vec2(96.0, 38.0));
                if solid_button(ui, save, "fpssave", "Lưu", Icon::Save).clicked() {
                    match self.fps_text.trim().parse::<i32>() {
                        Ok(fps) => commands.push(Command::SetFps(fps)),
                        Err(_) => self.fps_text = model.fps.to_string(),
                    }
                }
                painter.text(body.min + vec2(0.0, 70.0), Align2::LEFT_TOP, "Đổi FPS làm tính lại toàn bộ thời gian chờ của combo.", font(12.0, "h400"), MUTED);
            }
            Mini::Game => {
                painter.text(body.min + vec2(0.0, 8.0), Align2::LEFT_TOP, "Đường dẫn file game", font(12.0, "h500"), MUTED);
                let galley = painter.layout(model.unlocker.game_path.clone(), font(12.0, "h600"), TEXT, body.width());
                painter.galley(body.min + vec2(0.0, 30.0), galley, TEXT);
                painter.text(body.min + vec2(0.0, 100.0), Align2::LEFT_TOP, "Chỉnh chi tiết ở trang Tùy chỉnh.", font(12.0, "h400"), MUTED);
            }
        }
    }


    /// Lớp phủ tối và thẻ giữa màn hình; trả về `true` khi cần đóng (bấm nền hoặc Esc).
    fn overlay(ui: &mut Ui, rect: Rect, id: &str, width: f32, height: f32) -> (Rect, bool) {
        let painter = ui.painter().clone();
        painter.rect_filled(rect, 0.0, alpha(Color32::BLACK, 0.66));
        let backdrop = ui.interact(rect, egui::Id::NULL.with((id, "backdrop")), Sense::click());
        let card = Rect::from_center_size(rect.center(), vec2(width.min(rect.width() - 80.0), height.min(rect.height() - 60.0)));
        let inside = ui.interact(card, egui::Id::NULL.with((id, "card")), Sense::click());
        painter.rect_filled(card, radius(18.0), Color32::from_rgb(11, 17, 29));
        painter.rect_stroke(card, radius(18.0), Stroke::new(1.0, line_color()), StrokeKind::Inside);
        let close = (backdrop.clicked() && !inside.hovered()) || ui.input(|input| input.key_pressed(egui::Key::Escape));
        (card, close)
    }


    /// Phím hoặc nút chuột đầu tiên người dùng bấm trong cửa sổ. Chuột trái / phải không bao giờ được gán
    /// (đó là cách bấm các nút trong popup); Esc đã được `overlay` xử lý để hủy.
    fn captured_hotkey(ctx: &egui::Context) -> Option<String> {
        ctx.input_mut(|input| {
            let found = input.events.iter().enumerate().find_map(|(index, event)| match event {
                egui::Event::Key { key, physical_key, pressed: true, repeat: false, .. } => {
                    if *key == egui::Key::Escape { return None; }
                    let physical = physical_key.unwrap_or(*key);
                    let key = if physical.name().len() == 1 && physical.name().as_bytes()[0].is_ascii_alphanumeric() { physical } else { *key };
                    super::editor::keyboard_hotkey(key).map(|name| (index, name))
                }
                egui::Event::PointerButton { button, pressed: true, .. } => match button {
                    egui::PointerButton::Middle => Some((index, "middle".to_owned())),
                    egui::PointerButton::Extra1 => Some((index, "mouse_3".to_owned())),
                    egui::PointerButton::Extra2 => Some((index, "mouse_4".to_owned())),
                    _ => None,
                },
                _ => None,
            });
            found.map(|(index, name)| { input.events.remove(index); name })
        })
    }

    fn feed_modal(&mut self, ui: &mut Ui, rect: Rect, model: &Model<'_>) {
        let (card, close) = Self::overlay(ui, rect, "feed", 640.0, 600.0);
        let painter = ui.painter().clone();
        let image = Rect::from_min_size(card.min + vec2(16.0, 16.0), vec2(card.width() - 32.0, 170.0));
        let aspect = 16.0 / 9.0;
        let v = (image.width() / image.height() / aspect).recip().min(1.0);
        painter.image(self.update_image.id(), image, Rect::from_min_max(pos2(0.0, (1.0 - v) / 2.0), pos2(1.0, (1.0 + v) / 2.0)), Color32::WHITE);
        let tag = painter.layout_no_wrap(model.announcement.tag.clone(), font(10.0, "h700"), TEAL);
        let pill = Rect::from_min_size(pos2(card.left() + 24.0, image.bottom() + 16.0), vec2(tag.size().x + 20.0, 22.0));
        painter.rect_filled(pill, radius(6.0), alpha(TEAL, 0.18));
        painter.galley(pill.min + vec2(10.0, (22.0 - tag.size().y) / 2.0), tag, TEAL);
        painter.text(pos2(card.right() - 24.0, pill.center().y), Align2::RIGHT_CENTER, &model.announcement.date, font(12.0, "h500"), MUTED);
        let title = painter.layout(model.announcement.title.clone(), font(22.0, "h700"), Color32::WHITE, card.width() - 48.0);
        let title_height = title.size().y;
        painter.galley(pos2(card.left() + 24.0, pill.bottom() + 10.0), title, Color32::WHITE);
        let body = Rect::from_min_max(pos2(card.left() + 24.0, pill.bottom() + 20.0 + title_height), pos2(card.right() - 24.0, card.bottom() - 70.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(body));
        ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
            ui.set_width(body.width() - 10.0);
            ui.label(RichText::new(&model.announcement.content).size(13.5).color(MUTED));
        });
        let button = Rect::from_min_size(pos2(card.right() - 24.0 - 110.0, card.bottom() - 58.0), vec2(110.0, 42.0));
        if outline_neutral_button(ui, button, "feed-close", "Đóng", Icon::Plus).clicked() || close { self.feed_open = false; }
    }

    fn update_modal(&mut self, ui: &mut Ui, rect: Rect, model: &Model<'_>, commands: &mut Vec<Command>) {
        let tall = matches!(model.update, UpdateState::Available(_));
        let (card, close) = Self::overlay(ui, rect, "update", 560.0, if tall { 460.0 } else { 230.0 });
        let painter = ui.painter().clone();
        let (title, color) = match model.update {
            UpdateState::Available(_) => ("Có bản cập nhật mới!", TEAL),
            UpdateState::Current(_) => ("Bạn đang dùng bản mới nhất", TEAL),
            UpdateState::Failed(_) => ("Không kiểm tra được cập nhật", DANGER),
            _ => ("Đang kiểm tra cập nhật…", MUTED),
        };
        draw_icon(&painter, Icon::Bell, pos2(card.left() + 38.0, card.top() + 40.0), 22.0, color);
        painter.text(pos2(card.left() + 62.0, card.top() + 40.0), Align2::LEFT_CENTER, title, font(20.0, "h700"), TEXT);
        let mut y = card.top() + 82.0;
        let mut page = None;
        match model.update {
            UpdateState::Available(info) => {
                painter.text(pos2(card.left() + 28.0, y), Align2::LEFT_CENTER, format!("v{} → v{}", info.current, info.latest), font(15.0, "h600"), TEAL);
                painter.text(pos2(card.right() - 28.0, y), Align2::RIGHT_CENTER, format!("{} · {}", info.asset, format_bytes(info.size)), font(12.0, "h500"), MUTED);
                y += 22.0;
                let notes = Rect::from_min_max(pos2(card.left() + 28.0, y), pos2(card.right() - 28.0, card.bottom() - 76.0));
                painter.rect_filled(notes, radius(10.0), alpha(Color32::BLACK, 0.3));
                let mut child = ui.new_child(UiBuilder::new().max_rect(notes.shrink(12.0)));
                ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
                    ui.set_width(notes.width() - 34.0);
                    ui.label(RichText::new(if info.notes.trim().is_empty() { "Bản phát hành chưa có ghi chú." } else { info.notes.as_str() }).size(13.0).color(MUTED));
                });
                page = Some(info.page.clone());
            }
            UpdateState::Current(version) => { painter.text(pos2(card.left() + 28.0, y), Align2::LEFT_CENTER, format!("Phiên bản hiện tại: v{version}"), font(14.0, "h500"), MUTED); }
            UpdateState::Failed(error) => {
                let galley = painter.layout(error.clone(), font(13.0, "h500"), DANGER, card.width() - 56.0);
                painter.galley(pos2(card.left() + 28.0, y - 8.0), galley, DANGER);
            }
            _ => {}
        }
        let close_button = Rect::from_min_size(pos2(card.right() - 28.0 - 110.0, card.bottom() - 58.0), vec2(110.0, 42.0));
        if let Some(page) = page.filter(|page| page.starts_with("https://")) {
            let open = Rect::from_min_size(pos2(close_button.left() - 12.0 - 190.0, close_button.top()), vec2(190.0, 42.0));
            if solid_button(ui, open, "update-open", "Mở trang tải về", Icon::Save).clicked() { commands.push(Command::OpenUrl(page)); }
        }
        if outline_neutral_button(ui, close_button, "update-close", "Đóng", Icon::Plus).clicked() || close { self.update_open = false; }
    }


    /// Popup "Gán phím cho …" của bản gốc (`#keyBindModal`).
    fn bind_modal(&mut self, ui: &mut Ui, rect: Rect, id: &str, model: &Model<'_>, commands: &mut Vec<Command>) {
        let (card, close) = Self::overlay(ui, rect, "bind", 448.0, 330.0);
        if let Some(hotkey) = Self::captured_hotkey(ui.ctx()) { commands.push(Command::Bind { id: id.to_owned(), hotkey }); }
        let painter = ui.painter().clone();
        let name = model.profiles.iter().position(|p| p.id == id).map_or_else(|| "Combo".to_owned(), |index| {
            let profile = &model.profiles[index];
            if profile.custom { profile.name.clone() } else { format!("Combo {}: {}", index + 1, profile.name) }
        });
        let icon_box = Rect::from_center_size(pos2(card.center().x, card.top() + 56.0), vec2(48.0, 48.0));
        painter.rect_filled(icon_box, radius(12.0), alpha(TEAL, 0.15));
        painter.rect_stroke(icon_box, radius(12.0), Stroke::new(1.0, alpha(TEAL, 0.3)), StrokeKind::Inside);
        draw_icon(&painter, Icon::Keyboard, icon_box.center(), 26.0, TEAL);
        let title = painter.layout(format!("Gán phím cho \"{name}\""), font(18.0, "h700"), TEXT, card.width() - 56.0);
        let title_height = title.size().y;
        painter.galley(pos2(card.center().x - title.size().x / 2.0, card.top() + 98.0), title, TEXT);
        let mut y = card.top() + 98.0 + title_height + 12.0;
        painter.text(pos2(card.center().x, y), Align2::CENTER_TOP, "Nhấn phím bất kỳ trên bàn phím hoặc nút chuột để gán.", font(13.0, "h400"), MUTED);
        y += 22.0;
        painter.text(pos2(card.center().x, y), Align2::CENTER_TOP, "Nhấn ESC để hủy", font(13.0, "h500"), MUTED);
        y += 30.0;
        let badge = Rect::from_center_size(pos2(card.center().x, y + 16.0), vec2(220.0, 34.0));
        let pulse = 0.55 + 0.25 * ((ui.input(|input| input.time) * 3.0).sin() as f32 + 1.0) / 2.0;
        painter.rect_filled(badge, radius(10.0), alpha(TEAL, 0.10 * pulse + 0.04));
        painter.rect_stroke(badge, radius(10.0), Stroke::new(1.0, alpha(TEAL, 0.35)), StrokeKind::Inside);
        painter.text(badge.center(), Align2::CENTER_CENTER, "Đang chờ bấm phím...", font(13.0, "h600"), TEAL);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
        let top = card.bottom() - 62.0;
        let clear = Rect::from_min_size(pos2(card.center().x - 8.0 - 150.0, top), vec2(150.0, 42.0));
        let cancel = Rect::from_min_size(pos2(card.center().x + 8.0, top), vec2(110.0, 42.0));
        let clear_response = ui.interact(clear, egui::Id::NULL.with("bind-clear"), Sense::click());
        painter.rect_filled(clear, radius(12.0), alpha(DANGER, if clear_response.hovered() { 0.16 } else { 0.08 }));
        painter.rect_stroke(clear, radius(12.0), Stroke::new(1.0, alpha(DANGER, 0.4)), StrokeKind::Inside);
        painter.text(clear.center(), Align2::CENTER_CENTER, "Xóa phím gán", font(14.0, "h600"), DANGER);
        if clear_response.clicked() { commands.push(Command::Bind { id: id.to_owned(), hotkey: String::new() }); }
        let cancel_clicked = outline_neutral_button(ui, cancel, "bind-cancel", "Hủy", Icon::Plus).clicked();
        if cancel_clicked || close { commands.push(Command::CancelCapture); }
    }

    fn sync_fps(&mut self, fps: i32) {
        if self.fps_synced != fps { self.fps_synced = fps; self.fps_text = fps.to_string(); }
    }

    fn macros(&mut self, ui: &mut Ui, rect: Rect, model: &Model<'_>, commands: &mut Vec<Command>) {
        let painter = ui.painter().clone();
        let left = rect.left() + 40.0;
        let right = rect.right() - 40.0;
        painter.text(pos2(left, rect.top() + 40.0), Align2::LEFT_TOP, "Danh sách Combo", font(20.0, "h700"), TEXT);
        painter.text(pos2(left, rect.top() + 72.0), Align2::LEFT_TOP, "Hiển thị các combo hiện có và chỉnh sửa/gán phím.", font(14.0, "h400"), MUTED);
        let add = Rect::from_min_size(pos2(right - 170.0, rect.top() + 40.0), vec2(170.0, 46.0));
        let reload = Rect::from_min_size(pos2(right - 170.0 - 12.0 - 106.0, rect.top() + 40.0), vec2(106.0, 46.0));
        if outline_neutral_button(ui, reload, "reload", "Reload", Icon::Refresh).clicked() { commands.push(Command::Reload); }
        if solid_button(ui, add, "add", "Thêm Combo", Icon::Plus).clicked() { commands.push(Command::NewProfile); }
        painter.line_segment([pos2(left, rect.top() + 104.0), pos2(right, rect.top() + 104.0)], Stroke::new(1.0, line_color()));

        let area = Rect::from_min_max(pos2(left, rect.top() + 138.0), pos2(right + 8.0, rect.bottom() - 8.0));
        let width = right - left;
        let columns = if width >= 880.0 { 3 } else if width >= 580.0 { 2 } else { 1 };
        let gap = 24.0;
        let card_width = (width - gap * (columns as f32 - 1.0)) / columns as f32;
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
            ui.set_width(width);
            let rows = model.profiles.len().div_ceil(columns);
            let (origin, _) = ui.allocate_exact_size(vec2(width, rows as f32 * (188.0 + gap) + 24.0), Sense::hover());
            for (index, profile) in model.profiles.iter().enumerate() {
                let card = Rect::from_min_size(
                    pos2(origin.left() + (index % columns) as f32 * (card_width + gap), origin.top() + (index / columns) as f32 * (188.0 + gap)),
                    vec2(card_width, 188.0),
                );
                self.combo_card(ui, card, index, profile, commands);
            }
        });
    }

    fn combo_card(&mut self, ui: &mut Ui, card: Rect, index: usize, profile: &Profile, commands: &mut Vec<Command>) {
        let painter = ui.painter().clone();
        painter.rect_filled(card, radius(12.0), CARD);
        painter.rect_stroke(card, radius(12.0), Stroke::new(1.0, line_color()), StrokeKind::Inside);
        let inner = card.shrink(20.0);
        // Hàng trên: nhãn loại và phím đang gán.
        let tag = Rect::from_min_size(inner.min + vec2(0.0, 2.0), vec2(if profile.custom { 62.0 } else { 69.0 }, 22.0));
        let (tag_text, tag_color) = if profile.custom { ("Tự tạo", PURPLE) } else { ("Mặc định", BLUE) };
        painter.rect_filled(tag, radius(4.0), alpha(tag_color, 0.1));
        painter.rect_stroke(tag, radius(4.0), Stroke::new(1.0, alpha(tag_color, 0.2)), StrokeKind::Inside);
        painter.text(tag.center(), Align2::CENTER_CENTER, tag_text, font(11.0, "h500"), tag_color);
        if profile.hotkey.is_empty() {
            let pill = Rect::from_min_size(pos2(inner.right() - 111.0, inner.top()), vec2(111.0, 25.0));
            painter.rect_filled(pill, radius(6.0), alpha(Color32::WHITE, 0.05));
            painter.rect_stroke(pill, radius(6.0), Stroke::new(1.0, alpha(Color32::from_rgb(148, 163, 184), 0.3)), StrokeKind::Inside);
            painter.text(pill.center(), Align2::CENTER_CENTER, "Chưa gán phím", font(12.0, "mont"), alpha(MUTED, 0.7));
        } else {
            key_pill(&painter, pos2(inner.right(), inner.top() + 12.5), &profile.hotkey);
        }
        // Tên combo, tối đa hai dòng.
        let title = if profile.custom { profile.name.clone() } else { format!("Combo {}: {}", index + 1, profile.name) };
        let galley = painter.layout(title, font(16.0, "h600"), TEXT, inner.width());
        let clip = Rect::from_min_size(pos2(inner.left(), inner.top() + 37.0), vec2(inner.width(), 48.0));
        painter.with_clip_rect(clip).galley(clip.min, galley, TEXT);
        // Hàng dưới.
        painter.line_segment([pos2(inner.left(), inner.bottom() - 46.0), pos2(inner.right(), inner.bottom() - 46.0)], Stroke::new(1.0, alpha(Color32::from_rgb(148, 163, 184), 0.12)));
        let bind = Rect::from_min_size(pos2(inner.right() - 103.0, inner.bottom() - 33.0), vec2(103.0, 33.0));
        let hover_id = egui::Id::NULL.with(("combo", &profile.id));
        let response = ui.interact(bind, hover_id.with("bind"), Sense::click());
        painter.rect_filled(bind, radius(8.0), alpha(TEAL, if response.hovered() { 0.2 } else { 0.1 }));
        painter.rect_stroke(bind, radius(8.0), Stroke::new(1.0, alpha(TEAL, 0.3)), StrokeKind::Inside);
        draw_icon(&painter, Icon::Keyboard, pos2(bind.left() + 18.0, bind.center().y), 14.0, TEAL);
        painter.text(pos2(bind.left() + 32.0, bind.center().y), Align2::LEFT_CENTER, "Gán phím", font(12.0, "h500"), TEAL);
        if response.clicked() { commands.push(Command::CaptureBinding(profile.id.clone())); }
        if !profile.hotkey.is_empty() {
            let clear = Rect::from_min_size(pos2(bind.left() - 8.0 - 33.0 * 0.0 - 70.0, bind.top()), vec2(70.0, 33.0));
            let response = ui.interact(clear, hover_id.with("clear"), Sense::click());
            painter.text(clear.center(), Align2::CENTER_CENTER, "Bỏ phím", font(12.0, "h500"), if response.hovered() { TEXT } else { MUTED });
            if response.clicked() { commands.push(Command::Bind { id: profile.id.clone(), hotkey: String::new() }); }
        }
        if profile.custom {
            let edit = Rect::from_min_size(pos2(inner.left(), inner.bottom() - 33.0), vec2(33.0, 33.0));
            let delete = Rect::from_min_size(pos2(inner.left() + 41.0, inner.bottom() - 33.0), vec2(33.0, 33.0));
            let confirming = self.confirm_delete.as_deref() == Some(&profile.id);
            let edit_response = ui.interact(edit, hover_id.with("edit"), Sense::click());
            painter.rect_stroke(edit, radius(8.0), Stroke::new(1.0, line_color()), StrokeKind::Inside);
            draw_icon(&painter, Icon::Pencil, edit.center(), 15.0, if edit_response.hovered() { TEXT } else { MUTED });
            if edit_response.clicked() { commands.push(Command::EditProfile(profile.id.clone())); }
            let delete_response = ui.interact(if confirming { Rect::from_min_size(delete.min, vec2(104.0, 33.0)) } else { delete }, hover_id.with("delete"), Sense::click());
            let shown = if confirming { Rect::from_min_size(delete.min, vec2(104.0, 33.0)) } else { delete };
            painter.rect_stroke(shown, radius(8.0), Stroke::new(1.0, alpha(DANGER, if confirming { 0.6 } else { 0.25 })), StrokeKind::Inside);
            if confirming {
                painter.text(shown.center(), Align2::CENTER_CENTER, "Xác nhận xóa", font(12.0, "h600"), DANGER);
            } else {
                draw_icon(&painter, Icon::Trash, shown.center(), 15.0, DANGER);
            }
            if delete_response.clicked() {
                if confirming { commands.push(Command::DeleteProfile(profile.id.clone())); self.confirm_delete = None; }
                else { self.confirm_delete = Some(profile.id.clone()); }
            }
        }
    }

    fn settings(&mut self, ui: &mut Ui, rect: Rect, model: &mut Model<'_>, commands: &mut Vec<Command>) {
        let painter = ui.painter().clone();
        let left = rect.left() + 40.0;
        let right = rect.right() - 40.0;
        draw_icon(&painter, Icon::Tune, pos2(left + 6.0, rect.top() + 48.0), 13.0, TEAL);
        painter.text(pos2(left + 18.0, rect.top() + 48.0), Align2::LEFT_CENTER, "Cấu hình Game & Unlocker", font(12.0, "h500"), TEAL);
        painter.text(pos2(left, rect.top() + 64.0), Align2::LEFT_TOP, "Tùy chỉnh", font(32.0, "h700"), TEXT);
        let save = Rect::from_min_size(pos2(right - 160.0, rect.top() + 40.0), vec2(160.0, 48.0));
        if solid_button(ui, save, "unlocker-save", "Lưu cấu hình", Icon::Save).clicked() { commands.push(Command::SaveUnlocker); }

        let area = Rect::from_min_max(pos2(left, rect.top() + 112.0), pos2(right + 8.0, rect.bottom() - 8.0));
        let width = right - left;
        let columns = if width >= 760.0 { 2 } else { 1 };
        let gap = 24.0;
        let card_width = (width - gap * (columns as f32 - 1.0)) / columns as f32;
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
            ui.set_width(width);
            let entries = SETTINGS;
            let rows = entries.len().div_ceil(columns);
            let (origin, _) = ui.allocate_exact_size(vec2(width, rows as f32 * 124.0 + 120.0 + 24.0), Sense::hover());
            for (index, entry) in entries.iter().enumerate() {
                let card = Rect::from_min_size(
                    pos2(origin.left() + (index % columns) as f32 * (card_width + gap), origin.top() + (index / columns) as f32 * 124.0),
                    vec2(card_width, 104.0),
                );
                self.setting_card(ui, card, entry, model.unlocker);
            }
            let game = Rect::from_min_size(pos2(origin.left(), origin.top() + rows as f32 * 124.0), vec2(width, 104.0));
            self.game_card(ui, game, model.unlocker, commands);
        });
    }

    fn setting_card(&mut self, ui: &mut Ui, card: Rect, entry: &Setting, config: &mut UnlockerConfig) {
        let painter = ui.painter().clone();
        painter.rect_filled(card, radius(12.0), CARD);
        painter.rect_stroke(card, radius(12.0), Stroke::new(1.0, line_color()), StrokeKind::Inside);
        let icon_box = Rect::from_min_size(card.min + vec2(14.0, 14.0), vec2(40.0, 40.0));
        painter.rect_filled(icon_box, radius(8.0), alpha(entry.color, 0.15));
        draw_icon(&painter, entry.icon, icon_box.center(), 20.0, entry.color);
        painter.text(pos2(icon_box.right() + 14.0, icon_box.center().y), Align2::LEFT_CENTER, entry.label, font(15.0, "h600"), TEXT);
        let id = egui::Id::NULL.with(("setting", entry.key));
        match entry.kind {
            Kind::Toggle => {
                let toggle = Rect::from_min_size(pos2(card.right() - 20.0 - 44.0, icon_box.center().y - 12.0), vec2(44.0, 24.0));
                let response = ui.interact(toggle, id, Sense::click());
                let on = config.flag(entry.key);
                painter.rect_filled(toggle, radius(12.0), if on { TEAL } else { Color32::from_rgb(34, 46, 68) });
                let knob_x = if on { toggle.right() - 12.0 } else { toggle.left() + 12.0 };
                painter.circle_filled(pos2(knob_x, toggle.center().y), 9.0, if on { Color32::BLACK } else { Color32::from_rgb(203, 213, 225) });
                if response.clicked() { config.set(entry.key, i64::from(!on)); }
            }
            Kind::Range { min, max } => {
                let mut value = config.get(entry.key).clamp(min, max);
                let input = Rect::from_min_size(pos2(card.right() - 20.0 - 72.0, icon_box.center().y - 15.0), vec2(72.0, 30.0));
                let mut text = value.to_string();
                let edit = ui.put(input, TextEdit::singleline(&mut text).font(font(13.0, "h600")).align(egui::Align2::CENTER_CENTER).margin(egui::Margin::symmetric(6, 6)));
                if edit.changed() && let Ok(parsed) = text.trim().parse::<i64>() { value = parsed.clamp(min, max); config.set(entry.key, value); }
                let track = Rect::from_min_size(pos2(card.left() + 20.0, card.bottom() - 26.0), vec2(card.width() - 40.0, 8.0));
                let response = ui.interact(track.expand2(vec2(0.0, 8.0)), id, Sense::click_and_drag());
                if let Some(pointer) = response.interact_pointer_pos() {
                    let t = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
                    value = min + ((max - min) as f32 * t).round() as i64;
                    config.set(entry.key, value);
                }
                let t = (value - min) as f32 / (max - min) as f32;
                painter.rect_filled(track, radius(4.0), Color32::from_rgb(34, 46, 68));
                painter.rect_filled(Rect::from_min_max(track.min, pos2(track.left() + track.width() * t, track.bottom())), radius(4.0), TEAL);
                painter.circle_filled(pos2(track.left() + track.width() * t, track.center().y), 8.0, TEAL);
            }
            Kind::Key => {
                let vk = config.get(entry.key);
                let button = Rect::from_min_size(pos2(card.right() - 20.0 - 96.0, icon_box.center().y - 17.0), vec2(96.0, 34.0));
                let response = ui.interact(button, id, Sense::click());
                painter.rect_filled(button, radius(8.0), alpha(TEAL, 0.1));
                painter.rect_stroke(button, radius(8.0), Stroke::new(1.0, alpha(TEAL, 0.3)), StrokeKind::Inside);
                let label = if self.capture_network_key { "Nhấn phím…".to_owned() } else { vk_name(vk) };
                painter.text(button.center(), Align2::CENTER_CENTER, label, font(12.0, "h700"), TEAL);
                if response.clicked() { self.capture_network_key = true; }
                if self.capture_network_key {
                    let pressed = ui.input(|input| input.events.iter().find_map(|event| match event {
                        egui::Event::Key { key, pressed: true, .. } => Some(*key),
                        _ => None,
                    }));
                    if let Some(key) = pressed {
                        self.capture_network_key = false;
                        if key != egui::Key::Escape && let Some(code) = key_to_vk(key) { config.set(entry.key, code); }
                    }
                }
            }
        }
    }

    fn game_card(&mut self, ui: &mut Ui, card: Rect, config: &mut UnlockerConfig, commands: &mut Vec<Command>) {
        let painter = ui.painter().clone();
        painter.rect_filled(card, radius(12.0), CARD);
        painter.rect_stroke(card, radius(12.0), Stroke::new(1.0, line_color()), StrokeKind::Inside);
        let icon_box = Rect::from_min_size(card.min + vec2(14.0, 14.0), vec2(40.0, 40.0));
        painter.rect_filled(icon_box, radius(8.0), alpha(TEAL, 0.15));
        draw_icon(&painter, Icon::Gamepad, icon_box.center(), 20.0, TEAL);
        painter.text(pos2(icon_box.right() + 14.0, icon_box.center().y), Align2::LEFT_CENTER, "Đường dẫn file Game (GenshinImpact.exe)", font(15.0, "h600"), TEXT);
        let exists = Path::new(&config.game_path).is_file();
        let badge = Rect::from_min_size(pos2(card.right() - 20.0 - 112.0, icon_box.center().y - 12.0), vec2(112.0, 24.0));
        let color = if exists { TEAL } else { DANGER };
        painter.rect_filled(badge, radius(8.0), alpha(color, 0.12));
        painter.text(badge.center(), Align2::CENTER_CENTER, if exists { "File sẵn sàng" } else { "Không tìm thấy" }, font(11.0, "h600"), color);
        let browse = Rect::from_min_size(pos2(card.right() - 20.0 - 128.0, card.bottom() - 46.0), vec2(128.0, 34.0));
        if outline_neutral_button(ui, browse, "browse-game", "Duyệt file…", Icon::Folder).clicked() { commands.push(Command::BrowseGame); }
        let input = Rect::from_min_size(pos2(card.left() + 20.0, card.bottom() - 44.0), vec2(card.width() - 40.0 - 140.0, 32.0));
        ui.put(input, TextEdit::singleline(&mut config.game_path).font(font(12.0, "h500")).margin(egui::Margin::symmetric(10, 8)));
    }
}

#[derive(Clone, Copy)]
enum Kind { Toggle, Range { min: i64, max: i64 }, Key }

struct Setting { key: &'static str, label: &'static str, icon: Icon, color: Color32, kind: Kind }

const GREEN: Color32 = Color32::from_rgb(52, 211, 153);
const ORANGE: Color32 = Color32::from_rgb(245, 158, 11);
const RED: Color32 = Color32::from_rgb(244, 63, 94);
const INDIGO: Color32 = Color32::from_rgb(129, 140, 248);
const SLATE: Color32 = Color32::from_rgb(148, 163, 184);

// Thứ tự và nhãn đúng với `src/UI/db.html`.
const SETTINGS: [Setting; 12] = [
    Setting { key: "Vsync", label: "Tắt VSync", icon: Icon::Refresh, color: TEAL, kind: Kind::Toggle },
    Setting { key: "ShowFPS", label: "Hiển thị FPS (Show FPS)", icon: Icon::Speed, color: TEAL, kind: Kind::Toggle },
    Setting { key: "FpsUnlock", label: "Mở khóa FPS", icon: Icon::Speed, color: GREEN, kind: Kind::Toggle },
    Setting { key: "TargetFps", label: "FPS mục tiêu (Target FPS)", icon: Icon::Monitor, color: BLUE, kind: Kind::Range { min: 30, max: 360 } },
    Setting { key: "FovUnlock", label: "Mở khóa FOV (Góc nhìn)", icon: Icon::Focus, color: ORANGE, kind: Kind::Toggle },
    Setting { key: "FovValue", label: "Giá trị FOV (FovValue)", icon: Icon::Zoom, color: ORANGE, kind: Kind::Range { min: 30, max: 120 } },
    Setting { key: "RemoveTeamAnim", label: "Đổi nhân vật nhanh", icon: Icon::Groups, color: PURPLE, kind: Kind::Toggle },
    Setting { key: "DisableCameraMove", label: "Bỏ zoom khi dùng nộ", icon: Icon::CameraOff, color: RED, kind: Kind::Toggle },
    Setting { key: "DisableFog", label: "Tắt sương mù (Disable Fog)", icon: Icon::CloudOff, color: INDIGO, kind: Kind::Toggle },
    Setting { key: "HideUID", label: "Ẩn UID (Hide UID)", icon: Icon::EyeOff, color: SLATE, kind: Kind::Toggle },
    Setting { key: "EnableNetworkToggle", label: "Bật/Tắt ngắt mạng", icon: Icon::WifiOff, color: RED, kind: Kind::Toggle },
    Setting { key: "NetworkToggleKey", label: "Phím tắt ngắt mạng", icon: Icon::Keyboard, color: TEAL, kind: Kind::Key },
];

fn vk_name(vk: i64) -> String {
    match vk {
        112..=135 => format!("F{}", vk - 111),
        48..=57 | 65..=90 => char::from_u32(vk as u32).map(String::from).unwrap_or_default(),
        45 => "Insert".into(), 46 => "Delete".into(), 36 => "Home".into(), 35 => "End".into(), 33 => "PageUp".into(), 34 => "PageDown".into(),
        other => format!("VK {other}"),
    }
}

fn key_to_vk(key: egui::Key) -> Option<i64> {
    use egui::Key::*;
    let table: [(egui::Key, i64); 12] = [(F1, 112), (F2, 113), (F3, 114), (F4, 115), (F5, 116), (F6, 117), (F7, 118), (F8, 119), (F9, 120), (F10, 121), (F11, 122), (F12, 123)];
    if let Some((_, vk)) = table.iter().find(|(k, _)| *k == key) { return Some(*vk); }
    match key {
        Insert => Some(45), Delete => Some(46), Home => Some(36), End => Some(35), PageUp => Some(33), PageDown => Some(34),
        _ => {
            let name = key.name();
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase() as i64),
                _ => None,
            }
        }
    }
}

fn key_pill(painter: &Painter, right_center: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), font(12.0, "mont"), TEAL);
    let rect = Rect::from_min_size(pos2(right_center.x - galley.size().x - 24.0, right_center.y - 12.5), vec2(galley.size().x + 24.0, 25.0));
    painter.rect_filled(rect, radius(6.0), alpha(TEAL, 0.1));
    painter.rect_stroke(rect, radius(6.0), Stroke::new(1.0, alpha(TEAL, 0.4)), StrokeKind::Inside);
    painter.galley(rect.min + vec2(12.0, (25.0 - galley.size().y) / 2.0), galley, TEAL);
}

fn vertical_gradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// Nút viền màu của sidebar (RUN Macro, START Game).
fn outlined_button(ui: &mut Ui, rect: Rect, id: &str, label: &str, icon: Icon, text: Color32, border: Color32) -> egui::Response {
    let response = ui.interact(rect, egui::Id::NULL.with(id), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, radius(12.0), alpha(border, if response.hovered() { 0.16 } else { 0.07 }));
    painter.rect_stroke(rect, radius(12.0), Stroke::new(1.0, alpha(border, 0.45)), StrokeKind::Inside);
    let ink = alpha(text, 0.85);
    painter.circle_filled(pos2(rect.left() + 56.0, rect.center().y), 2.5, alpha(text, 0.5));
    draw_icon(painter, icon, pos2(rect.left() + 76.0, rect.center().y), 14.0, ink);
    painter.text(pos2(rect.left() + 92.0, rect.center().y), Align2::LEFT_CENTER, label, font(13.0, "mont"), ink);
    response
}

fn solid_button(ui: &mut Ui, rect: Rect, id: &str, label: &str, icon: Icon) -> egui::Response {
    let response = ui.interact(rect, egui::Id::NULL.with(id), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, radius(12.0), if response.hovered() { TEAL } else { TEAL_BTN });
    let galley = painter.layout_no_wrap(label.to_owned(), font(14.0, "h700"), Color32::BLACK);
    let total = galley.size().x + 26.0;
    let start = rect.center().x - total / 2.0;
    draw_icon(painter, icon, pos2(start + 9.0, rect.center().y), 18.0, Color32::BLACK);
    painter.galley(pos2(start + 26.0, rect.center().y - galley.size().y / 2.0), galley, Color32::BLACK);
    response
}

fn outline_neutral_button(ui: &mut Ui, rect: Rect, id: &str, label: &str, icon: Icon) -> egui::Response {
    let response = ui.interact(rect, egui::Id::NULL.with(id), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, radius(12.0), if response.hovered() { ACTIVE } else { alpha(Color32::WHITE, 0.03) });
    painter.rect_stroke(rect, radius(12.0), Stroke::new(1.0, line_color()), StrokeKind::Inside);
    let galley = painter.layout_no_wrap(label.to_owned(), font(14.0, "h600"), TEXT);
    let total = galley.size().x + 26.0;
    let start = rect.center().x - total / 2.0;
    draw_icon(painter, icon, pos2(start + 9.0, rect.center().y), 17.0, MUTED);
    painter.galley(pos2(start + 26.0, rect.center().y - galley.size().y / 2.0), galley, TEXT);
    response
}

#[derive(Clone, Copy)]
enum Icon { Folder, Home, Grid, Tune, Play, Gamepad, Plus, Refresh, Keyboard, Pencil, Trash, Save, Bell, Camera, Speed, Monitor, Focus, Zoom, Groups, CameraOff, CloudOff, EyeOff, WifiOff }

/// Biểu tượng vẽ bằng nét, thay cho Material Symbols (font đó dùng ligature, egui không hiển thị được).
fn draw_icon(painter: &Painter, icon: Icon, c: Pos2, size: f32, color: Color32) {
    let s = size / 24.0;
    let p = |x: f32, y: f32| pos2(c.x + (x - 12.0) * s, c.y + (y - 12.0) * s);
    let stroke = Stroke::new((1.7 * s).max(1.2), color);
    let line = |points: &[Pos2]| { painter.add(Shape::line(points.to_vec(), stroke)); };
    let ring = |x: f32, y: f32, r: f32| { painter.circle_stroke(p(x, y), r * s, stroke); };
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| {
        let points: Vec<Pos2> = (0..=24).map(|i| {
            let a = (from + (to - from) * i as f32 / 24.0).to_radians();
            p(cx + r * a.cos(), cy + r * a.sin())
        }).collect();
        painter.add(Shape::line(points, stroke));
    };
    match icon {
        Icon::Folder => { line(&[p(3.0, 7.0), p(3.0, 19.0), p(21.0, 19.0), p(21.0, 9.0), p(11.0, 9.0), p(9.0, 6.0), p(3.0, 6.0), p(3.0, 7.0)]); }
        Icon::Home => { line(&[p(3.0, 11.0), p(12.0, 3.5), p(21.0, 11.0)]); line(&[p(5.5, 9.5), p(5.5, 20.0), p(10.0, 20.0), p(10.0, 14.0), p(14.0, 14.0), p(14.0, 20.0), p(18.5, 20.0), p(18.5, 9.5)]); }
        Icon::Grid => { for x in [6.0, 12.0, 18.0] { for y in [6.0, 12.0, 18.0] { painter.circle_filled(p(x, y), 1.9 * s, color); } } }
        Icon::Tune => { for (y, x) in [(7.0, 9.0), (12.0, 15.0), (17.0, 8.0)] { line(&[p(4.0, y), p(20.0, y)]); painter.circle_filled(p(x, y), 2.4 * s, color); } }
        Icon::Play => { painter.add(Shape::convex_polygon(vec![p(8.0, 5.0), p(19.0, 12.0), p(8.0, 19.0)], color, Stroke::NONE)); }
        Icon::Gamepad => {
            painter.rect_stroke(Rect::from_min_max(p(2.5, 7.0), p(21.5, 17.0)), radius(4.0 * s), stroke, StrokeKind::Inside);
            line(&[p(7.0, 10.0), p(7.0, 14.0)]); line(&[p(5.0, 12.0), p(9.0, 12.0)]);
            painter.circle_filled(p(15.5, 10.5), 1.2 * s, color); painter.circle_filled(p(18.0, 13.0), 1.2 * s, color);
        }
        Icon::Plus => { line(&[p(12.0, 5.0), p(12.0, 19.0)]); line(&[p(5.0, 12.0), p(19.0, 12.0)]); }
        Icon::Refresh => { arc(12.0, 12.0, 7.5, -40.0, 250.0); line(&[p(18.0, 3.5), p(18.7, 8.0), p(14.2, 8.4)]); }
        Icon::Keyboard => {
            painter.rect_stroke(Rect::from_min_max(p(2.5, 6.0), p(21.5, 18.0)), radius(2.5 * s), stroke, StrokeKind::Inside);
            for x in [6.5, 10.0, 13.5, 17.0] { painter.circle_filled(p(x, 10.0), 0.9 * s, color); }
            line(&[p(7.0, 14.5), p(17.0, 14.5)]);
        }
        Icon::Pencil => { line(&[p(4.0, 20.0), p(5.0, 15.5), p(16.0, 4.5), p(19.5, 8.0), p(8.5, 19.0), p(4.0, 20.0)]); line(&[p(14.0, 6.5), p(17.5, 10.0)]); }
        Icon::Trash => { line(&[p(4.0, 7.0), p(20.0, 7.0)]); line(&[p(9.0, 7.0), p(9.0, 4.0), p(15.0, 4.0), p(15.0, 7.0)]); line(&[p(6.0, 7.0), p(7.0, 20.0), p(17.0, 20.0), p(18.0, 7.0)]); line(&[p(10.0, 11.0), p(10.0, 16.0)]); line(&[p(14.0, 11.0), p(14.0, 16.0)]); }
        Icon::Save => { line(&[p(4.0, 4.0), p(17.0, 4.0), p(20.0, 7.0), p(20.0, 20.0), p(4.0, 20.0), p(4.0, 4.0)]); line(&[p(8.0, 4.0), p(8.0, 9.0), p(15.0, 9.0), p(15.0, 4.0)]); ring(12.0, 15.0, 2.5); }
        Icon::Bell => { line(&[p(6.0, 17.0), p(6.0, 11.0)]); arc(12.0, 11.0, 6.0, 180.0, 360.0); line(&[p(18.0, 11.0), p(18.0, 17.0), p(20.0, 18.5), p(4.0, 18.5), p(6.0, 17.0)]); line(&[p(10.0, 21.0), p(14.0, 21.0)]); }
        Icon::Camera => { painter.rect_stroke(Rect::from_min_max(p(3.0, 7.0), p(21.0, 19.0)), radius(2.5 * s), stroke, StrokeKind::Inside); line(&[p(8.0, 7.0), p(9.5, 4.5), p(14.5, 4.5), p(16.0, 7.0)]); ring(12.0, 13.0, 3.3); }
        Icon::Speed => { arc(12.0, 14.0, 8.5, 150.0, 390.0); line(&[p(12.0, 14.0), p(16.5, 8.5)]); }
        Icon::Monitor => { painter.rect_stroke(Rect::from_min_max(p(3.0, 4.5), p(21.0, 16.0)), radius(2.0 * s), stroke, StrokeKind::Inside); line(&[p(8.0, 20.0), p(16.0, 20.0)]); line(&[p(12.0, 16.0), p(12.0, 20.0)]); }
        Icon::Focus => { for (a, b, d) in [(4.0, 4.0, 1.0), (20.0, 4.0, -1.0)] { line(&[p(a, b + 5.0), p(a, b), p(a + 5.0 * d, b)]); } for (a, b, d) in [(4.0, 20.0, 1.0), (20.0, 20.0, -1.0)] { line(&[p(a, b - 5.0), p(a, b), p(a + 5.0 * d, b)]); } ring(12.0, 12.0, 3.2); }
        Icon::Zoom => { ring(10.5, 10.5, 6.5); line(&[p(15.5, 15.5), p(21.0, 21.0)]); line(&[p(10.5, 7.5), p(10.5, 13.5)]); line(&[p(7.5, 10.5), p(13.5, 10.5)]); }
        Icon::Groups => { ring(12.0, 8.0, 3.0); ring(5.0, 10.0, 2.3); ring(19.0, 10.0, 2.3); arc(12.0, 20.0, 6.5, 200.0, 340.0); }
        Icon::CameraOff => { painter.rect_stroke(Rect::from_min_max(p(3.0, 7.0), p(16.0, 17.0)), radius(2.0 * s), stroke, StrokeKind::Inside); line(&[p(16.0, 11.0), p(21.0, 8.0), p(21.0, 16.0), p(16.0, 13.0)]); line(&[p(3.0, 3.0), p(21.0, 21.0)]); }
        Icon::CloudOff => { arc(9.5, 14.0, 5.0, 100.0, 270.0); arc(14.5, 11.0, 5.5, 270.0, 420.0); line(&[p(5.0, 19.0), p(17.0, 19.0)]); line(&[p(3.0, 3.0), p(21.0, 21.0)]); }
        Icon::EyeOff => { arc(12.0, 17.0, 10.0, 235.0, 305.0); arc(12.0, 7.0, 10.0, 55.0, 125.0); ring(12.0, 12.0, 3.0); line(&[p(3.0, 3.0), p(21.0, 21.0)]); }
        Icon::WifiOff => { arc(12.0, 19.0, 6.0, 225.0, 315.0); arc(12.0, 19.0, 11.0, 225.0, 315.0); painter.circle_filled(p(12.0, 19.0), 1.4 * s, color); line(&[p(3.0, 3.0), p(21.0, 21.0)]); }
    }
}
