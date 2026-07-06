use super::{MainWindow, Palette};
use crate::GeneratedTheme;
use slint::{Brush, Color, ComponentHandle};

// Same PNGs used for the `@image-url` badges in Slint `cards.slint`, embedded
// again here to allow the theme derivation to access the raw pixels,
// the images used by Slint are not accessible from Rust.
const ICON_GI: &[u8] = include_bytes!("../../ui/assets/gi-icon.png");
const ICON_HSR: &[u8] = include_bytes!("../../ui/assets/hsr-icon.png");
const ICON_ZZZ: &[u8] = include_bytes!("../../ui/assets/zzz-icon.png");

pub(crate) fn icon_bytes_for_tag(tag: &str) -> &'static [u8] {
    match tag {
        "GI" => ICON_GI,
        "HSR" => ICON_HSR,
        "ZZZ" => ICON_ZZZ,
        _ => ICON_GI,
    }
}

fn brush_of((r, g, b): (u8, u8, u8)) -> Brush {
    Brush::SolidColor(Color::from_rgb_u8(r, g, b))
}

pub(crate) fn apply_theme(ui: &MainWindow, theme: GeneratedTheme) {
    let palette = ui.global::<Palette>();
    palette.set_bg(brush_of(theme.bg));
    palette.set_surface(brush_of(theme.surface));
    palette.set_surface_raised(brush_of(theme.surface_raised));
    palette.set_surface_hover(brush_of(theme.surface_hover));
    palette.set_border(brush_of(theme.border));
    palette.set_border_hover(brush_of(theme.border_hover));
    palette.set_accent(brush_of(theme.accent));
    palette.set_accent_hover(brush_of(theme.accent_hover));
    palette.set_accent_pressed(brush_of(theme.accent_pressed));
    palette.set_accent_dim(brush_of(theme.accent_dim));
    palette.set_on_accent(brush_of(theme.on_accent));
}
