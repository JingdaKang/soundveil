//! How captions look: their colors and opacity, the ready-made presets, and whether a choice stays
//! readable. Saved styles use the same JSON as the Mac app's.

use serde::{Deserialize, Serialize};

/// An sRGB color packed as `0xRRGGBB`, so saved styles compare exactly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaptionColor {
    hex: u32,
}

impl CaptionColor {
    pub const fn new(hex: u32) -> Self {
        Self {
            hex: hex & 0xFF_FFFF,
        }
    }

    /// Takes channels from 0 to 1. Anything outside that range is clamped.
    pub fn from_channels(red: f64, green: f64, blue: f64) -> Self {
        let byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self::new(byte(red) << 16 | byte(green) << 8 | byte(blue))
    }

    /// Reads a color as a person types it, such as `#FFD14D` or `ffd14d`.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.trim();
        let digits = digits.strip_prefix('#').unwrap_or(digits);
        if digits.len() != 6 || !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(digits, 16).ok().map(Self::new)
    }

    /// The form [`Self::parse`] reads, such as `#FFD14D`.
    pub fn label(self) -> String {
        format!("#{:06X}", self.hex)
    }

    pub fn bytes(self) -> [u8; 3] {
        let [_, red, green, blue] = self.hex.to_be_bytes();
        [red, green, blue]
    }

    fn channels(self) -> [f64; 3] {
        self.bytes().map(|byte| f64::from(byte) / 255.0)
    }

    /// Relative luminance, as WCAG defines it.
    pub fn luminance(self) -> f64 {
        let linear = |value: f64| {
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        let [red, green, blue] = self.channels().map(linear);
        0.2126 * red + 0.7152 * green + 0.0722 * blue
    }

    pub fn contrast(self, other: Self) -> f64 {
        let (one, two) = (self.luminance(), other.luminance());
        (one.max(two) + 0.05) / (one.min(two) + 0.05)
    }

    pub fn blended_over(self, other: Self, opacity: f64) -> Self {
        let [top, under] = [self.channels(), other.channels()];
        let mix = |index: usize| top[index] * opacity + under[index] * (1.0 - opacity);
        Self::from_channels(mix(0), mix(1), mix(2))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CaptionStyle {
    pub text: CaptionColor,
    pub background: CaptionColor,
    /// Sound words, slang, and cues such as music.
    pub highlight: CaptionColor,
    pub background_opacity: f64,
    pub text_opacity: f64,
}

impl CaptionStyle {
    /// White on black, which is how captions looked before they could be styled.
    pub const STANDARD: Self = Self {
        text: CaptionColor::new(0xFFFFFF),
        background: CaptionColor::new(0x000000),
        highlight: CaptionColor::new(0xFFD14D),
        background_opacity: 0.78,
        text_opacity: 1.0,
    };

    pub const LOWEST_TEXT_OPACITY: f64 = 0.4;

    /// Below this the video behind matters more than the background color. White text on black at
    /// this opacity still clears 4.5:1 with a white video behind it.
    pub const SEE_THROUGH: f64 = 0.6;

    /// WCAG AA, which is also the usual advice for captions.
    const READABLE_CONTRAST: f64 = 4.5;

    pub fn is_see_through(&self) -> bool {
        self.background_opacity < Self::SEE_THROUGH
    }

    /// The color of the shadow, chosen to stand apart from the text.
    pub fn edge(&self) -> CaptionColor {
        CaptionColor::new(if self.text.luminance() > 0.4 {
            0x000000
        } else {
            0xFFFFFF
        })
    }

    /// A see-through overlay is judged by its shadow and the video, which can't be known here.
    pub fn is_hard_to_read(&self) -> bool {
        if self.is_see_through() {
            return false;
        }
        let shown = self.text.blended_over(self.background, self.text_opacity);
        shown
            .contrast(self.background)
            .min(self.highlight.contrast(self.background))
            < Self::READABLE_CONTRAST
    }

    /// Brings a style read from a file back in range, so an edited file can't hide the captions.
    pub fn in_range(self) -> Self {
        Self {
            text: CaptionColor::new(self.text.hex),
            background: CaptionColor::new(self.background.hex),
            highlight: CaptionColor::new(self.highlight.hex),
            background_opacity: self.background_opacity.clamp(0.0, 1.0),
            text_opacity: self.text_opacity.clamp(Self::LOWEST_TEXT_OPACITY, 1.0),
        }
    }
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self::STANDARD
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaptionPreset {
    pub name: &'static str,
    pub style: CaptionStyle,
}

impl CaptionPreset {
    /// Each mirrors a look people already ask for: the standard dark box, text alone over the
    /// video, and the high-contrast pairs that system caption settings offer. A highlight has to
    /// stand apart from its own text color, so it changes with the pair.
    pub const ALL: [Self; 5] = [
        Self {
            name: "Default",
            style: CaptionStyle::STANDARD,
        },
        Self {
            name: "Text only",
            style: CaptionStyle {
                background_opacity: 0.0,
                ..CaptionStyle::STANDARD
            },
        },
        Self::opaque("Yellow on black", 0xFFE600, 0x000000, 0x5CE1E6),
        Self::opaque("Black on white", 0x000000, 0xFFFFFF, 0x0B57D0),
        Self::opaque("Yellow on blue", 0xFFE600, 0x0A2A8C, 0xFFFFFF),
    ];

    const fn opaque(name: &'static str, text: u32, background: u32, highlight: u32) -> Self {
        Self {
            name,
            style: CaptionStyle {
                text: CaptionColor::new(text),
                background: CaptionColor::new(background),
                highlight: CaptionColor::new(highlight),
                background_opacity: 1.0,
                text_opacity: 1.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: CaptionColor = CaptionColor::new(0xFFFFFF);
    const BLACK: CaptionColor = CaptionColor::new(0x000000);
    const GRAY: CaptionColor = CaptionColor::new(0x777777);

    fn close(value: f64, expected: f64, accuracy: f64) {
        assert!(
            (value - expected).abs() <= accuracy,
            "{value} is not {expected}"
        );
    }

    #[test]
    fn color_channels_survive_a_round_trip() {
        let color = CaptionColor::new(0xFFD14D);
        let [red, green, blue] = color.channels();
        assert_eq!(CaptionColor::from_channels(red, green, blue), color);
    }

    #[test]
    fn channels_outside_the_range_are_clamped() {
        assert_eq!(
            CaptionColor::from_channels(2.0, -1.0, 0.0),
            CaptionColor::new(0xFF0000)
        );
    }

    #[test]
    fn bits_above_the_color_are_dropped() {
        assert_eq!(CaptionColor::new(0xAA00_0001).hex, 1);
    }

    #[test]
    fn typed_colors_are_read() {
        let color = Some(CaptionColor::new(0xFFD14D));
        assert_eq!(CaptionColor::parse("#FFD14D"), color);
        assert_eq!(CaptionColor::parse("ffd14d"), color);
        assert_eq!(CaptionColor::parse("  #ffD14d "), color);
    }

    #[test]
    fn unfinished_or_wrong_colors_are_refused() {
        for text in ["", "#", "#FFD14", "#FFD14D0", "#GGGGGG", "+12345", "#é1234"] {
            assert_eq!(CaptionColor::parse(text), None, "{text}");
        }
    }

    #[test]
    fn a_label_reads_back_as_the_same_color() {
        let color = CaptionColor::new(0x0A2A8C);
        assert_eq!(color.label(), "#0A2A8C");
        assert_eq!(CaptionColor::parse(&color.label()), Some(color));
    }

    #[test]
    fn contrast_matches_wcag() {
        close(WHITE.contrast(BLACK), 21.0, 0.001);
        close(BLACK.contrast(WHITE), 21.0, 0.001);
        close(GRAY.contrast(WHITE), 4.48, 0.01);
    }

    #[test]
    fn default_keeps_the_original_look() {
        let style = CaptionStyle::default();
        assert_eq!(style.text, WHITE);
        assert_eq!(style.background, BLACK);
        assert_eq!(style.highlight, CaptionColor::new(0xFFD14D));
        assert_eq!(style.background_opacity, 0.78);
        assert_eq!(style.text_opacity, 1.0);
        assert!(!style.is_see_through());
    }

    #[test]
    fn presets_are_readable() {
        for preset in CaptionPreset::ALL {
            let style = preset.style;
            assert!(!style.is_hard_to_read(), "{}", preset.name);
            assert!(
                style.text.contrast(style.background) >= 4.5,
                "{}",
                preset.name
            );
            assert!(
                style.highlight.contrast(style.background) >= 4.5,
                "{}",
                preset.name
            );
        }
    }

    #[test]
    fn highlights_stand_apart_from_their_text() {
        for preset in CaptionPreset::ALL {
            assert_ne!(preset.style.highlight, preset.style.text, "{}", preset.name);
        }
    }

    #[test]
    fn similar_colors_are_hard_to_read() {
        let style = CaptionStyle {
            background: CaptionColor::new(0xEEEEEE),
            ..CaptionStyle::STANDARD
        };
        assert!(style.is_hard_to_read());
    }

    #[test]
    fn a_faint_highlight_is_hard_to_read() {
        let style = CaptionStyle {
            highlight: CaptionColor::new(0x222222),
            ..CaptionStyle::STANDARD
        };
        assert!(style.is_hard_to_read());
    }

    #[test]
    fn faded_text_is_hard_to_read() {
        let mut style = CaptionStyle {
            text_opacity: 0.4,
            ..CaptionStyle::STANDARD
        };
        assert!(style.is_hard_to_read());
        style.text_opacity = 0.6;
        assert!(!style.is_hard_to_read());
    }

    #[test]
    fn see_through_styles_are_judged_by_their_shadow() {
        let style = CaptionStyle {
            background: CaptionColor::new(0xEEEEEE),
            background_opacity: 0.3,
            ..CaptionStyle::STANDARD
        };
        assert!(style.is_see_through());
        assert!(!style.is_hard_to_read());
    }

    #[test]
    fn the_shadow_starts_just_below_the_opacity_limit() {
        let mut style = CaptionStyle {
            background_opacity: CaptionStyle::SEE_THROUGH,
            ..CaptionStyle::STANDARD
        };
        assert!(!style.is_see_through());
        style.background_opacity = 0.59;
        assert!(style.is_see_through());
    }

    #[test]
    fn the_shadow_opposes_the_text() {
        assert_eq!(CaptionStyle::STANDARD.edge(), BLACK);
        let dark = CaptionStyle {
            text: BLACK,
            ..CaptionStyle::STANDARD
        };
        assert_eq!(dark.edge(), WHITE);
    }

    #[test]
    fn styles_survive_saving() {
        let style = CaptionPreset::ALL[3].style;
        let data = serde_json::to_vec(&style).unwrap();
        assert_eq!(
            serde_json::from_slice::<CaptionStyle>(&data).unwrap(),
            style
        );
        assert!(serde_json::from_slice::<CaptionStyle>(b"nonsense").is_err());
    }

    #[test]
    fn a_style_saved_by_the_mac_app_is_read() {
        let saved = r#"{"text":{"hex":16770560},"background":{"hex":666252},
            "highlight":{"hex":16777215},"backgroundOpacity":1,"textOpacity":1}"#;
        let style: CaptionStyle = serde_json::from_str(saved).unwrap();
        assert_eq!(style, CaptionPreset::ALL[4].style);
    }

    #[test]
    fn a_style_missing_a_value_takes_the_default() {
        let style: CaptionStyle = serde_json::from_str(r#"{"backgroundOpacity":0.5}"#).unwrap();
        assert_eq!(style.background_opacity, 0.5);
        assert_eq!(style.text, WHITE);
    }

    #[test]
    fn an_edited_file_cannot_hide_the_captions() {
        let saved = r#"{"text":{"hex":33554431},"backgroundOpacity":4,"textOpacity":0}"#;
        let style = serde_json::from_str::<CaptionStyle>(saved)
            .unwrap()
            .in_range();
        assert_eq!(style.text, WHITE);
        assert_eq!(style.background_opacity, 1.0);
        assert_eq!(style.text_opacity, CaptionStyle::LOWEST_TEXT_OPACITY);
    }

    #[test]
    fn styles_in_range_stay_as_they_are() {
        for preset in CaptionPreset::ALL {
            assert_eq!(preset.style.in_range(), preset.style, "{}", preset.name);
        }
    }
}
