//! Screen readers and the keyboard. egui hands screen readers (Narrator,
//! NVDA, VoiceOver, Orca) a tree of the window's widgets through AccessKit,
//! naming each widget by its text. This module names what has no text of its
//! own (icons, symbols, painted rows), names each window after its title, and
//! draws a ring around the widget the keyboard moved to.

use eframe::egui::{self, accesskit, Id, Key, Response};

/// Give a widget the name a screen reader reads, in place of the symbol it
/// shows (an `×` or `+` button, say). The widget keeps the role egui gave it.
pub fn label(response: &Response, text: &str) {
    response.ctx.accesskit_node_builder(response.id, |node| node.set_label(text));
}

/// Describe a widget egui knows nothing about, one painted by hand: what it
/// is (`role`), its name, and anything else `more` sets, such as whether it
/// is selected.
pub fn describe(response: &Response, role: accesskit::Role, text: &str, more: impl FnOnce(&mut accesskit::Node)) {
    let rect = response.rect;
    response.ctx.accesskit_node_builder(response.id, |node| {
        node.set_role(role);
        node.set_label(text);
        // Where it is, which egui gives only widgets that take the focus.
        node.set_bounds(accesskit::Rect {
            x0: rect.min.x.into(),
            y0: rect.min.y.into(),
            x1: rect.max.x.into(),
            y1: rect.max.y.into(),
        });
        if !response.enabled() {
            node.set_disabled();
        }
        more(node);
    });
}

/// Run `add` with what it draws grouped under one named node, such as the
/// download list's rows under a list.
pub fn group<R>(ctx: &egui::Context, id: Id, role: accesskit::Role, text: &str, add: impl FnOnce() -> R) -> R {
    // The node has to exist before widgets are put under it.
    let made = ctx.accesskit_node_builder(id, |node| {
        node.set_role(role);
        node.set_label(text);
    });
    match made {
        Some(()) => ctx.with_accessibility_parent(id, add),
        // No screen reader is listening.
        None => add(),
    }
}

/// Name the window being drawn after its title, so a screen reader says
/// which window the focus is in.
pub fn name_window(ctx: &egui::Context, title: &str) {
    ctx.accesskit_node_builder(egui::accesskit_root_id(), |node| node.set_label(title));
}

/// The keys that move the keyboard focus from one widget to another.
const MOVES: [Key; 9] = [
    Key::Tab,
    Key::ArrowUp,
    Key::ArrowDown,
    Key::ArrowLeft,
    Key::ArrowRight,
    Key::Home,
    Key::End,
    Key::PageUp,
    Key::PageDown,
];

/// A ring around the widget with the keyboard focus, from the time a key
/// moves the focus until the mouse is pressed. egui shows a focused button
/// only as slightly lighter, which is easy to lose. Call it last in each
/// window's frame, so the ring is drawn over the widget.
pub fn focus_ring(ctx: &egui::Context) {
    let key = Id::new("a11y_keyboard_focus");
    let (moved, pressed) = ctx.input(|i| {
        let moved =
            i.events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: true, .. } if MOVES.contains(key)));
        (moved, i.pointer.any_pressed())
    });
    let mut keyboard = ctx.data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
    if moved {
        keyboard = true;
        // The focus lands at the end of this frame; draw the ring there.
        ctx.request_repaint();
    } else if pressed {
        keyboard = false;
    }
    ctx.data_mut(|d| d.insert_temp(key, keyboard));
    if !keyboard {
        return;
    }
    let Some(focused) = ctx.memory(|m| m.focused()).and_then(|id| ctx.read_response(id)) else {
        return;
    };
    let color = ctx.style().visuals.strong_text_color();
    ctx.layer_painter(focused.layer_id).rect_stroke(
        focused.interact_rect.expand(1.0),
        2.0,
        egui::Stroke::new(2.0_f32, color),
        egui::StrokeKind::Outside,
    );
}
