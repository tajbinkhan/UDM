//! The window's look, after shadcn/ui: 1px borders, rounded corners, flat
//! fills and Segoe UI (what shadcn's system font stack picks on Windows).
//! The colors and sizes are a `pullbay_domain::theme::Look`: the built-in
//! Light, Dark and High contrast themes, the system's contrast colors, or a
//! theme file's. Following the system, egui picks Light or Dark as the
//! system's app mode is.

use std::sync::Arc;

use eframe::egui::{
    self,
    epaint::text::{FontInsert, FontPriority, InsertFontFamily},
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Margin, Shadow, Stroke, TextStyle, Theme,
    Vec2, Visuals,
};
use pullbay_application::window::{FontFile, ScriptFont};
use pullbay_domain::theme::{Appearance, Color, Look, Palette, Sizes, DARK, LIGHT};

/// Sets the fonts (the system's `system`, where it has them) and the
/// built-in light and dark looks, until the settings give one.
pub fn apply(ctx: &egui::Context, system: pullbay_application::window::SystemFonts) {
    ctx.set_fonts(fonts(system));
    set_looks(ctx, &LIGHT, &DARK);
}

/// Follows the Theme and Interface size options. Changing the zoom factor
/// scales every size, the dialogs' and the drop target's windows included.
pub fn set_appearance(ctx: &egui::Context, appearance: &Appearance, size_percent: u32) {
    match appearance {
        Appearance::FollowSystem { light, dark } => {
            set_looks(ctx, light, dark);
            ctx.set_theme(egui::ThemePreference::System);
        }
        Appearance::Fixed(look) => {
            // Both of egui's styles, so whichever it consults is this one.
            set_looks(ctx, look, look);
            ctx.set_theme(if look.palette.dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
        }
    }
    ctx.set_zoom_factor(size_percent as f32 / 100.0);
}

/// The id under which each egui theme's progress bar colors are kept.
fn bar_colors_id(dark: bool) -> Id {
    Id::new(("pullbay_progress_colors", dark))
}

fn set_looks(ctx: &egui::Context, light: &Look, dark: &Look) {
    for (theme, look) in [(Theme::Light, light), (Theme::Dark, dark)] {
        ctx.style_mut_of(theme, |style| set_style(style, look));
        let p = &look.palette;
        ctx.data_mut(|d| d.insert_temp(bar_colors_id(theme == Theme::Dark), (color(p.accent), color(p.progress))));
    }
}

fn set_style(style: &mut egui::Style, look: &Look) {
    style.visuals = visuals(&look.palette, &look.sizes);
    // At the normal sizes, the 9 pt Segoe UI of the window Pullbay's layout
    // follows, with its tight padding: 20 px controls, 4 px gaps. Other
    // text sizes scale the text styles and the icons along.
    let text = f32::from(look.sizes.text);
    let k = text / f32::from(Sizes::NORMAL.text);
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(10.5 * k)),
        (TextStyle::Body, FontId::proportional(text)),
        (TextStyle::Button, FontId::proportional(text)),
        (TextStyle::Heading, FontId::proportional(15.0 * k)),
        (TextStyle::Monospace, FontId::monospace(11.5 * k)),
    ]
    .into();
    let spacing = &mut style.spacing;
    spacing.item_spacing = Vec2::new(6.0, 4.0);
    spacing.button_padding = Vec2::new(8.0, 2.0);
    spacing.interact_size = Vec2::new(36.0, f32::from(look.sizes.control_height));
    spacing.icon_width = 13.0 * k;
    spacing.icon_width_inner = 7.0 * k;
    spacing.indent = 16.0;
    spacing.window_margin = Margin::same(10);
    spacing.menu_margin = Margin::same(3);
    spacing.combo_height = 160.0 * k;
}

/// Draws a progress bar on the look's accent track instead of the input
/// background, which matches the panel and would hide the empty part.
pub fn progress_bar(ui: &mut egui::Ui, bar: egui::ProgressBar) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let fallback = if dark { &DARK.palette } else { &LIGHT.palette };
    let (track, fill) = ui
        .ctx()
        .data(|d| d.get_temp::<(Color32, Color32)>(bar_colors_id(dark)))
        .unwrap_or((color(fallback.accent), color(fallback.progress)));
    ui.scope(|ui| {
        ui.visuals_mut().extreme_bg_color = track;
        ui.add(bar.fill(fill))
    })
    .inner
}

fn color(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// The system's fonts for text and code (Segoe UI and Consolas on Windows).
/// egui's own fonts stay behind them for any glyph they lack, and take over
/// where the system has none.
fn fonts(system: pullbay_application::window::SystemFonts) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes, family) in
        [("system-text", system.text, FontFamily::Proportional), ("system-code", system.code, FontFamily::Monospace)]
    {
        let Some(bytes) = bytes else {
            continue;
        };
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(family).or_default().insert(0, name.to_owned());
    }
    fonts
}

/// Adds a system font for `script` behind the window's fonts, so the
/// characters they lack come from it. `false`, adding nothing, when egui
/// could not use the file: it would stop the window on one it cannot read.
pub fn add_script_font(ctx: &egui::Context, script: ScriptFont, file: FontFile) -> bool {
    if !usable(&file) {
        return false;
    }
    let data = FontData { index: file.index, ..FontData::from_owned(file.bytes) };
    let families = [FontFamily::Proportional, FontFamily::Monospace]
        .into_iter()
        .map(|family| InsertFontFamily { family, priority: FontPriority::Lowest })
        .collect();
    ctx.add_font(FontInsert::new(&format!("system-{script:?}"), data, families));
    ctx.request_repaint();
    true
}

/// Whether egui can draw with the font: it parses, and its units per em
/// are in the range egui accepts.
fn usable(file: &FontFile) -> bool {
    use ab_glyph::Font;
    ab_glyph::FontRef::try_from_slice_and_index(&file.bytes, file.index)
        .is_ok_and(|font| font.units_per_em().is_some_and(|units| (16.0..=16384.0).contains(&units)))
}

fn visuals(p: &Palette, sizes: &Sizes) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    let radius = CornerRadius::same(sizes.corner_radius);
    let width = f32::from(sizes.border_width);
    let line = |c: Color| Stroke { width, color: color(c) };
    let border = line(p.border);
    // Text keeps egui's 1 px strokes; the width is for edges.
    let text = Stroke { width: 1.0, color: color(p.foreground) };

    let w = &mut v.widgets;
    // Labels, separators and frames. weak_bg_fill is also what weak text fades
    // toward, so it has to be the background.
    w.noninteractive.bg_fill = color(p.background);
    w.noninteractive.weak_bg_fill = color(p.background);
    w.noninteractive.bg_stroke = border;
    w.noninteractive.fg_stroke = text;
    w.noninteractive.corner_radius = radius;
    // Outline buttons fill with weak_bg_fill: the background, then the accent
    // on hover. Checkboxes fill with bg_fill, one step stronger, so an
    // unticked box still shows on the background.
    for (state, weak_fill, fill, edge) in [
        (&mut w.inactive, p.background, p.accent, p.input),
        (&mut w.hovered, p.accent, p.pressed, p.hover_border),
        (&mut w.active, p.pressed, p.pressed, p.hover_border),
        (&mut w.open, p.accent, p.accent, p.input),
    ] {
        state.weak_bg_fill = color(weak_fill);
        state.bg_fill = color(fill);
        state.bg_stroke = line(edge);
        state.fg_stroke = text;
        state.corner_radius = radius;
        state.expansion = 0.0;
    }

    v.selection.bg_fill = color(p.selected);
    // Also the text on selected items and progress bars, and the ring around
    // the focused text field.
    v.selection.stroke = Stroke { width, color: color(p.selected_text) };
    v.text_cursor.stroke = Stroke { width: 1.5, color: color(p.foreground) };

    v.hyperlink_color = color(p.link);
    v.error_fg_color = color(p.error);
    v.warn_fg_color = color(p.warning);
    v.faint_bg_color = color(p.stripe);
    v.extreme_bg_color = color(p.background);
    v.code_bg_color = color(p.accent);

    v.panel_fill = color(p.background);
    v.window_fill = color(p.background);
    v.window_stroke = border;
    // Windows and dialogs a little rounder than controls.
    v.window_corner_radius = CornerRadius::same(sizes.corner_radius.saturating_add(2));
    // egui fills the title bar of the front window; shadcn dialogs are one
    // flat surface.
    v.window_highlight_topmost = false;
    v.menu_corner_radius = radius;
    let shadow = color(p.shadow);
    v.window_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: shadow };
    v.popup_shadow = Shadow { offset: [0, 4], blur: 12, spread: 0, color: shadow };
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_is_no_font_is_not_added() {
        let ctx = egui::Context::default();
        let junk = FontFile { bytes: b"not a font".to_vec(), index: 0 };
        assert!(!add_script_font(&ctx, ScriptFont::Japanese, junk));
    }

    #[test]
    fn a_font_egui_reads_is_added() {
        // egui's own text font stands in for a system one.
        let bytes = FontDefinitions::default().font_data["Ubuntu-Light"].font.to_vec();
        assert!(usable(&FontFile { bytes: bytes.clone(), index: 0 }));
        assert!(!usable(&FontFile { bytes, index: 3 }));
    }
}
