//! Where Soundveil keeps its files on each system.

use soundveil_core::Model;
use std::env;
use std::path::PathBuf;

fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

#[cfg(windows)]
fn data_folder() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("Soundveil")
}

// The Mac app's folder, so a preview on a Mac uses models that are already downloaded.
#[cfg(target_os = "macos")]
fn data_folder() -> PathBuf {
    home().join("Library/Application Support/FreeCaptions")
}

#[cfg(not(any(windows, target_os = "macos")))]
fn data_folder() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"))
        .join("soundveil")
}

pub fn models() -> PathBuf {
    data_folder().join("Models")
}

pub fn model_file(model: Model) -> PathBuf {
    models().join(model.file_name())
}

pub fn settings_file() -> PathBuf {
    data_folder().join("settings.json")
}
