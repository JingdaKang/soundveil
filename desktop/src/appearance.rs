//! Hands a caption style to the interface. Each window holds its own copy of the look, so both have
//! to be told.

use crate::{CaptionAppearance, CaptionLook, CaptionOverlay, MainWindow};
use slint::{Color, ComponentHandle};
use soundveil_core::{CaptionColor, CaptionStyle};

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

pub fn show(ui: &MainWindow, overlay: &CaptionOverlay, style: &CaptionStyle) {
    let look = look(style);
    ui.global::<CaptionLook>().set_style(look.clone());
    overlay.global::<CaptionLook>().set_style(look);
}
