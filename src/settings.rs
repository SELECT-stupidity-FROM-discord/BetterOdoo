use gpui::{Image, ImageFormat};
use std::{
    env::var,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock, RwLock},
};

const AVATAR_EXTS: &[&str] = &["png", "jpg", "webp", "gif", "bmp", "tif", "ico", "svg"];
const MAX_AVATAR_BYTES: usize = 15 * 1024 * 1024;

static AVATAR: OnceLock<RwLock<Option<Arc<Image>>>> = OnceLock::new();

fn path(file: &str) -> Option<PathBuf> {
    let dir = if cfg!(target_os = "macos") {
        PathBuf::from(var("HOME").ok()?).join("Library/Application Support")
    } else if cfg!(windows) {
        PathBuf::from(var("APPDATA").ok()?)
    } else {
        var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .ok()?
    };
    Some(dir.join("BetterOdoo").join(file))
}

fn detect_image_format(bytes: &[u8], ext: Option<&str>) -> Option<ImageFormat> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(ImageFormat::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(ImageFormat::Jpeg);
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(ImageFormat::Webp);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(ImageFormat::Gif);
    }
    if bytes.starts_with(b"BM") {
        return Some(ImageFormat::Bmp);
    }
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return Some(ImageFormat::Tiff);
    }
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some(ImageFormat::Ico);
    }
    match ext.map(str::to_ascii_lowercase).as_deref() {
        Some("png") => Some(ImageFormat::Png),
        Some("jpg" | "jpeg") => Some(ImageFormat::Jpeg),
        Some("webp") => Some(ImageFormat::Webp),
        Some("gif") => Some(ImageFormat::Gif),
        Some("bmp") => Some(ImageFormat::Bmp),
        Some("tif" | "tiff") => Some(ImageFormat::Tiff),
        Some("ico") => Some(ImageFormat::Ico),
        Some("svg") => Some(ImageFormat::Svg),
        _ => None,
    }
}

fn load_avatar_from_disk() -> Option<Arc<Image>> {
    for ext in AVATAR_EXTS {
        let p = path(&format!("avatar.{ext}"))?;
        if let Ok(bytes) = fs::read(&p)
            && !bytes.is_empty()
            && let Some(format) = detect_image_format(&bytes, Some(ext))
        {
            return Some(Arc::new(Image::from_bytes(format, bytes)));
        }
    }
    None
}

fn avatar_slot() -> &'static RwLock<Option<Arc<Image>>> {
    AVATAR.get_or_init(|| RwLock::new(load_avatar_from_disk()))
}

pub fn profile_avatar() -> Option<Arc<Image>> {
    avatar_slot().read().ok().and_then(|g| g.clone())
}

pub fn set_profile_avatar(src: &Path) -> Result<(), String> {
    let bytes = fs::read(src).map_err(|e| format!("Couldn’t read image: {e}"))?;
    if bytes.is_empty() {
        return Err("That file is empty.".into());
    }
    if bytes.len() > MAX_AVATAR_BYTES {
        return Err("Image is too large (max 15 MB).".into());
    }
    let ext = src.extension().and_then(|e| e.to_str());
    let format = detect_image_format(&bytes, ext)
        .ok_or_else(|| "Unsupported image format. Choose a PNG, JPG, WebP, or GIF.".to_string())?;

    let target = path(&format!("avatar.{}", format.extension()))
        .ok_or_else(|| "Couldn’t find the local settings folder.".to_string())?;
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Couldn’t create settings folder: {e}"))?;
    }
    for old_ext in AVATAR_EXTS {
        if let Some(old) = path(&format!("avatar.{old_ext}"))
            && old != target
        {
            fs::remove_file(old).ok();
        }
    }
    fs::write(&target, &bytes).map_err(|e| format!("Couldn’t save image: {e}"))?;

    let image = Arc::new(Image::from_bytes(format, bytes));
    if let Ok(mut slot) = avatar_slot().write() {
        *slot = Some(image);
    }
    Ok(())
}

pub fn clear_profile_avatar() {
    for ext in AVATAR_EXTS {
        if let Some(p) = path(&format!("avatar.{ext}")) {
            fs::remove_file(p).ok();
        }
    }
    if let Ok(mut slot) = avatar_slot().write() {
        *slot = None;
    }
}

pub fn model() -> Option<String> {
    let name = fs::read_to_string(path("model")?).ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

pub fn set_model(name: &str) {
    let Some(p) = path("model") else { return };
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).ok();
    }
    fs::write(p, name).ok();
}

pub fn zoom() -> Option<f32> {
    fs::read_to_string(path("zoom")?).ok()?.trim().parse().ok()
}

pub fn set_zoom(v: f32) {
    let Some(p) = path("zoom") else { return };
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).ok();
    }
    fs::write(p, v.to_string()).ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_image_formats() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        let jpg = [0xFF, 0xD8, 0xFF, 0xE0];
        let webp = b"RIFF\x24\x00\x00\x00WEBP";
        let gif = b"GIF89a\x01\x00";

        assert_eq!(
            detect_image_format(&png, Some("jpg")),
            Some(ImageFormat::Png)
        );
        assert_eq!(detect_image_format(&jpg, None), Some(ImageFormat::Jpeg));
        assert_eq!(detect_image_format(webp, None), Some(ImageFormat::Webp));
        assert_eq!(detect_image_format(gif, None), Some(ImageFormat::Gif));
        assert_eq!(detect_image_format(b"hello world", Some("txt")), None);
        assert_eq!(
            detect_image_format(b"<svg></svg>", Some("SVG")),
            Some(ImageFormat::Svg)
        );
    }
}
