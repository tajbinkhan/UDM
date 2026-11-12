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
    // A step from the background toward the text: the faces of a desktop
    // window are grays between the two.
    let step = |share: f32| bg.mix(fg, share);
    let accent = tint(p.link, dark, Color::rgb(0x00, 0x78, 0xd4), Color::rgb(0x3b, 0x82, 0xf6));
    let progress = tint(p.progress, dark, Color::rgb(0x06, 0xb0, 0x25), Color::rgb(0x06, 0xb0, 0x25));
    let field = if dark { step(0.065) } else { bg };
    let colors: Vec<(&str, Color)> = vec![
        ("fg", fg),
        ("fg-muted", fg.mix(bg, 0.35)),
        ("fg-disabled", fg.mix(bg, if dark { 0.6 } else { 0.55 })),
        // The window's face: toolbars, dialogs, tabs.
        ("face", if dark { step(0.2) } else { step(0.06) }),
        // The menu bar and the panel behind the categories.
        ("bar", if dark { step(0.095) } else { bg }),
        // The download list, the tree and text boxes.
        ("field", field),
        ("input-bg", if dark { field.mix(accent, 0.12) } else { bg }),
        ("input-border", if dark { fg.mix(bg, 0.05) } else { step(0.45) }),
        ("header", field),
        // The download list, the categories tree, the list's frame and the
        // scroll bars' track.
        ("list", if dark { step(0.2) } else { bg }),
        ("tree", if dark { step(0.095) } else { bg }),
        ("frame", if dark { step(0.5) } else { step(0.45) }),
        ("scroll-track", if dark { step(0.055) } else { step(0.06) }),
        ("grid", if dark { step(0.31) } else { step(0.1) }),
        ("border", if dark { step(0.31) } else { step(0.18) }),
        ("button", if dark { step(0.18) } else { bg.mix(fg, 0.008) }),
        ("button-border", if dark { step(0.55) } else { step(0.18) }),
        ("button-hover", if dark { step(0.26) } else { bg.mix(accent, 0.1) }),
        ("button-pressed", if dark { step(0.32) } else { bg.mix(accent, 0.2) }),
        ("default-border", if dark { fg } else { accent }),
        ("hover", if dark { step(0.26) } else { bg.mix(accent, 0.1) }),
        ("selected", if dark { accent.mix(field, 0.3) } else { bg.mix(accent, 0.2) }),
        ("selected-fg", fg),
        ("selected-blur", if dark { step(0.28) } else { step(0.12) }),
        ("menu", if dark { step(0.13) } else { step(0.02) }),
        ("menu-hover", if dark { step(0.26) } else { bg.mix(accent, 0.12) }),
        ("focus", accent),
        ("progress", progress),
        ("progress-track", if dark { fg.mix(bg, 0.1) } else { step(0.1) }),
        ("segment", accent),
        ("link", p.link),
        ("error", p.error),
        ("warning", p.warning),
        ("shadow", p.shadow),
        ("tint-green", if dark { Color::rgb(0x4a, 0xc2, 0x6b) } else { Color::rgb(0x2e, 0x9e, 0x4f) }),
        ("tint-blue", accent),
        ("tint-purple", if dark { Color::rgb(0xb3, 0x9d, 0xfa) } else { Color::rgb(0x7c, 0x3a, 0xed) }),
        ("tint-red", p.error),
        ("tint-folder", if dark { Color::rgb(0xe8, 0xc0, 0x5a) } else { Color::rgb(0xd9, 0xa4, 0x2b) }),
    ];
    let mut out: Vec<CssVar> =
        colors.iter().map(|(name, c)| CssVar { name: format!("--{name}"), value: c.to_string() }).collect();
    let px = |name: &str, v: u8| CssVar { name: format!("--{name}"), value: format!("{v}px") };
    out.push(px("text", s.text));
    out.push(px("control-height", s.control_height));
    out.push(px("radius", s.corner_radius));
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
        assert_eq!(value(&t.light, "--face"), "#f0f0f0");
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
