// A release build opens no console window on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[allow(dead_code)]
mod paths;

#[allow(dead_code)]
mod settings;

use slint::ComponentHandle;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    ui.run()
}
