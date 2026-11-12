//! The window's looks: the colors and sizes a theme gives it, the built-in
//! themes, the themes a user writes as files, and which look the Theme
//! option and the system's contrast setting pick.
//!
//! A theme file is read by an adapter, which hands each `key = value` line
//! to a [`ThemeDraft`]; what the keys mean and which values they take is
//! decided here.

use crate::settings::Theme;

/// A color with its opacity, as `#rrggbb` or `#rrggbbaa` in a theme file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// 255 is opaque.
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    /// `#rgb`, `#rrggbb` or `#rrggbbaa`, the `#` optional.
    pub fn parse(text: &str) -> Result<Color, InvalidTheme> {
        let bad = || InvalidTheme::BadColor(text.trim().to_string());
        let hex = text.trim();
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        match hex.len() {
            3 => {
                let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).map(|d| d * 17).map_err(|_| bad());
                Ok(Color::rgb(digit(0)?, digit(1)?, digit(2)?))
            }
            6 => Ok(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Ok(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => Err(bad()),
        }
    }

    /// Its relative luminance, from 0 (black) to 1 (white), as WCAG
    /// defines it.
    pub fn luminance(self) -> f32 {
        let linear = |c: u8| {
            let c = f32::from(c) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    /// WCAG's contrast ratio of two colors, from 1 to 21.
    pub fn contrast(self, other: Color) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// `share` (0 to 1) of the way from this color to `other`.
    pub fn mix(self, other: Color, share: f32) -> Color {
        let share = share.clamp(0.0, 1.0);
        let one = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * share).round() as u8;
        Color::rgba(one(self.r, other.r), one(self.g, other.g), one(self.b, other.b), one(self.a, other.a))
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}

/// The name a theme file gives its theme, as Options lists it: up to
/// [`THEME_NAME_MAX`] characters, without line breaks or other control
/// characters, never empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThemeName(String);

/// The longest theme name, in characters.
pub const THEME_NAME_MAX: usize = 40;

impl ThemeName {
    pub fn parse(text: &str) -> Result<ThemeName, InvalidTheme> {
        let name = text.trim();
        if name.is_empty() || name.chars().count() > THEME_NAME_MAX || name.chars().any(char::is_control) {
            return Err(InvalidTheme::BadName(name.chars().take(THEME_NAME_MAX).collect()));
        }
        Ok(ThemeName(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether two names are the same theme's: case does not count.
    pub fn same(&self, other: &ThemeName) -> bool {
        self.0.to_lowercase() == other.0.to_lowercase()
    }
}

impl std::fmt::Display for ThemeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The colors of a look. Each is one key of a theme file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// A dark look: the window's webviews draw their own controls dark, and
    /// the tray menu and icons follow.
    pub dark: bool,
    /// Windows, panels and the download list.
    pub background: Color,
    /// Text and icons.
    pub foreground: Color,
    /// Hovered controls, secondary surfaces and a progress bar's track.
    pub accent: Color,
    /// Pressed controls.
    pub pressed: Color,
    /// Separators and the edges of windows and menus.
    pub border: Color,
    /// The edges of buttons, inputs and check boxes.
    pub input: Color,
    /// The edge of the control under the mouse.
    pub hover_border: Color,
    /// Selected items and selected text.
    pub selected: Color,
    /// Text on selected items and progress bars, and the edge of the text
    /// field being typed in.
    pub selected_text: Color,
    /// The filled part of a progress bar.
    pub progress: Color,
    /// Every other row of the download list.
    pub stripe: Color,
    pub link: Color,
    /// Errors and failed downloads.
    pub error: Color,
    pub warning: Color,
    /// The shadow under windows and menus; transparent for none.
    pub shadow: Color,
}

/// The sizes of a look, in points before the Interface size scales them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sizes {
    /// The size of normal text; small text, headings and code follow.
    pub text: u8,
    /// The height of buttons, inputs and other controls.
    pub control_height: u8,
    /// How round the corners of controls are (windows a little more).
    pub corner_radius: u8,
    /// The width of the edges of controls, windows and separators.
    pub border_width: u8,
}

/// The sizes a theme file can give, from the smallest to the largest.
pub const TEXT_RANGE: (u8, u8) = (9, 24);
pub const CONTROL_HEIGHT_RANGE: (u8, u8) = (16, 40);
pub const CORNER_RADIUS_RANGE: (u8, u8) = (0, 12);
pub const BORDER_WIDTH_RANGE: (u8, u8) = (1, 4);

impl Sizes {
    /// Sized like the 9 pt Segoe UI of the window UDM's layout follows,
    /// with its tight padding: 12 pt text, 20 pt controls, nearly square
    /// corners and hairline edges.
    pub const NORMAL: Sizes = Sizes { text: 12, control_height: 20, corner_radius: 2, border_width: 1 };
    /// The normal sizes with edges twice as wide, which high contrast
    /// needs to show where each control ends.
    pub const HIGH_CONTRAST: Sizes = Sizes { border_width: 2, ..Sizes::NORMAL };
}

/// What the window looks like: its colors and sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub palette: Palette,
    pub sizes: Sizes,
}

/// Tailwind's zinc scale, which the light and dark looks (after
/// shadcn/ui's neutral tokens) come from.
mod zinc {
    use super::Color;
    pub const Z50: Color = Color::rgb(0xfa, 0xfa, 0xfa);
    pub const Z100: Color = Color::rgb(0xf4, 0xf4, 0xf5);
    pub const Z200: Color = Color::rgb(0xe4, 0xe4, 0xe7);
    pub const Z300: Color = Color::rgb(0xd4, 0xd4, 0xd8);
    pub const Z600: Color = Color::rgb(0x52, 0x52, 0x5b);
    pub const Z700: Color = Color::rgb(0x3f, 0x3f, 0x46);
    pub const Z800: Color = Color::rgb(0x27, 0x27, 0x2a);
    pub const Z900: Color = Color::rgb(0x18, 0x18, 0x1b);
    pub const Z950: Color = Color::rgb(0x09, 0x09, 0x0b);
}

const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);
const BLACK: Color = Color::rgb(0, 0, 0);

/// The Light theme: shadcn's zinc on white.
pub const LIGHT: Look = Look {
    palette: Palette {
        dark: false,
        background: WHITE,
        foreground: zinc::Z950,
        accent: zinc::Z100,
        pressed: zinc::Z200,
        border: zinc::Z200,
        // A step darker than the border, because nothing casts a shadow
        // under a control to set it off.
        input: zinc::Z300,
        hover_border: zinc::Z300,
        selected: zinc::Z200,
        selected_text: zinc::Z950,
        // Light enough for the text on top.
        progress: zinc::Z300,
        stripe: zinc::Z50,
        link: Color::rgb(0x25, 0x63, 0xeb),
        error: Color::rgb(0xdc, 0x26, 0x26),
        warning: Color::rgb(0xb4, 0x53, 0x09),
        shadow: Color::rgba(0, 0, 0, 20),
    },
    sizes: Sizes::NORMAL,
};

/// The Dark theme: shadcn's zinc on near black.
pub const DARK: Look = Look {
    palette: Palette {
        dark: true,
        background: zinc::Z950,
        foreground: zinc::Z50,
        accent: zinc::Z800,
        pressed: zinc::Z700,
        border: zinc::Z800,
        input: zinc::Z800,
        hover_border: zinc::Z800,
        selected: zinc::Z700,
        selected_text: zinc::Z50,
        progress: zinc::Z600,
        stripe: zinc::Z900,
        link: Color::rgb(0x60, 0xa5, 0xfa),
        error: Color::rgb(0xf8, 0x71, 0x71),
        warning: Color::rgb(0xfb, 0xbf, 0x24),
        shadow: Color::rgba(0, 0, 0, 96),
    },
    sizes: Sizes::NORMAL,
};

/// The High contrast theme, for when the system has no contrast colors of
/// its own: white on black, every edge white and twice as wide, the
/// control under the mouse edged in yellow, selections in a blue that white
/// text reads on, and no shadows. Every color meets WCAG's 7:1 against
/// what it is drawn on.
pub const HIGH_CONTRAST: Look = Look {
    palette: Palette {
        dark: true,
        background: BLACK,
        foreground: WHITE,
        accent: Color::rgb(0x26, 0x26, 0x26),
        pressed: Color::rgb(0x40, 0x40, 0x40),
        border: WHITE,
        input: WHITE,
        hover_border: Color::rgb(0xff, 0xff, 0x00),
        selected: Color::rgb(0x00, 0x37, 0xda),
        selected_text: WHITE,
        progress: Color::rgb(0x00, 0x37, 0xda),
        stripe: Color::rgb(0x12, 0x12, 0x12),
        link: Color::rgb(0x00, 0xff, 0xff),
        error: Color::rgb(0xff, 0x8a, 0x8a),
        warning: Color::rgb(0xff, 0xc8, 0x57),
        shadow: Color::rgba(0, 0, 0, 0),
    },
    sizes: Sizes::HIGH_CONTRAST,
};

/// The built-in themes a theme file can start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltIn {
    Light,
    Dark,
    HighContrast,
}

impl BuiltIn {
    pub const ALL: [BuiltIn; 3] = [BuiltIn::Light, BuiltIn::Dark, BuiltIn::HighContrast];

    /// Its name after a theme file's `base =`.
    pub fn key(self) -> &'static str {
        match self {
            BuiltIn::Light => "light",
            BuiltIn::Dark => "dark",
            BuiltIn::HighContrast => "high_contrast",
        }
    }

    pub fn look(self) -> Look {
        match self {
            BuiltIn::Light => LIGHT,
            BuiltIn::Dark => DARK,
            BuiltIn::HighContrast => HIGH_CONTRAST,
        }
    }
}

/// The colors the system's high-contrast setting gives every program, as
/// Windows' contrast themes (Aquatic, Desert, Dusk, Night sky) set them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemContrast {
    /// The background of windows.
    pub window: Color,
    /// Text on it.
    pub text: Color,
    /// Selected items.
    pub highlight: Color,
    /// Text on selected items.
    pub highlight_text: Color,
    /// Links.
    pub link: Color,
    /// Disabled text.
    pub gray_text: Color,
    /// Buttons, and the text on them.
    pub button: Color,
    pub button_text: Color,
}

impl SystemContrast {
    /// The window in the system's contrast colors: its window and text
    /// colors throughout, its highlight for selections, progress and the
    /// control under the mouse, its link color, and edges in its text
    /// colors twice as wide. It has no error or warning color, so those are
    /// the text's.
    pub fn look(&self) -> Look {
        let dark = self.window.luminance() < 0.5;
        Look {
            palette: Palette {
                dark,
                background: self.window,
                foreground: self.text,
                accent: self.window.mix(self.text, 0.15),
                pressed: self.window.mix(self.text, 0.3),
                border: self.text,
                input: self.button_text,
                hover_border: self.highlight,
                selected: self.highlight,
                selected_text: self.highlight_text,
                progress: self.highlight,
                stripe: self.window,
                link: self.link,
                error: self.text,
                warning: self.text,
                shadow: Color::rgba(0, 0, 0, 0),
            },
            sizes: Sizes::HIGH_CONTRAST,
        }
    }
}

/// A theme from a file in the themes folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomTheme {
    pub name: ThemeName,
    pub look: Look,
}

/// What a theme file's line or name gets wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidTheme {
    #[error("{0} is not a theme setting.")]
    UnknownKey(String),
    #[error("{0} is not a color; write it as #rrggbb.")]
    BadColor(String),
    #[error("{0} is not yes or no.")]
    BadSwitch(String),
    #[error("{0} is not a built-in theme; use light, dark or high_contrast.")]
    BadBase(String),
    #[error("{key} must be a whole number from {lo} to {hi}.")]
    BadSize { key: String, lo: u8, hi: u8 },
    #[error("A theme's name must be 1 to 40 characters on one line.")]
    BadName(String),
}

/// Where in a palette one of its colors is.
type ColorField = fn(&mut Palette) -> &mut Color;

/// Where in the sizes one of them is.
type SizeField = fn(&mut Sizes) -> &mut u8;

/// The color keys of a theme file, each with its place in a palette.
const COLOR_KEYS: &[(&str, ColorField)] = &[
    ("background", |p| &mut p.background),
    ("foreground", |p| &mut p.foreground),
    ("accent", |p| &mut p.accent),
    ("pressed", |p| &mut p.pressed),
    ("border", |p| &mut p.border),
    ("input", |p| &mut p.input),
    ("hover_border", |p| &mut p.hover_border),
    ("selected", |p| &mut p.selected),
    ("selected_text", |p| &mut p.selected_text),
    ("progress", |p| &mut p.progress),
    ("stripe", |p| &mut p.stripe),
    ("link", |p| &mut p.link),
    ("error", |p| &mut p.error),
    ("warning", |p| &mut p.warning),
    ("shadow", |p| &mut p.shadow),
];

/// The size keys of a theme file, with their ranges.
const SIZE_KEYS: &[(&str, (u8, u8), SizeField)] = &[
    ("text_size", TEXT_RANGE, |s| &mut s.text),
    ("control_height", CONTROL_HEIGHT_RANGE, |s| &mut s.control_height),
    ("corner_radius", CORNER_RADIUS_RANGE, |s| &mut s.corner_radius),
    ("border_width", BORDER_WIDTH_RANGE, |s| &mut s.border_width),
];

/// Every key a theme file can set, in the order the docs list them.
pub fn keys() -> impl Iterator<Item = &'static str> {
    ["name", "base", "dark"]
        .into_iter()
        .chain(COLOR_KEYS.iter().map(|(k, _)| *k))
        .chain(SIZE_KEYS.iter().map(|(k, _, _)| *k))
}

/// A theme being read from its file, one setting at a time. What the file
/// leaves out comes from its base, the Light theme unless it names another,
/// wherever in the file `base` is.
#[derive(Debug, Clone)]
pub struct ThemeDraft {
    name: Option<ThemeName>,
    base: BuiltIn,
    dark: Option<bool>,
    colors: Vec<(usize, Color)>,
    sizes: Vec<(usize, u8)>,
}

impl ThemeDraft {
    /// A theme named `name` (its file's name) unless it says otherwise.
    pub fn new(name: Option<ThemeName>) -> ThemeDraft {
        ThemeDraft { name, base: BuiltIn::Light, dark: None, colors: Vec::new(), sizes: Vec::new() }
    }

    /// Take one setting. Keys ignore case, and `-` or a space reads as `_`.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), InvalidTheme> {
        let key = key.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        let value = value.trim();
        match key.as_str() {
            "name" => self.name = Some(ThemeName::parse(value)?),
            "base" => {
                let wanted = value.to_ascii_lowercase().replace(['-', ' '], "_");
                self.base = BuiltIn::ALL
                    .into_iter()
                    .find(|b| b.key() == wanted)
                    .ok_or_else(|| InvalidTheme::BadBase(value.to_string()))?;
            }
            "dark" => {
                self.dark = Some(match value.to_ascii_lowercase().as_str() {
                    "true" | "yes" | "1" => true,
                    "false" | "no" | "0" => false,
                    _ => return Err(InvalidTheme::BadSwitch(value.to_string())),
                })
            }
            _ => {
                if let Some(i) = COLOR_KEYS.iter().position(|(k, _)| *k == key) {
                    self.colors.push((i, Color::parse(value)?));
                } else if let Some(i) = SIZE_KEYS.iter().position(|(k, _, _)| *k == key) {
                    let (name, (lo, hi), _) = SIZE_KEYS[i];
                    let bad = || InvalidTheme::BadSize { key: name.to_string(), lo, hi };
                    let size: u8 = value.parse().map_err(|_| bad())?;
                    if !(lo..=hi).contains(&size) {
                        return Err(bad());
                    }
                    self.sizes.push((i, size));
                } else {
                    return Err(InvalidTheme::UnknownKey(key));
                }
            }
        }
        Ok(())
    }

    /// The theme, or `None` when neither its file's name nor a `name` line
    /// gives it a name.
    pub fn finish(self) -> Option<CustomTheme> {
        let mut look = self.base.look();
        look.palette.dark = self.dark.unwrap_or(look.palette.dark);
        for (i, color) in self.colors {
            *(COLOR_KEYS[i].1)(&mut look.palette) = color;
        }
        for (i, size) in self.sizes {
            *(SIZE_KEYS[i].2)(&mut look.sizes) = size;
        }
        Some(CustomTheme { name: self.name?, look })
    }
}

/// How the window looks, once the Theme option, the system's contrast
/// setting and the theme files are taken together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    /// Light or dark, as the system's app mode is.
    FollowSystem { light: Look, dark: Look },
    /// The one look, whatever the system's mode.
    Fixed(Look),
}

/// The look `theme` gives, with `contrast` the system's contrast colors
/// while its high-contrast setting is on and `themes` the theme files read.
///
/// Following the system takes its contrast colors while they are on. High
/// contrast is the system's contrast colors when it has them, and the
/// built-in ones otherwise. A theme whose file is gone (or not read yet)
/// follows the system.
pub fn appearance(theme: &Theme, contrast: Option<&SystemContrast>, themes: &[CustomTheme]) -> Appearance {
    let follow = || match contrast {
        Some(c) => Appearance::Fixed(c.look()),
        None => Appearance::FollowSystem { light: LIGHT, dark: DARK },
    };
    match theme {
        Theme::System => follow(),
        Theme::Light => Appearance::Fixed(LIGHT),
        Theme::Dark => Appearance::Fixed(DARK),
        Theme::HighContrast => Appearance::Fixed(contrast.map_or(HIGH_CONTRAST, SystemContrast::look)),
        Theme::Custom(name) => {
            themes.iter().find(|t| t.name.same(name)).map_or_else(follow, |t| Appearance::Fixed(t.look))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> ThemeName {
        ThemeName::parse(text).unwrap()
    }

    #[test]
    fn colors_parse_in_three_lengths() {
        assert_eq!(Color::parse("#0a1B2c"), Ok(Color::rgb(0x0a, 0x1b, 0x2c)));
        assert_eq!(Color::parse(" fff "), Ok(Color::rgb(255, 255, 255)));
        assert_eq!(Color::parse("#00000080"), Ok(Color::rgba(0, 0, 0, 0x80)));
        for bad in ["", "#", "#12345", "#gggggg", "red", "#1234567", "#ééé"] {
            assert!(Color::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(Color::rgb(1, 2, 3).to_string(), "#010203");
        assert_eq!(Color::rgba(1, 2, 3, 4).to_string(), "#01020304");
    }

    #[test]
    fn contrast_follows_wcag() {
        assert!((WHITE.contrast(BLACK) - 21.0).abs() < 0.01);
        assert!((WHITE.contrast(WHITE) - 1.0).abs() < 0.01);
        assert_eq!(BLACK.mix(WHITE, 0.5), Color::rgb(128, 128, 128));
    }

    #[test]
    fn the_high_contrast_theme_reads_at_seven_to_one() {
        let p = HIGH_CONTRAST.palette;
        for (what, fg, bg) in [
            ("text", p.foreground, p.background),
            ("hovered text", p.foreground, p.accent),
            ("pressed text", p.foreground, p.pressed),
            ("selected text", p.selected_text, p.selected),
            ("progress text", p.selected_text, p.progress),
            ("edges", p.border, p.background),
            ("hover edge", p.hover_border, p.background),
            ("links", p.link, p.background),
            ("errors", p.error, p.background),
            ("warnings", p.warning, p.background),
        ] {
            assert!(fg.contrast(bg) >= 7.0, "{what}: {}", fg.contrast(bg));
        }
        const { assert!(HIGH_CONTRAST.sizes.border_width > LIGHT.sizes.border_width) };
    }

    #[test]
    fn names_are_one_short_line() {
        assert_eq!(name("  Ocean ").as_str(), "Ocean");
        assert!(ThemeName::parse(" ").is_err());
        assert!(ThemeName::parse("a\nb").is_err());
        assert!(ThemeName::parse(&"x".repeat(THEME_NAME_MAX + 1)).is_err());
        assert!(name("Ocean").same(&name("OCEAN")));
    }

    #[test]
    fn a_draft_takes_its_base_then_its_own_settings() {
        let mut d = ThemeDraft::new(Some(name("file")));
        d.set("background", "#102030").unwrap();
        // The base can come after the colors; the colors still win.
        d.set("Base", "high-contrast").unwrap();
        d.set("text size", "16").unwrap();
        d.set("name", "Ocean").unwrap();
        let t = d.finish().unwrap();
        assert_eq!(t.name, name("Ocean"));
        assert_eq!(t.look.palette.background, Color::rgb(0x10, 0x20, 0x30));
        assert_eq!(t.look.palette.foreground, HIGH_CONTRAST.palette.foreground);
        assert_eq!(t.look.sizes, Sizes { text: 16, ..Sizes::HIGH_CONTRAST });
        assert!(t.look.palette.dark);
    }

    #[test]
    fn a_draft_refuses_what_it_does_not_know() {
        let mut d = ThemeDraft::new(None);
        assert_eq!(d.set("colour", "#fff"), Err(InvalidTheme::UnknownKey("colour".into())));
        assert_eq!(d.set("link", "blue"), Err(InvalidTheme::BadColor("blue".into())));
        assert_eq!(d.set("base", "sepia"), Err(InvalidTheme::BadBase("sepia".into())));
        assert_eq!(d.set("border_width", "9"), Err(InvalidTheme::BadSize { key: "border_width".into(), lo: 1, hi: 4 }));
        assert!(d.set("text_size", "big").is_err());
        assert!(d.set("dark", "maybe").is_err());
        d.set("dark", "yes").unwrap();
        assert_eq!(d.clone().finish(), None, "no name from the file or a line");
        for key in keys().skip(3) {
            assert!(["#fff", "2", "16"].iter().any(|v| ThemeDraft::new(None).set(key, v).is_ok()), "{key}");
        }
    }

    #[test]
    fn the_system_contrast_colors_make_a_look() {
        let c = SystemContrast {
            window: BLACK,
            text: WHITE,
            highlight: Color::rgb(0x1a, 0xeb, 0xff),
            highlight_text: BLACK,
            link: Color::rgb(0xff, 0xff, 0x00),
            gray_text: Color::rgb(0xa6, 0xa6, 0xa6),
            button: BLACK,
            button_text: WHITE,
        };
        let look = c.look();
        assert!(look.palette.dark);
        assert_eq!(look.palette.selected, c.highlight);
        assert_eq!(look.palette.selected_text, c.highlight_text);
        assert_eq!(look.sizes.border_width, 2);
        let light = SystemContrast { window: WHITE, text: BLACK, ..c }.look();
        assert!(!light.palette.dark);
    }

    #[test]
    fn the_theme_option_picks_a_look() {
        let ocean = CustomTheme { name: name("Ocean"), look: DARK };
        let c = SystemContrast {
            window: WHITE,
            text: BLACK,
            highlight: BLACK,
            highlight_text: WHITE,
            link: BLACK,
            gray_text: BLACK,
            button: WHITE,
            button_text: BLACK,
        };
        let follow = Appearance::FollowSystem { light: LIGHT, dark: DARK };
        assert_eq!(appearance(&Theme::System, None, &[]), follow);
        assert_eq!(appearance(&Theme::System, Some(&c), &[]), Appearance::Fixed(c.look()));
        assert_eq!(appearance(&Theme::Light, Some(&c), &[]), Appearance::Fixed(LIGHT));
        assert_eq!(appearance(&Theme::Dark, None, &[]), Appearance::Fixed(DARK));
        assert_eq!(appearance(&Theme::HighContrast, None, &[]), Appearance::Fixed(HIGH_CONTRAST));
        assert_eq!(appearance(&Theme::HighContrast, Some(&c), &[]), Appearance::Fixed(c.look()));
        let custom = Theme::Custom(name("ocean"));
        assert_eq!(appearance(&custom, None, std::slice::from_ref(&ocean)), Appearance::Fixed(DARK));
        assert_eq!(appearance(&custom, None, &[]), follow, "a theme whose file is gone follows the system");
    }
}
