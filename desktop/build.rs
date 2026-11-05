fn main() {
    slint_build::compile("ui/app.slint").expect("the interface compiles");
    // Gives the program its icon in Explorer and the taskbar. This needs Windows's own tools, so
    // it's skipped when only checking the Rust from another system.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|system| system == "windows");
    if windows && std::env::var_os("SOUNDVEIL_CHECK_ONLY").is_none() {
        println!("cargo:rerun-if-changed=packaging/windows/soundveil.ico");
        winresource::WindowsResource::new()
            .set_icon("packaging/windows/soundveil.ico")
            .compile()
            .expect("the icon is embedded");
    }
}
