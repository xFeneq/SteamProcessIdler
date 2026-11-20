fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("ProductName", "SteamProcessIdler");
        res.set("FileDescription", "Steam Process Idler");
        res.set("ProductVersion", "1.3.0.0");
        res.set("FileVersion", "1.3.0.0");
        res.set("LegalCopyright", "Copyright (c) 2026");
        let _ = res.compile();
    }
}
