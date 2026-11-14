//! The theme's CSS custom properties (AGENTS.md section 7.4): the shell
//! makes them from `udm_domain::theme::appearance`, so components never
//! hardcode a color. Following the system sends a light and a dark set and
//! the frontend picks one by the system's mode.

use udm_domain::theme::{Appearance, Color, Look, Palette};

use crate::contract::{CssVar, Theme};

/// The window's look, from the appearance the settings give.
pub fn theme(appearance: &Appearance) -> Theme {
    match appearance {
        Appearance::FollowSystem { light, dark } => Theme { light: vars(light), dark: vars(dark), follow_system: true },
        Appearance::Fixed(look) => Theme { light: vars(look), dark: vars(look), follow_system: false },
    }
}

/// How saturated a color is, from 0 (gray) to 1.
fn saturation(c: Color) -> f32 {
    let max = c.r.max(c.g).max(c.b);
    let min = c.r.min(c.g).min(c.b);
    if max == 0 {
        0.0
    } else {
        f32::from(max - min) / f32::from(max)
    }
}

/// A tint for icons and progress bars: the palette's own when it has one,
/// else a fixed hue that reads on the background.
fn tint(own: Color, dark: bool, light_hue: Color, dark_hue: Color) -> Color {
    if saturation(own) > 0.35 {
        own
    } else if dark {
        dark_hue
    } else {
        light_hue
    }
}

fn vars(look: &Look) -> Vec<CssVar> {
    let p: &Palette = &look.palette;
    let s = look.sizes;
    let dark = p.dark;
    let (bg, fg) = (p.background, p.foreground);
    // A step from the background toward the text. The surfaces follow
    // Windows 11's: a Mica face behind everything, lists and fields on
    // lighter layers, controls a hair off the layer they sit on.
    let step = |share: f32| bg.mix(fg, share);
    let accent = tint(p.link, dark, Color::rgb(0x00, 0x5f, 0xb8), Color::rgb(0x60, 0xcd, 0xff));
    let progress = tint(p.progress, dark, Color::rgb(0x06, 0xb0, 0x25), Color::rgb(0x06, 0xb0, 0x25));
    // Text on the accent: whichever of black and white reads better.
    let white = Color::rgb(0xff, 0xff, 0xff);
    let black = Color::rgb(0, 0, 0);
    let on = |c: Color| if c.contrast(white) >= c.contrast(black) { white } else { black };
    let on_accent = on(accent);
    // Hover and press fills are the text's color thinly over whatever
    // surface they sit on, so they read on the face and on a list alike.
    let veil = |alpha: u8| Color::rgba(fg.r, fg.g, fg.b, alpha);
    let face = if dark { step(0.095) } else { step(0.05) };
    let field = if dark { step(0.14) } else { bg };
    let colors: Vec<(&str, Color)> = vec![
        ("fg", fg),
        ("fg-muted", fg.mix(bg, if dark { 0.3 } else { 0.4 })),
        ("fg-disabled", fg.mix(bg, if dark { 0.6 } else { 0.55 })),
        // The window's face: toolbars, dialogs, tabs.
        ("face", face),
        // The menu bar and dialogs' title strips, one with the face.
        ("bar", face),
        // The download list, list boxes and the tabs' pages.
        ("field", field),
        ("input-bg", if dark { step(0.16) } else { bg }),
        ("input-border", if dark { step(0.2) } else { step(0.12) }),
        // The underline a text box draws along its foot.
        ("input-underline", if dark { step(0.6) } else { step(0.52) }),
        ("header", field),
        ("list", field),
        // The categories, on a panel like the download list's.
        ("tree", field),
        // The edge round lists, panels and dialogs.
        ("frame", if dark { step(0.2) } else { step(0.1) }),
        ("scroll-track", field),
        ("scroll-thumb", if dark { step(0.45) } else { step(0.4) }),
        ("grid", if dark { step(0.2) } else { step(0.08) }),
        ("border", if dark { step(0.2) } else { step(0.1) }),
        ("button", if dark { step(0.16) } else { bg.mix(fg, 0.008) }),
        ("button-border", if dark { step(0.21) } else { step(0.1) }),
        // Windows 11 draws a button's foot a shade darker than its sides.
        ("button-border-foot", if dark { step(0.17) } else { step(0.2) }),
        ("button-hover", if dark { step(0.19) } else { step(0.025) }),
        ("button-pressed", if dark { step(0.13) } else { step(0.04) }),
        ("default-border", accent),
        ("accent", accent),
        ("accent-hover", accent.mix(bg, 0.1)),
        ("accent-pressed", accent.mix(bg, 0.2)),
        ("on-accent", on_accent),
        ("hover", if dark { veil(15) } else { veil(10) }),
        ("active", if dark { veil(10) } else { veil(6) }),
        ("selected", if dark { field.mix(accent, 0.24) } else { bg.mix(accent, 0.12) }),
        ("selected-fg", fg),
        ("selected-blur", if dark { step(0.21) } else { step(0.07) }),
        ("menu", if dark { step(0.15) } else { bg.mix(fg, 0.01) }),
        ("menu-hover", if dark { veil(15) } else { veil(10) }),
        ("focus", accent),
        // The keyboard's focus ring, in the text's color as Windows 11 draws it.
        ("focus-ring", fg),
        ("progress", progress),
        ("progress-track", if dark { step(0.25) } else { step(0.1) }),
        ("segment", accent),
        ("link", p.link),
        ("error", p.error),
        // Text on the error color, such as a dialog's close button under the mouse.
        ("on-error", on(p.error)),
        ("warning", p.warning),
        ("shadow", p.shadow),
        ("tint-green", if dark { Color::rgb(0x4a, 0xc2, 0x6b) } else { Color::rgb(0x2e, 0x9e, 0x4f) }),
        ("tint-blue", accent),
        ("tint-purple", if dark { Color::rgb(0xb3, 0x9d, 0xfa) } else { Color::rgb(0x7c, 0x3a, 0xed) }),
        ("tint-red", p.error),
        ("tint-folder", if dark { Color::rgb(0xe8, 0xc0, 0x5a) } else { Color::rgb(0xd9, 0xa4, 0x2b) }),
        // A QR code reads only dark on light, whatever the look.
        ("qr-dark", Color::rgb(0, 0, 0)),
        ("qr-light", Color::rgb(0xff, 0xff, 0xff)),
    ];
    let mut out: Vec<CssVar> =
        colors.iter().map(|(name, c)| CssVar { name: format!("--{name}"), value: c.to_string() }).collect();
    let px = |name: &str, v: u8| CssVar { name: format!("--{name}"), value: format!("{v}px") };
    out.push(px("text", s.text));
    out.push(px("control-height", s.control_height));
    out.push(px("radius", s.corner_radius));
    // Menus, dialogs and panels: twice a control's corner, as Windows 11's are.
    out.push(px("radius-large", s.corner_radius.saturating_mul(2)));
    out.push(px("border-width", s.border_width));
    out.push(CssVar { name: "--scheme".into(), value: if dark { "dark".into() } else { "light".into() } });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use udm_domain::theme::{DARK, LIGHT};

    fn value<'a>(vars: &'a [CssVar], name: &str) -> &'a str {
        vars.iter().find(|v| v.name == name).map(|v| v.value.as_str()).unwrap_or_default()
    }

    #[test]
    fn following_the_system_sends_both_looks() {
        let t = theme(&Appearance::FollowSystem { light: LIGHT, dark: DARK });
        assert!(t.follow_system);
        assert_eq!(value(&t.light, "--scheme"), "light");
        assert_eq!(value(&t.dark, "--scheme"), "dark");
        assert_eq!(value(&t.light, "--field"), "#ffffff");
        // Windows 11's light Mica.
        assert_eq!(value(&t.light, "--face"), "#f3f3f3");
    }

    #[test]
    fn text_on_the_accent_reads() {
        let t = theme(&Appearance::FollowSystem { light: LIGHT, dark: DARK });
        assert_eq!(value(&t.light, "--accent"), "#005fb8");
        assert_eq!(value(&t.light, "--on-accent"), "#ffffff");
        assert_eq!(value(&t.dark, "--accent"), "#60cdff");
        assert_eq!(value(&t.dark, "--on-accent"), "#000000");
    }

    #[test]
    fn menus_and_dialogs_round_twice_as_much_as_controls() {
        let t = theme(&Appearance::Fixed(LIGHT));
        assert_eq!(value(&t.light, "--radius"), "4px");
        assert_eq!(value(&t.light, "--radius-large"), "8px");
    }

    #[test]
    fn a_gray_progress_color_becomes_green_and_a_colored_one_stays() {
        let t = theme(&Appearance::Fixed(LIGHT));
        assert_eq!(value(&t.light, "--progress"), "#06b025");
        let mut look = LIGHT;
        look.palette.progress = Color::rgb(0xff, 0x00, 0x80);
        let t = theme(&Appearance::Fixed(look));
        assert_eq!(value(&t.light, "--progress"), "#ff0080");
    }

    #[test]
    fn every_variable_is_named_once() {
        let t = theme(&Appearance::Fixed(DARK));
        let mut names: Vec<_> = t.dark.iter().map(|v| v.name.clone()).collect();
        let all = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), all);
    }
}
