//! The download list: a table with resizable columns, grid lines and
//! whole-row selection. A click selects a row, Ctrl+click adds or removes
//! one, Shift+click selects a run; a double-click opens a finished file or
//! shows a download's window; a right-click opens the row's menu. Dragging a
//! finished download's row drags its file out to a folder or another
//! program. Column widths dragged here are saved with the layout.
//!
//! From the keyboard, Tab reaches the rows; Up, Down, Page Up, Page Down,
//! Home and End move through them (with Shift selecting a run, with Ctrl
//! leaving the selection alone), Space selects the focused row (Ctrl+Space
//! adds or removes it), and Shift+F10 opens its menu. A screen reader reads
//! each row's name, status, progress and speed (`row_label`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use eframe::egui::{self, accesskit, Align, Id, Key, Pos2, Rect, Sense, Stroke};
use egui_extras::TableBuilder;
use udm_domain::download::{Answer, DownloadEntry, Status};
use udm_domain::id::DownloadId;
use udm_domain::queue::Queue;

use crate::columns::{self, Column, Layout};
use crate::i18n::{tr, trf};
use crate::{display_name, human_bytes, human_duration, human_rate, scan_label, Action};

/// IDM's rows are 17 px; the row's text sets the floor here.
const ROW_HEIGHT: f32 = 18.0;
const HEADER_HEIGHT: f32 = 20.0;
/// How far the pointer moves with the button down before a file drag
/// starts, so a click that wobbles still clicks.
const DRAG_DISTANCE: f32 = 4.0;

/// Everything the list needs for one frame.
pub struct List<'a> {
    pub shown: &'a [&'a DownloadEntry],
    pub speeds: &'a HashMap<DownloadId, f64>,
    pub selected: &'a HashSet<DownloadId>,
    pub queues: &'a [Queue],
    /// In a queue's view: that queue, whose order the list shows.
    pub queue: Option<&'a Queue>,
    pub connected: bool,
    /// The match the find bar moved to, and whether to scroll to it.
    pub found: Option<DownloadId>,
    pub scroll: bool,
}

/// The table's own state between frames.
#[derive(Default)]
pub struct TableState {
    /// The widths last measured, to tell a change from the Columns dialog
    /// (which restarts the table at the new widths) from a drag here.
    measured: BTreeMap<Column, f32>,
    generation: u64,
    /// The row with the keyboard focus in the last frame, and where its
    /// menu opens.
    focused: Option<(DownloadId, Pos2)>,
    /// A row the keyboard moved to, which takes the focus once it is drawn.
    focus_next: Option<DownloadId>,
    /// How many rows the last frame showed, for Page Up and Page Down.
    page: usize,
    /// A row's menu, while it is open.
    menu: Option<RowMenu>,
}

/// A row's menu, opened by a right-click or Shift+F10. It is drawn here
/// rather than by egui's context menu, which would make every row's first
/// cell a stop for Tab.
#[derive(Clone, Copy)]
struct RowMenu {
    id: DownloadId,
    at: Pos2,
    /// Opened this frame: the click that opened it closes nothing.
    fresh: bool,
    /// Its first item takes the keyboard focus (Shift+F10).
    focus_first: bool,
}

/// A key's move through the list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Move {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// The row a move lands on, from row `from` (`None` when no row has the
/// focus) in a list of `len` rows that shows `page` at a time. `None` for
/// an empty list.
pub fn step(from: Option<usize>, len: usize, page: usize, m: Move) -> Option<usize> {
    let last = len.checked_sub(1)?;
    let page = page.saturating_sub(1).max(1);
    Some(match (m, from) {
        (Move::Home, _) | (Move::Down | Move::PageDown, None) => 0,
        (Move::End, _) | (Move::Up | Move::PageUp, None) => last,
        (Move::Up, Some(at)) => at.saturating_sub(1),
        (Move::Down, Some(at)) => (at + 1).min(last),
        (Move::PageUp, Some(at)) => at.saturating_sub(page),
        (Move::PageDown, Some(at)) => (at + page).min(last),
    })
}

/// What a screen reader reads for a row: its name, status, progress and
/// speed, then the time left and why it failed, where there is one.
pub fn row_label(e: &DownloadEntry, speed: Option<f64>) -> String {
    let running = speed.filter(|s| *s > 0.0 && e.status == Status::Downloading);
    let mut parts = vec![display_name(e), status_word(e.status).to_string()];
    let percent = columns::fraction(e).map(|f| crate::i18n::number(f64::from(f * 100.0), 1));
    match (percent, e.total) {
        (Some(percent), Some(total)) => {
            parts.push(trf!("{percent}% of {size}", percent = percent, size = human_bytes(total)))
        }
        (Some(percent), None) => parts.push(trf!("{percent}% done", percent = percent)),
        (None, _) if !e.done.is_zero() => parts.push(trf!("{size} downloaded", size = human_bytes(e.done))),
        (None, _) => {}
    }
    if let Some(bps) = running {
        parts.push(format!("{}/s", human_rate(bps as u64)));
        if let Some(left) = columns::time_left(e, running) {
            parts.push(trf!("{time} left", time = human_duration(left)));
        }
    }
    if let Some(err) = e.error.as_deref().filter(|_| e.status == Status::Failed) {
        parts.push(err.to_string());
    }
    parts.join(", ")
}

/// A status in a word.
fn status_word(status: Status) -> &'static str {
    match status {
        Status::Queued => tr("Queued"),
        Status::Downloading => tr("Downloading"),
        Status::Done => tr("Complete"),
        Status::Failed => tr("Failed"),
        Status::Canceled => tr("Canceled"),
        Status::Interrupted => tr("Interrupted"),
        Status::Confirming => tr("Confirm"),
        Status::Waiting => tr("Waiting"),
    }
}

/// The ID of a download's row, which takes the keyboard focus.
fn row_id(id: DownloadId) -> Id {
    Id::new(("download_row", id))
}

/// Whether the list's keys (Ctrl+A, Delete, Enter and those of `keys`) go
/// to the list: while one of its rows has the focus, or nothing has.
pub fn list_has_keys(ctx: &egui::Context, state: &TableState) -> bool {
    match ctx.memory(|m| m.focused()) {
        None => true,
        Some(focused) => state.focused.is_some_and(|(id, _)| row_id(id) == focused),
    }
}

/// The keys that move through the list, select rows and open a row's menu.
/// `order` is the rows as shown; `anchor` the row the last click selected,
/// where the keys start while no row has the focus. Call only while
/// `list_has_keys`.
pub fn keys(
    ctx: &egui::Context,
    state: &mut TableState,
    order: &[DownloadId],
    anchor: Option<DownloadId>,
    actions: &mut Vec<Action>,
) {
    let focused = state.focused.filter(|(id, _)| ctx.memory(|m| m.has_focus(row_id(*id))));
    let at = |id: DownloadId| order.iter().position(|x| *x == id);
    let from = focused.map(|(id, _)| id).or(anchor).and_then(at);
    let (moved, space, menu, mods) = ctx.input(|i| {
        let moves = [
            (Key::ArrowUp, Move::Up),
            (Key::ArrowDown, Move::Down),
            (Key::PageUp, Move::PageUp),
            (Key::PageDown, Move::PageDown),
            (Key::Home, Move::Home),
            (Key::End, Move::End),
        ];
        let moved = moves.into_iter().find(|(k, _)| i.key_pressed(*k)).map(|(_, m)| m);
        (moved, i.key_pressed(Key::Space), i.modifiers.shift && i.key_pressed(Key::F10), i.modifiers)
    });
    if let Some(to) = moved.and_then(|m| step(from, order.len(), state.page, m)) {
        let id = order[to];
        state.focus_next = Some(id);
        // Ctrl moves the focus alone, as in Explorer.
        if !mods.command {
            actions.push(Action::Click { id, ctrl: false, shift: mods.shift });
        }
    }
    if let Some((id, pos)) = focused {
        if space {
            actions.push(Action::Click { id, ctrl: mods.command, shift: mods.shift });
        }
        if menu {
            state.menu = Some(RowMenu { id, at: pos, fresh: true, focus_first: true });
        }
    }
}

/// What the list asked for besides actions.
#[derive(Default)]
pub struct Outcome {
    pub open_columns: bool,
}

pub fn show(
    ui: &mut egui::Ui,
    list: &List,
    layout: &mut Layout,
    state: &mut TableState,
    actions: &mut Vec<Action>,
) -> Outcome {
    let mut outcome = Outcome::default();
    let columns = layout.columns.clone();
    let wanted: BTreeMap<Column, f32> = columns.iter().map(|c| (*c, layout.shown_width(*c))).collect();
    if !state.measured.is_empty() && state.measured != wanted {
        state.generation += 1;
    }

    // Each download's place in its queue, for the Q column.
    let places: HashMap<DownloadId, usize> =
        list.queues.iter().flat_map(|q| q.order.iter().enumerate().map(|(i, id)| (*id, i + 1))).collect();

    // View > Font: the list's text, and rows tall enough for it.
    let font = layout.list_font.map(|f| f.id());
    let row_height = font.as_ref().map_or(ROW_HEIGHT, |f| ROW_HEIGHT.max(f.size + 6.0));
    let header_height = font.as_ref().map_or(HEADER_HEIGHT, |f| HEADER_HEIGHT.max(f.size + 8.0));

    let ctx = ui.ctx().clone();
    let list_id = ui.id().with("downloads_list");
    crate::a11y::group(&ctx, list_id, accesskit::Role::ListBox, tr("Downloads"), || {
        ui.scope(|ui| {
            let style = ui.style_mut();
            style.spacing.item_spacing = egui::vec2(8.0, 0.0);
            if let Some(font) = &font {
                style.text_styles.insert(egui::TextStyle::Body, font.clone());
                style.text_styles.insert(egui::TextStyle::Button, font.clone());
            }
            let grid = ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6);
            let line = Stroke::new(1.0_f32, grid);
            let outer = ui.clip_rect();

            let mut table = TableBuilder::new(ui)
                .id_salt(("downloads", state.generation, &columns))
                .striped(false)
                .resizable(true)
                // The cells take clicks and drags but not the focus: each row
                // takes it as a whole (below).
                .sense(Sense::CLICK | Sense::DRAG)
                .cell_layout(egui::Layout::left_to_right(Align::Center))
                .auto_shrink([false, false])
                .min_scrolled_height(0.0);
            for c in &columns {
                table = table.column(
                    egui_extras::Column::initial(layout.shown_width(*c))
                        .at_least(columns::WIDTH_RANGE.0.min(c.default_width()))
                        .clip(true),
                );
            }
            // The list is one stop for Tab, as a list box is: the row with the
            // focus, or the one the keys moved to, else the first row in view.
            // The keys move from row to row (`keys`).
            let tab_stop = state
                .focused
                .map(|(id, _)| id)
                .or(state.focus_next)
                .filter(|id| list.shown.iter().any(|e| e.id == *id));
            let focus_to = state.focus_next.and_then(|id| list.shown.iter().position(|e| e.id == id));
            if focus_to.is_none() {
                state.focus_next = None;
            }
            if let Some(i) = focus_to {
                table = table.scroll_to_row(i, None);
            } else if list.scroll {
                if let Some(i) = list.shown.iter().position(|e| Some(e.id) == list.found) {
                    table = table.scroll_to_row(i, Some(Align::Center));
                }
            }

            let mut measured = BTreeMap::new();
            table
                .header(header_height, |mut header| {
                    for &column in &columns {
                        header.col(|ui| {
                            let rect = ui.max_rect();
                            grid_lines(ui, outer, line);
                            measured.insert(column, rect.width());
                            ui.add(
                                egui::Label::new(egui::RichText::new(column.title()).strong())
                                    .truncate()
                                    .selectable(false),
                            );
                            let id = ui.id().with(("header", column));
                            // A stop for Tab, where Enter sorts by it (its menu
                            // makes it one in any case).
                            let click = ui.interact(rect, id, Sense::click());
                            let sorted = layout.sort.filter(|s| s.column == column && list.queue.is_none());
                            let name = match sorted {
                                Some(s) if s.descending => trf!("{column}, sorted descending", column = column.title()),
                                Some(_) => trf!("{column}, sorted ascending", column = column.title()),
                                None => column.title().to_string(),
                            };
                            crate::a11y::describe(&click, accesskit::Role::ColumnHeader, &name, |_| {});
                            let click = if list.queue.is_some() {
                                click.on_hover_text(tr("A queue's downloads stay in its run order"))
                            } else {
                                click
                            };
                            if click.clicked() && list.queue.is_none() && column.sortable() {
                                layout.click(column);
                            }
                            click.context_menu(|ui| {
                                if ui.button(tr("Columns...")).clicked() {
                                    outcome.open_columns = true;
                                    ui.close_menu();
                                }
                            });
                            if let Some(sort) = layout.sort.filter(|s| s.column == column && list.queue.is_none()) {
                                sort_arrow(ui, rect, sort.descending);
                            }
                        });
                    }
                })
                .body(|body| {
                    let mut page = 0;
                    let mut focused = None;
                    body.rows(row_height, list.shown.len(), |mut row| {
                        page += 1;
                        let i = row.index();
                        let e = list.shown[i];
                        let selected = list.selected.contains(&e.id);
                        row.set_selected(selected || list.found == Some(e.id));
                        let speed = list.speeds.get(&e.id).copied();
                        let mut left = None;
                        let mut focus = None;
                        let stop = tab_stop.map_or(page == 1, |id| id == e.id);
                        let sense = if stop { Sense::focusable_noninteractive() } else { Sense::hover() };
                        for (n, &column) in columns.iter().enumerate() {
                            row.col(|ui| {
                                grid_lines(ui, outer, line);
                                cell(ui, column, e, speed, places.get(&e.id).copied());
                                let cell = ui.max_rect();
                                let left = *left.get_or_insert(cell.left());
                                // The row's focus, across all its cells, made in
                                // the last one.
                                if n + 1 == columns.len() {
                                    let gap = ui.spacing().item_spacing.x / 2.0;
                                    let whole =
                                        Rect::from_x_y_ranges((left - gap)..=(cell.right() + gap), cell.y_range());
                                    let clip = Rect::from_x_y_ranges(outer.x_range(), ui.clip_rect().y_range());
                                    ui.set_clip_rect(clip);
                                    focus = Some(ui.interact(whole, row_id(e.id), sense));
                                }
                            });
                        }
                        if let Some(focus) = &focus {
                            crate::a11y::describe(
                                focus,
                                accesskit::Role::ListBoxOption,
                                &row_label(e, speed),
                                |node| {
                                    node.set_selected(selected);
                                },
                            );
                            if state.focus_next == Some(e.id) {
                                focus.request_focus();
                                state.focus_next = None;
                            }
                            if focus.has_focus() {
                                // Up and Down move through the list (`keys`), not
                                // to whatever widget is above or below.
                                focus.ctx.memory_mut(|m| {
                                    m.set_focus_lock_filter(
                                        focus.id,
                                        egui::EventFilter { vertical_arrows: true, ..Default::default() },
                                    );
                                });
                                focused = Some((e.id, focus.rect.left_bottom()));
                            }
                        }
                        let response = row.response();
                        let (ctrl, shift) = response.ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
                        if response.clicked() {
                            actions.push(Action::Click { id: e.id, ctrl, shift });
                            // The focus follows the click, so a screen reader
                            // reads the row and the keys go on from it.
                            state.focus_next = Some(e.id);
                        }
                        if response.double_clicked() {
                            actions.push(if e.status == Status::Done {
                                Action::Open(e.id)
                            } else {
                                Action::OpenProgress(e.id)
                            });
                        }
                        if response.secondary_clicked() {
                            if !list.selected.contains(&e.id) {
                                actions.push(Action::Click { id: e.id, ctrl: false, shift: false });
                            }
                            let at = response.interact_pointer_pos().unwrap_or(response.rect.left_bottom());
                            state.menu = Some(RowMenu { id: e.id, at, fresh: true, focus_first: false });
                        }
                        if let Some(path) = e.path.as_deref().filter(|_| e.status == Status::Done) {
                            drag_out(&response, path);
                        }
                    });
                    state.page = page;
                    state.focused = focused;
                });
            // The grips between headers that drag a column's width, which egui
            // leaves unnamed. The Columns dialog sets widths from the keyboard.
            for (i, column) in columns.iter().enumerate() {
                if let Some(grip) = ui.ctx().read_response(ui.id().with("resize_column").with(i)) {
                    let name = trf!("Width of {column}", column = column.title());
                    crate::a11y::describe(&grip, accesskit::Role::Splitter, &name, |_| {});
                }
            }
            for (column, width) in measured {
                if (layout.shown_width(column) - width).abs() >= 1.0 {
                    layout.set_width(column, width);
                }
            }
            state.measured = columns.iter().map(|c| (*c, layout.shown_width(*c))).collect();
        })
    });
    menu_area(ui, list, state, actions);
    outcome
}

/// The open row menu: at the pointer after a right-click, or below the
/// focused row with the focus on its first item after Shift+F10. It closes
/// on a choice, Escape or a click elsewhere, and the focus goes back to the
/// row.
fn menu_area(ui: &mut egui::Ui, list: &List, state: &mut TableState, actions: &mut Vec<Action>) {
    let Some(menu) = state.menu else {
        return;
    };
    let Some(i) = list.shown.iter().position(|e| e.id == menu.id) else {
        state.menu = None;
        return;
    };
    let e = list.shown[i];
    let before = actions.len();
    let shown = egui::Area::new(ui.id().with("row_menu"))
        .kind(egui::UiKind::Menu)
        .order(egui::Order::Foreground)
        .fixed_pos(menu.at)
        .constrain(true)
        .show(ui.ctx(), |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_max_width(ui.spacing().menu_width);
                ui.with_layout(egui::Layout::top_down_justified(Align::LEFT), |ui| {
                    row_menu(ui, e, list, i, actions, menu.focus_first);
                });
            });
        });
    let escape = ui.input(|i| i.key_pressed(Key::Escape));
    let elsewhere = !menu.fresh && shown.response.clicked_elsewhere();
    if actions.len() > before || escape || elsewhere {
        state.menu = None;
        state.focus_next = Some(menu.id);
    } else {
        state.menu = Some(RowMenu { fresh: false, focus_first: false, ..menu });
    }
}

/// Drag the file at `path` out of the window once the pointer has moved a
/// few points with the button down on `response`, once per press. A file
/// that is gone drags nothing. The completion dialog's file icon uses this
/// too.
pub fn drag_out(response: &egui::Response, path: &str) {
    let key = response.id.with("drag_out");
    if !response.dragged_by(egui::PointerButton::Primary) {
        response.ctx.data_mut(|d| d.remove_temp::<bool>(key));
        return;
    }
    let moved = response.ctx.input(|i| match (i.pointer.press_origin(), i.pointer.interact_pos()) {
        (Some(from), Some(to)) => from.distance(to) >= DRAG_DISTANCE,
        _ => false,
    });
    if !moved || response.ctx.data(|d| d.get_temp::<bool>(key)).is_some() {
        return;
    }
    response.ctx.data_mut(|d| d.insert_temp(key, true));
    if crate::ports::ports().files.is_file(Path::new(path)) {
        crate::win::drag_file(path);
    }
}

/// The grid: a line at the cell's right edge and along its bottom, in the
/// gap between cells. The cell clips its contents to itself, so the lines
/// are drawn with the table's clip across and the cell's clip down (which
/// keeps rows scrolled under the header from drawing over it).
fn grid_lines(ui: &egui::Ui, outer: Rect, line: Stroke) {
    let r = ui.max_rect();
    let gap = ui.spacing().item_spacing.x / 2.0;
    let clip = Rect::from_x_y_ranges(outer.x_range(), ui.clip_rect().y_range());
    let painter = egui::Painter::new(ui.ctx().clone(), ui.layer_id(), clip);
    painter.vline(r.right() + gap, r.y_range(), line);
    painter.hline((r.left() - gap)..=(r.right() + gap), r.bottom() - 0.5, line);
}

/// The sort direction at the right edge of a header.
fn sort_arrow(ui: &egui::Ui, rect: Rect, descending: bool) {
    let c = egui::pos2(rect.right() - 7.0, rect.center().y);
    let (tip, base) = if descending { (4.0, -3.0) } else { (-4.0, 3.0) };
    let color = ui.visuals().weak_text_color();
    ui.painter().add(egui::Shape::convex_polygon(
        vec![egui::pos2(c.x, c.y + tip), egui::pos2(c.x - 4.0, c.y + base), egui::pos2(c.x + 4.0, c.y + base)],
        color,
        Stroke::NONE,
    ));
}

/// One cell of a download's row.
fn cell(ui: &mut egui::Ui, column: Column, e: &DownloadEntry, speed: Option<f64>, place: Option<usize>) {
    let running = speed.filter(|s| *s > 0.0 && e.status == Status::Downloading);
    let text = |ui: &mut egui::Ui, s: &str| {
        let label = ui.add(egui::Label::new(s).truncate().selectable(false));
        if !s.is_empty() {
            label.on_hover_text(s);
        }
    };
    match column {
        Column::Name => {
            ui.add(egui::Label::new(display_name(e)).truncate().selectable(false))
                .on_hover_text(e.path.as_deref().unwrap_or(e.url.as_str()));
        }
        Column::Queue => {
            if let Some(n) = place {
                ui.add(egui::Label::new(n.to_string()).selectable(false));
            }
        }
        Column::Size => {
            // The whole size once known, as in IDM; until then what has
            // arrived. A stream counts segments.
            let size = match e.total {
                Some(total) => human_bytes(total),
                None if !e.done.is_zero() => human_bytes(e.done),
                None => String::new(),
            };
            let size = ui.add(egui::Label::new(size).selectable(false));
            if let Some([done, all]) = e.parts.filter(|[_, all]| *all > 0) {
                size.on_hover_text(trf!("{done} of {all} segments", done = done, all = all));
            }
        }
        Column::Progress => {
            let bar = egui::ProgressBar::new(columns::fraction(e).unwrap_or(0.0)).desired_height(12.0);
            crate::theme::progress_bar(
                ui,
                if e.total.is_some() || e.status == Status::Done {
                    bar.show_percentage()
                } else {
                    bar.text(human_bytes(e.done))
                },
            );
        }
        Column::Speed => {
            ui.add(
                egui::Label::new(running.map(|bps| format!("{}/s", human_rate(bps as u64))).unwrap_or_default())
                    .selectable(false),
            );
        }
        Column::TimeLeft => {
            ui.add(
                egui::Label::new(columns::time_left(e, running).map(human_duration).unwrap_or_default())
                    .selectable(false),
            );
        }
        Column::Status => status(ui, e),
        Column::LastTry => {
            ui.add(egui::Label::new(e.last_try.map(columns::local_date).unwrap_or_default()).selectable(false));
        }
        Column::SaveTo => {
            let folder = e.path.as_deref().and_then(|p| Path::new(p).parent()).map(|d| d.display().to_string());
            text(ui, folder.as_deref().unwrap_or(""));
        }
        Column::Page => text(ui, e.page.as_deref().unwrap_or("")),
        Column::Referer => text(ui, e.referrer.as_deref().unwrap_or("")),
        Column::Description => text(ui, e.description.as_deref().unwrap_or("")),
    }
}

/// The Status cell: "Complete", the percentage while downloading, or why it
/// stopped.
fn status(ui: &mut egui::Ui, e: &DownloadEntry) {
    let visuals = ui.visuals().clone();
    let percent;
    let (text, color) = match e.status {
        Status::Queued => (tr("Queued"), visuals.weak_text_color()),
        Status::Downloading => match columns::fraction(e).filter(|_| e.total.is_some()) {
            Some(f) => {
                percent = format!("{}%", crate::i18n::number(f64::from(f * 100.0), 2));
                (percent.as_str(), visuals.text_color())
            }
            None => (tr("Downloading"), visuals.text_color()),
        },
        Status::Done => (tr("Complete"), visuals.text_color()),
        Status::Failed => (tr("Failed"), visuals.error_fg_color),
        Status::Canceled => (tr("Canceled"), visuals.warn_fg_color),
        Status::Interrupted => (tr("Interrupted"), visuals.warn_fg_color),
        Status::Confirming => (tr("Confirm"), visuals.weak_text_color()),
        Status::Waiting => (tr("Waiting"), visuals.weak_text_color()),
    };
    let label = ui.add(egui::Label::new(egui::RichText::new(text).color(color)).selectable(false));
    if let Some(err) = &e.error {
        label.on_hover_text(err);
    }
    if let Some(scan) = &e.scan {
        scan_label(ui, scan);
    }
    if let Some(note) = &e.note {
        ui.weak(tr("(note)")).on_hover_text(note);
    }
    if let Some(limit) = e.speed_limit {
        ui.weak(tr("(limited)")).on_hover_text(trf!("Limited to {speed}/s", speed = human_rate(limit)));
    }
}

/// A row's menu, from a right-click or Shift+F10. `focus_first` gives
/// its first item the keyboard focus.
fn row_menu(ui: &mut egui::Ui, e: &DownloadEntry, list: &List, i: usize, actions: &mut Vec<Action>, focus_first: bool) {
    let on = list.connected;
    let focus = std::cell::Cell::new(focus_first);
    let add = |ui: &mut egui::Ui, enabled: bool, text: &str| {
        let button = ui.add_enabled(enabled, egui::Button::new(text));
        if enabled && focus.replace(false) {
            button.request_focus();
        }
        button.clicked()
    };
    let mut item = |ui: &mut egui::Ui, enabled: bool, text: &str, action: Action| {
        if add(ui, enabled, text) {
            actions.push(action);
            ui.close_menu();
        }
    };
    let done = e.status == Status::Done;
    if let Some(path) = e.path.as_ref().filter(|_| done) {
        item(ui, true, tr("Open"), Action::Open(e.id));
        item(ui, true, tr("Open with..."), Action::OpenWith(path.clone()));
        item(ui, true, tr("Open folder"), Action::ShowInFolder(path.clone()));
        item(ui, true, tr("Verify checksum..."), Action::OpenVerify(e.id));
        ui.separator();
    }
    if let Some(q) = list.queue {
        let len = list.shown.len();
        item(
            ui,
            on && i > 0,
            tr("Move up"),
            Action::MoveToQueue { id: e.id, queue: q.id, position: Some(i.saturating_sub(1)) },
        );
        item(
            ui,
            on && i + 1 < len,
            tr("Move down"),
            Action::MoveToQueue { id: e.id, queue: q.id, position: Some(i + 1) },
        );
        ui.separator();
    }
    let stopped = matches!(e.status, Status::Failed | Status::Canceled | Status::Interrupted);
    item(ui, on && (done || stopped), tr("Redownload"), Action::Redownload(e.id));
    match e.status {
        Status::Waiting => item(ui, on, tr("Download Now"), Action::Confirm(Answer::start(e.id))),
        _ => item(ui, on && stopped, tr("Resume Download"), Action::Retry(e.id)),
    }
    item(
        ui,
        on && matches!(e.status, Status::Queued | Status::Downloading | Status::Confirming),
        tr("Stop Download"),
        Action::Cancel(e.id),
    );
    item(
        ui,
        on && (stopped || e.status == Status::Waiting),
        tr("Refresh download address..."),
        Action::OpenRefresh(e.id),
    );
    if e.error.as_deref().and_then(udm_domain::download::login_wanted).is_some() {
        item(ui, on, tr("Log in..."), Action::OpenLogin(e.id));
    }
    let ids: Vec<DownloadId> = if list.selected.contains(&e.id) {
        list.shown.iter().filter(|r| list.selected.contains(&r.id) && r.status.is_terminal()).map(|r| r.id).collect()
    } else {
        vec![e.id]
    };
    item(ui, on && e.status.is_terminal(), tr("Remove"), Action::Remove(ids));
    // Leaves the download in the list, outside every queue, so no queue
    // starts it; Move to queue puts it back.
    let queued = list.queues.iter().any(|q| q.order.contains(&e.id));
    item(ui, on && queued, tr("Remove from queue"), Action::RemoveFromQueue(e.id));
    ui.add_enabled_ui(on && e.status.is_terminal() && !done, |ui| {
        ui.menu_button(tr("Move to queue"), |ui| {
            for q in list.queues {
                if ui.button(&q.name).clicked() {
                    actions.push(Action::MoveToQueue { id: e.id, queue: q.id, position: None });
                    ui.close_menu();
                }
            }
        });
    });
    ui.separator();
    let mut item = |ui: &mut egui::Ui, enabled: bool, text: &str, action: Action| {
        if add(ui, enabled, text) {
            actions.push(action);
            ui.close_menu();
        }
    };
    if !done {
        item(ui, true, tr("Download window"), Action::OpenProgress(e.id));
    }
    item(ui, true, tr("Connections..."), Action::OpenConnections(e.id));
    item(ui, on && !done, tr("Speed limit..."), Action::OpenSpeedLimit(e.id));
    if e.path.as_deref().filter(|_| done).is_some_and(udm_domain::media::is_video) {
        item(ui, on, tr("Convert..."), Action::OpenConvert(e.id));
    }
    ui.separator();
    item(ui, true, tr("Properties"), Action::OpenProperties(e.id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use udm_domain::bytes::ByteCount;

    fn entry(status: Status, total: Option<u64>, done: u64) -> DownloadEntry {
        DownloadEntry {
            id: DownloadId::new(1),
            url: "https://example.com/files/movie.mkv".into(),
            path: None,
            total: total.map(ByteCount::new),
            done: ByteCount::new(done),
            status,
            error: None,
            resumable: true,
            connections: 0,
            scan: None,
            parts: None,
            note: None,
            duplicate_of: None,
            referrer: None,
            page: None,
            last_try: None,
            description: None,
            checksum: None,
            category: None,
            login_user: None,
            speed_limit: None,
            limit_remember: true,
            partial: None,
            from_browser: false,
            on_done: None,
            validator: None,
        }
    }

    #[test]
    fn a_running_row_reads_its_name_status_progress_and_speed() {
        let e = entry(Status::Downloading, Some(4 * 1024 * 1024), 1024 * 1024);
        let label = row_label(&e, Some(512.0 * 1024.0));
        assert_eq!(label, "movie.mkv, Downloading, 25.0% of 4.0 MB, 512.0 KB/s, 6s left");
    }

    #[test]
    fn a_row_without_speed_or_size_reads_what_it_has() {
        let queued = entry(Status::Queued, None, 0);
        assert_eq!(row_label(&queued, None), "movie.mkv, Queued");
        let unknown = entry(Status::Downloading, None, 2048);
        // No speed is read while the download is not moving.
        assert_eq!(row_label(&unknown, Some(0.0)), "movie.mkv, Downloading, 2.0 KB downloaded");
        let done = DownloadEntry { path: Some("/tmp/Movie (1).mkv".into()), ..entry(Status::Done, Some(10), 10) };
        assert_eq!(row_label(&done, Some(100.0)), "Movie (1).mkv, Complete, 100.0% of 10 B");
    }

    #[test]
    fn a_failed_row_reads_why() {
        let e = DownloadEntry { error: Some("HTTP 404".into()), ..entry(Status::Failed, Some(100), 50) };
        assert_eq!(row_label(&e, None), "movie.mkv, Failed, 50.0% of 100 B, HTTP 404");
    }

    /// Draw the list for a frame with `events`, as the window does: the
    /// list's keys, then the table.
    fn frame(
        ctx: &egui::Context,
        rows: &[DownloadEntry],
        state: &mut TableState,
        selected: &HashSet<DownloadId>,
        anchor: Option<DownloadId>,
        events: Vec<egui::Event>,
    ) -> Vec<Action> {
        let shown: Vec<&DownloadEntry> = rows.iter().collect();
        let order: Vec<DownloadId> = rows.iter().map(|e| e.id).collect();
        let speeds = HashMap::new();
        let list = List {
            shown: &shown,
            speeds: &speeds,
            selected,
            queues: &[],
            queue: None,
            connected: true,
            found: None,
            scroll: false,
        };
        let mut layout = Layout::default();
        let mut actions = Vec::new();
        // The keys held, as a backend reports them beside the key events.
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 400.0))),
            events,
            modifiers,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            if list_has_keys(ctx, state) {
                keys(ctx, state, &order, anchor, &mut actions);
            }
            egui::CentralPanel::default().show(ctx, |ui| show(ui, &list, &mut layout, state, &mut actions));
        });
        actions
    }

    fn key(key: Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    fn clicked(actions: &[Action]) -> Vec<(u64, bool, bool)> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Click { id, ctrl, shift } => Some((id.get(), *ctrl, *shift)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_keys_select_rows_and_the_focus_follows() {
        let rows: Vec<DownloadEntry> =
            (1..=3).map(|n| DownloadEntry { id: DownloadId::new(n), ..entry(Status::Queued, None, 0) }).collect();
        let ctx = egui::Context::default();
        let mut state = TableState::default();
        let none = HashSet::new();
        let plain = egui::Modifiers::NONE;
        frame(&ctx, &rows, &mut state, &none, None, vec![]);
        // With nothing focused, Down starts at the first row.
        let actions = frame(&ctx, &rows, &mut state, &none, None, vec![key(Key::ArrowDown, plain)]);
        assert_eq!(clicked(&actions), [(1, false, false)]);
        frame(&ctx, &rows, &mut state, &none, None, vec![]);
        let row = |n| row_id(DownloadId::new(n));
        assert!(ctx.memory(|m| m.has_focus(row(1))));
        assert!(list_has_keys(&ctx, &state));

        // From the focused row: Down, Shift+Down (a run), Ctrl+Down (the
        // focus alone) and Space.
        let one: HashSet<DownloadId> = [DownloadId::new(1)].into();
        let actions = frame(&ctx, &rows, &mut state, &one, Some(DownloadId::new(1)), vec![key(Key::ArrowDown, plain)]);
        assert_eq!(clicked(&actions), [(2, false, false)]);
        frame(&ctx, &rows, &mut state, &one, None, vec![]);
        assert!(ctx.memory(|m| m.has_focus(row(2))));
        let actions = frame(&ctx, &rows, &mut state, &one, None, vec![key(Key::ArrowDown, egui::Modifiers::SHIFT)]);
        assert_eq!(clicked(&actions), [(3, false, true)]);
        frame(&ctx, &rows, &mut state, &one, None, vec![]);
        let actions = frame(&ctx, &rows, &mut state, &one, None, vec![key(Key::ArrowUp, egui::Modifiers::COMMAND)]);
        assert!(clicked(&actions).is_empty());
        frame(&ctx, &rows, &mut state, &one, None, vec![]);
        assert!(ctx.memory(|m| m.has_focus(row(2))));
        let actions = frame(&ctx, &rows, &mut state, &one, None, vec![key(Key::Space, egui::Modifiers::COMMAND)]);
        assert_eq!(clicked(&actions), [(2, true, false)]);

        // Shift+F10 opens the focused row's menu with the focus in it, and
        // Escape closes it and gives the focus back to the row.
        frame(&ctx, &rows, &mut state, &one, None, vec![key(Key::F10, egui::Modifiers::SHIFT)]);
        assert!(state.menu.is_some_and(|m| m.id == DownloadId::new(2)));
        frame(&ctx, &rows, &mut state, &one, None, vec![]);
        assert!(!ctx.memory(|m| m.has_focus(row(2))), "the menu's first item has the focus");
        frame(&ctx, &rows, &mut state, &one, None, vec![key(Key::Escape, plain)]);
        assert!(state.menu.is_none());
        frame(&ctx, &rows, &mut state, &one, None, vec![]);
        assert!(ctx.memory(|m| m.has_focus(row(2))));
    }

    #[test]
    fn keys_move_within_the_list() {
        assert_eq!(step(Some(3), 10, 5, Move::Up), Some(2));
        assert_eq!(step(Some(0), 10, 5, Move::Up), Some(0));
        assert_eq!(step(Some(9), 10, 5, Move::Down), Some(9));
        // A page is one row less than the rows shown, so one stays in view.
        assert_eq!(step(Some(1), 10, 5, Move::PageDown), Some(5));
        assert_eq!(step(Some(8), 10, 5, Move::PageDown), Some(9));
        assert_eq!(step(Some(2), 10, 5, Move::PageUp), Some(0));
        assert_eq!(step(Some(4), 10, 5, Move::Home), Some(0));
        assert_eq!(step(Some(4), 10, 5, Move::End), Some(9));
        // A page of one row (or none shown yet) still moves.
        assert_eq!(step(Some(4), 10, 0, Move::PageDown), Some(5));
    }

    #[test]
    fn keys_start_at_an_end_without_a_row() {
        assert_eq!(step(None, 10, 5, Move::Down), Some(0));
        assert_eq!(step(None, 10, 5, Move::PageDown), Some(0));
        assert_eq!(step(None, 10, 5, Move::Up), Some(9));
        assert_eq!(step(None, 0, 5, Move::Down), None);
        assert_eq!(step(Some(0), 0, 5, Move::End), None);
    }
}
