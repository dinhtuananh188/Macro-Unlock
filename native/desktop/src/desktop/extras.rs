//! Phần phụ của bản Electron: tin trang chủ, ảnh bìa tùy chọn, kiểm tra cập nhật và mở liên kết.
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

const RELEASES_URL: &str = "https://api.github.com/repos/UnlockerMacroGenshinVN/CUTTOOL/releases/latest";
const MAX_BANNER_WIDTH: u32 = 2560;

// ── Tin trang chủ (`announcement.js`) ────────────────────────────────────────
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Announcement {
    pub title: String,
    pub summary: String,
    pub date: String,
    pub tag: String,
    pub content: String,
}

impl Announcement {
    /// Đọc tin đóng sẵn từ đúng tệp `announcement.js` của bản gốc.
    pub fn bundled() -> Self { Self::parse(include_str!("../../../../src/UI/announcement.js")) }

    pub fn parse(source: &str) -> Self {
        Self {
            title: field(source, "title"), summary: field(source, "summary"), date: field(source, "date"),
            tag: field(source, "tag"), content: field(source, "content"),
        }
    }
}

/// Giá trị chuỗi `key: "…"` hoặc `key: `…`` của đối tượng JavaScript.
fn field(source: &str, key: &str) -> String {
    let needle = format!("\n    {key}:");
    let Some(start) = source.find(&needle) else { return String::new() };
    let rest = source[start + needle.len()..].trim_start();
    let mut chars = rest.chars();
    let Some(quote) = chars.next().filter(|c| matches!(c, '"' | '`')) else { return String::new() };
    let mut out = String::new();
    let mut escaped = false;
    for c in chars {
        if escaped {
            out.push(match c { 'n' => '\n', other => other });
            escaped = false;
        } else if c == '\\' && quote == '"' {
            escaped = true;
        } else if c == quote {
            break;
        } else {
            out.push(c);
        }
    }
    out.trim_end().to_owned()
}

// ── Ảnh bìa tùy chọn ─────────────────────────────────────────────────────────
pub fn custom_banner_path(config: &Path) -> PathBuf {
    config.parent().unwrap_or_else(|| Path::new(".")).join("custom-banner.png")
}

/// Đọc ảnh bất kỳ (PNG, JPEG, WebP), thu nhỏ nếu quá rộng rồi lưu thành PNG làm ảnh bìa.
pub fn import_banner(source: &Path, target: &Path) -> Result<(), String> {
    let image = image::open(source).map_err(|error| format!("Không đọc được ảnh: {error}"))?;
    let image = if image.width() > MAX_BANNER_WIDTH {
        let height = (u64::from(image.height()) * u64::from(MAX_BANNER_WIDTH) / u64::from(image.width())).max(1) as u32;
        image.resize_exact(MAX_BANNER_WIDTH, height, image::imageops::FilterType::Triangle)
    } else { image };
    let temporary = target.with_extension("tmp");
    image.save_with_format(&temporary, image::ImageFormat::Png).map_err(|error| format!("Không lưu được ảnh bìa: {error}"))?;
    std::fs::rename(&temporary, target).map_err(|error| { let _ = std::fs::remove_file(&temporary); format!("Không lưu được ảnh bìa: {error}") })
}

// ── Cập nhật ─────────────────────────────────────────────────────────────────
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub notes: String,
    pub asset: String,
    pub size: u64,
    pub page: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Current(String),
    Available(UpdateInfo),
    Failed(String),
}

pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |v: &str| v.trim_start_matches(['v', 'V']).split('.').map(|n| n.trim().parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (pa, pb) = (parse(a), parse(b));
    (0..pa.len().max(pb.len())).map(|i| pa.get(i).unwrap_or(&0).cmp(pb.get(i).unwrap_or(&0))).find(|o| o.is_ne()).unwrap_or(std::cmp::Ordering::Equal)
}

/// Phân tích phản hồi `releases/latest` của GitHub.
pub fn parse_release(body: &str, current: &str) -> Result<UpdateState, String> {
    let json: serde_json::Value = serde_json::from_str(body).map_err(|error| format!("Lỗi phân tích JSON từ GitHub: {error}"))?;
    let latest = json["tag_name"].as_str().unwrap_or("").trim_start_matches(['v', 'V']).to_owned();
    if latest.is_empty() { return Err("GitHub không trả về số phiên bản.".into()); }
    if compare_versions(&latest, current).is_le() { return Ok(UpdateState::Current(current.to_owned())); }
    let assets = json["assets"].as_array().cloned().unwrap_or_default();
    let asset = assets.iter().find(|a| a["name"].as_str().is_some_and(|n| { let n = n.to_lowercase(); n.ends_with(".rar") || n.ends_with(".zip") })).or_else(|| assets.first());
    Ok(UpdateState::Available(UpdateInfo {
        current: current.to_owned(),
        latest,
        notes: json["body"].as_str().unwrap_or("").to_owned(),
        asset: asset.and_then(|a| a["name"].as_str()).unwrap_or("").to_owned(),
        size: asset.and_then(|a| a["size"].as_u64()).unwrap_or(0),
        page: json["html_url"].as_str().unwrap_or("").to_owned(),
    }))
}

/// Kiểm tra cập nhật ở luồng nền; kết quả gửi qua kênh và đánh thức giao diện.
pub fn spawn_check(current: String, ctx: eframe::egui::Context) -> Receiver<UpdateState> {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let config = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).build();
        let agent = ureq::Agent::new_with_config(config);
        let result = agent.get(RELEASES_URL).header("User-Agent", "CutTool-Updater").header("Accept", "application/vnd.github.v3+json").call()
            .map_err(|error| format!("Lỗi kết nối GitHub: {error}"))
            .and_then(|mut response| response.body_mut().read_to_string().map_err(|error| format!("Lỗi đọc phản hồi GitHub: {error}")))
            .and_then(|body| parse_release(&body, &current));
        let _ = sender.send(result.unwrap_or_else(UpdateState::Failed));
        ctx.request_repaint();
    });
    receiver
}

pub fn format_bytes(bytes: u64) -> String {
    match bytes {
        0 => "—".into(),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / f64::from(1u32 << 20)),
        b => format!("{:.0} KB", b as f64 / 1024.0),
    }
}

/// Mở liên kết https bằng trình duyệt mặc định. Chỉ nhận `https://`.
pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") { return Err("Chỉ mở được liên kết https.".into()); }
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (verb, target) = (wide("open"), wide(url));
    // Null parent và thư mục mặc định; trả về giá trị > 32 khi thành công.
    let code = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), target.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) } as usize;
    if code > 32 { Ok(()) } else { Err(format!("Không mở được liên kết (mã {code}).")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_bundled_announcement() {
        let news = Announcement::bundled();
        assert_eq!(news.date, "31/10/2026");
        assert!(news.title.starts_with("v1.2.2"));
        assert!(news.tag.contains("GENSHIN IMPACT 7.2"));
        assert!(news.content.starts_with("Peak update v1.2.2!"));
        assert!(news.content.contains("Sửa đổi giao diện trang chính"));
    }

    #[test]
    fn compares_versions_like_the_original_updater() {
        assert!(compare_versions("1.2.10", "1.2.9").is_gt());
        assert!(compare_versions("v1.3", "1.3.0").is_eq());
        assert!(compare_versions("1.2.2", "1.3.0").is_lt());
    }

    #[test]
    fn parses_a_release_and_prefers_zip_or_rar_assets() {
        let body = r#"{"tag_name":"v1.3.0","body":"Ghi chú","html_url":"https://github.com/x/y/releases/v1.3.0","assets":[{"name":"notes.txt","size":10},{"name":"Cuttool.zip","size":5242880}]}"#;
        let UpdateState::Available(info) = parse_release(body, "1.2.2").unwrap() else { panic!("phải có bản mới") };
        assert_eq!((info.latest.as_str(), info.asset.as_str(), info.size), ("1.3.0", "Cuttool.zip", 5_242_880));
        assert_eq!(parse_release(body, "1.3.0").unwrap(), UpdateState::Current("1.3.0".into()));
        assert!(parse_release("{}", "1.0.0").is_err());
        assert_eq!(format_bytes(5_242_880), "5.0 MB");
    }

    #[test]
    fn refuses_to_open_non_https_links() {
        assert!(open_url("file:///C:/Windows/System32/calc.exe").is_err());
        assert!(open_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn imports_an_image_as_a_png_banner() {
        let dir = std::env::temp_dir().join(format!("cryss-banner-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let source = dir.join("anh.png");
        image::RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 30, 255])).save(&source).unwrap();
        let target = dir.join("custom-banner.png");
        import_banner(&source, &target).unwrap();
        assert_eq!(image::open(&target).unwrap().width(), 4);
        assert!(import_banner(&dir.join("khong-co.png"), &target).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
