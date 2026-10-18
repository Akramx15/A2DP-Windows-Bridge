//! Embeds the app icon (ID 101, also the tray icon; drawn by
//! app/resources/make_icon.py) and the manifest (per-monitor DPI awareness,
//! Windows 10/11)

fn main() {
    embed_resource::compile("resources/core.rc", embed_resource::NONE).manifest_required().unwrap();
    println!("cargo:rerun-if-changed=resources/core.rc");
}
