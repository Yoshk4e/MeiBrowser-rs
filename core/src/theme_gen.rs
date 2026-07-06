//! Builds out the application’s entire color scheme based off of a game icon.
//!
//! Absolutely nothing about this is hardcoded per game; we just analyze the pixels in the icon,
//! extract the “most characterful” color from it (by giving precedence to more saturated colors
//! at medium lightness levels over shades closer to white/black, and ignoring any transparent
//! backgrounds), and then construct every other part of the palette from there using HSL arithmetic.

use image::GenericImageView;

/// One binding slot for each exposed `in-out` brush in the Slint `Palette`.
/// The colors are just simple triples (r, g, b); the caller converts
/// them to `slint::Color` and `slint::Brush`.
#[derive(Clone, Copy, Debug)]
pub struct GeneratedTheme {
    pub bg: (u8, u8, u8),
    pub surface: (u8, u8, u8),
    pub surface_raised: (u8, u8, u8),
    pub surface_hover: (u8, u8, u8),
    pub border: (u8, u8, u8),
    pub border_hover: (u8, u8, u8),
    pub accent: (u8, u8, u8),
    pub accent_hover: (u8, u8, u8),
    pub accent_pressed: (u8, u8, u8),
    pub accent_dim: (u8, u8, u8),
    pub on_accent: (u8, u8, u8),
}

pub const DEFAULT_THEME: GeneratedTheme = GeneratedTheme {
    bg: (0x0f, 0x11, 0x15),
    surface: (0x17, 0x1a, 0x21),
    surface_raised: (0x12, 0x14, 0x1a),
    surface_hover: (0x1d, 0x22, 0x2c),
    border: (0x26, 0x2b, 0x36),
    border_hover: (0x3a, 0x41, 0x52),
    accent: (0xe8, 0xac, 0x3d),
    accent_hover: (0xf0, 0xb9, 0x4d),
    accent_pressed: (0xd9, 0xa4, 0x41),
    accent_dim: (0x2a, 0x24, 0x11),
    on_accent: (0x14, 0x17, 0x1f),
};

pub fn theme_from_icon(icon_bytes: &[u8]) -> GeneratedTheme {
    let seed = dominant_color(icon_bytes).unwrap_or(DEFAULT_THEME.accent);
    theme_from_seed(seed)
}

fn dominant_color(bytes: &[u8]) -> Option<(u8, u8, u8)> {
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let rgba = img.to_rgba8();

    let mut r_sum = 0u64;
    let mut g_sum = 0u64;
    let mut b_sum = 0u64;
    let mut weight_sum = 0u64;

    for px in rgba.pixels() {
        let [r, g, b, a] = px.0;
        if a < 40 {
            continue; // transparent background
        }

        let (h, s, l) = rgb_to_hsl(r, g, b);
        let _ = h;
        // De-emphasize near-white / near-black / washed-out pixels; a chibi's
        // white shirt or black outline shouldn't out-vote its hair color.
        if l < 0.08 || l > 0.92 {
            continue;
        }

        let weight = 1.0 + s * s * 6.0; // saturation squared: vivid colors dominate hard
        let weight = (weight * a as f32 / 255.0) as u64;
        if weight == 0 {
            continue;
        }

        r_sum += r as u64 * weight;
        g_sum += g as u64 * weight;
        b_sum += b as u64 * weight;
        weight_sum += weight;
    }

    if weight_sum == 0 {
        return None;
    }

    Some((
        (r_sum / weight_sum) as u8,
        (g_sum / weight_sum) as u8,
        (b_sum / weight_sum) as u8,
    ))
}

/// Build all palette slots from a single seed color using the same hue
/// throughout both a darker, less saturated UI ramp (surfaces/borders)
/// and a brighter, more saturated ramp (accent states).
fn theme_from_seed((r, g, b): (u8, u8, u8)) -> GeneratedTheme {
    let (h, s, _l) = rgb_to_hsl(r, g, b);

    // Keep the accent itself readable regardless of how dark/light or how
    // muted the source pixel happened to be.
    let accent_s = s.clamp(0.45, 0.85);
    let accent = hsl_to_rgb(h, accent_s, 0.58);
    let accent_hover = hsl_to_rgb(h, accent_s, 0.66);
    let accent_pressed = hsl_to_rgb(h, accent_s, 0.52);
    let accent_dim = hsl_to_rgb(h, (accent_s * 0.75).min(0.6), 0.14);

    // Neutral-ish, barely-tinted dark surfaces so the UI stays legible and
    // doesn't turn into a colored slab, while still reading as "themed".
    let bg = hsl_to_rgb(h, 0.18, 0.07);
    let surface = hsl_to_rgb(h, 0.16, 0.11);
    let surface_raised = hsl_to_rgb(h, 0.16, 0.09);
    let surface_hover = hsl_to_rgb(h, 0.18, 0.15);
    let border = hsl_to_rgb(h, 0.16, 0.20);
    let border_hover = hsl_to_rgb(h, 0.18, 0.28);

    // Pick readable text-on-accent (dark text on light/vivid accents, light
    // text on the rare very dark accent) using relative luminance.
    let on_accent = if relative_luminance(accent) > 0.5 {
        (0x14, 0x17, 0x1f)
    } else {
        (0xff, 0xff, 0xff)
    };

    GeneratedTheme {
        bg,
        surface,
        surface_raised,
        surface_hover,
        border,
        border_hover,
        accent,
        accent_hover,
        accent_pressed,
        accent_dim,
        on_accent,
    }
}

fn relative_luminance((r, g, b): (u8, u8, u8)) -> f32 {
    // Perceptual (not linearized-sRGB, but good enough for a UI contrast pick).
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };

    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s <= 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    let to_channel = |t: f32| -> f32 {
        let mut t = t;
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    };

    let r = to_channel(h + 1.0 / 3.0);
    let g = to_channel(h);
    let b = to_channel(h - 1.0 / 3.0);

    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}
