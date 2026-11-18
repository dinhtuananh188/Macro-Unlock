//! Editor chuyên sâu: timeline nhiều track như `cuscombo-advanced.html`, kèm Import / Export chuỗi lệnh macro.
use super::editor::{BACKGROUND, BORDER, DANGER, MUTED, PANEL, SURFACE, TEAL, TEXT, WARNING, button, eyebrow, frame, keyboard_hotkey, shell};
use crate::clips::{self, Clip, Kind, Loop};
use eframe::egui::{
    self, Align, Align2, Color32, Context, CornerRadius, Event, FontFamily, FontId, Id, Key, Layout, Margin, Modal, PointerButton, Pos2, Rect,
    RichText, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui, pos2, vec2,
};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

const HEAD_W: f32 = 58.0;
const RULER_H: f32 = 28.0;
const TRACK_H: f32 = 38.0;
const MIN_TRACKS: usize = 4;
const EDGE_PX: f32 = 6.0;
const SNAP_PX: f32 = 8.0;
const HISTORY_LIMIT: usize = 100;

pub enum AdvancedAction {
    Save(Value),
    Cancel,
    /// Quay về editor cơ bản (chỉ khi combo biểu diễn được bằng các khối Skirk nối tiếp).
    Basic,
    /// Làm lại một combo trắng ngay trong editor.
    NewCombo,
}

#[derive(Clone)]
struct Snapshot { clips: Vec<Clip>, region: Option<Loop> }

enum Drag {
    Move { anchor_ms: i64, anchor_track: i64, origins: Vec<(String, i64, usize)> },
    ResizeLeft { id: String, end: i64 },
    ResizeRight { id: String },
    Playhead,
    Marquee { from: Pos2 },
}

struct Io {
    text: String,
    replace: bool,
    tap_ms: i64,
    cheat_open: bool,
}

pub struct Advanced {
    id: String,
    original: Map<String, Value>,
    name: String,
    hotkey: Option<String>,
    clips: Vec<Clip>,
    region: Option<Loop>,
    tracks: usize,
    active_track: usize,
    playhead: i64,
    zoom: f32,
    snap: bool,
    selected: HashSet<String>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    clipboard: Vec<Clip>,
    drag: Option<Drag>,
    drag_dirty: bool,
    capturing_hotkey: bool,
    capturing_clip: Option<String>,
    error: Option<String>,
    status: Option<(String, bool)>,
    io: Option<Io>,
    fps: f64,
    playing_since: Option<(std::time::Instant, i64)>,
    gap_ms: i64,
    recording: Option<Recording>,
    record_rect: Option<Rect>,
}

/// Phiên ghi: thời gian tính từ thao tác đầu tiên, clip đang giữ dài ra theo thời gian thật.
struct Recording {
    origin: i64,
    t0: Option<Instant>,
    open: HashMap<String, String>,
}

impl Recording {
    fn now(&self) -> Option<i64> {
        self.t0.map(|t0| clips::clamp_ms(self.origin + t0.elapsed().as_millis() as i64))
    }
}

/// Nhấn một phím / nút: đặt clip mới vào track đầu tiên còn trống kể từ `start`.
fn record_down(list: &mut Vec<Clip>, open: &mut HashMap<String, String>, kind: Kind, start: i64, fps: f64) {
    let id = match &kind { Kind::Key(k) => format!("key:{k}"), Kind::Mouse(b) => format!("mouse:{b}"), Kind::Block(f) => format!("block:{f}") };
    if open.contains_key(&id) { return; }
    let holding: HashSet<&String> = open.values().collect();
    let busy = |track: usize| list.iter().any(|c| c.track == track && (holding.contains(&c.id) || c.end() > start));
    let track = (0..).find(|t| !busy(*t)).unwrap_or(0);
    let clip = Clip::new(kind, track, start, 1, fps);
    open.insert(id, clip.id.clone());
    list.push(clip);
}

/// Nhả phím / nút: chốt độ dài clip đang giữ.
fn record_up(list: &mut [Clip], open: &mut HashMap<String, String>, kind: &Kind, now: i64) {
    let id = match kind { Kind::Key(k) => format!("key:{k}"), Kind::Mouse(b) => format!("mouse:{b}"), Kind::Block(f) => format!("block:{f}") };
    if let Some(clip_id) = open.remove(&id) && let Some(clip) = list.iter_mut().find(|c| c.id == clip_id) {
        clip.dur = (now - clip.start).max(1);
    }
}

fn ms_i(value: f32) -> i64 { value.round() as i64 }

impl Advanced {
    pub fn new(id: String, fps: f64) -> Self {
        Self {
            id, original: Map::new(), name: String::new(), hotkey: None, clips: Vec::new(), region: None, tracks: MIN_TRACKS, active_track: 0,
            playhead: 0, zoom: 0.5, snap: true, selected: HashSet::new(), undo: Vec::new(), redo: Vec::new(), clipboard: Vec::new(), drag: None,
            drag_dirty: false, capturing_hotkey: false, capturing_clip: None, error: None, status: None, io: None, fps, playing_since: None, gap_ms: 100, recording: None, record_rect: None,
        }
    }

    /// Mở lại combo đã lưu: ưu tiên `clips` + `loop`, không có thì dựng lại từ bước phẳng.
    pub fn from_profile(id: String, profile: &Value, fps: f64) -> Result<Self, String> {
        let original = profile.as_object().cloned().ok_or("Dữ liệu combo phải là một đối tượng JSON.")?;
        let mut editor = Self::new(original.get("id").and_then(Value::as_str).filter(|i| !i.is_empty()).map_or(id, str::to_owned), fps);
        editor.name = original.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
        editor.hotkey = original.get("hotkey").and_then(Value::as_str).filter(|h| !h.is_empty()).map(str::to_owned);
        if let Some(stored) = original.get("clips").and_then(Value::as_array) {
            for item in stored {
                let kind = match item["kind"].as_str() {
                    Some("key") => Kind::Key(item["key"].as_str().unwrap_or("").to_owned()),
                    Some("mouse") => Kind::Mouse(item["button"].as_str().unwrap_or("left").to_owned()),
                    Some("block") => Kind::Block(item["fn"].as_str().unwrap_or("").to_owned()),
                    _ => continue,
                };
                let mut clip = Clip::new(kind, item["track"].as_u64().unwrap_or(0) as usize, item["start"].as_i64().unwrap_or(0), item["dur"].as_i64().unwrap_or(100), fps);
                if let Some(id) = item["id"].as_str() { clip.id = id.to_owned(); }
                editor.clips.push(clip);
            }
            editor.region = original.get("loop").filter(|l| l.is_object()).and_then(|l| Some(Loop { start: l["start"].as_i64()?, end: l["end"].as_i64()? }));
        } else {
            let steps: Vec<Value> = original.get("timeline").and_then(Value::as_array).cloned()
                .or_else(|| original.get("pythonSequence").and_then(Value::as_array).map(|s| s.iter().filter(|v| v.as_str() != Some("is_no_key_pressed")).cloned().collect()))
                .unwrap_or_default();
            let (clips, region) = clips::steps_to_clips(&steps, fps);
            editor.clips = clips;
            editor.region = region;
        }
        editor.original = original;
        editor.tracks = editor.track_count();
        Ok(editor)
    }

    pub fn with_basic(id: String, name: String, hotkey: Option<String>, functions: &[&str], fps: f64) -> Self {
        let mut editor = Self::new(id, fps);
        editor.name = name;
        editor.hotkey = hotkey;
        let mut at = 0;
        for function in functions {
            let clip = Clip::new(Kind::Block((*function).to_owned()), 0, at, clips::DEFAULT_BLOCK_MS, fps);
            at = clip.end();
            editor.clips.push(clip);
        }
        editor
    }

    pub fn open_import(&mut self) { self.io = Some(Io { text: String::new(), replace: true, tap_ms: 50, cheat_open: false }); }

    pub fn set_fps(&mut self, fps: f64) {
        if (self.fps - fps).abs() < f64::EPSILON { return; }
        self.fps = fps;
        for clip in &mut self.clips {
            if let Kind::Block(function) = &clip.kind && let Some(ms) = clips::block_duration_ms(function, fps) { clip.dur = ms; }
        }
    }

    /// Khối Skirk theo thứ tự thời gian, và số clip phím / chuột bị bỏ khi về editor cơ bản (giống `openBasicMode`).
    pub fn block_functions(&self) -> (Vec<String>, usize) {
        let mut blocks: Vec<&Clip> = self.clips.iter().filter(|c| matches!(&c.kind, Kind::Block(f) if clips::is_known_block(f))).collect();
        blocks.sort_by_key(|c| (c.start, c.track));
        let functions = blocks.iter().filter_map(|c| match &c.kind { Kind::Block(f) => Some(f.clone()), _ => None }).collect();
        (functions, self.clips.len() - blocks.len())
    }

    pub fn id(&self) -> &str { &self.id }
    pub fn name(&self) -> &str { &self.name }
    pub fn hotkey(&self) -> Option<&str> { self.hotkey.as_deref() }

    fn track_count(&self) -> usize {
        self.clips.iter().map(|c| c.track + 1).max().unwrap_or(0).max(self.tracks).max(MIN_TRACKS)
    }
    fn content_end(&self) -> i64 {
        self.clips.iter().map(Clip::end).max().unwrap_or(0).max(self.region.map_or(0, |r| r.end))
    }

    // ── Lịch sử ──────────────────────────────────────────────────────────────
    fn snapshot(&self) -> Snapshot { Snapshot { clips: self.clips.clone(), region: self.region } }
    fn remember(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > HISTORY_LIMIT { self.undo.remove(0); }
        self.redo.clear();
        self.error = None;
    }
    fn restore(&mut self, snap: Snapshot) {
        self.clips = snap.clips;
        self.region = snap.region;
        let ids: HashSet<_> = self.clips.iter().map(|c| c.id.clone()).collect();
        self.selected.retain(|id| ids.contains(id));
    }
    fn undo_step(&mut self) {
        if let Some(prev) = self.undo.pop() { self.redo.push(self.snapshot()); self.restore(prev); }
    }
    fn redo_step(&mut self) {
        if let Some(next) = self.redo.pop() { self.undo.push(self.snapshot()); self.restore(next); }
    }

    // ── Chỉnh sửa clip ───────────────────────────────────────────────────────
    fn free_track(&self, start: i64, dur: i64, from: usize, ignore: &HashSet<String>) -> usize {
        (from..).find(|track| !self.clips.iter().any(|c| c.track == *track && !ignore.contains(&c.id) && c.start < start + dur && start < c.end())).unwrap_or(from)
    }
    fn add_clips(&mut self, list: Vec<Clip>, select: bool) {
        let mut ids = HashSet::new();
        for mut clip in list {
            clip.track = self.free_track(clip.start, clip.dur, clip.track, &HashSet::new());
            ids.insert(clip.id.clone());
            self.clips.push(clip);
        }
        if select { self.selected = ids; }
        self.tracks = self.track_count();
    }
    fn add_key(&mut self) {
        self.remember();
        let clip = Clip::new(Kind::Key(String::new()), self.active_track, self.playhead, clips::DEFAULT_CLIP_MS, self.fps);
        let id = clip.id.clone();
        self.add_clips(vec![clip], true);
        self.capturing_clip = Some(id);
    }
    fn add_mouse(&mut self, button: &str) {
        self.remember();
        let clip = Clip::new(Kind::Mouse(button.into()), self.active_track, self.playhead, clips::DEFAULT_CLIP_MS, self.fps);
        self.add_clips(vec![clip], true);
    }
    fn add_block(&mut self, function: &str) {
        self.remember();
        let clip = Clip::new(Kind::Block(function.into()), self.active_track, self.playhead, clips::DEFAULT_BLOCK_MS, self.fps);
        self.add_clips(vec![clip], true);
    }
    fn remove_selected(&mut self) {
        if self.selected.is_empty() { return; }
        self.remember();
        let selected = std::mem::take(&mut self.selected);
        self.clips.retain(|c| !selected.contains(&c.id));
    }
    fn copy_selection(&mut self, cut: bool) {
        let mut picked: Vec<Clip> = self.clips.iter().filter(|c| self.selected.contains(&c.id)).cloned().collect();
        if picked.is_empty() { return; }
        let origin = picked.iter().map(|c| c.start).min().unwrap_or(0);
        for clip in &mut picked { clip.start -= origin; }
        self.clipboard = picked;
        if cut { self.remove_selected(); }
    }
    fn paste(&mut self) {
        if self.clipboard.is_empty() { return; }
        self.remember();
        let list: Vec<Clip> = self.clipboard.iter().map(|c| Clip { id: clips::next_id(), start: self.playhead + c.start, ..c.clone() }).collect();
        self.add_clips(list, true);
    }
    fn duplicate(&mut self) {
        let picked: Vec<Clip> = self.clips.iter().filter(|c| self.selected.contains(&c.id)).cloned().collect();
        if picked.is_empty() { return; }
        self.remember();
        let end = picked.iter().map(Clip::end).max().unwrap_or(0);
        let origin = picked.iter().map(|c| c.start).min().unwrap_or(0);
        let list = picked.into_iter().map(|c| Clip { id: clips::next_id(), start: c.start - origin + end, ..c }).collect();
        self.add_clips(list, true);
    }
    fn split_at_playhead(&mut self) {
        let at = self.playhead;
        let targets: Vec<usize> = self.clips.iter().enumerate()
            .filter(|(_, c)| !matches!(c.kind, Kind::Block(_)) && c.start < at && c.end() > at && (self.selected.is_empty() || self.selected.contains(&c.id)))
            .map(|(i, _)| i).collect();
        if targets.is_empty() { return; }
        self.remember();
        for index in targets {
            let tail = Clip { id: clips::next_id(), start: at, dur: self.clips[index].end() - at, ..self.clips[index].clone() };
            self.clips[index].dur = at - self.clips[index].start;
            self.clips.push(tail);
        }
    }
    fn nudge(&mut self, delta: i64) {
        if self.selected.is_empty() { return; }
        self.remember();
        for clip in &mut self.clips { if self.selected.contains(&clip.id) { clip.start = clips::clamp_ms(clip.start + delta); } }
    }
    fn select_all(&mut self) { self.selected = self.clips.iter().map(|c| c.id.clone()).collect(); }

    // ── Hotkey và phím của clip ──────────────────────────────────────────────
    fn capture_input(&mut self, ctx: &Context) {
        if !self.capturing_hotkey && self.capturing_clip.is_none() { return; }
        let captured = ctx.input_mut(|input| {
            let found = input.events.iter().enumerate().find_map(|(index, event)| match event {
                Event::Key { key, physical_key, pressed: true, repeat: false, .. } => {
                    let physical = physical_key.unwrap_or(*key);
                    let key = if physical.name().len() == 1 && physical.name().as_bytes()[0].is_ascii_alphanumeric() { physical } else { *key };
                    Some((index, if key == Key::Escape { Ok(None) } else { keyboard_hotkey(key).map(Some).ok_or(()) }))
                }
                Event::PointerButton { button, pressed: true, .. } => match button {
                    PointerButton::Middle => Some((index, Ok(Some("middle".to_owned())))),
                    PointerButton::Extra1 => Some((index, Ok(Some("mouse_3".to_owned())))),
                    PointerButton::Extra2 => Some((index, Ok(Some("mouse_4".to_owned())))),
                    _ => None,
                },
                _ => None,
            });
            found.map(|(index, value)| { input.events.remove(index); value })
        });
        match captured {
            Some(Ok(value)) => {
                if let Some(value) = value {
                    if self.capturing_hotkey { self.hotkey = Some(value); }
                    else if let Some(id) = self.capturing_clip.clone() {
                        self.remember();
                        if let Some(clip) = self.clips.iter_mut().find(|c| c.id == id) {
                            match &mut clip.kind {
                                Kind::Key(key) if !value.starts_with("mouse_") && value != "middle" => *key = value,
                                Kind::Mouse(button) if value.starts_with("mouse_") || value == "middle" => *button = value,
                                _ => {}
                            }
                        }
                    }
                }
                self.capturing_hotkey = false;
                self.capturing_clip = None;
            }
            Some(Err(())) => self.status = Some(("Phím này chưa được hỗ trợ.".into(), true)),
            None => {}
        }
    }

    // ── Giao diện ────────────────────────────────────────────────────────────
    pub fn show(&mut self, ctx: &Context) -> Option<AdvancedAction> {
        self.record_events(ctx);
        self.capture_input(ctx);
        if self.io.is_none() && self.recording.is_none() { self.shortcuts(ctx); }
        if let Some((started, base)) = self.playing_since {
            let elapsed = started.elapsed().as_millis() as i64;
            self.playhead = base + elapsed;
            if self.playhead > self.content_end() + 300 { self.playing_since = None; self.playhead = base; } else { ctx.request_repaint(); }
        }
        let io_open = self.io.is_some();
        let out = shell(ctx, |ui, _| self.editor_ui(ui));
        let mut action = out.inner;
        if action.is_none() && out.close { action = Some(AdvancedAction::Cancel); }
        if io_open && let Some(io_action) = self.io_modal(ctx) { action = action.or(io_action); }
        if action.is_none() && !io_open && self.recording.is_none() && !self.capturing_hotkey && self.capturing_clip.is_none() && out.top
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            action = Some(AdvancedAction::Cancel);
        }
        action
    }

    fn shortcuts(&mut self, ctx: &Context) {
        if ctx.egui_wants_keyboard_input() || self.capturing_hotkey || self.capturing_clip.is_some() { return; }
        let command = egui::Modifiers::COMMAND;
        let (undo, redo, copy, cut, paste, dup, all, delete) = ctx.input_mut(|i| (
            i.consume_key(command, Key::Z), i.consume_key(command, Key::Y), i.consume_key(command, Key::C), i.consume_key(command, Key::X),
            i.consume_key(command, Key::V), i.consume_key(command, Key::D), i.consume_key(command, Key::A), i.consume_key(egui::Modifiers::NONE, Key::Delete),
        ));
        if undo { self.undo_step(); }
        if redo { self.redo_step(); }
        if copy { self.copy_selection(false); }
        if cut { self.copy_selection(true); }
        if paste { self.paste(); }
        if dup { self.duplicate(); }
        if all { self.select_all(); }
        if delete { self.remove_selected(); }
        let (c, i_key, o_key, space, left, right, shift_left, shift_right) = ctx.input_mut(|i| (
            i.consume_key(egui::Modifiers::NONE, Key::C), i.consume_key(egui::Modifiers::NONE, Key::I), i.consume_key(egui::Modifiers::NONE, Key::O),
            i.consume_key(egui::Modifiers::NONE, Key::Space), i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft), i.consume_key(egui::Modifiers::NONE, Key::ArrowRight),
            i.consume_key(egui::Modifiers::SHIFT, Key::ArrowLeft), i.consume_key(egui::Modifiers::SHIFT, Key::ArrowRight),
        ));
        if c { self.split_at_playhead(); }
        if i_key { self.set_loop_edge(true); }
        if o_key { self.set_loop_edge(false); }
        if space { self.toggle_play(); }
        if left { self.nudge(-1); }
        if right { self.nudge(1); }
        if shift_left { self.nudge(-10); }
        if shift_right { self.nudge(10); }
    }

    fn toggle_recording(&mut self) {
        if self.recording.is_some() { self.stop_recording(); return; }
        if self.capturing_hotkey || self.capturing_clip.is_some() { return; }
        self.playing_since = None;
        self.remember();
        self.selected.clear();
        self.recording = Some(Recording { origin: self.playhead, t0: None, open: HashMap::new() });
        self.status = Some((format!("Đang ghi từ {} ms: thời gian tính từ thao tác đầu tiên, giữ bao lâu thì clip dài bấy nhiêu.", self.playhead), false));
    }

    fn stop_recording(&mut self) {
        let Some(rec) = self.recording.take() else { return };
        if let Some(now) = rec.now() {
            for clip_id in rec.open.values() {
                if let Some(clip) = self.clips.iter_mut().find(|c| &c.id == clip_id) { clip.dur = (now - clip.start).max(1); }
            }
            self.playhead = now;
        }
        self.tracks = self.track_count();
        self.status = Some(("Đã dừng ghi. Kéo chỉnh lại clip nếu cần rồi Lưu.".into(), false));
    }

    /// Thu phím / chuột thao tác trong cửa sổ thành clip. Sự kiện đã thu bị gỡ để không kích hoạt nút khác.
    fn record_events(&mut self, ctx: &Context) {
        let Some(rec) = &self.recording else { return };
        let record_rect = self.record_rect;
        let has_open = !rec.open.is_empty();
        let taken = ctx.input_mut(|input| {
            let mut taken: Vec<(Kind, bool)> = Vec::new();
            input.events.retain(|event| match event {
                Event::Key { key, physical_key, pressed, repeat, .. } => {
                    if !(*repeat && *pressed) {
                        let physical = physical_key.unwrap_or(*key);
                        let key = if physical.name().len() == 1 && physical.name().as_bytes()[0].is_ascii_alphanumeric() { physical } else { *key };
                        if let Some(name) = keyboard_hotkey(key) { taken.push((Kind::Key(name), *pressed)); }
                    }
                    false
                }
                Event::PointerButton { pos, button, pressed, .. } => {
                    // Bấm vào nút Ghi / Dừng thì không tính là một thao tác cần ghi.
                    if record_rect.is_some_and(|r| r.contains(*pos)) && (*pressed || !has_open) { return true; }
                    let name = match button { PointerButton::Primary => "left", PointerButton::Secondary => "right", PointerButton::Middle => "middle", PointerButton::Extra1 => "mouse_3", PointerButton::Extra2 => "mouse_4" };
                    taken.push((Kind::Mouse(name.to_owned()), *pressed));
                    false
                }
                _ => true,
            });
            taken
        });
        let fps = self.fps;
        for (kind, down) in taken {
            let Some(rec) = self.recording.as_mut() else { return };
            if down && rec.t0.is_none() { rec.t0 = Some(Instant::now()); }
            let Some(now) = rec.now() else { continue };
            if down { record_down(&mut self.clips, &mut rec.open, kind, now, fps); } else { record_up(&mut self.clips, &mut rec.open, &kind, now); }
        }
        if let Some(rec) = &self.recording {
            if let Some(now) = rec.now() {
                for clip_id in rec.open.values() {
                    if let Some(clip) = self.clips.iter_mut().find(|c| &c.id == clip_id) { clip.dur = (now - clip.start).max(1); }
                }
                self.playhead = now;
            }
            self.tracks = self.track_count().max(self.tracks);
            ctx.request_repaint();
        }
    }

    fn toggle_play(&mut self) {
        self.playing_since = if self.playing_since.is_some() { None } else { Some((std::time::Instant::now(), self.playhead)) };
    }

    fn set_loop_edge(&mut self, start: bool) {
        self.remember();
        let at = self.playhead;
        let end_default = self.content_end().max(at + 1);
        let mut region = self.region.unwrap_or(Loop { start: 0, end: end_default });
        if start { region.start = at.min(region.end - 1); } else { region.end = at.max(region.start + 1); }
        self.region = Some(region);
    }

    fn editor_ui(&mut self, ui: &mut Ui) -> Option<AdvancedAction> {
        let mut result = None;
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("SKIRK MACRO / CUSTOM COMBO").color(TEAL).size(11.0).family(FontFamily::Name("h700".into())));
                ui.label(RichText::new(if self.original.is_empty() { "Tạo Custom Combo" } else { "Chỉnh sửa Custom Combo" }).size(36.0).family(FontFamily::Name("h700".into())));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let ghost = Color32::from_rgb(27, 39, 60);
                if ui.add(button("Cơ bản", Color32::from_rgb(215, 229, 247), ghost, BORDER).min_size(vec2(0.0, 40.0))).clicked() { result = Some(AdvancedAction::Basic); }
                if ui.add(button("Combo mới", Color32::from_rgb(215, 229, 247), ghost, BORDER).min_size(vec2(0.0, 40.0))).clicked() {
                    result = Some(AdvancedAction::NewCombo);
                }
                if ui.add(button("Import / Export", Color32::from_rgb(215, 229, 247), ghost, BORDER).min_size(vec2(0.0, 40.0))).clicked() { self.open_import(); }
            });
        });
        ui.label(RichText::new("Xếp phím / chuột thành các clip trên nhiều track như Premiere: clip dài bao nhiêu thì giữ bấy lâu, clip chồng nhau theo chiều dọc thì bấm cùng lúc.").color(MUTED));
        ui.add_space(8.0);
        frame(PANEL, BORDER, 18, 22).show(ui, |ui| {
            eyebrow(ui, "TIMELINE");
            ui.horizontal(|ui| {
                ui.label(RichText::new("Các clip của combo").size(17.0).family(FontFamily::Name("h700".into())));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    frame(SURFACE, BORDER, 14, 5).show(ui, |ui| ui.label(RichText::new(format!("{} clip", self.clips.len())).color(MUTED).size(12.0)));
                });
            });
            egui::CollapsingHeader::new(RichText::new("Cách dùng").family(FontFamily::Name("h600".into()))).show(ui, |ui| {
                for line in [
                    "Kéo clip để dời (ngang = thời điểm, dọc = track). Kéo mép trái / phải để đổi thời gian giữ. Kéo vùng trống để khoanh chọn nhiều clip.",
                    "Bấm thước thời gian để đặt đầu phát. Nút Phím / LC / RC thêm clip tại đầu phát; nhấp đúp vào chỗ trống để thêm clip phím tại đó.",
                    "C cắt clip tại đầu phát, I / O đặt đầu / cuối vùng lặp, Space xem thử, ← → dời clip 1 ms (Shift 10 ms), Alt khi kéo để tắt hít.",
                    "Ctrl+C / X / V / D / Z / Y / A, Delete để xóa. Dán luôn đặt tại đầu phát.",
                ] { ui.label(RichText::new(format!("• {line}")).color(MUTED).size(12.5)); }
            });
            if let Some(saved) = self.toolbar(ui) { result = result.take().or(Some(saved)); }
            self.timeline(ui);
            self.status_bar(ui);
            self.inspector(ui);
            ui.add_space(8.0);
            if let Some(saved) = self.form_ui(ui) { result = result.take().or(Some(saved)); }
        });
        result
    }

    fn insert_gap(&mut self) {
        let gap = clips::clamp_ms(self.gap_ms);
        if gap < 1 { self.status = Some(("Nhập độ dài khoảng chèn (ms) lớn hơn 0.".into(), true)); return; }
        self.remember();
        let at = self.playhead;
        for clip in &mut self.clips {
            if clip.start >= at { clip.start += gap; }
            else if clip.end() > at && !matches!(clip.kind, Kind::Block(_)) { clip.dur += gap; }
        }
        if let Some(region) = &mut self.region {
            if region.start >= at { region.start += gap; }
            if region.end >= at { region.end += gap; }
        }
        self.status = Some((format!("Đã chèn khoảng {gap} ms tại {at} ms."), false));
    }

    fn clear_all(&mut self) {
        if self.clips.is_empty() && self.region.is_none() { return; }
        self.remember();
        self.clips.clear();
        self.region = None;
        self.selected.clear();
        self.playhead = 0;
    }

    /// Thanh công cụ icon như editor gốc: hoàn tác, lưu, import, sao chép, cắt, vùng lặp, xem thử, hít, thu phóng.
    fn toolbar(&mut self, ui: &mut Ui) -> Option<AdvancedAction> {
        let mut result = None;
        let has_selection = !self.selected.is_empty();
        let has_clips = !self.clips.is_empty();
        let playing = self.playing_since.is_some();
        frame(Color32::from_rgb(5, 9, 18), BORDER, 14, 8).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 4.0);
                let sep = |ui: &mut Ui| { let (rect, _) = ui.allocate_exact_size(vec2(7.0, 28.0), Sense::hover()); ui.painter().line_segment([rect.center_top() + vec2(0.0, 3.0), rect.center_bottom() - vec2(0.0, 3.0)], Stroke::new(1.0, BORDER)); };
                if tool_button(ui, Tool::Undo, "Hoàn tác (Ctrl+Z)", !self.undo.is_empty(), false).clicked() { self.undo_step(); }
                if tool_button(ui, Tool::Redo, "Làm lại (Ctrl+Y)", !self.redo.is_empty(), false).clicked() { self.redo_step(); }
                sep(ui);
                if tool_button(ui, Tool::Save, "Lưu combo", true, false).clicked() {
                    match self.profile() { Ok(profile) => result = Some(AdvancedAction::Save(profile)), Err(message) => self.error = Some(message) }
                }
                if tool_button(ui, Tool::Io, "Import / Export chuỗi lệnh Macro (import.md)", true, false).clicked() { self.open_import(); }
                sep(ui);
                if tool_button(ui, Tool::Cut, "Cắt (Ctrl+X)", has_selection, false).clicked() { self.copy_selection(true); }
                if tool_button(ui, Tool::Copy, "Copy (Ctrl+C)", has_selection, false).clicked() { self.copy_selection(false); }
                if tool_button(ui, Tool::Paste, "Dán tại đầu phát (Ctrl+V)", !self.clipboard.is_empty(), false).clicked() { self.paste(); }
                if tool_button(ui, Tool::Dup, "Nhân đôi, đặt ngay sau (Ctrl+D)", has_selection, false).clicked() { self.duplicate(); }
                sep(ui);
                if tool_button(ui, Tool::Razor, "Cắt đôi clip tại đầu phát (C)", has_clips, false).clicked() { self.split_at_playhead(); }
                if tool_button(ui, Tool::Trash, "Xóa clip đã chọn (Delete)", has_selection, false).clicked() { self.remove_selected(); }
                if tool_button(ui, Tool::Clear, "Xóa hết timeline", has_clips, false).clicked() { self.clear_all(); }
                sep(ui);
                if tool_button(ui, Tool::LoopIn, "Đặt đầu vùng lặp tại đầu phát (I)", true, false).clicked() { self.set_loop_edge(true); }
                if tool_button(ui, Tool::LoopOut, "Đặt cuối vùng lặp tại đầu phát (O)", true, false).clicked() { self.set_loop_edge(false); }
                if tool_button(ui, Tool::LoopOff, "Bỏ vùng lặp", self.region.is_some(), false).clicked() { self.remember(); self.region = None; }
                sep(ui);
                if tool_button(ui, if playing { Tool::Stop } else { Tool::Play }, "Xem thử đầu phát chạy (Space)", true, playing).clicked() { self.toggle_play(); }
                let recording = self.recording.is_some();
                let record = tool_button(ui, if recording { Tool::Stop } else { Tool::Record }, if recording { "Dừng ghi" } else { "Ghi: bấm rồi thao tác bàn phím / chuột trong cửa sổ này, clip được đặt từ đầu phát" }, true, recording);
                self.record_rect = Some(record.rect);
                if record.clicked() { self.toggle_recording(); }
                sep(ui);
                if tool_button(ui, Tool::Magnet, "Hít vào mép clip / đầu phát khi kéo (giữ Alt để tạm tắt)", true, self.snap).clicked() { self.snap = !self.snap; }
                if tool_button(ui, Tool::ZoomOut, "Thu nhỏ (-)", true, false).clicked() { self.zoom = (self.zoom * 0.8).max(0.02); }
                if tool_button(ui, Tool::ZoomIn, "Phóng to (+)", true, false).clicked() { self.zoom = (self.zoom * 1.25).min(8.0); }
                if tool_button(ui, Tool::Fit, "Vừa khung", true, false).clicked() {
                    self.zoom = ((ui.available_width().max(600.0) - HEAD_W - 120.0) / (self.content_end().max(1000) as f32)).clamp(0.02, 8.0);
                }
                ui.add_space(8.0);
                ui.add(egui::Label::new(RichText::new(format!("{} ms / 100 px", (100.0 / self.zoom).round() as i64)).color(MUTED).size(12.5)).wrap_mode(egui::TextWrapMode::Extend));
            });
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if label_button(ui, Tool::Key, "Phím").clicked() { self.add_key(); }
            if label_button(ui, Tool::Mouse, "LC").clicked() { self.add_mouse("left"); }
            if label_button(ui, Tool::Mouse, "RC").clicked() { self.add_mouse("right"); }
            if label_button(ui, Tool::Gap, "K.trống").on_hover_text("Chèn khoảng trống tại đầu phát, đẩy các clip phía sau").clicked() { self.insert_gap(); }
            ui.add(egui::DragValue::new(&mut self.gap_ms).range(1..=clips::MAX_MS).speed(1.0));
            ui.label(RichText::new("ms").color(MUTED));
            if label_button(ui, Tool::Tracks, "+Track").clicked() { self.tracks = self.track_count() + 1; }
            let skirk = label_button(ui, Tool::Skirk, "Skirk");
            egui::Popup::menu(&skirk).show(|ui| {
                for function in ["skke", "skk2a", "skk2as", "skk2az", "skk2azs", "skk2azs_slow", "skk2aq", "skk3as", "skk3aw", "skk5as", "skk5a"] {
                    if ui.button(clips::block_label(function)).clicked() { self.add_block(function); ui.close(); }
                }
            });
        });
        result
    }

    /// Dòng trạng thái dưới timeline: đầu phát, tổng thời lượng, track chèn, vùng lặp.
    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Đầu phát").color(MUTED).size(12.5));
            ui.add(egui::DragValue::new(&mut self.playhead).range(0..=clips::MAX_MS).speed(1.0));
            ui.label(RichText::new("ms").color(MUTED).size(12.5));
            ui.label(RichText::new(format!("Tổng: {} ms", self.content_end())).color(MUTED).size(12.5));
            ui.label(RichText::new(format!("Track chèn: T{}", self.active_track + 1)).color(MUTED).size(12.5));
            match self.region {
                Some(mut region) => {
                    ui.label(RichText::new("Lặp").color(WARNING).size(12.5));
                    let a = ui.add(egui::DragValue::new(&mut region.start).range(0..=clips::MAX_MS));
                    ui.label(RichText::new("→").color(WARNING));
                    let b = ui.add(egui::DragValue::new(&mut region.end).range(1..=clips::MAX_MS));
                    ui.label(RichText::new(format!("ms ({} ms / vòng)", region.end - region.start)).color(WARNING).size(12.5));
                    if (a.changed() || b.changed()) && region.end > region.start { self.remember(); self.region = Some(region); }
                }
                None => { ui.label(RichText::new("Không lặp (I / O để đặt vùng lặp)").color(MUTED).size(12.5)); }
            }
        });
    }

    fn timeline(&mut self, ui: &mut Ui) {
        let tracks = self.track_count();
        let clash = clips::clashing_ids(&self.clips);
        let zoom = self.zoom;
        let content = self.content_end().max(3000) + 2000;
        let width = (HEAD_W + content as f32 * zoom).max(ui.available_width());
        let height = RULER_H + tracks as f32 * TRACK_H;
        let mut deferred_click: Option<(Pos2, bool)> = None;
        ScrollArea::horizontal().id_salt("advanced-timeline").show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, CornerRadius::same(12), SURFACE);
            let x_of = |ms: i64| rect.left() + HEAD_W + ms as f32 * zoom;
            let ms_of = |x: f32| ms_i((x - rect.left() - HEAD_W) / zoom);
            // Thước thời gian.
            let step = [10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000].into_iter().find(|s| *s as f32 * zoom >= 60.0).unwrap_or(10000);
            painter.text(pos2(rect.left() + HEAD_W / 2.0, rect.top() + 14.0), Align2::CENTER_CENTER, "ms", FontId::new(11.0, FontFamily::Name("h500".into())), MUTED);
            let mut at = 0;
            while at <= content {
                let x = x_of(at);
                painter.line_segment([pos2(x, rect.top() + RULER_H - 8.0), pos2(x, rect.top() + RULER_H)], Stroke::new(1.0, BORDER));
                let label = if at % 1000 == 0 { format!("{}s", at / 1000) } else if at >= 1000 { format!("{:.1}s", at as f32 / 1000.0) } else { format!("{at}") };
                painter.text(pos2(x + 3.0, rect.top() + 9.0), Align2::LEFT_CENTER, label, FontId::new(11.0, FontFamily::Name("h500".into())), MUTED);
                for minor in 1..5 {
                    let mx = x_of(at + step * minor / 5);
                    painter.line_segment([pos2(mx, rect.top() + RULER_H - 4.0), pos2(mx, rect.top() + RULER_H)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(148, 163, 184, 40)));
                }
                at += step;
            }
            painter.line_segment([pos2(rect.left(), rect.top() + RULER_H), pos2(rect.right(), rect.top() + RULER_H)], Stroke::new(1.0, BORDER));
            for track in 0..tracks {
                let top = rect.top() + RULER_H + track as f32 * TRACK_H;
                let row = Rect::from_min_size(pos2(rect.left(), top), vec2(width, TRACK_H));
                if track == self.active_track { painter.rect_filled(row, 0.0, Color32::from_rgba_unmultiplied(45, 212, 191, 10)); }
                painter.line_segment([pos2(rect.left(), top + TRACK_H), pos2(rect.right(), top + TRACK_H)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(148, 163, 184, 28)));
                painter.text(pos2(rect.left() + 10.0, top + TRACK_H / 2.0), Align2::LEFT_CENTER, format!("T{}", track + 1), FontId::new(11.0, FontFamily::Name("h700".into())), MUTED);
            }
            if self.clips.is_empty() {
                painter.text(pos2(rect.left() + HEAD_W + 16.0, rect.top() + RULER_H + TRACK_H / 2.0), Align2::LEFT_CENTER, "Timeline trống: bấm Phím / Chuột ở trên, nhấp đúp vào track, hoặc bấm Ghi.", FontId::new(13.0, FontFamily::Name("h500".into())), MUTED);
            }
            if let Some(region) = self.region {
                let band = Rect::from_min_max(pos2(x_of(region.start), rect.top()), pos2(x_of(region.end), rect.bottom()));
                painter.rect_filled(band, 0.0, Color32::from_rgba_unmultiplied(251, 191, 36, 18));
                painter.line_segment([band.left_top(), band.left_bottom()], Stroke::new(1.5, WARNING));
                painter.line_segment([band.right_top(), band.right_bottom()], Stroke::new(1.5, WARNING));
            }
            let clip_rect = |clip: &Clip| Rect::from_min_size(pos2(x_of(clip.start), rect.top() + RULER_H + clip.track as f32 * TRACK_H + 3.0), vec2((clip.dur as f32 * zoom).max(6.0), TRACK_H - 6.0));
            for clip in &self.clips {
                let r = clip_rect(clip);
                let selected = self.selected.contains(&clip.id);
                let (fill, line) = match clip.kind {
                    Kind::Key(_) => (Color32::from_rgb(18, 60, 65), Color32::from_rgb(39, 119, 118)),
                    Kind::Mouse(_) => (Color32::from_rgb(48, 41, 85), Color32::from_rgb(91, 75, 134)),
                    Kind::Block(_) => (Color32::from_rgb(70, 52, 18), Color32::from_rgb(160, 120, 40)),
                };
                painter.rect_filled(r, CornerRadius::same(8), fill);
                let stroke = if clash.contains(&clip.id) { Stroke::new(2.0, DANGER) } else if selected { Stroke::new(2.0, TEAL) } else { Stroke::new(1.0, line) };
                painter.rect_stroke(r, CornerRadius::same(8), stroke, StrokeKind::Inside);
                let label = match &clip.kind { Kind::Mouse(b) => short_mouse(b).to_owned(), _ => clips::clip_name(clip) };
                painter.with_clip_rect(r.shrink(3.0)).text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, label, FontId::new(12.0, FontFamily::Name("h700".into())), TEXT);
            }
            if self.recording.is_some() { painter.rect_stroke(rect, CornerRadius::same(12), Stroke::new(2.0, Color32::from_rgb(226, 85, 75)), StrokeKind::Inside); }
            let head = x_of(self.playhead);
            painter.line_segment([pos2(head, rect.top()), pos2(head, rect.bottom())], Stroke::new(1.5, TEAL));
            painter.add(egui::Shape::convex_polygon(vec![pos2(head - 6.0, rect.top()), pos2(head + 6.0, rect.top()), pos2(head, rect.top() + 10.0)], TEAL, Stroke::NONE));

            // Tương tác chuột.
            let hit = |list: &[Clip], pos: Pos2| list.iter().rev().find(|c| clip_rect(c).expand2(vec2(2.0, 0.0)).contains(pos)).map(|c| (c.id.clone(), clip_rect(c)));
            let shift = ui.input(|i| i.modifiers.shift);
            if response.drag_started() && let Some(pos) = response.interact_pointer_pos() {
                self.drag_dirty = false;
                if pos.y < rect.top() + RULER_H {
                    self.drag = Some(Drag::Playhead);
                } else if let Some((id, r)) = hit(&self.clips, pos) {
                    if !self.selected.contains(&id) { if !shift { self.selected.clear(); } self.selected.insert(id.clone()); }
                    let Some(clip) = self.clips.iter().find(|c| c.id == id) else { return };
                    let editable = !matches!(clip.kind, Kind::Block(_));
                    if editable && pos.x - r.left() <= EDGE_PX { self.drag = Some(Drag::ResizeLeft { id, end: clip.end() }); }
                    else if editable && r.right() - pos.x <= EDGE_PX { self.drag = Some(Drag::ResizeRight { id }); }
                    else {
                        let origins = self.clips.iter().filter(|c| self.selected.contains(&c.id)).map(|c| (c.id.clone(), c.start, c.track)).collect();
                        self.drag = Some(Drag::Move { anchor_ms: ms_of(pos.x), anchor_track: ((pos.y - rect.top() - RULER_H) / TRACK_H).floor() as i64, origins });
                    }
                } else {
                    if !shift { self.selected.clear(); }
                    self.drag = Some(Drag::Marquee { from: pos });
                }
            }
            if response.dragged() && let Some(pos) = response.interact_pointer_pos() {
                let snap_targets: Vec<i64> = if self.snap {
                    let moving: HashSet<String> = match &self.drag { Some(Drag::Move { origins, .. }) => origins.iter().map(|o| o.0.clone()).collect(), Some(Drag::ResizeLeft { id, .. } | Drag::ResizeRight { id }) => HashSet::from([id.clone()]), _ => HashSet::new() };
                    self.clips.iter().filter(|c| !moving.contains(&c.id)).flat_map(|c| [c.start, c.end()]).chain([self.playhead, 0]).collect()
                } else { Vec::new() };
                let snapped = |ms: i64| snap_targets.iter().copied().find(|t| ((*t - ms).abs() as f32) * zoom <= SNAP_PX && !ui.input(|i| i.modifiers.alt)).unwrap_or(ms);
                match &self.drag {
                    Some(Drag::Playhead) => self.playhead = clips::clamp_ms(ms_of(pos.x)),
                    Some(Drag::Move { anchor_ms, anchor_track, origins }) => {
                        if !self.drag_dirty { self.drag_dirty = true; let snap = self.snapshot(); self.undo.push(snap); self.redo.clear(); }
                        let mut delta = ms_of(pos.x) - anchor_ms;
                        if let Some(first) = origins.first() { delta = snapped(first.1 + delta) - first.1; }
                        let track_delta = ((pos.y - rect.top() - RULER_H) / TRACK_H).floor() as i64 - anchor_track;
                        let origins = origins.clone();
                        for (id, start, track) in origins {
                            if let Some(clip) = self.clips.iter_mut().find(|c| c.id == id) {
                                clip.start = clips::clamp_ms(start + delta);
                                clip.track = (track as i64 + track_delta).max(0) as usize;
                            }
                        }
                        self.tracks = self.track_count();
                    }
                    Some(Drag::ResizeRight { id }) => {
                        if !self.drag_dirty { self.drag_dirty = true; let snap = self.snapshot(); self.undo.push(snap); self.redo.clear(); }
                        let id = id.clone();
                        let end = snapped(ms_of(pos.x));
                        if let Some(clip) = self.clips.iter_mut().find(|c| c.id == id) { clip.dur = (end - clip.start).clamp(1, clips::MAX_MS); }
                    }
                    Some(Drag::ResizeLeft { id, end }) => {
                        if !self.drag_dirty { self.drag_dirty = true; let snap = self.snapshot(); self.undo.push(snap); self.redo.clear(); }
                        let (id, end) = (id.clone(), *end);
                        let start = snapped(ms_of(pos.x)).clamp(0, end - 1);
                        if let Some(clip) = self.clips.iter_mut().find(|c| c.id == id) { clip.start = start; clip.dur = end - start; }
                    }
                    Some(Drag::Marquee { from }) => {
                        let area = Rect::from_two_pos(*from, pos);
                        painter.rect_stroke(area, CornerRadius::same(2), Stroke::new(1.0, TEAL), StrokeKind::Inside);
                        let inside: HashSet<String> = self.clips.iter().filter(|c| clip_rect(c).intersects(area)).map(|c| c.id.clone()).collect();
                        if shift { self.selected.extend(inside); } else { self.selected = inside; }
                    }
                    None => {}
                }
            }
            if response.drag_stopped() { self.drag = None; }
            if response.clicked() && let Some(pos) = response.interact_pointer_pos() { deferred_click = Some((pos, shift)); }
            if response.double_clicked() && let Some(pos) = response.interact_pointer_pos() {
                if let Some((id, _)) = hit(&self.clips, pos) {
                    if self.clips.iter().any(|c| c.id == id && !matches!(c.kind, Kind::Block(_))) { self.capturing_clip = Some(id); }
                } else if pos.y > rect.top() + RULER_H {
                    let track = (((pos.y - rect.top() - RULER_H) / TRACK_H).floor().max(0.0)) as usize;
                    self.remember();
                    let clip = Clip::new(Kind::Key(String::new()), track, ms_of(pos.x), clips::DEFAULT_CLIP_MS, self.fps);
                    let id = clip.id.clone();
                    self.add_clips(vec![clip], true);
                    self.capturing_clip = Some(id);
                }
            }
            // Căn thước theo vị trí đặt chuột (đặt đầu phát khi bấm vào thước).
            if let Some((pos, shift)) = deferred_click.take() {
                if pos.y < rect.top() + RULER_H { self.playhead = clips::clamp_ms(ms_of(pos.x)); }
                else if let Some((id, _)) = hit(&self.clips, pos) {
                    if shift { if !self.selected.remove(&id) { self.selected.insert(id); } } else { self.selected = HashSet::from([id]); }
                } else {
                    self.selected.clear();
                    self.active_track = (((pos.y - rect.top() - RULER_H) / TRACK_H).floor().max(0.0)) as usize;
                }
            }
        });
    }

    fn inspector(&mut self, ui: &mut Ui) {
        if self.selected.len() != 1 { return; }
        let id = self.selected.iter().next().cloned().unwrap_or_default();
        let Some(index) = self.clips.iter().position(|c| c.id == id) else { return };
        let before = self.clips[index].clone();
        let mut clip = before.clone();
        frame(SURFACE, BORDER, 12, 10).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(clips::clip_name(&clip)).family(FontFamily::Name("h700".into())));
                ui.label(RichText::new("Bắt đầu (ms)").color(MUTED).size(12.0));
                ui.add(egui::DragValue::new(&mut clip.start).range(0..=clips::MAX_MS).speed(1.0));
                if !matches!(clip.kind, Kind::Block(_)) {
                    ui.label(RichText::new("Giữ (ms)").color(MUTED).size(12.0));
                    ui.add(egui::DragValue::new(&mut clip.dur).range(1..=clips::MAX_MS).speed(1.0));
                } else {
                    ui.label(RichText::new(format!("Chạy chặn {} ms theo FPS {}", clip.dur, self.fps)).color(MUTED).size(12.0));
                }
                ui.label(RichText::new("Track").color(MUTED).size(12.0));
                let mut track = clip.track + 1;
                ui.add(egui::DragValue::new(&mut track).range(1..=64));
                clip.track = track - 1;
                match &mut clip.kind {
                    Kind::Key(key) => {
                        let label = if key.is_empty() { "Chưa chọn".to_owned() } else { clips::hotkey_label(key) };
                        let capturing = self.capturing_clip.as_deref() == Some(&clip.id);
                        if ui.add(button(if capturing { "Nhấn phím…" } else { &label }, TEAL, SURFACE, TEAL)).clicked() { self.capturing_clip = Some(clip.id.clone()); }
                    }
                    Kind::Mouse(button_name) => {
                        egui::ComboBox::from_id_salt("clip-mouse").selected_text(clips::mouse_label(button_name).to_owned()).show_ui(ui, |ui| {
                            for value in ["left", "right", "middle", "mouse_3", "mouse_4"] { ui.selectable_value(button_name, value.to_owned(), clips::mouse_label(value)); }
                        });
                    }
                    Kind::Block(_) => {}
                }
            });
        });
        if clip != before {
            self.remember();
            clip.start = clips::clamp_ms(clip.start);
            self.clips[index] = clip;
            self.tracks = self.track_count();
        }
    }

    fn compile_preview(&self) -> String {
        if self.clips.is_empty() { return "Chưa có clip.".into(); }
        clips::compile_clips(&self.clips, self.region).iter().map(|step| match step {
            Value::String(function) => format!("{}()   ← khối Skirk", clips::block_label(function)),
            step => match step["type"].as_str().unwrap_or("") {
                "wait" => format!("  chờ {} ms", step["ms"]),
                "loop_start" => "── bắt đầu lặp ──".to_owned(),
                "loop_end" => "── hết vòng, quay lại đầu vùng lặp ──".to_owned(),
                kind => {
                    let target = if kind.starts_with("key_") { clips::hotkey_label(step["key"].as_str().unwrap_or("?")) } else { clips::mouse_label(step["button"].as_str().unwrap_or("left")).to_owned() };
                    format!("{} {target}", if kind.ends_with("_down") { "Nhấn" } else { "Nhả" })
                }
            },
        }).collect::<Vec<_>>().join("\n")
    }

    fn form_ui(&mut self, ui: &mut Ui) -> Option<AdvancedAction> {
        let bold = FontFamily::Name("h700".into());
        ui.label(RichText::new("Tên Combo").family(bold.clone()));
        let name = ui.add(TextEdit::singleline(&mut self.name).hint_text("Ví dụ: Skirk Combo 1").char_limit(80).desired_width(f32::INFINITY).min_size(vec2(0.0, 40.0)).margin(Margin::symmetric(12, 8)).background_color(SURFACE));
        if name.changed() { self.error = None; }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Hotkey").family(bold.clone()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(RichText::new("Không bắt buộc").size(12.0).color(MUTED)));
        });
        ui.horizontal(|ui| {
            let label = if self.capturing_hotkey { "Nhấn một phím hoặc nút chuột… (Esc để hủy)".to_owned() }
                else { self.hotkey.as_deref().map_or("Nhấn để gán phím hoặc nút chuột".to_owned(), clips::hotkey_label) };
            let width = ui.available_width() - 40.0;
            if ui.add(button(&label, TEXT, SURFACE, if self.capturing_hotkey { TEAL } else { BORDER }).min_size(vec2(width, 38.0))).clicked() { self.capturing_hotkey = !self.capturing_hotkey; self.capturing_clip = None; }
            if ui.add(button("×", TEXT, SURFACE, BORDER).min_size(vec2(32.0, 38.0))).on_hover_text("Xóa hotkey").clicked() { self.hotkey = None; self.capturing_hotkey = false; }
        });
        egui::CollapsingHeader::new(RichText::new("Các bước sẽ chạy (xem trước)").family(bold.clone())).show(ui, |ui| {
            ui.label(RichText::new(self.compile_preview()).monospace().color(TEAL).size(12.0));
        });
        let mut result = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("combo chuyên sâu đang chỉnh").color(MUTED).size(12.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(button("Save Combo", BACKGROUND, TEAL, TEAL).min_size(vec2(112.0, 40.0))).clicked() {
                    match self.profile() { Ok(profile) => result = Some(AdvancedAction::Save(profile)), Err(message) => self.error = Some(message) }
                }
            });
        });
        if let Some((message, is_error)) = &self.status { ui.label(RichText::new(message).color(if *is_error { WARNING } else { TEAL })); }
        if let Some(error) = &self.error { ui.label(RichText::new(error).color(DANGER)); }
        result
    }

    fn stored_clips(&self) -> Vec<Value> {
        self.clips.iter().map(|c| {
            let mut out = json!({"id": c.id, "track": c.track, "start": c.start, "dur": c.dur});
            match &c.kind {
                Kind::Key(key) => { out["kind"] = json!("key"); out["key"] = json!(key); }
                Kind::Mouse(button) => { out["kind"] = json!("mouse"); out["button"] = json!(button); }
                Kind::Block(function) => { out["kind"] = json!("block"); out["fn"] = json!(function); }
            }
            out
        }).collect()
    }

    pub fn profile(&self) -> Result<Value, String> {
        let name = self.name.trim();
        if name.is_empty() { return Err("Nhập Tên Combo trước khi lưu.".into()); }
        if self.clips.is_empty() { return Err("Timeline cần ít nhất một clip.".into()); }
        if let Some(problem) = clips::validate(&self.clips, self.region, self.hotkey.as_deref()) { return Err(problem); }
        let steps = clips::compile_clips(&self.clips, self.region);
        let mut profile = self.original.clone();
        profile.insert("id".into(), json!(self.id));
        profile.insert("name".into(), json!(name));
        profile.insert("hotkey".into(), json!(self.hotkey));
        profile.insert("clips".into(), json!(self.stored_clips()));
        profile.insert("loop".into(), self.region.map_or(Value::Null, |r| json!({"start": r.start, "end": r.end})));
        profile.insert("pythonSequence".into(), json!(clips::python_sequence(&steps)));
        profile.insert("timeline".into(), json!(steps));
        Ok(Value::Object(profile))
    }

    // ── Hộp thoại Import / Export ────────────────────────────────────────────
    fn io_modal(&mut self, ctx: &Context) -> Option<Option<AdvancedAction>> {
        let mut io = self.io.take()?;
        let mut close = false;
        let mut import = false;
        let mut refresh = false;
        let bold = FontFamily::Name("h700".into());
        let ghost = Color32::from_rgb(27, 39, 60);
        let fps = self.fps;
        let parsed = if io.text.trim().is_empty() { None } else { Some(clips::parse_macro_text(&io.text, io.tap_ms, fps)) };
        let width = (ctx.content_rect().width() - 80.0).clamp(300.0, 860.0);
        let modal = Modal::new(Id::unique("advanced-io-modal")).frame(frame(BACKGROUND, BORDER, 16, 18)).show(ctx, |ui| {
            ui.set_width(width);
            ui.visuals_mut().override_text_color = Some(TEXT);
            ScrollArea::vertical().max_height((ctx.content_rect().height() - 120.0).max(200.0)).auto_shrink([false, true]).show(ui, |ui| {
                ui.set_width(width);
                ui.label(RichText::new("MACRO SEQUENCE / IMPORT.MD").color(TEAL).size(11.0).family(bold.clone()));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Import / Export chuỗi lệnh Macro").size(22.0).family(bold.clone()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if ui.add(button("×", TEXT, SURFACE, BORDER)).on_hover_text("Đóng (Esc)").clicked() { close = true; } });
                });
                ui.label(RichText::new("Định dạng chuỗi lệnh theo quy tắc import.md (ví dụ: {LMBD}{WAITMS:100}{PRESS}w{WAITMS:50}{HOLDMS:80}e{RELEASE}w{LMBU}, {CTRL}a, {ALT}{F4}). Các sự kiện xảy ra đồng thời sẽ tự động được phân bổ lên nhiều track song song.").color(MUTED).size(12.5));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Chế độ Import").family(bold.clone()));
                    egui::ComboBox::from_id_salt("io-mode").selected_text(if io.replace { "Thay thế timeline hiện tại (từ 0 ms)" } else { "Chèn tại đầu phát hiện tại" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut io.replace, true, "Thay thế timeline hiện tại (từ 0 ms)");
                        ui.selectable_value(&mut io.replace, false, "Chèn tại đầu phát hiện tại");
                    });
                    ui.label(RichText::new("Giữ mặc định khi gõ phím (ms)").family(bold.clone()));
                    ui.add(egui::DragValue::new(&mut io.tap_ms).range(1..=10000));
                });
                // 4 nút: Nạp file, Tải file, Lấy từ Timeline, Copy văn bản.
                ui.horizontal_wrapped(|ui| {
                    let load = ui.add(button("Nạp file .txt", TEXT, ghost, BORDER)).on_hover_text("Đọc chuỗi lệnh từ file .txt / .md").clicked();
                    if load && let Some(path) = rfd::FileDialog::new().add_filter("Văn bản", &["txt", "md"]).pick_file() {
                        match std::fs::read_to_string(&path) {
                            Ok(text) => { io.text = text.trim_start_matches('\u{feff}').to_owned(); self.status = Some((format!("Đã nạp {}.", path.display()), false)); }
                            Err(error) => self.status = Some((format!("Không đọc được file: {error}"), true)),
                        }
                    }
                    if ui.add(button("Tải file .txt", TEXT, ghost, BORDER)).on_hover_text("Tải chuỗi lệnh hiện tại về máy dạng .txt").clicked() {
                        if io.text.trim().is_empty() { self.status = Some(("Chưa có chuỗi lệnh để tải.".into(), true)); }
                        else if let Some(path) = rfd::FileDialog::new().set_file_name("macro-sequence.txt").add_filter("Văn bản", &["txt"]).save_file() {
                            self.status = Some(match std::fs::write(&path, io.text.as_bytes()) { Ok(()) => (format!("Đã lưu {}.", path.display()), false), Err(error) => (format!("Không ghi được file: {error}"), true) });
                        }
                    }
                    if ui.add(button("Lấy từ Timeline", TEXT, ghost, BORDER)).on_hover_text("Tạo lại chuỗi lệnh từ timeline hiện tại").clicked() { refresh = true; }
                    if ui.add(button("Copy văn bản", TEXT, ghost, BORDER)).on_hover_text("Sao chép văn bản vào bộ nhớ tạm").clicked() {
                        if io.text.is_empty() { self.status = Some(("Chưa có văn bản để sao chép.".into(), true)); }
                        else { ui.ctx().copy_text(io.text.clone()); self.status = Some(("Đã sao chép chuỗi lệnh.".into(), false)); }
                    }
                });
                ui.label(RichText::new("Chuỗi lệnh Macro (Import / Export)").family(bold.clone()));
                ui.add(TextEdit::multiline(&mut io.text).code_editor().desired_rows(7).desired_width(f32::INFINITY)
                    .hint_text("Ví dụ: {LMBD}{WAITMS:100}{PRESS}w{WAITMS:150}{RELEASE}w{LMBU} hoặc {CTRL}a{WAITMS:50}{HOLDMS:100}r").background_color(SURFACE));
                match &parsed {
                    Some(Err(error)) => { ui.label(RichText::new(error).color(DANGER)); }
                    Some(Ok(imported)) => {
                        ui.label(RichText::new(format!("Hợp lệ: {} clip, {} track song song, tổng {} ms.", imported.clips.len(), imported.max_concurrent_tracks, imported.total_ms)).color(TEAL));
                        for note in &imported.notes { ui.label(RichText::new(note).color(WARNING).size(12.0)); }
                        if !imported.system_tags.is_empty() {
                            ui.label(RichText::new(format!("Bỏ qua {} tag hệ thống (không chạy trong combo): {}", imported.system_tags.len(), imported.system_tags.join(" "))).color(WARNING).size(12.0));
                        }
                    }
                    None => { ui.label(RichText::new("Dán chuỗi lệnh vào đây hoặc bấm Lấy từ Timeline.").color(MUTED).size(12.0)); }
                }
                egui::CollapsingHeader::new(RichText::new("Tra cứu nhanh cú pháp chuẩn (import.md)").family(bold.clone())).default_open(io.cheat_open).show(ui, |ui| {
                    for line in [
                        "Modifier (kế tiếp): {CTRL} {RCTRL} {ALT} {RALT} {SHIFT} {RSHIFT} {LWIN} {RWIN} {APPS}",
                        "Chuột: {LMB} {RMB} {MMB} {MB4} {MB5} · Nhấn/thả: {LMBD}/{LMBU}, {RMBD}/{RMBU}…",
                        "Thời gian: {WAITMS:n} {WAIT:n} {WAITMS:x-y} · Giữ phím kế: {HOLDMS:n} {HOLD:n}",
                        "Nhấn / Thả đa track: {PRESS}...{RELEASE}... (bắt buộc có cặp đóng mở tương ứng)",
                        "Phím đặc biệt: {SPACE} {RETURN} {ESCAPE} {TAB} {BACKSPACE} {DEL} {INS} {HOME} {END} {PGUP} {PGDN}",
                        "Điều hướng & Fn: {UP} {DOWN} {LEFT} {RIGHT} · {F1}…{F24} · {NUM0}…{NUM9}",
                    ] { ui.label(RichText::new(line).monospace().size(11.5).color(MUTED)); }
                });
                if let Some((message, is_error)) = &self.status { ui.label(RichText::new(message).color(if *is_error { WARNING } else { TEAL })); }
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let ready = matches!(parsed, Some(Ok(_)));
                        if ui.add_enabled(ready, button("Import vào Timeline", BACKGROUND, TEAL, TEAL).min_size(vec2(0.0, 40.0))).clicked() { import = true; }
                        if ui.add(button("Đóng", TEXT, ghost, BORDER).min_size(vec2(0.0, 40.0))).clicked() { close = true; }
                    });
                });
            });
        });
        if refresh { io.text = clips::export_macro_text(&self.clips, fps); if io.text.is_empty() { self.status = Some(("Timeline chưa có clip để xuất.".into(), true)); } }
        if import && let Some(Ok(imported)) = parsed {
            self.remember();
            if io.replace {
                self.clips.clear();
                self.region = None;
                self.playhead = 0;
                self.add_clips(imported.clips, true);
            } else {
                let at = self.playhead;
                let list = imported.clips.into_iter().map(|c| Clip { start: clips::clamp_ms(c.start + at), ..c }).collect();
                self.add_clips(list, true);
            }
            self.status = Some((format!("Đã import {} ms vào timeline.", imported.total_ms), false));
            close = true;
        }
        if modal.should_close() { close = true; }
        if !close { self.io = Some(io); }
        Some(None)
    }
}

fn short_mouse(button: &str) -> &str {
    match button { "left" => "LC", "right" => "RC", "middle" => "MC", "mouse_3" => "M3", "mouse_4" => "M4", other => other }
}

#[derive(Clone, Copy)]
enum Tool { Undo, Redo, Save, Io, Cut, Copy, Paste, Dup, Razor, Trash, Clear, LoopIn, LoopOut, LoopOff, Play, Stop, Record, Magnet, ZoomOut, ZoomIn, Fit, Key, Mouse, Gap, Tracks, Skirk }

fn tool_button(ui: &mut Ui, tool: Tool, tip: &str, enabled: bool, active: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(32.0, 34.0), if enabled { Sense::click() } else { Sense::hover() });
    let painter = ui.painter();
    if active { painter.rect_filled(rect, CornerRadius::same(10), Color32::from_rgba_unmultiplied(45, 212, 191, 40)); }
    else if response.hovered() && enabled { painter.rect_filled(rect, CornerRadius::same(10), Color32::from_rgb(27, 39, 60)); }
    let color = if !enabled { Color32::from_rgb(70, 82, 104) } else if active { TEAL } else if matches!(tool, Tool::Record) { Color32::from_rgb(226, 85, 75) } else { Color32::from_rgb(200, 212, 232) };
    draw_tool(painter, tool, rect.center(), 17.0, color);
    response.on_hover_text(tip)
}

fn label_button(ui: &mut Ui, tool: Tool, label: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::new(14.0, FontFamily::Name("h600".into())), TEXT);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 52.0, 40.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(10), if response.hovered() { Color32::from_rgb(27, 39, 60) } else { Color32::from_rgb(8, 13, 24) });
    painter.rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0, BORDER), StrokeKind::Inside);
    draw_tool(painter, tool, pos2(rect.left() + 22.0, rect.center().y), 16.0, Color32::from_rgb(200, 212, 232));
    painter.galley(pos2(rect.left() + 38.0, rect.center().y - galley.size().y / 2.0), galley, TEXT);
    response
}

/// Icon vẽ bằng nét, thay cho bộ SVG của bản gốc.
fn draw_tool(painter: &egui::Painter, tool: Tool, c: Pos2, size: f32, color: Color32) {
    let k = size / 24.0;
    let p = |x: f32, y: f32| pos2(c.x + (x - 12.0) * k, c.y + (y - 12.0) * k);
    let stroke = Stroke::new((1.8 * k).max(1.2), color);
    let line = |points: &[Pos2]| { painter.add(egui::Shape::line(points.to_vec(), stroke)); };
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| {
        let pts: Vec<Pos2> = (0..=20).map(|i| { let a = (from + (to - from) * i as f32 / 20.0).to_radians(); p(cx + r * a.cos(), cy + r * a.sin()) }).collect();
        painter.add(egui::Shape::line(pts, stroke));
    };
    let rrect = |x0: f32, y0: f32, x1: f32, y1: f32| { painter.rect_stroke(Rect::from_min_max(p(x0, y0), p(x1, y1)), CornerRadius::same((2.0 * k) as u8), stroke, StrokeKind::Inside); };
    match tool {
        Tool::Undo => { arc(12.0, 13.0, 7.0, 180.0, 400.0); line(&[p(4.0, 5.0), p(5.0, 10.5), p(10.0, 9.0)]); }
        Tool::Redo => { arc(12.0, 13.0, 7.0, 140.0, 360.0); line(&[p(20.0, 5.0), p(19.0, 10.5), p(14.0, 9.0)]); }
        Tool::Save => { line(&[p(4.0, 4.0), p(17.0, 4.0), p(20.0, 7.0), p(20.0, 20.0), p(4.0, 20.0), p(4.0, 4.0)]); line(&[p(8.0, 4.0), p(8.0, 9.0), p(15.0, 9.0), p(15.0, 4.0)]); painter.circle_stroke(p(12.0, 15.0), 2.5 * k, stroke); }
        Tool::Io => { line(&[p(7.0, 4.0), p(7.0, 18.0)]); line(&[p(3.0, 8.0), p(7.0, 4.0), p(11.0, 8.0)]); line(&[p(17.0, 20.0), p(17.0, 6.0)]); line(&[p(13.0, 16.0), p(17.0, 20.0), p(21.0, 16.0)]); }
        Tool::Cut => { painter.circle_stroke(p(6.0, 17.0), 2.8 * k, stroke); painter.circle_stroke(p(18.0, 17.0), 2.8 * k, stroke); line(&[p(7.5, 14.5), p(18.0, 4.0)]); line(&[p(16.5, 14.5), p(6.0, 4.0)]); }
        Tool::Copy => { rrect(9.0, 9.0, 20.0, 20.0); line(&[p(15.0, 9.0), p(15.0, 5.0), p(5.0, 5.0), p(5.0, 15.0), p(9.0, 15.0)]); }
        Tool::Paste => { rrect(5.0, 5.0, 19.0, 21.0); rrect(9.0, 3.0, 15.0, 7.0); line(&[p(8.0, 12.0), p(16.0, 12.0)]); line(&[p(8.0, 16.0), p(14.0, 16.0)]); }
        Tool::Dup => { rrect(3.0, 8.0, 16.0, 21.0); line(&[p(8.0, 8.0), p(8.0, 3.0), p(21.0, 3.0), p(21.0, 16.0), p(16.0, 16.0)]); }
        Tool::Razor => { line(&[p(12.0, 3.0), p(12.0, 21.0)]); line(&[p(5.0, 12.0), p(9.0, 12.0)]); line(&[p(15.0, 12.0), p(19.0, 12.0)]); line(&[p(7.0, 9.0), p(9.0, 12.0), p(7.0, 15.0)]); line(&[p(17.0, 9.0), p(15.0, 12.0), p(17.0, 15.0)]); }
        Tool::Trash => { line(&[p(4.0, 7.0), p(20.0, 7.0)]); line(&[p(9.0, 7.0), p(9.0, 4.0), p(15.0, 4.0), p(15.0, 7.0)]); line(&[p(6.0, 7.0), p(7.0, 20.0), p(17.0, 20.0), p(18.0, 7.0)]); }
        Tool::Clear => { line(&[p(4.0, 7.0), p(20.0, 7.0)]); line(&[p(6.0, 7.0), p(7.0, 20.0), p(17.0, 20.0), p(18.0, 7.0)]); line(&[p(10.0, 11.0), p(14.0, 16.0)]); line(&[p(14.0, 11.0), p(10.0, 16.0)]); }
        Tool::LoopIn => { line(&[p(9.0, 4.0), p(5.0, 4.0), p(5.0, 20.0), p(9.0, 20.0)]); line(&[p(10.0, 12.0), p(19.0, 12.0)]); }
        Tool::LoopOut => { line(&[p(15.0, 4.0), p(19.0, 4.0), p(19.0, 20.0), p(15.0, 20.0)]); line(&[p(14.0, 12.0), p(5.0, 12.0)]); }
        Tool::LoopOff => { arc(12.0, 14.0, 7.0, 200.0, 340.0); line(&[p(19.0, 8.0), p(19.5, 12.5), p(15.0, 12.0)]); }
        Tool::Play => { painter.add(egui::Shape::convex_polygon(vec![p(7.0, 4.0), p(19.0, 12.0), p(7.0, 20.0)], Color32::TRANSPARENT, stroke)); }
        Tool::Stop => { painter.rect_filled(Rect::from_min_max(p(6.0, 6.0), p(18.0, 18.0)), CornerRadius::same(2), color); }
        Tool::Record => { painter.circle_filled(p(12.0, 12.0), 5.5 * k, color); }
        Tool::Magnet => { arc(12.0, 11.0, 7.0, 180.0, 360.0); line(&[p(5.0, 11.0), p(5.0, 18.0), p(9.0, 18.0), p(9.0, 11.0)]); line(&[p(15.0, 11.0), p(15.0, 18.0), p(19.0, 18.0), p(19.0, 11.0)]); }
        Tool::ZoomOut => { painter.circle_stroke(p(10.5, 10.5), 6.5 * k, stroke); line(&[p(15.5, 15.5), p(21.0, 21.0)]); line(&[p(7.5, 10.5), p(13.5, 10.5)]); }
        Tool::ZoomIn => { painter.circle_stroke(p(10.5, 10.5), 6.5 * k, stroke); line(&[p(15.5, 15.5), p(21.0, 21.0)]); line(&[p(7.5, 10.5), p(13.5, 10.5)]); line(&[p(10.5, 7.5), p(10.5, 13.5)]); }
        Tool::Fit => { for (x, y, dx, dy) in [(4.0, 4.0, 1.0, 1.0), (20.0, 4.0, -1.0, 1.0), (4.0, 20.0, 1.0, -1.0), (20.0, 20.0, -1.0, -1.0)] { line(&[p(x + 5.0 * dx, y), p(x, y), p(x, y + 5.0 * dy)]); } }
        Tool::Key => { rrect(3.0, 6.0, 21.0, 18.0); line(&[p(9.0, 12.0), p(15.0, 12.0)]); line(&[p(13.0, 9.0), p(16.0, 12.0), p(13.0, 15.0)]); }
        Tool::Skirk => { line(&[p(12.0, 3.0), p(20.0, 7.5), p(20.0, 16.5), p(12.0, 21.0), p(4.0, 16.5), p(4.0, 7.5), p(12.0, 3.0)]); line(&[p(4.0, 7.5), p(12.0, 12.0), p(20.0, 7.5)]); line(&[p(12.0, 12.0), p(12.0, 21.0)]); }
        Tool::Mouse => { rrect(7.0, 3.0, 17.0, 21.0); line(&[p(12.0, 3.0), p(12.0, 10.0)]); }
        Tool::Gap => { line(&[p(4.0, 5.0), p(4.0, 19.0)]); line(&[p(20.0, 5.0), p(20.0, 19.0)]); line(&[p(8.0, 12.0), p(16.0, 12.0)]); line(&[p(11.0, 9.0), p(8.0, 12.0), p(11.0, 15.0)]); line(&[p(13.0, 9.0), p(16.0, 12.0), p(13.0, 15.0)]); }
        Tool::Tracks => { line(&[p(4.0, 7.0), p(20.0, 7.0)]); line(&[p(4.0, 12.0), p(14.0, 12.0)]); line(&[p(4.0, 17.0), p(14.0, 17.0)]); line(&[p(18.0, 14.0), p(18.0, 20.0)]); line(&[p(15.0, 17.0), p(21.0, 17.0)]); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> Kind { Kind::Key(name.into()) }

    #[test]
    fn recording_puts_overlapping_holds_on_parallel_tracks_and_closes_them_on_release() {
        let (mut clips, mut open) = (Vec::new(), HashMap::new());
        record_down(&mut clips, &mut open, key("w"), 0, 120.0);
        record_down(&mut clips, &mut open, key("e"), 100, 120.0);
        record_up(&mut clips, &mut open, &key("e"), 180);
        record_up(&mut clips, &mut open, &key("w"), 230);
        let mut shape: Vec<_> = clips.iter().map(|c| (c.track, c.start, c.dur)).collect();
        shape.sort();
        assert_eq!(shape, vec![(0, 0, 230), (1, 100, 80)]);
        assert!(open.is_empty());
    }

    #[test]
    fn a_held_key_is_not_duplicated_and_a_finished_track_is_reused() {
        let (mut clips, mut open) = (Vec::new(), HashMap::new());
        record_down(&mut clips, &mut open, key("w"), 0, 120.0);
        record_down(&mut clips, &mut open, key("w"), 10, 120.0);
        assert_eq!(clips.len(), 1);
        record_up(&mut clips, &mut open, &key("w"), 50);
        record_down(&mut clips, &mut open, Kind::Mouse("left".into()), 80, 120.0);
        assert_eq!((clips[1].track, clips[1].start), (0, 80));
        record_up(&mut clips, &mut open, &key("w"), 90);
        assert_eq!(clips[0].dur, 50);
    }

    #[test]
    fn switching_back_to_basic_keeps_blocks_in_time_order_and_drops_the_rest() {
        let mut editor = Advanced::with_basic("c1".into(), "Mẫu".into(), Some("f6".into()), &["skk2a", "skke"], 120.0);
        editor.add_mouse("left");
        let (functions, dropped) = editor.block_functions();
        assert_eq!(functions, vec!["skk2a", "skke"]);
        assert_eq!(dropped, 1);
        let steps: Vec<Value> = functions.iter().map(|f| json!(f)).collect();
        let profile = json!({"id": editor.id(), "name": editor.name(), "hotkey": editor.hotkey(), "timeline": steps});
        assert!(super::super::editor::Editor::new("c1".into(), Some(&profile)).is_ok());
        let empty = Advanced::new("c2".into(), 120.0);
        let (none, zero) = empty.block_functions();
        let profile = json!({"id": "c2", "name": "", "hotkey": null, "timeline": none});
        assert_eq!(zero, 0);
        assert!(super::super::editor::Editor::new("c2".into(), Some(&profile)).is_ok());
    }
}
