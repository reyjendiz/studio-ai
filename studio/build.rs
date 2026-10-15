// Embeds the round app icon and version info into Studio.exe (needs rc.exe from the Windows SDK).
fn main() {
    println!("cargo:rerun-if-changed=../ui/studio.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../ui/studio.ico").set("ProductName", "Студия").set("FileDescription", "Студия: локальная AI-студия").set("LegalCopyright", "Local AI Studio");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
