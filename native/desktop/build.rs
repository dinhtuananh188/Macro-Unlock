//! Nhúng manifest Windows. Bản release đòi quyền quản trị như bản Electron (`requireAdministrator`): macro chỉ nhận
//! phím và chuột khi game chạy quyền admin nếu chính nó cũng chạy quyền admin. Bản debug giữ `asInvoker` để phát triển
//! và kiểm thử không bị UAC chặn.
use embed_manifest::manifest::ExecutionLevel;
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    let level = if release { ExecutionLevel::RequireAdministrator } else { ExecutionLevel::AsInvoker };
    embed_manifest(new_manifest("Cryss.Native").requested_execution_level(level)).expect("không nhúng được manifest Windows");
}
