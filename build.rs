// Embeds the app icon and metadata into the Windows exe (also used for the window and taskbar icon).
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/logo/icon.ico");
        res.set("ProductName", "BetterOdoo");
        res.set("FileDescription", "BetterOdoo");
        res.compile().expect("embed Windows resources");
    }
}
