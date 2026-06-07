use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const ICON_PNG: &str = "src/assets/icon.png";

fn main() {
    println!("cargo:rerun-if-changed={ICON_PNG}");
    println!("cargo:rerun-if-changed=build.rs");

    // Resources are a Windows-only concept; skip on other targets.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let png = fs::read(ICON_PNG).expect("read icon.png");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let ico_path = out_dir.join("icon.ico");
    write_ico(&ico_path, &png);

    let mut res = winresource::WindowsResource::new();
    res.set_icon_with_id(ico_path.to_str().expect("ico path utf-8"), "1");
    res.compile().expect("embed windows resources (icon)");
}

// Wrap a single 256x256 PNG in a minimal ICONDIR so it becomes a valid .ico.
// Windows (Vista+) accepts PNG-compressed icon entries and scales this entry
// down for the smaller shell sizes.
fn write_ico(path: &Path, png: &[u8]) {
    let mut file = fs::File::create(path).expect("create icon.ico");
    let len = png.len() as u32;

    // ICONDIR header.
    file.write_all(&0u16.to_le_bytes()).unwrap(); // reserved
    file.write_all(&1u16.to_le_bytes()).unwrap(); // type: 1 = icon
    file.write_all(&1u16.to_le_bytes()).unwrap(); // image count

    // ICONDIRENTRY.
    file.write_all(&[0]).unwrap(); // width  (0 => 256)
    file.write_all(&[0]).unwrap(); // height (0 => 256)
    file.write_all(&[0]).unwrap(); // color count (0 => truecolor)
    file.write_all(&[0]).unwrap(); // reserved
    file.write_all(&1u16.to_le_bytes()).unwrap(); // color planes
    file.write_all(&32u16.to_le_bytes()).unwrap(); // bits per pixel
    file.write_all(&len.to_le_bytes()).unwrap(); // size of image data
    file.write_all(&22u32.to_le_bytes()).unwrap(); // offset (6 + 16)

    file.write_all(png).unwrap();
}
