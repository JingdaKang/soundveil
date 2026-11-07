//! Hands a caption style to the interface. Each window holds its own copy of the look, so both have
//! to be told.

use crate::{
    Appearance, CaptionAppearance, CaptionLook, CaptionOverlay, ColorSwatch, MainWindow,
    StylePreset,
};
use slint::{Color, ComponentHandle, ModelRc, VecModel};
use soundveil_core::{CaptionColor, CaptionPreset, CaptionStyle};

/// The colors the presets use, plus a gray and a red to round them out.
const SWATCHES: [u32; 9] = [
    0xFFFFFF, 0x000000, 0x8A8A8A, 0xFFE600, 0xFFD14D, 0xE5484D, 0x5CE1E6, 0x0B57D0, 0x0A2A8C,
];

fn color(color: CaptionColor) -> Color {
    let [red, green, blue] = color.bytes();
    Color::from_rgb_u8(red, green, blue)
}

fn look(style: &CaptionStyle) -> CaptionAppearance {
    CaptionAppearance {
        text: color(style.text),
        background: color(style.background),
        highlight: color(style.highlight),
        background_opacity: style.background_opacity as f32,
        text_opacity: style.text_opacity as f32,
        shadowed: style.is_see_through(),
        edge: color(style.edge()),
    }
}

/// The slider moves in hundredths and is a little off in single precision. Rounding keeps a style
/// that was only dragged equal to a preset, and the saved file readable.
fn hundredths(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

fn preset_index(style: &CaptionStyle) -> i32 {
    CaptionPreset::ALL
        .iter()
        .position(|preset| preset.style == *style)
        .map_or(-1, |index| index as i32)
}

/// Fills in what never changes.
pub fn prepare(ui: &MainWindow) {
    let settings = ui.global::<Appearance>();
    let presets = CaptionPreset::ALL.map(|preset| StylePreset {
        name: preset.name.into(),
        look: look(&preset.style),
    });
    let swatches = SWATCHES.map(|hex| {
        let swatch = CaptionColor::new(hex);
        ColorSwatch {
            value: color(swatch),
            hex: swatch.label().into(),
        }
    });
    settings.set_presets(ModelRc::new(VecModel::from(presets.to_vec())));
    settings.set_swatches(ModelRc::new(VecModel::from(swatches.to_vec())));
    settings.set_lowest_text_opacity(CaptionStyle::LOWEST_TEXT_OPACITY as f32);
}

/// Draws a style everywhere. The color fields are left alone, because rewriting one would move
/// what someone is typing out from under them.
pub fn draw(ui: &MainWindow, overlay: &CaptionOverlay, style: &CaptionStyle) {
    let look = look(style);
    ui.global::<CaptionLook>().set_style(look.clone());
    overlay.global::<CaptionLook>().set_style(look);
    let settings = ui.global::<Appearance>();
    settings.set_preset(preset_index(style));
    settings.set_hard_to_read(style.is_hard_to_read());
}

/// Draws a style and puts the fields in step with it.
pub fn show(ui: &MainWindow, overlay: &CaptionOverlay, style: &CaptionStyle) {
    draw(ui, overlay, style);
    let settings = ui.global::<Appearance>();
    settings.set_text_hex(style.text.label().into());
    settings.set_background_hex(style.background.label().into());
    settings.set_highlight_hex(style.highlight.label().into());
    settings.set_background_opacity(style.background_opacity as f32);
    settings.set_text_opacity(style.text_opacity as f32);
}

/// Applies a color from Settings. Returns whether it was one, because it may be half typed.
pub fn choose_color(style: &mut CaptionStyle, which: i32, text: &str) -> bool {
    let Some(color) = CaptionColor::parse(text) else {
        return false;
    };
    match which {
        0 => style.text = color,
        1 => style.background = color,
        _ => style.highlight = color,
    }
    true
}

/// Reads both sliders into a style.
pub fn read_opacity(ui: &MainWindow, style: &mut CaptionStyle) {
    let settings = ui.global::<Appearance>();
    style.background_opacity = hundredths(settings.get_background_opacity());
    style.text_opacity = hundredths(settings.get_text_opacity());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_a_slider_back_lands_on_the_preset() {
        let mut style = CaptionStyle::STANDARD;
        style.background_opacity = hundredths(0.78_f32);
        assert_eq!(preset_index(&style), 0);
    }

    #[test]
    fn a_changed_style_matches_no_preset() {
        let mut style = CaptionStyle::STANDARD;
        style.background_opacity = hundredths(0.77_f32);
        assert_eq!(preset_index(&style), -1);
    }

    #[test]
    fn every_preset_is_found() {
        for (index, preset) in CaptionPreset::ALL.iter().enumerate() {
            assert_eq!(preset_index(&preset.style), index as i32);
        }
    }

    #[test]
    fn a_swatch_sets_the_color_it_was_chosen_for() {
        let mut style = CaptionStyle::STANDARD;
        assert!(choose_color(&mut style, 1, "#0A2A8C"));
        assert_eq!(style.background, CaptionColor::new(0x0A2A8C));
        assert_eq!(style.text, CaptionStyle::STANDARD.text);
        assert!(choose_color(&mut style, 2, "#5CE1E6"));
        assert_eq!(style.highlight, CaptionColor::new(0x5CE1E6));
    }

    #[test]
    fn half_typed_colors_change_nothing() {
        let mut style = CaptionStyle::STANDARD;
        assert!(!choose_color(&mut style, 0, "#FFD1"));
        assert_eq!(style, CaptionStyle::STANDARD);
    }

    #[test]
    fn every_preset_color_has_a_swatch() {
        for preset in CaptionPreset::ALL {
            let style = preset.style;
            for color in [style.text, style.background, style.highlight] {
                let [red, green, blue] = color.bytes();
                let hex = u32::from_be_bytes([0, red, green, blue]);
                assert!(SWATCHES.contains(&hex), "{}", color.label());
            }
        }
    }
}
