// Sets the Dock icon at runtime so it also shows when running unbundled.
#[cfg(target_os = "macos")]
pub fn set() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(include_bytes!("../assets/logo/icon-mac-512.png"));
    if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
        // Safe here: main thread, and the image is a valid NSImage.
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set() {}

// X11 window icon; Windows and macOS take theirs from the bundle, Wayland from the .desktop file.
pub fn window() -> Option<std::sync::Arc<image::RgbaImage>> {
    let img = image::load_from_memory(include_bytes!("../assets/logo/icon-512.png")).ok()?;
    Some(std::sync::Arc::new(img.into_rgba8()))
}
