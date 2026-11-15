fn main() {
    // The window of the installer / uninstaller (ui/setup.slint). The logo is embedded into the exe.
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed=../radiant-ui/assets/logo.png");
    slint_build::compile("ui/setup.slint").unwrap();

    // Icon of the setup exe (Explorer, taskbar): the same icon as radiant.exe.
    println!("cargo:rerun-if-changed=../radiant-ui/assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../radiant-ui/assets/icon.ico");
        res.set("FileDescription", "Radiant Setup");
        res.set("ProductName", "Radiant");
        res.compile().unwrap();
    }
}
