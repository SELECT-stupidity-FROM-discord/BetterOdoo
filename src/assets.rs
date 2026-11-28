use anyhow::Result;
use gpui::{App, AssetSource, SharedString};
use std::borrow::Cow;

macro_rules! icons {
    ($($name:literal),*) => {
        &[$(($name, include_bytes!(concat!("../assets/icons/", $name, ".svg")))),*]
    };
}

const ICONS: &[(&str, &[u8])] = icons!(
    "logo",
    "search",
    "tasks",
    "clock",
    "inbox",
    "sparkle",
    "status",
    "user",
    "calendar",
    "check",
    "lock",
    "keyboard",
    "filter",
    "cog",
    "chevron-left",
    "chevron-right",
    "chevron-down",
    "plus",
    "grip",
    "attach",
    "tag",
    "todo",
    "users",
    "window-minimize",
    "window-maximize",
    "window-restore",
    "window-close"
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let name = path.trim_start_matches("icons/").trim_end_matches(".svg");
        Ok(ICONS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| Cow::Borrowed(*b)))
    }

    fn list(&self, _: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .map(|(n, _)| format!("icons/{n}.svg").into())
            .collect())
    }
}

pub fn load_fonts(cx: &App) -> Result<()> {
    cx.text_system().add_fonts(vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Bold.ttf")),
    ])
}
