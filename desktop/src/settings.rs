//! Choices that survive a restart.

use crate::paths;
use serde::{Deserialize, Serialize};
use soundveil_core::CaptionStyle;
use std::fs;

/// Where the overlay sits. The position is in physical pixels and the size in logical ones, which
/// is how the two are set.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub model: String,
    pub update_speed: String,
    pub caption_size: f32,
    pub caption_style: CaptionStyle,
    pub quiet_threshold: f32,
    pub highlight_words: bool,
    pub click_through: bool,
    pub overlay: Option<Frame>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: soundveil_core::Model::default().raw_value().into(),
            update_speed: soundveil_core::UpdateSpeed::default().raw_value().into(),
            caption_size: 28.0,
            caption_style: CaptionStyle::STANDARD,
            quiet_threshold: -50.0,
            highlight_words: true,
            click_through: false,
            overlay: None,
        }
    }
}

impl Settings {
    /// A missing or damaged file gives the defaults, so a bad save can't stop the app opening.
    pub fn load() -> Self {
        fs::read(paths::settings_file())
            .ok()
            .and_then(|data| Self::read(&data))
            .unwrap_or_default()
    }

    fn read(data: &[u8]) -> Option<Self> {
        let settings: Self = serde_json::from_slice(data).ok()?;
        Some(Self {
            caption_style: settings.caption_style.in_range(),
            ..settings
        })
    }

    pub fn save(&self) -> Result<(), String> {
        let path = paths::settings_file();
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, data).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_files_keep_defaults_for_new_choices() {
        let saved: Settings = serde_json::from_str(r#"{"captionSize":32}"#).unwrap();
        assert_eq!(saved.caption_size, 32.0);
        assert_eq!(saved.model, "large-v3-turbo");
        assert!(saved.highlight_words);
        assert_eq!(saved.caption_style, CaptionStyle::STANDARD);
    }

    #[test]
    fn a_chosen_style_is_kept() {
        let settings = Settings {
            caption_style: soundveil_core::CaptionPreset::ALL[4].style,
            ..Settings::default()
        };
        let data = serde_json::to_vec(&settings).unwrap();
        assert_eq!(Settings::read(&data), Some(settings));
    }

    #[test]
    fn an_edited_file_cannot_hide_the_captions() {
        let saved = Settings::read(br#"{"captionStyle":{"textOpacity":0}}"#).unwrap();
        assert_eq!(saved.caption_style.text_opacity, 0.4);
    }

    #[test]
    fn choices_round_trip() {
        let settings = Settings {
            click_through: true,
            ..Settings::default()
        };
        let data = serde_json::to_vec(&settings).unwrap();
        assert_eq!(serde_json::from_slice::<Settings>(&data).unwrap(), settings);
    }
}
