use eframe::egui;
use eframe::egui::{
    Align, Align2, Button, Color32, Context, CornerRadius, Event, Frame, Id, Key, Layout, Margin,
    Modal, PointerButton, Rect, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, vec2,
};
use serde_json::{Map, Value, json};

pub(super) const BACKGROUND: Color32 = Color32::from_rgb(9, 13, 22);
pub(super) const PANEL: Color32 = Color32::from_rgb(24, 33, 51);
pub(super) const SURFACE: Color32 = Color32::from_rgb(13, 19, 31);
pub(super) const BORDER: Color32 = Color32::from_rgb(48, 59, 80);
pub(super) const TEXT: Color32 = Color32::from_rgb(231, 237, 248);
pub(super) const MUTED: Color32 = Color32::from_rgb(143, 160, 186);
pub(super) const TEAL: Color32 = Color32::from_rgb(45, 212, 191);
pub(super) const PURPLE: Color32 = Color32::from_rgb(167, 139, 250);
pub(super) const WARNING: Color32 = Color32::from_rgb(251, 191, 36);
pub(super) const DANGER: Color32 = Color32::from_rgb(253, 164, 175);
const ACTION_GATE: &str = "is_no_key_pressed";

struct Action {
    label: &'static str,
    function: &'static str,
}

const ACTIONS: [Action; 11] = [
    Action {
        label: "E",
        function: "skke",
    },
    Action {
        label: "n2",
        function: "skk2a",
    },
    Action {
        label: "n2d",
        function: "skk2as",
    },
    Action {
        label: "n2c",
        function: "skk2az",
    },
    Action {
        label: "n2cd",
        function: "skk2azs",
    },
    Action {
        label: "n2cd_slow",
        function: "skk2azs_slow",
    },
    Action {
        label: "n2q",
        function: "skk2aq",
    },
    Action {
        label: "n3d",
        function: "skk3as",
    },
    Action {
        label: "n3w",
        function: "skk3aw",
    },
    Action {
        label: "n5d",
        function: "skk5as",
    },
    Action {
        label: "n5",
        function: "skk5a",
    },
];

#[derive(Clone, Copy)]
enum DragAction {
    Palette(&'static Action),
    Timeline(usize),
}

pub enum EditorAction {
    Save(Value),
    Cancel,
    /// Làm lại một combo trắng ngay trong editor, như nút Combo mới của bản gốc.
    NewCombo,
    /// Chuyển sang editor chuyên sâu, mở thẳng hộp thoại Import / Export khi `import`.
    Advanced { import: bool },
}

pub struct Editor {
    id: String,
    original: Map<String, Value>,
    name: String,
    hotkey: Option<String>,
    timeline: Vec<&'static Action>,
    selected: Option<usize>,
    capturing_hotkey: bool,
    unsupported_hotkey: bool,
    error: Option<String>,
}

impl Editor {
    pub fn new(id: String, existing: Option<&Value>) -> Result<Self, String> {
        let original = match existing {
            Some(profile) => profile
                .as_object()
                .cloned()
                .ok_or("Dữ liệu combo phải là một đối tượng JSON.")?,
            None => Map::new(),
        };
        if original.get("clips").is_some_and(Value::is_array)
            || ["timeline", "pythonSequence"].into_iter().any(|key| {
                original
                    .get(key)
                    .and_then(Value::as_array)
                    .is_some_and(|steps| {
                        steps.iter().any(|step| step.is_object() || step.is_array())
                    })
            })
        {
            return Err(
                "Combo chuyên sâu cần được chỉnh sửa trong trình chỉnh sửa chuyên sâu.".into(),
            );
        }

        let sequence = profile_steps(&original, "pythonSequence")?;
        let timeline = profile_steps(&original, "timeline")?;
        let steps: Vec<_> = match timeline {
            Some(steps) => steps.iter().collect(),
            None => sequence
                .into_iter()
                .flatten()
                .filter(|step| step.as_str() != Some(ACTION_GATE))
                .collect(),
        };
        let timeline: Vec<_> = steps
            .into_iter()
            .map(|step| {
                ACTIONS
                    .iter()
                    .find(|action| Some(action.function) == step.as_str())
                    .ok_or_else(|| {
                        format!("Action không được hỗ trợ trong timeline cơ bản: {step}")
                    })
            })
            .collect::<Result<_, _>>()?;
        if let Some(sequence) = sequence
            && !sequence.is_empty()
            && !sequence
                .iter()
                .map(Value::as_str)
                .eq(python_sequence(&timeline).into_iter().map(Some))
        {
            return Err("Python sequence không tương thích với timeline cơ bản.".into());
        }

        Ok(Self {
            id: profile_field(&original, "id")?
                .filter(|id| !id.is_empty())
                .unwrap_or(id),
            name: profile_field(&original, "name")?.unwrap_or_default(),
            hotkey: profile_field(&original, "hotkey")?.filter(|key| !key.is_empty()),
            original,
            timeline,
            selected: None,
            capturing_hotkey: false,
            unsupported_hotkey: false,
            error: None,
        })
    }

    /// Chuyển timeline cơ bản thành clip cho editor chuyên sâu.
    pub fn to_advanced(&self, fps: f64, import: bool) -> super::advanced::Advanced {
        let functions: Vec<&str> = self.timeline.iter().map(|action| action.function).collect();
        let mut advanced = super::advanced::Advanced::with_basic(self.id.clone(), self.name.clone(), self.hotkey.clone(), &functions, fps);
        if import { advanced.open_import(); }
        advanced
    }

    pub fn show(&mut self, ctx: &Context) -> Option<EditorAction> {
        self.capture_hotkey(ctx);
        let out = shell(ctx, |ui, _| self.editor_ui(ui));
        let mut action = out.inner;
        if action.is_none() && out.close {
            action = Some(EditorAction::Cancel);
        }
        if action.is_none()
            && !self.capturing_hotkey
            && out.top
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            action = Some(EditorAction::Cancel);
        }
        action
    }

    fn editor_ui(&mut self, ui: &mut Ui) -> Option<EditorAction> {
        let mut result = None;
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("SKIRK MACRO / CUSTOM COMBO")
                        .color(TEAL)
                        .size(11.0)
                        .family(egui::FontFamily::Name("h700".into())),
                );
                ui.label(
                    RichText::new(if self.original.is_empty() {
                        "Tạo Custom Combo"
                    } else {
                        "Chỉnh sửa Custom Combo"
                    })
                    .size(36.0)
                    .family(egui::FontFamily::Name("h700".into())),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let ghost = Color32::from_rgb(27, 39, 60);
                for (label, outcome) in [
                    ("Chuyên sâu", EditorAction::Advanced { import: false }),
                    ("Combo mới", EditorAction::NewCombo),
                    ("Import / Export", EditorAction::Advanced { import: true }),
                ] {
                    if ui.add(button(label, Color32::from_rgb(215, 229, 247), ghost, BORDER).min_size(vec2(0.0, 40.0))).clicked() {
                        result = Some(outcome);
                    }
                }
            });
        });
        ui.label(
            RichText::new("Kéo action từ palette vào timeline, sắp xếp rồi lưu.").color(MUTED),
        );
        ui.add_space(8.0);
        frame(PANEL, BORDER, 18, 22).show(ui, |ui| self.palette_ui(ui));
        ui.add_space(8.0);
        frame(PANEL, BORDER, 18, 22).show(ui, |ui| {
            self.timeline_ui(ui);
            ui.add_space(12.0);
            let saved = self.form_ui(ui);
            result = result.take().or(saved);
        });
        result
    }

    fn palette_ui(&mut self, ui: &mut Ui) {
        eyebrow(ui, "VÙNG 1");
        panel_heading(ui, "Action palette", ACTIONS.len());
        ui.label(RichText::new("Kéo để thêm action. Palette luôn giữ nguyên.").color(MUTED));
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
            for action in &ACTIONS {
                let chip = chip(ui, action.label, true, false, false);
                let response = chip.body.on_hover_text(format!("Thêm {}", action.label));
                response.dnd_set_drag_payload(DragAction::Palette(action));
                if response.clicked() {
                    self.drop_action(DragAction::Palette(action), self.timeline.len());
                }
            }
        });
    }

    fn timeline_ui(&mut self, ui: &mut Ui) {
        eyebrow(ui, "VÙNG 2");
        panel_heading(ui, "Timeline", self.timeline.len());
        ui.label(
            RichText::new("Kéo để đổi thứ tự. Chọn action rồi nhấn Delete để xóa.").color(MUTED),
        );
        ui.add_space(6.0);
        let mut items = Vec::new();
        let mut remove = None;
        let surface = frame(Color32::from_rgb(8, 13, 23), Color32::from_rgba_unmultiplied(148, 163, 184, 48), 14, 12)
            .show(ui, |ui| {
                let width = ui.available_width();
                ui.set_min_size(vec2(width, 124.0));
                if self.timeline.is_empty() {
                    let (rect, _) = ui.allocate_exact_size(vec2(width, 124.0), Sense::hover());
                    for (text, offset, size, color) in [
                        ("+", -25.0, 25.0, TEAL),
                        ("Timeline đang trống", 1.0, 15.0, MUTED),
                        ("Kéo action vào đây để bắt đầu.", 23.0, 12.0, MUTED),
                    ] {
                        ui.painter().text(
                            rect.center() + vec2(0.0, offset),
                            Align2::CENTER_CENTER,
                            text,
                            egui::FontId::proportional(size),
                            color,
                        );
                    }
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                        for (index, action) in self.timeline.iter().enumerate() {
                            let chip = chip(ui, action.label, false, self.selected == Some(index), true);
                            chip.body.dnd_set_drag_payload(DragAction::Timeline(index));
                            if chip.body.clicked() || chip.body.drag_started() {
                                self.selected = Some(index);
                            }
                            if chip.close.as_ref().is_some_and(egui::Response::clicked) {
                                remove = Some(index);
                            }
                            items.push(chip.rect);
                        }
                    });
                }
            })
            .response;
        let pointer = ui.ctx().pointer_interact_pos();
        // The surface only senses hover, so `contains_pointer` is unreliable while another widget is dragged.
        let over = pointer.filter(|pointer| surface.rect.contains(*pointer));
        let ctx = ui.ctx().clone();
        if over.is_some()
            && ui.input(|input| input.pointer.any_released())
            && let Some(drag) = egui::DragAndDrop::take_payload::<DragAction>(&ctx)
        {
            self.drop_action(*drag, insertion_index(&items, pointer));
        } else if over.is_some() && egui::DragAndDrop::payload::<DragAction>(&ctx).is_some()
            && let Some(pointer) = pointer
        {
            let insertion = insertion_index(&items, Some(pointer));
            let edge = items
                .get(insertion)
                .map(|item| (item.left() - 4.0, *item))
                .or_else(|| items.last().map(|item| (item.right() + 4.0, *item)));
            if let Some((x, item)) = edge {
                ui.painter().line_segment(
                    [egui::pos2(x, item.top()), egui::pos2(x, item.bottom())],
                    Stroke::new(2.0, TEAL),
                );
            } else {
                ui.painter().rect_stroke(
                    surface.rect,
                    14,
                    Stroke::new(1.0, TEAL),
                    egui::StrokeKind::Inside,
                );
            }
        }
        if let Some(index) = remove {
            self.remove_action(index);
        }
        if let Some(index) = self.selected {
            ui.horizontal(|ui| {
                let earlier = ui
                    .add_enabled(index > 0, button("←  Trước", TEXT, SURFACE, BORDER))
                    .on_hover_text("Di chuyển action về trước");
                if earlier.clicked() {
                    self.drop_action(DragAction::Timeline(index), index - 1);
                }
                let later = ui
                    .add_enabled(
                        index + 1 < self.timeline.len(),
                        button("Sau  →", TEXT, SURFACE, BORDER),
                    )
                    .on_hover_text("Di chuyển action về sau");
                if later.clicked() {
                    self.drop_action(DragAction::Timeline(index), index + 2);
                }
            });
        }
    }

    fn form_ui(&mut self, ui: &mut Ui) -> Option<EditorAction> {
        ui.label(RichText::new("Tên Combo").family(egui::FontFamily::Name("h700".into())));
        let name = ui.add(
            TextEdit::singleline(&mut self.name)
                .hint_text("Ví dụ: Skirk Combo 1")
                .char_limit(80)
                .desired_width(f32::INFINITY)
                .min_size(vec2(0.0, 40.0))
                .margin(Margin::symmetric(12, 8))
                .background_color(SURFACE),
        );
        if name.changed() {
            self.error = None;
        }
        if !name.has_focus()
            && !self.capturing_hotkey
            && let Some(index) = self.selected
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Delete))
        {
            self.remove_action(index);
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("Hotkey").family(egui::FontFamily::Name("h700".into())));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("Không bắt buộc").size(12.0).color(MUTED));
            });
        });
        ui.horizontal(|ui| {
            let label = if self.capturing_hotkey {
                "Nhấn phím hoặc nút chuột… Esc để hủy".into()
            } else if let Some(hotkey) = &self.hotkey {
                hotkey_label(hotkey)
            } else {
                "Nhấn để gán phím hoặc nút chuột".into()
            };
            let outline = if self.capturing_hotkey {
                WARNING
            } else if self.hotkey.is_some() {
                PURPLE
            } else {
                BORDER
            };
            let width = (ui.available_width() - 42.0).max(85.0);
            if ui
                .add(button(&label, TEXT, SURFACE, outline).min_size(vec2(width, 38.0)))
                .clicked()
            {
                self.capturing_hotkey = !self.capturing_hotkey;
                self.unsupported_hotkey = false;
            }
            if ui
                .add(button("×", TEXT, SURFACE, BORDER).min_size(vec2(32.0, 38.0)))
                .on_hover_text("Xóa hotkey")
                .clicked()
            {
                self.hotkey = None;
                self.capturing_hotkey = false;
                self.unsupported_hotkey = false;
            }
        });
        if self.unsupported_hotkey {
            ui.label(RichText::new("Phím này không thể dùng làm hotkey.").color(DANGER));
        }

        frame(SURFACE, BORDER, 9, 10).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.collapsing("Python sequence xem trước", |ui| {
                if self.timeline.is_empty() {
                    ui.label(RichText::new("Chưa có action.").color(MUTED));
                } else {
                    let preview = python_sequence(&self.timeline)
                        .into_iter()
                        .map(|function| format!("{function}()"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        ui.label(RichText::new(preview).monospace().color(TEAL).size(12.0));
                    });
                }
            });
        });
        let mut result = None;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(button("Save Combo", BACKGROUND, TEAL, TEAL).min_size(vec2(112.0, 40.0)))
                .clicked()
            {
                match self.profile() {
                    Ok(profile) => result = Some(EditorAction::Save(profile)),
                    Err(error) => {
                        if self.name.trim().is_empty() {
                            name.request_focus();
                        }
                        self.error = Some(error);
                    }
                }
            }
            if let Some(error) = &self.error {
                ui.label(RichText::new(error).color(DANGER));
            }
        });
        result
    }

    fn capture_hotkey(&mut self, ctx: &Context) {
        if !self.capturing_hotkey {
            return;
        }
        let captured = ctx.input_mut(|input| {
            let capture = input.events.iter().enumerate().find_map(|(index, event)| {
                let key = match event {
                    Event::Key {
                        key,
                        physical_key,
                        pressed: true,
                        repeat: false,
                        ..
                    } => {
                        let physical = physical_key.unwrap_or(*key);
                        if physical.name().len() == 1
                            && physical.name().as_bytes()[0].is_ascii_alphanumeric()
                        {
                            Some(physical)
                        } else {
                            Some(*key)
                        }
                    }
                    Event::PointerButton {
                        button,
                        pressed: true,
                        ..
                    } => {
                        let name = match button {
                            PointerButton::Middle => "middle",
                            PointerButton::Extra1 => "mouse_3",
                            PointerButton::Extra2 => "mouse_4",
                            PointerButton::Primary | PointerButton::Secondary => return None,
                        };
                        return Some((index, Ok(Some(name.to_owned()))));
                    }
                    _ => None,
                }?;
                let value = if key == Key::Escape {
                    Ok(None)
                } else {
                    keyboard_hotkey(key).map(Some).ok_or(())
                };
                Some((index, value))
            });
            capture.map(|(index, value)| {
                input.events.remove(index);
                value
            })
        });
        match captured {
            Some(Ok(key)) => {
                if let Some(key) = key {
                    self.hotkey = Some(key);
                }
                self.capturing_hotkey = false;
                self.unsupported_hotkey = false;
            }
            Some(Err(())) => self.unsupported_hotkey = true,
            None => {}
        }
    }

    fn remove_action(&mut self, index: usize) {
        self.timeline.remove(index);
        self.selected = None;
        self.error = None;
    }

    fn drop_action(&mut self, drag: DragAction, destination: usize) {
        let (action, destination) = match drag {
            DragAction::Palette(action) => (action, destination),
            DragAction::Timeline(index) if index < self.timeline.len() => (
                self.timeline.remove(index),
                destination - usize::from(destination > index),
            ),
            DragAction::Timeline(_) => return,
        };
        self.timeline.insert(destination, action);
        self.selected = Some(destination);
        self.error = None;
    }

    fn profile(&self) -> Result<Value, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Nhập Tên Combo trước khi lưu.".into());
        }
        if self.timeline.is_empty() {
            return Err("Timeline cần ít nhất một action.".into());
        }
        let mut profile = self.original.clone();
        profile.insert("id".into(), json!(self.id));
        profile.insert("name".into(), json!(name));
        profile.insert("hotkey".into(), json!(self.hotkey));
        profile.insert(
            "timeline".into(),
            json!(
                self.timeline
                    .iter()
                    .map(|action| action.function)
                    .collect::<Vec<_>>()
            ),
        );
        profile.insert(
            "pythonSequence".into(),
            json!(python_sequence(&self.timeline)),
        );
        Ok(Value::Object(profile))
    }
}

fn profile_steps<'a>(
    profile: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a Vec<Value>>, String> {
    match profile.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(steps)) => Ok(Some(steps)),
        Some(_) => Err(format!("Trường {key} của combo phải là một danh sách.")),
    }
}

fn profile_field(profile: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match profile.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("Trường {key} của combo phải là văn bản.")),
    }
}

fn python_sequence(timeline: &[&Action]) -> Vec<&'static str> {
    let mut sequence = Vec::with_capacity(timeline.len().saturating_mul(2));
    for (index, action) in timeline.iter().enumerate() {
        if index > 0 {
            sequence.push(ACTION_GATE);
        }
        sequence.push(action.function);
    }
    sequence
}

fn insertion_index(items: &[Rect], pointer: Option<egui::Pos2>) -> usize {
    pointer
        .and_then(|pointer| {
            items.iter().position(|item| {
                let center = item.center();
                pointer.y < item.top() || (pointer.y <= item.bottom() && pointer.x < center.x)
            })
        })
        .unwrap_or(items.len())
}


/// Đường viền hình chữ nhật bo góc (dùng cho nền gradient và viền nét đứt).
fn rounded_outline(rect: Rect, radius: f32) -> Vec<egui::Pos2> {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let mut points = Vec::new();
    for (corner, start) in [
        (egui::pos2(rect.right() - r, rect.top() + r), -90.0_f32),
        (egui::pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (egui::pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (egui::pos2(rect.left() + r, rect.top() + r), 180.0),
    ] {
        for step in 0..=6 {
            let angle = (start + 15.0 * step as f32).to_radians();
            points.push(corner + vec2(angle.cos(), angle.sin()) * r);
        }
    }
    points
}

/// Nền gradient chéo như `linear-gradient(145deg, …)` của bản gốc.
fn gradient_rounded(painter: &egui::Painter, rect: Rect, radius: f32, from: Color32, to: Color32) {
    let outline = rounded_outline(rect, radius);
    let mut mesh = egui::Mesh::default();
    let color_at = |p: egui::Pos2| {
        let t = (((p.x - rect.left()) / rect.width().max(1.0)) * 0.45 + ((p.y - rect.top()) / rect.height().max(1.0)) * 0.55).clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Color32::from_rgba_unmultiplied(lerp(from.r(), to.r()), lerp(from.g(), to.g()), lerp(from.b(), to.b()), lerp(from.a(), to.a()))
    };
    mesh.colored_vertex(rect.center(), color_at(rect.center()));
    for point in &outline { mesh.colored_vertex(*point, color_at(*point)); }
    let count = outline.len() as u32;
    for i in 0..count { mesh.add_triangle(0, 1 + i, 1 + (i + 1) % count); }
    painter.add(egui::Shape::mesh(mesh));
}

pub(super) struct Chip {
    pub body: egui::Response,
    pub close: Option<egui::Response>,
    pub rect: Rect,
}

/// Chip action của bản gốc: một khối liền gồm dấu kéo, tên và (nếu ở timeline) nút ×.
pub(super) fn chip(ui: &mut Ui, label: &str, palette: bool, selected: bool, removable: bool) -> Chip {
    let ink = if palette { Color32::from_rgb(238, 234, 255) } else { Color32::from_rgb(216, 255, 251) };
    let name = ui.painter().layout_no_wrap(label.to_owned(), egui::FontId::new(16.0, egui::FontFamily::Name("h600".into())), ink);
    let grip = ui.painter().layout_no_wrap("••".to_owned(), egui::FontId::new(11.8, egui::FontFamily::Name("h700".into())), Color32::from_rgba_unmultiplied(221, 214, 254, 166));
    let left = if palette { 14.0 } else { 13.0 };
    let width = left + grip.size().x + 9.0 + name.size().x + if removable { 9.0 + 26.0 + 8.0 } else { 14.0 };
    let (rect, body) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click_and_drag());
    let hovered = body.hovered() || body.dragged();
    let painter = ui.painter();
    let (from, to, edge) = if palette {
        (Color32::from_rgba_unmultiplied(104, 76, 168, 150), Color32::from_rgba_unmultiplied(42, 32, 75, 215), Color32::from_rgba_unmultiplied(167, 139, 250, if hovered { 255 } else { 92 }))
    } else {
        (Color32::from_rgba_unmultiplied(11, 94, 92, 170), Color32::from_rgba_unmultiplied(15, 41, 51, 245), Color32::from_rgba_unmultiplied(45, 212, 191, if hovered || selected { 255 } else { 90 }))
    };
    if selected || (hovered && !palette) {
        painter.rect_filled(rect.expand(2.0), CornerRadius::same(13), Color32::from_rgba_unmultiplied(45, 212, 191, 30));
    }
    gradient_rounded(painter, rect, 11.0, from, to);
    painter.line_segment([rect.left_top() + vec2(8.0, 1.5), rect.right_top() + vec2(-8.0, 1.5)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 16)));
    painter.rect_stroke(rect, CornerRadius::same(11), Stroke::new(if selected { 1.6 } else { 1.0 }, edge), egui::StrokeKind::Inside);
    painter.galley(egui::pos2(rect.left() + left, rect.center().y - grip.size().y / 2.0), grip.clone(), ink);
    painter.galley(egui::pos2(rect.left() + left + grip.size().x + 9.0, rect.center().y - name.size().y / 2.0), name, ink);
    let close = removable.then(|| {
        let x = Rect::from_center_size(egui::pos2(rect.right() - 8.0 - 13.0, rect.center().y), vec2(26.0, 26.0));
        let response = ui.interact(x, body.id.with("remove"), Sense::click());
        let painter = ui.painter();
        let hot = response.hovered();
        painter.rect_filled(x, CornerRadius::same(7), if hot { Color32::from_rgba_unmultiplied(244, 63, 94, 184) } else { Color32::from_rgba_unmultiplied(2, 15, 22, 97) });
        let c = x.center();
        let stroke = Stroke::new(1.6, if hot { Color32::WHITE } else { Color32::from_rgb(215, 249, 245) });
        painter.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], stroke);
        painter.line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, -4.0)], stroke);
        response.on_hover_text("Xóa action")
    });
    Chip { body, close, rect }
}

pub(super) struct Shell<R> {
    pub inner: R,
    pub close: bool,
    pub top: bool,
}

/// Khung modal chung của hai editor, như `#comboModal` của bản gốc: thanh tiêu đề cố định có nút ×,
/// nội dung đổi bên trong. Dùng cùng một khung nên chuyển Cơ bản ⇄ Chuyên sâu không bị nhảy.
pub(super) fn shell<R>(ctx: &Context, body: impl FnOnce(&mut Ui, f32) -> R) -> Shell<R> {
    const BAR: f32 = 52.0;
    let width = (ctx.content_rect().width() - 56.0).clamp(300.0, 1040.0);
    let height = (ctx.content_rect().height() - 56.0).max(200.0);
    let mut close = false;
    let modal = Modal::new(Id::unique("combo-editor-shell"))
        .frame(Frame::new().fill(BACKGROUND).stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(16)).inner_margin(Margin::ZERO))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.visuals_mut().override_text_color = Some(TEXT);
            ui.spacing_mut().item_spacing = vec2(9.0, 8.0);
            let (bar, _) = ui.allocate_exact_size(vec2(width, BAR), Sense::hover());
            let painter = ui.painter();
            painter.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(1.0, BORDER));
            // Biểu tượng tune + tiêu đề.
            let icon = egui::pos2(bar.left() + 28.0, bar.center().y);
            for (dy, dx) in [(-5.0, -3.0), (0.0, 4.0), (5.0, -2.0)] {
                painter.line_segment([icon + vec2(-8.0, dy), icon + vec2(8.0, dy)], Stroke::new(1.6, TEAL));
                painter.circle_filled(icon + vec2(dx, dy), 2.3, TEAL);
            }
            painter.text(egui::pos2(bar.left() + 50.0, bar.center().y), Align2::LEFT_CENTER, "Tùy chỉnh Combo (Custom Combo)", egui::FontId::new(15.0, egui::FontFamily::Name("h700".into())), TEXT);
            let x = Rect::from_center_size(egui::pos2(bar.right() - 30.0, bar.center().y), vec2(34.0, 34.0));
            let response = ui.interact(x, Id::unique("combo-editor-close"), Sense::click());
            let painter = ui.painter();
            if response.hovered() { painter.rect_filled(x, CornerRadius::same(9), Color32::from_rgb(27, 39, 60)); }
            let c = x.center();
            let ink = if response.hovered() { TEXT } else { MUTED };
            painter.line_segment([c + vec2(-6.0, -6.0), c + vec2(6.0, 6.0)], Stroke::new(1.8, ink));
            painter.line_segment([c + vec2(-6.0, 6.0), c + vec2(6.0, -6.0)], Stroke::new(1.8, ink));
            if response.on_hover_text("Đóng (Esc)").clicked() { close = true; }
            let area = (height - BAR).max(120.0);
            ScrollArea::vertical().max_height(area).min_scrolled_height(area).auto_shrink([false, false]).show(ui, |ui| {
                Frame::new().inner_margin(Margin::symmetric(24, 16)).show(ui, |ui| {
                    let inner = width - 48.0;
                    ui.set_width(inner);
                    body(ui, inner)
                }).inner
            }).inner
        });
    Shell { inner: modal.inner, close, top: modal.is_top_modal }
}

pub(super) fn keyboard_hotkey(key: Key) -> Option<String> {
    let name = match key {
        Key::ArrowUp => "up",
        Key::ArrowDown => "down",
        Key::PageUp => "page_up",
        Key::PageDown => "page_down",
        Key::ControlLeft => "ctrl",
        Key::ControlRight => "ctrl_r",
        Key::AltLeft => "alt",
        Key::AltRight => "alt_gr",
        Key::SuperLeft => "cmd",
        Key::SuperRight => "cmd_r",
        Key::CapsLock => "caps_lock",
        Key::NumLock => "num_lock",
        Key::ScrollLock => "scroll_lock",
        Key::PrintScreen => "print_screen",
        Key::MediaPlayPause => "media_play_pause",
        Key::MediaTrackNext => "media_next",
        Key::MediaTrackPrevious => "media_previous",
        Key::MediaStop => "media_stop",
        Key::AudioVolumeMute => "media_volume_mute",
        Key::AudioVolumeDown => "media_volume_down",
        Key::AudioVolumeUp => "media_volume_up",
        Key::Quote => "'",
        Key::Minus => "-",
        Key::Space
        | Key::Enter
        | Key::Backspace
        | Key::Tab
        | Key::Delete
        | Key::Insert
        | Key::Home
        | Key::End
        | Key::Pause
        | Key::Menu => key.name(),
        _ if key.symbol_or_name().chars().count() == 1
            && !matches!(key, Key::ArrowLeft | Key::ArrowRight) =>
        {
            key.symbol_or_name()
        }
        _ if key.name().strip_prefix('F').is_some_and(|number| {
            number
                .parse::<u8>()
                .is_ok_and(|number| (1..=24).contains(&number))
        }) =>
        {
            key.name()
        }
        _ => return None,
    };
    Some(name.to_lowercase())
}

fn hotkey_label(hotkey: &str) -> String {
    match hotkey {
        "shift" => "Shift".into(),
        "shift_r" => "Right Shift".into(),
        "ctrl" => "Ctrl".into(),
        "ctrl_r" => "Right Ctrl".into(),
        "alt" => "Alt".into(),
        "alt_gr" => "Right Alt".into(),
        "caps_lock" => "CapsLock".into(),
        "num_lock" => "NumLock".into(),
        "scroll_lock" => "ScrollLock".into(),
        "space" => "Space".into(),
        "enter" => "Enter".into(),
        "esc" => "Esc".into(),
        "delete" => "Delete".into(),
        name if name.chars().count() == 1
            || name
                .strip_prefix('f')
                .is_some_and(|number| number.parse::<u8>().is_ok()) =>
        {
            name.to_uppercase()
        }
        name => name.into(),
    }
}

pub(super) fn frame(fill: Color32, outline: Color32, radius: u8, padding: i8) -> Frame {
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, outline))
        .corner_radius(CornerRadius::same(radius))
        .inner_margin(Margin::same(padding))
}

pub(super) fn button<'a>(label: &str, text: Color32, fill: Color32, outline: Color32) -> Button<'a> {
    Button::new(RichText::new(label).color(text).family(egui::FontFamily::Name("h700".into())))
        .fill(fill)
        .stroke(Stroke::new(1.0, outline))
        .corner_radius(CornerRadius::same(9))
        .min_size(vec2(0.0, 30.0))
}

pub(super) fn eyebrow(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).color(TEAL).size(11.0).family(egui::FontFamily::Name("h700".into())));
}
pub(super) fn panel_heading(ui: &mut Ui, title: &str, actions: usize) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(17.0).family(egui::FontFamily::Name("h700".into())));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let count = format!("{actions} action");
            frame(SURFACE, BORDER, 14, 5).show(ui, |ui| {
                ui.label(RichText::new(count).color(MUTED).size(12.0));
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor_frame(
        editor: &mut Editor,
        context: &Context,
        events: Vec<Event>,
    ) -> egui::FullOutput {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 900.0))),
                time: Some(context.cumulative_frame_nr() as f64 / 60.0),
                events,
                ..Default::default()
            },
            |ui| {
                editor.show(ui.ctx());
            },
        );
        output.textures_delta.clear();
        output
    }

    fn label_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Editor did not draw label {label:?}"))
    }

    fn pointer_button(pos: egui::Pos2, button: PointerButton, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn renders_and_accepts_a_palette_drop_and_mouse_hotkey() {
        let ctx = Context::default();
        let mut editor = Editor::new("gui-test".into(), None).unwrap();
        editor_frame(&mut editor, &ctx, Vec::new());
        let output = editor_frame(&mut editor, &ctx, Vec::new());
        let source = label_position(&output, "E");
        let target = label_position(&output, "Timeline đang trống");
        editor_frame(
            &mut editor,
            &ctx,
            vec![
                Event::PointerMoved(source),
                pointer_button(source, PointerButton::Primary, true),
            ],
        );
        editor_frame(&mut editor, &ctx, vec![Event::PointerMoved(target)]);
        editor_frame(&mut editor, &ctx, Vec::new());
        editor_frame(
            &mut editor,
            &ctx,
            vec![pointer_button(target, PointerButton::Primary, false)],
        );
        assert_eq!(editor.timeline.len(), 1);
        assert_eq!(editor.timeline[0].function, "skke");

        editor.capturing_hotkey = true;
        editor_frame(
            &mut editor,
            &ctx,
            vec![pointer_button(target, PointerButton::Extra2, true)],
        );
        assert_eq!(editor.hotkey.as_deref(), Some("mouse_4"));
        assert!(!editor.capturing_hotkey);
    }

    #[test]
    fn saves_original_fields_and_rebuilds_python_sequence() {
        let source = json!({
            "id": "existing",
            "name": "Tên cũ",
            "hotkey": "mouse_4",
            "timeline": ["skke", "skk2a", "skk5a"],
            "pythonSequence": ["skke", "is_no_key_pressed", "skk2a", "is_no_key_pressed", "skk5a"],
            "metadata": {"tag": "giữ nguyên"}
        });
        let mut editor = Editor::new("unused".into(), Some(&source)).unwrap();
        editor.name = "  Kỹ năng  ".into();
        editor.drop_action(DragAction::Timeline(0), 3);
        editor.remove_action(0);
        let saved = editor.profile().unwrap();
        assert_eq!(
            saved,
            json!({
                "id": "existing",
                "name": "Kỹ năng",
                "hotkey": "mouse_4",
                "timeline": ["skk5a", "skke"],
                "pythonSequence": ["skk5a", "is_no_key_pressed", "skke"],
                "metadata": {"tag": "giữ nguyên"}
            }),
        );
    }

    #[test]
    fn rejects_profiles_that_the_basic_editor_cannot_round_trip() {
        for source in [
            json!({"clips": []}),
            json!({"timeline": [{"type": "wait"}]}),
            json!({"pythonSequence": [{"type": "wait"}]}),
            json!({"timeline": ["unknown_function"]}),
            json!({"timeline": ["skke"], "pythonSequence": ["skk5a"]}),
        ] {
            assert!(
                Editor::new("existing".into(), Some(&source)).is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn validates_name_and_nonempty_timeline_and_sets_null_default_hotkey() {
        let mut editor = Editor::new("new".into(), None).unwrap();
        assert!(editor.profile().is_err());
        editor.name = "Tấn công".into();
        assert!(editor.profile().is_err());
        editor.drop_action(DragAction::Palette(&ACTIONS[0]), 0);
        assert_eq!(
            editor.profile().unwrap(),
            json!({
                "id": "new",
                "name": "Tấn công",
                "hotkey": null,
                "timeline": ["skke"],
                "pythonSequence": ["skke"]
            }),
        );
    }

    #[test]
    fn recovers_a_basic_profile_with_only_a_python_sequence() {
        let source =
            json!({"name": "Cũ", "pythonSequence": ["skke", "is_no_key_pressed", "skk2a"]});
        let editor = Editor::new("legacy".into(), Some(&source)).unwrap();
        assert_eq!(
            editor.profile().unwrap()["timeline"],
            json!(["skke", "skk2a"])
        );
    }

    #[test]
    fn captures_backend_hotkey_names() {
        assert_eq!(keyboard_hotkey(egui::Key::A).as_deref(), Some("a"));
        assert_eq!(
            keyboard_hotkey(egui::Key::PageUp).as_deref(),
            Some("page_up")
        );
        assert_eq!(keyboard_hotkey(egui::Key::F24).as_deref(), Some("f24"));
        assert_eq!(keyboard_hotkey(egui::Key::Quote).as_deref(), Some("'"));
        assert_eq!(keyboard_hotkey(egui::Key::Minus).as_deref(), Some("-"));
        assert_eq!(keyboard_hotkey(egui::Key::ArrowLeft), None);
        assert_eq!(keyboard_hotkey(egui::Key::ShiftLeft), None);
    }
}
