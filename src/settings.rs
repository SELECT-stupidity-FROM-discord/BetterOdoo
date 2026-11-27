use std::{env::var, fs, path::PathBuf};

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
