//! Mô hình clip nhiều track của editor chuyên sâu và chuyển đổi chuỗi lệnh macro (`import.md`).
//!
//! Port từ `src/UI/cuscombo-advanced.js`. Không phụ thuộc giao diện nên kiểm thử được.
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_MS: i64 = 600_000;
pub const DEFAULT_CLIP_MS: i64 = 100;
pub const DEFAULT_BLOCK_MS: i64 = 300;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Key(String),
    Mouse(String),
    /// Khối Skirk dựng sẵn, lưu theo tên hàm (`skke`, `skk2a`...).
    Block(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clip {
    pub id: String,
    pub kind: Kind,
    pub track: usize,
    pub start: i64,
    pub dur: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loop { pub start: i64, pub end: i64 }

impl Clip {
    pub fn end(&self) -> i64 { self.start + self.dur }
    pub fn new(kind: Kind, track: usize, start: i64, dur: i64, fps: f64) -> Self {
        let mut clip = Self { id: next_id(), kind, track, start: clamp_ms(start), dur: clamp_ms(dur).max(1) };
        if let Kind::Block(function) = &clip.kind {
            if let Some(ms) = block_duration_ms(function, fps) { clip.dur = ms; }
        }
        clip
    }
    fn target(&self) -> Option<String> {
        match &self.kind {
            Kind::Key(key) if !key.is_empty() => Some(format!("key:{key}")),
            Kind::Key(_) => None,
            Kind::Mouse(button) => Some(format!("mouse:{button}")),
            Kind::Block(_) => None,
        }
    }
}

pub fn next_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    format!("clip-{}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed))
}

pub fn clamp_ms(value: i64) -> i64 { value.clamp(0, MAX_MS) }

// ── Thời lượng khối Skirk (chép từ macros.py / fps_utils.py) ─────────────────
type Table = (&'static [f64], &'static [f64]);
const T_3AW: Table = (&[30.0, 60.0, 100.0, 120.0, 220.0], &[1.5, 1.366, 1.34, 1.32, 1.285]);
const T_2AS_FIRST: Table = (&[60.0, 120.0, 220.0], &[0.44, 0.40, 0.37]);
const T_2AS_SECOND: Table = (&[60.0, 120.0, 220.0], &[0.63, 0.57, 0.56]);
const T_2AZ_FIRST: Table = (&[60.0, 120.0, 220.0], &[0.24, 0.22, 0.20]);
const T_2AZ_END: Table = (&[60.0, 120.0, 220.0], &[1.16, 1.12, 1.09]);
const T_2AZS_END: Table = (&[60.0, 120.0, 220.0], &[0.90, 0.86, 0.82]);
const T_2AQ_END: Table = (&[60.0, 120.0, 220.0], &[1.17, 1.09, 1.07]);
const T_5A: Table = (&[60.0, 120.0, 220.0], &[2.22, 2.14, 2.12]);
const T_5AS_END: Table = (&[60.0, 120.0, 220.0], &[2.41, 2.28, 2.24]);
const F_3AS: Table = (&[60.0, 120.0, 144.0, 240.0], &[18.0, 20.0, 22.0, 24.0]);

fn fps2t((fps_values, values): Table, fps: f64) -> f64 {
    let last = fps_values.len() - 1;
    if fps >= fps_values[last] { return values[last]; }
    if fps <= fps_values[0] { return values[0]; }
    let mut i = 0;
    while i < fps_values.len() - 2 && fps >= fps_values[i + 1] { i += 1; }
    let weight = (fps - fps_values[i]) / (fps_values[i + 1] - fps_values[i]);
    values[i] + (values[i + 1] - values[i]) * weight
}

fn spam_time(count: i64, step: f64, limit: f64) -> f64 {
    for k in 1..=count {
        if k as f64 * step >= limit - 1e-9 { return k as f64 * step; }
    }
    count as f64 * step
}

fn block_seconds(function: &str, fps: f64) -> Option<f64> {
    let floor = |factor: f64| (factor * fps).floor() as i64;
    Some(match function {
        "skke" => spam_time(floor(0.1), 2.0 / fps, 0.1) + 0.19,
        "skk2a" => spam_time(floor(0.26), 2.0 / fps, 0.32).max(fps2t(T_2AS_FIRST, fps)),
        "skk2as" => (fps2t(T_2AS_FIRST, fps) + 3.0 / fps).max(fps2t(T_2AS_SECOND, fps)),
        "skk2az" => (fps2t(T_2AZ_FIRST, fps) + 2.0 / fps + 0.44).max(fps2t(T_2AZ_END, fps)),
        "skk2azs" => (fps2t(T_2AZ_FIRST, fps) + 2.0 / fps + 0.44 + 5.0 / fps).max(fps2t(T_2AZS_END, fps)),
        "skk2azs_slow" => if fps < 105.0 { block_seconds("skk2azs", fps)? } else { 0.87 },
        "skk2aq" => fps2t(T_2AQ_END, fps),
        "skk3as" => spam_time(floor(0.6), 3.0 / fps, 0.6) + fps2t(F_3AS, fps) / fps,
        "skk3aw" => fps2t(T_3AW, fps) + 1.0 / fps,
        "skk5as" => fps2t(T_5AS_END, fps) + 2.0 / fps,
        "skk5a" => fps2t(T_5A, fps) + 2.0 / fps,
        _ => return None,
    })
}

/// Thời lượng cố định của khối ở `fps`, hoặc `None` nếu khối không có bảng.
pub fn block_duration_ms(function: &str, fps: f64) -> Option<i64> {
    if let Some(ms) = function.strip_prefix("wait_").and_then(|n| n.parse::<i64>().ok()) { return Some(ms); }
    block_seconds(function, fps).map(|s| ((s * 1000.0).round() as i64).max(1))
}

/// Tên hàm khối hợp lệ (11 khối Skirk và các khối cũ vẫn giữ để mở lại combo đã lưu).
pub fn is_known_block(function: &str) -> bool {
    matches!(function, "skke" | "skk2a" | "skk2as" | "skk2az" | "skk2azs" | "skk2azs_slow" | "skk2aq" | "skk3as" | "skk3aw" | "skk5as" | "skk5a"
        | "mav_cdcdcf" | "mav_b_d" | "mav_c_hold" | "mav_c_rel" | "mav_d" | "mav_q" | "mav_b_click" | "mav_b_click2" | "mav_b_c" | "mav_b_cd" | "mav_b_cdf" | "mav_b_q")
        || function.strip_prefix("wait_").is_some_and(|n| n.parse::<u32>().is_ok())
}

// ── Biên dịch clip ⇄ bước phẳng ──────────────────────────────────────────────
/// Cùng thời điểm: nhả trước, rồi mốc lặp, rồi mới nhấn (giống `compileClips`).
pub fn compile_clips(clips: &[Clip], loop_region: Option<Loop>) -> Vec<Value> {
    struct Event { t: i64, order: u8, track: i64, step: Value, block_dur: i64 }
    let mut events = Vec::new();
    for clip in clips {
        match &clip.kind {
            Kind::Block(function) => events.push(Event { t: clip.start, order: 3, track: clip.track as i64, step: json!(function), block_dur: clip.dur }),
            Kind::Key(key) => {
                events.push(Event { t: clip.start, order: 3, track: clip.track as i64, step: json!({"type": "key_down", "key": key}), block_dur: 0 });
                events.push(Event { t: clip.end(), order: 0, track: clip.track as i64, step: json!({"type": "key_up", "key": key}), block_dur: 0 });
            }
            Kind::Mouse(button) => {
                events.push(Event { t: clip.start, order: 3, track: clip.track as i64, step: json!({"type": "mouse_down", "button": button}), block_dur: 0 });
                events.push(Event { t: clip.end(), order: 0, track: clip.track as i64, step: json!({"type": "mouse_up", "button": button}), block_dur: 0 });
            }
        }
    }
    if let Some(region) = loop_region {
        events.push(Event { t: region.start, order: 2, track: -1, step: json!({"type": "loop_start"}), block_dur: 0 });
        events.push(Event { t: region.end, order: 1, track: -1, step: json!({"type": "loop_end"}), block_dur: 0 });
    }
    events.sort_by_key(|e| (e.t, e.order, e.track));
    let mut steps = Vec::new();
    let mut cursor = 0;
    for event in events {
        let gap = event.t - cursor;
        if gap > 0 { steps.push(json!({"type": "wait", "ms": gap})); }
        steps.push(event.step);
        cursor = cursor.max(event.t) + event.block_dur;
    }
    steps
}

/// `pythonSequence` mà backend đọc: xen `is_no_key_pressed` giữa các bước.
pub fn python_sequence(steps: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        out.push(step.clone());
        if index + 1 != steps.len() { out.push(json!("is_no_key_pressed")); }
    }
    out
}

/// Bước phẳng của combo cũ → clip (chờ cộng dồn, Nhấn mở clip, Nhả đóng clip).
pub fn steps_to_clips(steps: &[Value], fps: f64) -> (Vec<Clip>, Option<Loop>) {
    let mut clips: Vec<Clip> = Vec::new();
    let mut open: HashMap<String, usize> = HashMap::new();
    let (mut t, mut loop_start, mut region) = (0i64, None, None);
    fn close(clips: &mut [Clip], open: &mut HashMap<String, usize>, id: &str, at: i64) {
        if let Some(index) = open.remove(id) { clips[index].dur = (at - clips[index].start).max(1); }
    }
    for item in steps {
        if let Some(function) = item.as_str() {
            if !is_known_block(function) { continue; }
            if let Some(ms) = function.strip_prefix("wait_").and_then(|n| n.parse::<i64>().ok()) { t += ms; continue; }
            let dur = block_duration_ms(function, fps).unwrap_or(DEFAULT_BLOCK_MS);
            clips.push(Clip { id: next_id(), kind: Kind::Block(function.into()), track: 0, start: t, dur });
            t += dur;
            continue;
        }
        let Some(kind) = item["type"].as_str() else { continue };
        match kind {
            "wait" => { t += item["ms"].as_f64().unwrap_or(0.0).max(0.0) as i64; continue; }
            "loop_start" => { loop_start = Some(t); continue; }
            "loop_end" => { if let Some(start) = loop_start { region = Some(Loop { start, end: t.max(start + 1) }); } continue; }
            _ => {}
        }
        let is_key = kind.starts_with("key_");
        let target = if is_key { item["key"].as_str().unwrap_or("").to_owned() } else { item["button"].as_str().filter(|b| !b.is_empty()).unwrap_or("left").to_owned() };
        let id = format!("{}:{target}", if is_key { "key" } else { "mouse" });
        let make = |start, dur| Clip { id: next_id(), kind: if is_key { Kind::Key(target.clone()) } else { Kind::Mouse(target.clone()) }, track: 0, start, dur };
        match kind {
            "key_tap" | "mouse_click" => {
                let hold = item["hold"].as_f64().filter(|h| *h > 0.0).map_or(50, |h| h as i64).max(1);
                close(&mut clips, &mut open, &id, t);
                clips.push(make(t, hold));
                t += hold;
            }
            "key_down" | "mouse_down" => {
                close(&mut clips, &mut open, &id, t);
                clips.push(make(t, 1));
                open.insert(id, clips.len() - 1);
            }
            "key_up" | "mouse_up" => close(&mut clips, &mut open, &id, t),
            _ => {}
        }
    }
    let ids: Vec<String> = open.keys().cloned().collect();
    for id in ids { let at = t.max(clips[open[&id]].start + 1); close(&mut clips, &mut open, &id, at); }
    // Mỗi clip vào track thấp nhất còn trống.
    let mut order: Vec<usize> = (0..clips.len()).collect();
    order.sort_by_key(|&i| clips[i].start);
    let mut ends: Vec<i64> = Vec::new();
    for index in order {
        let start = clips[index].start;
        let track = ends.iter().position(|end| *end <= start).unwrap_or_else(|| { ends.push(0); ends.len() - 1 });
        clips[index].track = track;
        ends[track] = clips[index].end();
    }
    (clips, region)
}

/// Clip bị đè hoặc nằm trong khối chạy chặn (viền đỏ trong editor gốc).
pub fn clashing_ids(clips: &[Clip]) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut by_target: BTreeMap<String, Vec<&Clip>> = BTreeMap::new();
    for clip in clips { if let Some(target) = clip.target() { by_target.entry(target).or_default().push(clip); } }
    let overlaps = |a: &Clip, b: &Clip| a.start < b.end() && b.start < a.end();
    for list in by_target.values() {
        for i in 0..list.len() { for j in i + 1..list.len() {
            if overlaps(list[i], list[j]) { ids.insert(list[i].id.clone()); ids.insert(list[j].id.clone()); }
        } }
    }
    for block in clips.iter().filter(|c| matches!(c.kind, Kind::Block(_))) {
        let inside = |t: i64| t >= block.start && t < block.end();
        for clip in clips {
            if clip.id != block.id && (inside(clip.start) || (clip.end() > block.start && inside(clip.end()))) {
                ids.insert(block.id.clone()); ids.insert(clip.id.clone());
            }
        }
    }
    ids
}

/// Kiểm tra trước khi lưu; trả về câu báo lỗi đầu tiên.
pub fn validate(clips: &[Clip], loop_region: Option<Loop>, hotkey: Option<&str>) -> Option<String> {
    let name = |clip: &Clip| format!("Clip {} ở {} ms (T{})", clip_name(clip), clip.start, clip.track + 1);
    for clip in clips {
        match &clip.kind {
            Kind::Key(key) if key.is_empty() => return Some(format!("{} chưa chọn phím. Nhấp đúp vào clip để chọn.", name(clip))),
            Kind::Block(function) if !is_known_block(function) => return Some(format!("{} không nhận ra được, hãy xóa.", name(clip))),
            _ => {}
        }
        if clip.dur < 1 { return Some(format!("{}: thời gian giữ phải từ 1 ms.", name(clip))); }
    }
    let clashes = clashing_ids(clips);
    if !clashes.is_empty() {
        if clips.iter().any(|c| matches!(c.kind, Kind::Block(_)) && clashes.contains(&c.id)) {
            return Some("Khối Skirk chạy chặn: không đặt clip khác nhấn / nhả bên trong khối (các clip viền đỏ).".into());
        }
        let clip = clips.iter().find(|c| clashes.contains(&c.id) && !matches!(c.kind, Kind::Block(_)))?;
        return Some(format!("{} bị giữ chồng lên chính nó ở hai track (viền đỏ). Dời để hai clip không trùng thời gian.", clip_name(clip)));
    }
    if let Some(Loop { start, end }) = loop_region {
        if let Some(clip) = clips.iter().find(|c| (c.start < start && c.end() > start) || (c.start < end && c.end() > end)) {
            return Some(format!("{} vắt qua mép vùng lặp. Cắt clip (C) tại mép hoặc dời cho nằm hẳn trong / ngoài vùng lặp.", name(clip)));
        }
        if let Some(clip) = clips.iter().find(|c| c.start >= end) {
            return Some(format!("{} nằm sau vùng lặp nên không bao giờ chạy (vùng lặp chạy mãi tới khi nhả hotkey).", name(clip)));
        }
        if !clips.iter().any(|c| c.start >= start && c.end() <= end) { return Some("Vùng lặp đang trống, hãy đặt clip vào trong.".into()); }
    }
    if let Some(hotkey) = hotkey.filter(|h| !h.is_empty()) {
        let clash = clips.iter().find(|c| match &c.kind { Kind::Key(k) => k == hotkey, Kind::Mouse(b) => b == hotkey, Kind::Block(_) => false });
        if let Some(clip) = clash {
            return Some(format!("{} bấm chính hotkey {} của combo, sẽ tự kích hoạt lại. Đổi hotkey hoặc clip đó.", name(clip), hotkey_label(hotkey)));
        }
    }
    None
}

pub fn mouse_label(button: &str) -> &str {
    match button { "left" => "Chuột trái", "right" => "Chuột phải", "middle" => "Chuột giữa", "mouse_3" => "Chuột phụ 1 (mouse_3)", "mouse_4" => "Chuột phụ 2 (mouse_4)", other => other }
}

pub fn clip_name(clip: &Clip) -> String {
    match &clip.kind {
        Kind::Key(key) if key.is_empty() => "?".into(),
        Kind::Key(key) => hotkey_label(key),
        Kind::Mouse(button) => mouse_label(button).into(),
        Kind::Block(function) => format!("Khối {}", block_label(function)),
    }
}

pub fn block_label(function: &str) -> String {
    match function {
        "skke" => "E", "skk2a" => "n2", "skk2as" => "n2d", "skk2az" => "n2c", "skk2azs" => "n2cd", "skk2azs_slow" => "n2cd_slow",
        "skk2aq" => "n2q", "skk3as" => "n3d", "skk3aw" => "n3w", "skk5as" => "n5d", "skk5a" => "n5",
        other => other,
    }.into()
}

/// Nhãn dễ đọc của một phím (giống `hotkeyLabel`).
pub fn hotkey_label(key: &str) -> String {
    let named = match key {
        "shift" => "Shift", "shift_r" => "Right Shift", "ctrl" => "Ctrl", "ctrl_r" => "Right Ctrl", "alt" => "Alt", "alt_gr" => "Right Alt",
        "cmd" => "LWin", "cmd_r" => "RWin", "menu" => "Apps", "caps_lock" => "CapsLock", "num_lock" => "NumLock", "scroll_lock" => "ScrollLock",
        "space" => "Space", "enter" => "Enter", "esc" => "Esc", "delete" => "Delete", "insert" => "Insert", "home" => "Home", "end" => "End",
        "page_up" => "PgUp", "page_down" => "PgDn", "backspace" => "Backspace", "tab" => "Tab", "print_screen" => "PrtScn", "pause" => "Pause",
        "break" => "Break", "ctrl_break" => "Ctrl+Break", "up" => "↑", "down" => "↓", "left" => "←", "right" => "→",
        "media_volume_up" => "Vol+", "media_volume_down" => "Vol-", "media_volume_mute" => "Mute", "media_play_pause" => "MediaPlay",
        "media_stop" => "MediaStop", "media_next" => "MediaNext", "media_previous" => "MediaPrev",
        "num_add" => "Num+", "num_sub" => "Num-", "num_dec" => "Num.", "num_div" => "Num/", "num_mul" => "Num*", "num_enter" => "NumEnter",
        _ => "",
    };
    if !named.is_empty() { return named.into(); }
    let lower = key.to_ascii_lowercase();
    if let Some(digit) = lower.strip_prefix("num_").filter(|d| d.len() == 1 && d.chars().all(|c| c.is_ascii_digit())) { return format!("Num{digit}"); }
    if lower.starts_with('f') && lower[1..].chars().all(|c| c.is_ascii_digit()) && lower.len() > 1 { return key.to_uppercase(); }
    if ["vkc:", "ext:", "sc:", "sce:"].iter().any(|p| lower.starts_with(p)) { return key.to_uppercase(); }
    if key.chars().count() == 1 { key.to_uppercase() } else { key.into() }
}

// ── Bảng tag của import.md ──────────────────────────────────────────────────
const MODIFIERS: [(&str, &str); 9] = [("CTRL", "ctrl"), ("RCTRL", "ctrl_r"), ("ALT", "alt"), ("RALT", "alt_gr"), ("SHIFT", "shift"), ("RSHIFT", "shift_r"), ("LWIN", "cmd"), ("RWIN", "cmd_r"), ("APPS", "menu")];

fn modifier_key(tag: &str) -> Option<&'static str> { MODIFIERS.iter().find(|(t, _)| *t == tag).map(|(_, k)| *k) }
fn modifier_tag(key: &str) -> Option<&'static str> { MODIFIERS.iter().find(|(_, k)| *k == key).map(|(t, _)| *t) }

const SPECIAL: &[(&str, &str)] = &[
    ("DEL", "delete"), ("DELETE", "delete"), ("INS", "insert"), ("PGUP", "page_up"), ("PGDN", "page_down"), ("HOME", "home"), ("END", "end"),
    ("RETURN", "enter"), ("ENTER", "enter"), ("ESCAPE", "esc"), ("ESC", "esc"), ("BACKSPACE", "backspace"), ("TAB", "tab"), ("PRTSCN", "print_screen"),
    ("PAUSE", "pause"), ("SPACE", "space"), ("CAPSLOCK", "caps_lock"), ("NUMLOCK", "num_lock"), ("SCROLLLOCK", "scroll_lock"), ("BREAK", "break"),
    ("CTRLBREAK", "ctrl_break"), ("UP", "up"), ("DOWN", "down"), ("LEFT", "left"), ("RIGHT", "right"), ("VOL+", "media_volume_up"),
    ("VOL-", "media_volume_down"), ("MUTE", "media_volume_mute"), ("MEDIAPLAY", "media_play_pause"), ("MEDIASTOP", "media_stop"),
    ("MEDIANEXT", "media_next"), ("MEDIAPREV", "media_previous"), ("NUM+", "num_add"), ("NUM-", "num_sub"), ("NUM.", "num_dec"), ("NUM/", "num_div"),
    ("NUM*", "num_mul"), ("NUMENTER", "num_enter"), ("BACK", "browser_back"), ("FORWARD", "browser_forward"), ("STOP", "browser_stop"),
    ("REFRESH", "browser_refresh"), ("WEBHOME", "browser_home"), ("SEARCH", "browser_search"), ("FAVORITES", "browser_favorites"),
    ("NUMLOCKON", "num_lock_on"), ("NUMLOCKOFF", "num_lock_off"), ("CAPSLOCKON", "caps_lock_on"), ("CAPSLOCKOFF", "caps_lock_off"),
    ("SCROLLLOCKON", "scroll_lock_on"), ("SCROLLLOCKOFF", "scroll_lock_off"),
];

fn special_key(tag: &str) -> Option<String> {
    if let Some((_, key)) = SPECIAL.iter().find(|(t, k)| *t == tag && !k.is_empty()) { return Some((*key).to_owned()); }
    if let Some(n) = tag.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()).filter(|n| (1..=24).contains(n)) { return Some(format!("f{n}")); }
    if let Some(d) = tag.strip_prefix("NUM").filter(|d| d.len() == 1 && d.chars().all(|c| c.is_ascii_digit())) { return Some(format!("num_{d}")); }
    None
}

fn canonical_tag(key: &str) -> Option<String> {
    if let Some(tag) = modifier_tag(key) { return Some(tag.to_owned()); }
    // Chiều ngược của `SPECIAL`, ưu tiên tên trong import.md.
    const PREFERRED: [&str; 7] = ["DEL", "RETURN", "ESCAPE", "INS", "PGUP", "PGDN", "UP"];
    for tag in PREFERRED { if special_key(tag).as_deref() == Some(key) { return Some(tag.to_owned()); } }
    if let Some((tag, _)) = SPECIAL.iter().find(|(_, k)| *k == key) { return Some((*tag).to_owned()); }
    if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()).filter(|n| (1..=24).contains(n)) { return Some(format!("F{n}")); }
    key.strip_prefix("num_").filter(|d| d.len() == 1 && d.chars().all(|c| c.is_ascii_digit())).map(|d| format!("NUM{d}"))
}

struct MouseSpec { button: &'static str, action: &'static str, down: &'static str, up: &'static str }

fn mouse_spec(tag: &str) -> Option<MouseSpec> {
    let (button, down, up, rest) = if let Some((prefix, rest)) = ["LMB", "RMB", "MMB"].iter().find_map(|p| tag.strip_prefix(p).map(|r| (*p, r))) {
        match prefix { "LMB" => ("left", "LMBD", "LMBU", rest), "RMB" => ("right", "RMBD", "RMBU", rest), _ => ("middle", "MMBD", "MMBU", rest) }
    } else if let Some(rest) = ["MB4", "XMB1"].iter().find_map(|p| tag.strip_prefix(p)) {
        ("mouse_3", "MB4D", "MB4U", rest)
    } else {
        let rest = ["MB5", "XMB2"].iter().find_map(|p| tag.strip_prefix(p))?;
        ("mouse_4", "MB5D", "MB5U", rest)
    };
    let action = match rest { "" | "/XMB1" | "/XMB2" => "tap", "D" => "down", "U" => "up", _ => return None };
    Some(MouseSpec { button, action, down, up })
}

fn mouse_tags(button: &str) -> (&'static str, &'static str) {
    match button { "right" => ("{RMBD}", "{RMBU}"), "middle" => ("{MMBD}", "{MMBU}"), "mouse_3" => ("{MB4D}", "{MB4U}"), "mouse_4" => ("{MB5D}", "{MB5U}"), _ => ("{LMBD}", "{LMBU}") }
}

const SYSTEM_NOARG: [&str; 21] = ["MWUP", "MWDN", "TILTL", "TILTR", "ACTIVATE", "ACTIVATEPARENT", "ACTIVATETOP", "CURSORBUSY", "CURSORDEFAULT", "INVERTXY", "INVERTX", "INVERTY", "LOCKXY", "LOCKX", "LOCKY", "LOCKC", "UNLOCKXY", "UNLOCKX", "UNLOCKY", "OD", "OU"];

fn parse_dec_or_hex(text: &str) -> Option<i64> {
    let t = text.trim();
    let (sign, body) = match t.strip_prefix('-') { Some(r) => (-1, r), None => (1, t.strip_prefix('+').unwrap_or(t)) };
    if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        return (!hex.is_empty()).then(|| i64::from_str_radix(hex, 16).ok()).flatten().map(|v| v * sign);
    }
    (!body.is_empty() && body.chars().all(|c| c.is_ascii_digit())).then(|| body.parse::<i64>().ok()).flatten().map(|v| v * sign)
}

// ── Export: clip → chuỗi lệnh chuẩn import.md ────────────────────────────────
fn format_key_token(key: &str) -> String {
    if key.is_empty() { return String::new(); }
    if let Some(tag) = canonical_tag(key) { return format!("{{{tag}}}"); }
    let lower = key.to_ascii_lowercase();
    if ["vkc:", "ext:", "sc:", "sce:"].iter().any(|p| lower.starts_with(p)) { return format!("{{{}}}", key.to_uppercase()); }
    if key.chars().count() == 1 { key.to_lowercase() } else { format!("{{{}}}", key.to_uppercase()) }
}

struct Flat { id: String, key: Option<String>, button: Option<String>, start: i64, dur: i64, track: usize }

/// Mở khối Skirk thành clip phím / chuột tương đương theo FPS.
pub fn expand_block(clip: &Clip, fps: f64) -> Vec<(Kind, i64, i64)> {
    let Kind::Block(function) = &clip.kind else { return Vec::new() };
    let s0 = clip.start as f64;
    let inv = 1000.0 / fps;
    let tap = inv.round().max(1.0);
    let mut out: Vec<(Kind, i64, i64)> = Vec::new();
    let mut mouse = |button: &str, rel: f64, dur: f64| out.push((Kind::Mouse(button.into()), (s0 + rel).round() as i64, dur.round().max(1.0) as i64));
    // `spam_left` cần ghi vào `out` nên viết lại thành closure trả về danh sách phụ.
    let spam = |factor: f64, limit_sec: f64, hold_frames: f64, cycle_frames: f64| -> (Vec<(f64, f64)>, f64) {
        let count = (factor * fps).floor() as i64;
        let limit_ms = limit_sec * 1000.0;
        let hold = (hold_frames * inv).round().max(1.0);
        let cycle = (cycle_frames * inv).round().max(hold + 1.0);
        let (mut elapsed, mut hits) = (0.0, Vec::new());
        for _ in 0..count {
            if elapsed > limit_ms - 1e-6 { break; }
            hits.push((elapsed, hold));
            elapsed += cycle;
        }
        (hits, elapsed)
    };
    let mut keys: Vec<(String, f64, f64)> = Vec::new();
    let apply = |hits: &[(f64, f64)], mouse: &mut dyn FnMut(&str, f64, f64)| for (at, dur) in hits { mouse("left", *at, *dur); };
    if function.starts_with("wait_") { return out; }
    match function.as_str() {
        "skke" => {
            let count = (0.1 * fps).floor() as i64;
            let cycle = (2.0 * inv).round().max(2.0);
            let mut elapsed = 0.0;
            for _ in 0..count { if elapsed > 100.0 - 1e-6 { break; } keys.push(("e".into(), elapsed, tap)); elapsed += cycle; }
        }
        "skk2a" => { let (hits, _) = spam(0.26, 0.32, 1.0, 2.0); apply(&hits, &mut mouse); }
        "skk2as" => {
            let (hits, _) = spam(0.26, 0.32, 1.0, 2.0); apply(&hits, &mut mouse);
            let t1 = fps2t(T_2AS_FIRST, fps) * 1000.0;
            mouse("right", t1, tap); keys.push(("w".into(), t1 + 2.0 * inv, tap));
        }
        "skk2az" => {
            let t1 = fps2t(T_2AZ_FIRST, fps);
            let (hits, _) = spam(0.2, t1, 1.0, 2.0); apply(&hits, &mut mouse);
            mouse("left", (t1 + 2.0 / fps) * 1000.0, 440.0);
        }
        "skk2azs" | "skk2azs_slow" if function == "skk2azs" || fps < 105.0 => {
            let t1 = fps2t(T_2AZ_FIRST, fps);
            let (hits, _) = spam(0.2, t1, 1.0, 2.0); apply(&hits, &mut mouse);
            let hold_start = (t1 + 2.0 / fps) * 1000.0;
            mouse("left", hold_start, 440.0);
            let r = hold_start + 440.0 + inv;
            mouse("right", r, tap); keys.push(("w".into(), r + 2.0 * inv, tap));
        }
        "skk2azs_slow" => {
            let (hits, _) = spam(0.2, 0.26, 1.0, 2.0); apply(&hits, &mut mouse);
            mouse("left", 280.0, 440.0); mouse("right", 725.0, tap); keys.push(("w".into(), 725.0 + 2.0 * inv, tap));
        }
        "skk2aq" => {
            let (hits, elapsed) = spam(0.26, 0.32, 1.0, 2.0); apply(&hits, &mut mouse);
            keys.push(("q".into(), elapsed.max(400.0), tap));
        }
        "skk3as" => {
            let (hits, elapsed) = spam(0.6, 0.6, 1.0, 3.0); apply(&hits, &mut mouse);
            mouse("right", elapsed + fps2t(F_3AS, fps) * inv, tap);
        }
        "skk3aw" => {
            let (hits, _) = spam(0.6, 0.6, 1.0, 2.0); apply(&hits, &mut mouse);
            keys.push(("w".into(), fps2t(T_3AW, fps) * 1000.0, tap));
        }
        "skk5a" => { let (hits, _) = spam(2.0, 2.1, 1.0, 2.0); apply(&hits, &mut mouse); }
        "skk5as" => {
            let (hits, _) = spam(2.0, 2.1, 1.0, 2.0); apply(&hits, &mut mouse);
            mouse("right", (fps2t(T_5A, fps) + 2.0 / fps) * 1000.0, tap);
        }
        _ => mouse("left", 0.0, clip.dur.max(1) as f64),
    }
    for (key, rel, dur) in keys { out.push((Kind::Key(key), (s0 + rel).round() as i64, dur.round().max(1.0) as i64)); }
    out
}

/// Timeline nhiều track → chuỗi lệnh `{PRESS}`, `{RELEASE}`, `{WAITMS:n}`, `{HOLDMS:n}` (giống `exportClipsToMacroText`).
pub fn export_macro_text(clips: &[Clip], fps: f64) -> String {
    let mut flat: Vec<Flat> = Vec::new();
    let mut min_total_end = 0;
    for clip in clips {
        min_total_end = min_total_end.max(clip.end());
        match &clip.kind {
            Kind::Block(_) => for (index, (kind, start, dur)) in expand_block(clip, fps).into_iter().enumerate() {
                let (key, button) = match kind { Kind::Key(k) => (Some(k), None), Kind::Mouse(b) => (None, Some(b)), Kind::Block(_) => continue };
                flat.push(Flat { id: format!("{}_{index}", clip.id), key, button, start: clamp_ms(start), dur: clamp_ms(dur).max(1), track: clip.track });
            },
            Kind::Key(key) if !key.is_empty() => flat.push(Flat { id: clip.id.clone(), key: Some(key.clone()), button: None, start: clamp_ms(clip.start), dur: clamp_ms(clip.dur).max(1), track: clip.track }),
            Kind::Mouse(button) if !button.is_empty() => flat.push(Flat { id: clip.id.clone(), key: None, button: Some(button.clone()), start: clamp_ms(clip.start), dur: clamp_ms(clip.dur).max(1), track: clip.track }),
            _ => {}
        }
    }
    if flat.is_empty() { return String::new(); }

    // Nhóm cùng [start, end] chỉ gồm phím (đúng một phím thường + modifier) rút gọn thành {HOLDMS}{MOD}key.
    let mut by_span: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (index, clip) in flat.iter().enumerate() { by_span.entry((clip.start, clip.start + clip.dur)).or_default().push(index); }
    struct Hold { start: i64, dur: i64, modifiers: Vec<usize>, main: usize }
    let mut holds: Vec<Hold> = Vec::new();
    let mut in_hold: HashSet<String> = HashSet::new();
    for (&(s, e), group) in &by_span {
        if !group.iter().all(|&i| flat[i].key.is_some()) { continue; }
        let is_mod = |i: usize| flat[i].key.as_deref().is_some_and(|k| modifier_tag(k).is_some());
        let modifiers: Vec<usize> = group.iter().copied().filter(|&i| is_mod(i)).collect();
        let others: Vec<usize> = group.iter().copied().filter(|&i| !is_mod(i)).collect();
        if others.len() != 1 { continue; }
        let distinct: HashSet<_> = modifiers.iter().map(|&i| flat[i].key.clone()).collect();
        if distinct.len() != modifiers.len() { continue; }
        let interference = flat.iter().enumerate().any(|(i, other)| {
            if group.contains(&i) { return false; }
            let oe = other.start + other.dur;
            other.start == s || oe == e || (other.start > s && other.start < e) || (oe > s && oe < e)
        });
        if interference { continue; }
        let mut modifiers = modifiers;
        modifiers.sort_by_key(|&i| flat[i].track);
        for &i in group { in_hold.insert(flat[i].id.clone()); }
        holds.push(Hold { start: s, dur: e - s, modifiers, main: others[0] });
    }

    struct Event { t: i64, order: u8, track: i64, token: String, advance: i64 }
    let mut events: Vec<Event> = Vec::new();
    for hold in &holds {
        let mods: String = hold.modifiers.iter().filter_map(|&i| flat[i].key.as_deref().and_then(modifier_tag)).map(|t| format!("{{{t}}}")).collect();
        let main = format_key_token(flat[hold.main].key.as_deref().unwrap_or(""));
        events.push(Event { t: hold.start, order: 1, track: flat[hold.main].track as i64, token: format!("{{HOLDMS:{}}}{mods}{main}", hold.dur), advance: hold.dur });
    }
    for clip in &flat {
        if in_hold.contains(&clip.id) { continue; }
        let (s, e, track) = (clip.start, clip.start + clip.dur, clip.track as i64);
        if let Some(button) = &clip.button {
            let (down, up) = mouse_tags(button);
            events.push(Event { t: s, order: 2, track, token: down.into(), advance: 0 });
            events.push(Event { t: e, order: 0, track: -track, token: up.into(), advance: 0 });
        } else {
            let token = format_key_token(clip.key.as_deref().unwrap_or(""));
            events.push(Event { t: s, order: 2, track, token: format!("{{PRESS}}{token}"), advance: 0 });
            events.push(Event { t: e, order: 0, track: -track, token: format!("{{RELEASE}}{token}"), advance: 0 });
        }
    }
    events.sort_by_key(|e| (e.t, e.order, e.track));
    let mut out = String::new();
    let mut cursor = 0;
    for event in events {
        let gap = event.t - cursor;
        if gap > 0 { out.push_str(&format!("{{WAITMS:{gap}}}")); cursor += gap; }
        out.push_str(&event.token);
        if event.advance > 0 { cursor += event.advance; }
    }
    if min_total_end > cursor { out.push_str(&format!("{{WAITMS:{}}}", min_total_end - cursor)); }
    out
}

// ── Import: chuỗi lệnh → clip đa track ───────────────────────────────────────
#[derive(Debug, PartialEq)]
pub struct Imported {
    pub clips: Vec<Clip>,
    pub max_concurrent_tracks: usize,
    pub total_ms: i64,
    pub notes: Vec<String>,
    pub system_tags: Vec<String>,
}

enum Token { Tag { raw: String, full: String }, Char(char) }

fn tokenize(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\r' | '\n' => { i += 1; }
            '}' => return Err(format!("Lỗi cú pháp tại vị trí {}: gặp dấu '}}' thừa không có '{{' mở tương ứng.", i + 1)),
            '{' => {
                let Some(close) = chars[i + 1..].iter().position(|c| *c == '}').map(|p| p + i + 1) else {
                    return Err(format!("Lỗi cú pháp tại vị trí {}: thẻ '{{' chưa được đóng bằng dấu '}}'.", i + 1));
                };
                let inside: String = chars[i + 1..close].iter().collect();
                if inside.trim().is_empty() { return Err(format!("Lỗi cú pháp tại vị trí {}: thẻ rỗng '{{}}'.", i + 1)); }
                tokens.push(Token::Tag { full: format!("{{{inside}}}"), raw: inside });
                i = close + 1;
            }
            ch => {
                let prev_is_mod_or_press = matches!(tokens.last(), Some(Token::Tag { raw, .. }) if modifier_key(&raw.to_uppercase()).is_some() || raw.eq_ignore_ascii_case("PRESS") || raw.eq_ignore_ascii_case("RELEASE"));
                let word: String = chars[i..chars.len().min(i + 6)].iter().collect();
                if prev_is_mod_or_press && word.eq_ignore_ascii_case("DELETE") && chars.get(i + 6).is_none_or(|n| matches!(n, '{' | '\r' | '\n' | ' ')) {
                    tokens.push(Token::Tag { raw: "DEL".into(), full: "DELETE".into() });
                    i += 6;
                    continue;
                }
                tokens.push(Token::Char(ch));
                i += 1;
            }
        }
    }
    Ok(tokens)
}

enum Info {
    PressMode { press: bool, full: String },
    Clear,
    Modifier { key: &'static str, full: String },
    Key { key: String, full: String },
    Mouse { spec: MouseSpec, full: String },
    System { full: String },
    Wait { ms: i64, full: String, note: Option<String> },
    Hold { ms: i64, full: String },
}

fn classify(raw: &str, full: &str) -> Result<Info, String> {
    let upper = raw.trim().to_uppercase();
    let full = full.to_owned();
    match upper.as_str() {
        "PRESS" => return Ok(Info::PressMode { press: true, full }),
        "RELEASE" => return Ok(Info::PressMode { press: false, full }),
        "CLEAR" => return Ok(Info::Clear),
        _ => {}
    }
    if let Some(key) = modifier_key(&upper) { return Ok(Info::Modifier { key, full }); }
    if let Some(key) = special_key(&upper) { return Ok(Info::Key { key, full }); }
    if let Some(spec) = mouse_spec(&upper) { return Ok(Info::Mouse { spec, full }); }
    if SYSTEM_NOARG.contains(&upper.as_str()) || upper == "OR" { return Ok(Info::System { full }); }
    if let Some(number) = upper.strip_prefix("WAITMS").filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.')) {
        if let Ok(value) = number.parse::<f64>() {
            let ms = value.round() as i64;
            return Ok(Info::Wait { ms, note: Some(format!("{full} → chờ {ms} ms")), full });
        }
    }
    if let Some(colon) = raw.find(':') {
        let prefix = raw[..colon].trim().to_uppercase();
        let param = raw[colon + 1..].trim();
        let number = |value: &str| value.trim().parse::<f64>().ok().filter(|n| n.is_finite());
        match prefix.as_str() {
            "WAIT" => return match number(param).filter(|s| *s >= 0.0) {
                Some(sec) => Ok(Info::Wait { ms: (sec * 1000.0).round() as i64, full, note: None }),
                None => Err(format!("Tham số thời gian không hợp lệ trong {full} (cần số giây >= 0).")),
            },
            "WAITMS" => {
                if let Some((x, y)) = param.split_once('-') {
                    if let (Some(x), Some(y)) = (number(x), number(y)) {
                        if x > y { return Err(format!("Khoảng thời gian ngẫu nhiên không hợp lệ trong {full} ({x} > {y}).")); }
                        let mid = ((x + y) / 2.0).round() as i64;
                        return Ok(Info::Wait { ms: mid, note: Some(format!("{full} (ngẫu nhiên {x}–{y} ms) được đặt trung bình {mid} ms trên timeline.")), full });
                    }
                }
                return match number(param).filter(|ms| *ms >= 0.0) {
                    Some(ms) => Ok(Info::Wait { ms: ms.round() as i64, full, note: None }),
                    None => Err(format!("Tham số thời gian không hợp lệ trong {full} (cần số ms >= 0 hoặc x-y).")),
                };
            }
            "HOLD" => return match number(param).filter(|s| *s > 0.0) {
                Some(sec) => Ok(Info::Hold { ms: ((sec * 1000.0).round() as i64).max(1), full }),
                None => Err(format!("Tham số giữ phím không hợp lệ trong {full} (cần số giây > 0).")),
            },
            "HOLDMS" => return match number(param).filter(|ms| *ms > 0.0) {
                Some(ms) => Ok(Info::Hold { ms: (ms.round() as i64).max(1), full }),
                None => Err(format!("Tham số giữ phím không hợp lệ trong {full} (cần số ms > 0).")),
            },
            "VKC" | "EXT" | "SC" | "SCE" => return match parse_dec_or_hex(param).filter(|c| *c >= 0) {
                Some(_) => Ok(Info::Key { key: format!("{}:{param}", prefix.to_lowercase()), full }),
                None => Err(format!("Mã phím cấp thấp trong {full} phải là số nguyên hợp lệ.")),
            },
            "CB" => return Ok(Info::System { full }),
            "MADD" | "MSET" | "PSET" | "ASET" => {
                let parts: Vec<&str> = param.split(',').collect();
                return if parts.len() == 2 && parts.iter().all(|p| parse_dec_or_hex(p).is_some()) { Ok(Info::System { full }) }
                else { Err(format!("Tọa độ trong {full} phải có dạng x,y (hai số nguyên).")) };
            }
            "MSAVE" | "MREST" => return match parse_dec_or_hex(param).filter(|s| (1..=10).contains(s)) {
                Some(_) => Ok(Info::System { full }),
                None => Err(format!("Ô nhớ vị trí con trỏ trong {full} phải từ 1 đến 10 (theo mục 15 import.md).")),
            },
            "RUN" | "RUNHID" | "RUNMAX" | "RUNMIN" | "RUNADM" => {
                let lower = param.to_lowercase();
                let has_exe = lower.contains(".exe") && (lower.ends_with(".exe") || lower.contains(".exe "));
                return if !param.is_empty() && has_exe { Ok(Info::System { full }) }
                else { Err(format!("Quy tắc mục 16 import.md: {full} phải cung cấp đường dẫn bao gồm phần mở rộng .EXE.")) };
            }
            "KILL" => return if param.is_empty() { Err(format!("Thiếu tên executable trong {full}.")) } else { Ok(Info::System { full }) },
            "POSTWM" | "SENDWM" => {
                let parts: Vec<&str> = param.split(',').collect();
                return if parts.len() == 3 && parts.iter().all(|p| parse_dec_or_hex(p).is_some()) { Ok(Info::System { full }) }
                else { Err(format!("Tham số trong {full} phải gồm 3 giá trị số <M>,<L>,<W> (thập phân hoặc 0x hex).")) };
            }
            "LAYER" => return if ["next", "back", "last"].contains(&param.to_lowercase().as_str()) || parse_dec_or_hex(param).is_some() { Ok(Info::System { full }) }
                else { Err(format!("Tham số layer trong {full} phải là số hoặc next/back/last.")) },
            _ => {}
        }
    }
    Err(format!("Tag không xác định hoặc sai định dạng theo import.md: {full}"))
}

struct Raw { kind: Kind, start: i64, dur: i64, order: u64 }

struct OpenHold { kind: Kind, start: i64, order: u64, from_press: bool, open_tag: String, close_tag: String }

/// Phân tích chuỗi lệnh thành clip đa track (giống `parseMacroSequence`).
pub fn parse_macro_text(text: &str, default_tap_ms: i64, fps: f64) -> Result<Imported, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() { return Ok(Imported { clips: Vec::new(), max_concurrent_tracks: 0, total_ms: 0, notes: Vec::new(), system_tags: Vec::new() }); }
    let tokens = tokenize(trimmed)?;
    let (mut raws, mut notes, mut system_tags): (Vec<Raw>, Vec<String>, Vec<String>) = (Vec::new(), Vec::new(), Vec::new());
    let (mut order_counter, mut t) = (0u64, 0i64);
    let mut modifiers: Vec<(String, String)> = Vec::new();
    let mut hold: Option<(i64, String)> = None;
    // (press?, full tag, số phím đã đi kèm)
    let mut mode: Option<(bool, String, u32)> = None;
    let mut open: BTreeMap<String, OpenHold> = BTreeMap::new();
    let target_id = |kind: &Kind| match kind { Kind::Key(k) => format!("key:{k}"), Kind::Mouse(b) => format!("mouse:{b}"), Kind::Block(f) => format!("block:{f}") };
    let label = |kind: &Kind| match kind { Kind::Key(k) => hotkey_label(k), Kind::Mouse(b) => mouse_label(b).to_owned(), Kind::Block(f) => f.clone() };
    let join_mods = |mods: &[(String, String)]| mods.iter().map(|m| m.1.as_str()).collect::<String>();

    let handle = |kind: Kind, desc: &str, t: &mut i64, hold: &mut Option<(i64, String)>, modifiers: &mut Vec<(String, String)>, mode: &mut Option<(bool, String, u32)>, open: &mut BTreeMap<String, OpenHold>, raws: &mut Vec<Raw>, order_counter: &mut u64| -> Result<(), String> {
        let id = target_id(&kind);
        let name = label(&kind);
        if let Some((press, full, consumed)) = mode.as_mut() {
            if let Some((_, hold_full)) = hold { return Err(format!("Không thể kết hợp {hold_full} bên trong {full} (tại {desc}).")); }
            if !modifiers.is_empty() {
                return Err(format!("Modifier {} không thể đứng trước {full}. Hãy dùng {{PRESS}}{m}...{{RELEASE}}{m}.", join_mods(modifiers), m = modifiers[0].1));
            }
            if *press {
                if let Some(existing) = open.get(&id) {
                    return Err(format!("Phím/nút \"{name}\" ({desc}) đã được nhấn giữ tại {} ms bằng {} nhưng chưa được thả trước khi nhấn lại.", existing.start, existing.open_tag));
                }
                *order_counter += 1;
                open.insert(id, OpenHold { kind, start: *t, order: *order_counter, from_press: true, open_tag: format!("{{PRESS}}{desc}"), close_tag: format!("{{RELEASE}}{desc}") });
                *consumed += 1;
                return Ok(());
            }
            let existing = match open.remove(&id) { Some(e) if e.from_press => e, _ => return Err(format!("Gặp {{RELEASE}}{desc} tại {} ms nhưng \"{name}\" chưa được nhấn bằng {{PRESS}} trước đó (vi phạm quy tắc mục 20 import.md).", *t)) };
            raws.push(Raw { kind: existing.kind, start: existing.start, dur: (*t - existing.start).max(1), order: existing.order });
            *consumed += 1;
            return Ok(());
        }
        if let Some(existing) = open.get(&id) {
            return Err(format!("Phím/nút \"{name}\" đang được giữ từ {} ms ({}), không thể gõ đè tại {} ms trước khi thả ({}).", existing.start, existing.open_tag, *t, existing.close_tag));
        }
        let dur = hold.take().map_or(default_tap_ms, |(ms, _)| ms);
        let mut seen = HashSet::new();
        for (key, _) in modifiers.drain(..) {
            if !seen.insert(key.clone()) || open.contains_key(&format!("key:{key}")) { continue; }
            *order_counter += 1;
            raws.push(Raw { kind: Kind::Key(key), start: *t, dur, order: *order_counter });
        }
        *order_counter += 1;
        raws.push(Raw { kind, start: *t, dur, order: *order_counter });
        *t += dur;
        Ok(())
    };

    for token in tokens {
        if let Token::Char(ch) = token {
            let key = if ch == ' ' { "space".to_owned() } else { ch.to_lowercase().to_string() };
            let desc = if ch == ' ' { "{SPACE}".to_owned() } else { ch.to_string() };
            handle(Kind::Key(key), &desc, &mut t, &mut hold, &mut modifiers, &mut mode, &mut open, &mut raws, &mut order_counter)?;
            continue;
        }
        let Token::Tag { raw, full } = token else { continue };
        let info = classify(&raw, &full)?;
        match info {
            Info::PressMode { press, full } => {
                if let Some((_, prev, 0)) = &mode { return Err(format!("Thẻ {prev} đứng trước {full} nhưng chưa có phím nào đi kèm.")); }
                if let Some((_, hold_full)) = &hold { return Err(format!("Thẻ {hold_full} phải đứng trước một phím, không thể đứng trước {full}.")); }
                if !modifiers.is_empty() { return Err(format!("Modifier {} chỉ áp dụng cho phím kế tiếp, không thể đứng trước {full}.", join_mods(&modifiers))); }
                mode = Some((press, full, 0));
            }
            Info::Clear => { mode = None; modifiers.clear(); }
            Info::Wait { ms, full, note } => {
                if let Some(note) = note { notes.push(note); }
                if let Some((_, prev, 0)) = &mode { return Err(format!("Thẻ {prev} chưa có phím đi kèm trước khi gặp {full}.")); }
                mode = None;
                if let Some((_, hold_full)) = &hold { return Err(format!("Thẻ {hold_full} phải áp dụng cho phím kế tiếp, không thể đứng ngay trước {full}.")); }
                if !modifiers.is_empty() {
                    let mods = join_mods(&modifiers);
                    return Err(format!("Modifier {mods} chỉ áp dụng cho phím kế tiếp, không thể đứng ngay trước {full}. Nếu muốn giữ modifier trong lúc chờ, hãy dùng {{PRESS}}{mods}{full}...{{RELEASE}}{mods}."));
                }
                t += ms;
            }
            Info::Hold { ms, full } => {
                if let Some((_, prev, 0)) = &mode { return Err(format!("Thẻ {prev} không thể kết hợp trực tiếp với {full}.")); }
                mode = None;
                hold = Some((ms, full));
            }
            Info::Modifier { key, full } => {
                if mode.is_some() { handle(Kind::Key(key.into()), &full, &mut t, &mut hold, &mut modifiers, &mut mode, &mut open, &mut raws, &mut order_counter)?; }
                else { modifiers.push((key.into(), full)); }
            }
            Info::Key { key, full } => handle(Kind::Key(key), &full, &mut t, &mut hold, &mut modifiers, &mut mode, &mut open, &mut raws, &mut order_counter)?,
            Info::Mouse { spec, full } => {
                if spec.action == "tap" { handle(Kind::Mouse(spec.button.into()), &full, &mut t, &mut hold, &mut modifiers, &mut mode, &mut open, &mut raws, &mut order_counter)?; continue; }
                if let Some((_, prev, 0)) = &mode { return Err(format!("Thẻ {prev} không cần thiết trước {full} (vì {full} đã là tag nhấn/thả chuột).")); }
                mode = None;
                if let Some((_, hold_full)) = &hold { return Err(format!("Không dùng {hold_full} trước {full}. Hãy dùng {{WAITMS:n}} giữa {{{}}} và {{{}}}.", spec.down, spec.up)); }
                if !modifiers.is_empty() { return Err(format!("Modifier {} đang chờ phím kế tiếp, không thể áp dụng trực tiếp vào {full}.", join_mods(&modifiers))); }
                let kind = Kind::Mouse(spec.button.into());
                let id = target_id(&kind);
                let name = label(&kind);
                if spec.action == "down" {
                    if let Some(existing) = open.get(&id) { return Err(format!("Nút \"{name}\" đã được nhấn tại {} ms ({}) nhưng chưa được thả trước khi gặp {full}.", existing.start, existing.open_tag)); }
                    order_counter += 1;
                    open.insert(id, OpenHold { kind, start: t, order: order_counter, from_press: false, open_tag: full, close_tag: format!("{{{}}}", spec.up) });
                } else {
                    match open.remove(&id) {
                        Some(existing) if !existing.from_press => raws.push(Raw { kind: existing.kind, start: existing.start, dur: (t - existing.start).max(1), order: existing.order }),
                        _ => return Err(format!("Gặp {full} tại {t} ms nhưng chưa có {{{}}} tương ứng trước đó.", spec.down)),
                    }
                }
            }
            Info::System { full } => { mode = None; system_tags.push(full); }
        }
    }
    if let Some((_, full, 0)) = &mode { return Err(format!("Thẻ {full} ở cuối chuỗi không có phím nào đi kèm.")); }
    if let Some((_, full)) = &hold { return Err(format!("Thẻ {full} ở cuối chuỗi không có phím nào đi kèm.")); }
    if !modifiers.is_empty() { return Err(format!("Modifier {} ở cuối chuỗi không có phím kế tiếp để áp dụng (hoặc thiếu {{CLEAR}}).", join_mods(&modifiers))); }
    if !open.is_empty() {
        let unclosed: Vec<String> = open.values().map(|h| format!("{} (tại {} ms → thiếu {})", h.open_tag, h.start, h.close_tag)).collect();
        return Err(format!("Vi phạm quy tắc cặp nhấn/thả trong import.md: {}.", unclosed.join(", ")));
    }

    // Xếp lên nhiều track song song.
    raws.sort_by(|a, b| a.start.cmp(&b.start).then(a.order.cmp(&b.order)).then(b.dur.cmp(&a.dur)));
    let mut track_ends: Vec<i64> = Vec::new();
    let mut max_tracks = 0;
    let mut clips = Vec::new();
    for raw in raws {
        let start = clamp_ms(raw.start);
        let dur = clamp_ms(raw.dur).max(1);
        let track = track_ends.iter().position(|end| *end <= start).unwrap_or_else(|| { track_ends.push(0); track_ends.len() - 1 });
        track_ends[track] = start + dur;
        max_tracks = max_tracks.max(track + 1);
        clips.push(Clip::new(raw.kind, track, start, dur, fps));
    }
    let total_ms = t.max(clips.iter().map(Clip::end).max().unwrap_or(0));
    Ok(Imported { clips, max_concurrent_tracks: max_tracks, total_ms, notes, system_tags })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str, track: usize, start: i64, dur: i64) -> Clip { Clip::new(Kind::Key(key.into()), track, start, dur, 120.0) }
    fn mouse(button: &str, track: usize, start: i64, dur: i64) -> Clip { Clip::new(Kind::Mouse(button.into()), track, start, dur, 120.0) }
    fn shape(clips: &[Clip]) -> Vec<(String, usize, i64, i64)> {
        let mut shape: Vec<_> = clips.iter().map(|c| (clip_name(c), c.track, c.start, c.dur)).collect();
        shape.sort();
        shape
    }

    #[test]
    fn imports_the_documented_example_into_parallel_tracks() {
        let text = "{LMBD}{WAITMS:100}{PRESS}w{WAITMS:50}{HOLDMS:80}e{RELEASE}w{LMBU}";
        let imported = parse_macro_text(text, 50, 120.0).unwrap();
        assert_eq!(shape(&imported.clips), vec![
            ("Chuột trái".to_owned(), 0, 0, 230),
            ("E".to_owned(), 2, 150, 80),
            ("W".to_owned(), 1, 100, 130),
        ]);
        assert_eq!(imported.max_concurrent_tracks, 3);
    }

    #[test]
    fn modifier_applies_only_to_the_next_key_and_chords_stack_on_tracks() {
        let imported = parse_macro_text("{CTRL}a{ALT}{F4}", 50, 120.0).unwrap();
        let mut expected = vec![("A".to_owned(), 1, 0, 50), ("Alt".to_owned(), 0, 50, 50), ("Ctrl".to_owned(), 0, 0, 50), ("F4".to_owned(), 1, 50, 50)];
        expected.sort();
        assert_eq!(shape(&imported.clips), expected);
    }

    #[test]
    fn reports_the_same_errors_as_the_original_parser() {
        assert!(parse_macro_text("{CTRL}", 50, 120.0).unwrap_err().contains("ở cuối chuỗi"));
        assert!(parse_macro_text("{PRESS}w", 50, 120.0).unwrap_err().contains("Vi phạm quy tắc cặp nhấn/thả"));
        assert!(parse_macro_text("{RELEASE}w", 50, 120.0).unwrap_err().contains("chưa được nhấn bằng {PRESS}"));
        assert!(parse_macro_text("{NOPE}", 50, 120.0).unwrap_err().contains("Tag không xác định"));
        assert!(parse_macro_text("}", 50, 120.0).unwrap_err().contains("dấu '}' thừa"));
        assert!(parse_macro_text("{WAITMS:9-3}", 50, 120.0).unwrap_err().contains("ngẫu nhiên"));
        assert!(parse_macro_text("{RUN:notepad}", 50, 120.0).unwrap_err().contains(".EXE"));
    }

    #[test]
    fn system_tags_and_random_waits_are_reported_not_executed() {
        let imported = parse_macro_text("{WAITMS:10-30}{MWUP}a", 50, 120.0).unwrap();
        assert_eq!(imported.system_tags, vec!["{MWUP}"]);
        assert!(imported.notes[0].contains("trung bình 20 ms"));
        assert_eq!(imported.clips[0].start, 20);
    }

    #[test]
    fn export_round_trips_through_import() {
        let clips = vec![mouse("left", 0, 0, 280), key("w", 1, 100, 130), key("e", 2, 150, 80)];
        let text = export_macro_text(&clips, 120.0);
        let back = parse_macro_text(&text, 50, 120.0).unwrap();
        assert_eq!(shape(&back.clips), shape(&clips), "{text}");
    }

    #[test]
    fn export_collapses_a_chord_into_holdms() {
        let clips = vec![key("ctrl", 0, 0, 80), key("a", 1, 0, 80)];
        assert_eq!(export_macro_text(&clips, 120.0), "{HOLDMS:80}{CTRL}a");
    }

    #[test]
    fn compile_matches_the_original_ordering_and_steps_round_trip() {
        let clips = vec![key("w", 0, 0, 50), key("w", 0, 50, 50), mouse("left", 1, 20, 100)];
        let steps = compile_clips(&clips, Some(Loop { start: 0, end: 150 }));
        let kinds: Vec<_> = steps.iter().map(|s| s["type"].as_str().unwrap().to_owned()).collect();
        assert_eq!(kinds.first().map(String::as_str), Some("loop_start"));
        assert_eq!(kinds.last().map(String::as_str), Some("loop_end"));
        let (back, region) = steps_to_clips(&steps, 120.0);
        assert_eq!(region, Some(Loop { start: 0, end: 150 }));
        assert_eq!(shape(&back), shape(&clips));
        assert_eq!(python_sequence(&[json!("a"), json!("b")]), vec![json!("a"), json!("is_no_key_pressed"), json!("b")]);
    }

    #[test]
    fn validation_rejects_self_triggering_clashing_and_loop_edge_clips() {
        assert!(validate(&[key("", 0, 0, 10)], None, None).unwrap().contains("chưa chọn phím"));
        assert!(validate(&[key("w", 0, 0, 100), key("w", 1, 50, 100)], None, None).unwrap().contains("chồng lên chính nó"));
        assert!(validate(&[key("x", 0, 0, 100)], None, Some("x")).unwrap().contains("tự kích hoạt lại"));
        assert!(validate(&[key("w", 0, 0, 100)], Some(Loop { start: 50, end: 200 }), None).unwrap().contains("vắt qua mép"));
        assert_eq!(validate(&[key("w", 0, 0, 100)], Some(Loop { start: 0, end: 100 }), Some("f6")), None);
    }

    #[test]
    fn blocks_use_the_fps_tables_and_export_to_plain_input() {
        assert_eq!(block_duration_ms("skk5a", 120.0), Some(2157));
        assert_eq!(block_duration_ms("wait_300", 120.0), Some(300));
        let block = Clip::new(Kind::Block("skke".into()), 0, 0, 1, 120.0);
        let text = export_macro_text(&[block], 120.0);
        assert!(text.starts_with("{HOLDMS:8}e{WAITMS:9}"), "{text}");
        assert!(parse_macro_text(&text, 50, 120.0).is_ok());
    }
}
