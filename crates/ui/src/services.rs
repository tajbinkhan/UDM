//! What Options reaches beyond the settings: the tools UDM can install,
//! remote control and pairing a phone, the browsers UDM is registered with,
//! and the theme files. The state the daemon and the workers report, and
//! what Options shows of it. Plain functions, tested without Tauri.

use udm_application::daemon::{Event, Failure};
use udm_application::themes::ThemesRead;
use udm_application::window::BrowserSlot;
use udm_domain::bytes::ByteCount;
use udm_domain::debrid::{DebridCheck, DebridService};
use udm_domain::pairing::RemoteState;
use udm_domain::settings::{AddedBrowser, Theme};
use udm_domain::tools::{Tool, ToolState};

use crate::contract::{
    AdapterRow, Banner, BrowserRowView, Choice, DeviceRow, Id, MissingTool, MissingTools, OptionsView, PairingView,
    RemoteView, ToolKey, ToolProgressView, ToolRow,
};
use crate::i18n::{fill, Language};
use crate::projection::{format, Ctx};
use crate::state::AppState;

/// One browser Options lists: one UDM found, or one the user added by its
/// program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRow {
    /// The browser whose registration it uses.
    pub browser: BrowserSlot,
    pub name: String,
    pub added: Option<AddedBrowser>,
    /// Its place is known rather than guessed from its family.
    pub known: bool,
    pub registered: bool,
    /// UDM can close it and start it again.
    pub can_restart: bool,
}

/// The browsers a worker read, and those that read Chrome's registration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BrowsersRead {
    pub rows: Vec<BrowserRow>,
    pub chrome_readers: Vec<String>,
}

/// What Options shows beyond the settings.
#[derive(Debug, Default)]
pub struct Services {
    pub tools: Vec<ToolState>,
    /// The tools being installed.
    pub tool_busy: Vec<Tool>,
    /// The last install's tool and why it failed, if it did.
    pub tool_done: Option<(Tool, Option<Failure>)>,
    /// How far each install's download is: bytes come, and the size the
    /// server gave.
    pub tool_progress: Vec<(Tool, ByteCount, Option<ByteCount>)>,
    pub remote: Option<RemoteState>,
    /// `None` until a worker read them.
    pub browsers: Option<BrowsersRead>,
    /// What the last browser change said.
    pub browser_note: Option<Banner>,
    /// The theme files, and the system's contrast colors.
    pub themes: ThemesRead,
    /// Check account, and what it found.
    pub debrid: DebridState,
    /// The network adapters a worker read; `None` until it did.
    pub adapters: Option<Result<Vec<udm_domain::network::NetworkAdapter>, udm_domain::network::NetworkError>>,
}

/// Where Options' Check account stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DebridState {
    /// Not asked since Options opened.
    #[default]
    Idle,
    /// Waiting for the daemon's answer about this service's account.
    Checking(DebridService),
    /// What the service said.
    Checked(DebridCheck),
    /// The daemon went, or answered with an error, before it answered.
    Unanswered,
}

/// Take what the daemon said of the tools and of remote control.
pub fn take(state: &mut AppState, event: &Event) {
    let s = &mut state.services;
    // A daemon that went took its installs with it.
    if let Event::Lost(_) = event {
        s.tool_busy.clear();
        s.tool_progress.clear();
    }
    match event {
        Event::Tools { tools, done, error } => {
            s.tools = tools.clone();
            if let Some(tool) = done {
                s.tool_busy.retain(|t| t != tool);
                s.tool_progress.retain(|(t, _, _)| t != tool);
                s.tool_done = Some((*tool, error.clone()));
            }
        }
        // Only an install the window started; a late one after `Tools` is dropped.
        Event::ToolProgress { tool, done, total } if s.tool_busy.contains(tool) => {
            s.tool_progress.retain(|(t, _, _)| t != tool);
            s.tool_progress.push((*tool, *done, *total));
        }
        Event::Remote { state } => s.remote = Some(state.clone()),
        Event::Debrid { check } => s.debrid = DebridState::Checked(check.clone()),
        // Options shows a refusal while downloads went on without the
        // account, unless a check is waiting for its own answer.
        Event::DebridRefused { refused } if !matches!(s.debrid, DebridState::Checking(_)) => {
            let check = DebridCheck { service: refused.service, outcome: Err(refused.error.clone()) };
            s.debrid = DebridState::Checked(check);
        }
        // A check the daemon will not answer does not wait for ever.
        Event::Lost(_) | Event::Error { .. } if matches!(s.debrid, DebridState::Checking(_)) => {
            s.debrid = DebridState::Unanswered
        }
        _ => {}
    }
}

pub fn tool_key(t: Tool) -> ToolKey {
    match t {
        Tool::YtDlp => ToolKey::YtDlp,
        Tool::Ffmpeg => ToolKey::Ffmpeg,
        Tool::GalleryDl => ToolKey::GalleryDl,
    }
}

/// The tool a key names.
pub fn tool_of(k: ToolKey) -> Tool {
    match k {
        ToolKey::YtDlp => Tool::YtDlp,
        ToolKey::Ffmpeg => Tool::Ffmpeg,
        ToolKey::GalleryDl => Tool::GalleryDl,
    }
}

fn tool_rows(lang: &'static Language, s: &Services) -> Vec<ToolRow> {
    s.tools
        .iter()
        .map(|t| {
            let name = t.tool.name();
            let status = match (&t.installed, &t.path) {
                (Some(version), Some(path)) => {
                    fill(lang.tr("Installed by UDM ({version}): {path}"), &[("version", version), ("path", path)])
                }
                (None, Some(path)) => fill(lang.tr("Using {path}"), &[("path", path)]),
                _ => fill(lang.tr("{tool} was not found."), &[("tool", name)]),
            };
            let action = if t.installed.is_some() { "Update {tool}" } else { "Install {tool}" };
            let hint = if !t.installable {
                fill(lang.tr("UDM cannot install it here. Install it with: {command}"), &[("command", &t.command)])
            } else if t.tool == Tool::GalleryDl {
                lang.tr("Downloads the official build from Codeberg and checks it against its published checksum")
                    .to_string()
            } else {
                lang.tr("Downloads the official build from GitHub and checks it against its published checksum")
                    .to_string()
            };
            ToolRow {
                tool: tool_key(t.tool),
                status,
                action: fill(lang.tr(action), &[("tool", name)]),
                hint,
                installable: t.installable,
                busy: s.tool_busy.contains(&t.tool),
                progress: tool_progress(lang, s, t.tool),
            }
        })
        .collect()
}

/// How far `tool`'s download is, while it installs and the daemon has said.
fn tool_progress(lang: &'static Language, s: &Services, tool: Tool) -> Option<ToolProgressView> {
    let (_, done, total) = s.tool_progress.iter().find(|(t, _, _)| *t == tool)?;
    Some(progress_view(lang, *done, *total))
}

/// How far a download of `done` bytes of `total` is, as the shell words it,
/// for a tool's install and for Install now's.
pub(crate) fn progress_view(lang: &'static Language, done: ByteCount, total: Option<ByteCount>) -> ToolProgressView {
    let size = |n: ByteCount| format::bytes(lang, n, 1);
    match total.filter(|t| t.get() > 0) {
        Some(total) if done >= total => {
            ToolProgressView { text: lang.tr("Checking and installing...").to_string(), percent: Some(100) }
        }
        Some(total) => {
            let percent = percent_of(done, total);
            ToolProgressView {
                text: fill(
                    lang.tr("{done} of {total} ({percent}%)"),
                    &[("done", &size(done)), ("total", &size(total)), ("percent", &percent.to_string())],
                ),
                percent: Some(percent),
            }
        }
        None => ToolProgressView { text: fill(lang.tr("{done} downloaded"), &[("done", &size(done))]), percent: None },
    }
}

/// `done` of `total` in whole percent, at most 100.
fn percent_of(done: ByteCount, total: ByteCount) -> u8 {
    (done.get().saturating_mul(100) / total.get().max(1)).min(100) as u8
}

/// The strip's word while tools install: with how far, over every install
/// whose size is known.
fn busy_text(lang: &'static Language, s: &Services) -> String {
    let sized: Vec<_> = s.tool_progress.iter().filter_map(|(_, done, total)| total.map(|t| (*done, t))).collect();
    let all = sized.iter().fold(0u64, |sum, (_, t)| sum.saturating_add(t.get()));
    if sized.is_empty() || all == 0 {
        return lang.tr("Installing...").to_string();
    }
    let done = sized.iter().fold(0u64, |sum, (d, t)| sum.saturating_add(d.get().min(t.get())));
    let percent = percent_of(ByteCount::new(done), ByteCount::new(all));
    fill(lang.tr("Installing... {percent}%"), &[("percent", &percent.to_string())])
}

/// The key in `window.json` that remembers the first start's tools offer
/// was answered.
pub const TOOLS_ASKED: &str = "tools_asked";
/// The key in `window.json` naming the missing tools the strip was closed
/// for; it shows again when another goes missing.
pub const TOOLS_DISMISSED: &str = "dismissed_tools";

/// The names of the tools UDM did not find, as the strip's closing keeps
/// them.
pub fn missing_names(s: &Services) -> String {
    s.tools.iter().filter(|t| t.path.is_none()).map(|t| t.tool.name()).collect::<Vec<_>>().join(", ")
}

/// What each tool is for, in the offer.
fn purpose(t: Tool) -> &'static str {
    match t {
        Tool::YtDlp => "Downloads videos from YouTube and many other video sites.",
        Tool::Ffmpeg => {
            "Joins streamed videos into one .mp4. Without it they are saved as .ts, or as separate video and audio."
        }
        Tool::GalleryDl => "Downloads the pictures of image galleries.",
    }
}

/// The strip and the first start's sheet offering the tools UDM did not
/// find, while connected to a daemon that said where they are.
pub fn missing_tools(lang: &'static Language, state: &AppState) -> Option<MissingTools> {
    let rest = &state.layout_rest;
    let s = &state.services;
    let missing: Vec<&ToolState> = s.tools.iter().filter(|t| t.path.is_none()).collect();
    if missing.is_empty() || !state.connected() {
        return None;
    }
    let names = missing_names(s);
    let tools = missing
        .iter()
        .map(|t| MissingTool {
            tool: tool_key(t.tool),
            name: t.tool.name().to_string(),
            purpose: lang.tr(purpose(t.tool)).to_string(),
            installable: t.installable,
            hint: if t.installable {
                String::new()
            } else {
                fill(lang.tr("UDM cannot install it here. Install it with: {command}"), &[("command", &t.command)])
            },
            progress: tool_progress(lang, s, t.tool),
        })
        .collect();
    Some(MissingTools {
        text: fill(lang.tr("Some downloads need tools UDM did not find: {tools}."), &[("tools", &names)]),
        strip: rest.get(TOOLS_DISMISSED).and_then(|v| v.as_str()) != Some(names.as_str()),
        ask: !rest.contains_key(TOOLS_ASKED),
        busy: missing.iter().any(|t| s.tool_busy.contains(&t.tool)),
        busy_text: busy_text(lang, s),
        tools,
    })
}

/// What the last check of the debrid account said.
fn debrid_note(ctx: &Ctx, s: &Services) -> Option<Banner> {
    let lang = ctx.lang;
    let check = match &s.debrid {
        DebridState::Checked(check) => check,
        DebridState::Unanswered => {
            let text = lang.tr("UDM's background service did not answer. Try again.").to_string();
            return Some(Banner { text, error: true });
        }
        DebridState::Idle | DebridState::Checking(_) => return None,
    };
    let service = check.service.name();
    Some(match &check.outcome {
        Ok(account) => {
            let user = account.username.as_deref().unwrap_or(service);
            let text = match (account.premium, account.premium_until) {
                (true, Some(until)) => fill(
                    lang.tr("The key works: {user}, premium until {date}."),
                    &[("user", user), ("date", &format::date(lang, until, ctx.offset))],
                ),
                (true, None) => fill(lang.tr("The key works: {user}, premium."), &[("user", user)]),
                (false, _) => fill(lang.tr("The key works, but {user} is not a premium account."), &[("user", user)]),
            };
            Banner { text, error: !account.premium }
        }
        Err(e) => Banner {
            text: fill(
                lang.tr("{service}: {error}"),
                &[("service", service), ("error", &crate::errors::text::debrid(lang, e))],
            ),
            error: true,
        },
    })
}

fn tool_note(lang: &'static Language, s: &Services) -> Option<Banner> {
    let (tool, error) = s.tool_done.as_ref()?;
    Some(match error {
        Some(e) => Banner { text: crate::errors::failure(lang, e), error: true },
        None => Banner { text: fill(lang.tr("{tool} is installed."), &[("tool", tool.name())]), error: false },
    })
}

/// The QR code of `text`, row by row, and its side; `None` when it does
/// not fit in one.
pub fn qr(text: &str) -> Option<(Vec<bool>, u32)> {
    let code = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::M).ok()?;
    let side = code.width() as u32;
    Some((code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect(), side))
}

fn remote_view(ctx: &Ctx, state: &AppState) -> Option<RemoteView> {
    let lang = ctx.lang;
    let r = state.services.remote.as_ref()?;
    let on = state.settings.as_ref().is_some_and(|s| s.remote_enabled);
    let status = match (&r.listener.page, &r.listener.fault, on) {
        (Some(page), _, _) => fill(lang.tr("The page is at {page}"), &[("page", page)]),
        (None, Some(fault), _) => crate::errors::text::remote_fault(lang, fault),
        (None, None, true) => lang.tr("Starting…").to_string(),
        (None, None, false) => lang.tr("Remote control is off.").to_string(),
    };
    let devices = r
        .devices
        .iter()
        .map(|d| {
            let name = if d.name.as_str().is_empty() { lang.tr("Unnamed device") } else { d.name.as_str() };
            let mut detail = fill(lang.tr("Paired {date}"), &[("date", &format::date(lang, d.paired_at, ctx.offset))]);
            if let Some(seen) = d.last_seen {
                detail = format!(
                    "{detail} · {}",
                    fill(lang.tr("Last seen {date}"), &[("date", &format::date(lang, seen, ctx.offset))])
                );
            }
            DeviceRow { id: Id(d.id.get()), name: name.to_string(), detail }
        })
        .collect();
    let pairing = r.pairing.as_ref().map(|p| {
        let (modules, side) = qr(&p.link).unwrap_or_default();
        let page = r.listener.page.clone().unwrap_or_default();
        PairingView {
            link: p.link.clone(),
            code: String::from(p.code.clone()),
            expired: p.expires_at <= ctx.unix,
            modules,
            side,
            type_note: fill(lang.tr("Or open {page} on the phone and type this code:"), &[("page", &page)]),
        }
    });
    Some(RemoteView {
        status,
        error: r.listener.fault.is_some(),
        fingerprint: r.listener.fingerprint.clone(),
        devices,
        pairing,
        can_pair: on && r.listener.page.is_some(),
    })
}

fn browser_rows(lang: &'static Language, b: &BrowsersRead) -> Vec<BrowserRowView> {
    b.rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let detail = match &r.added {
                Some(a) if r.known => fill(
                    lang.tr("{program}; uses {place}'s registration"),
                    &[("program", &a.program), ("place", &r.browser.place)],
                ),
                Some(a) => fill(
                    lang.tr("{program}; registered where {place} looks, which most browsers of its kind read"),
                    &[("program", &a.program), ("place", &r.browser.place)],
                ),
                None => String::new(),
            };
            BrowserRowView {
                index: i as u32,
                name: r.name.clone(),
                detail,
                registered: r.registered,
                added: r.added.is_some(),
                can_restart: r.can_restart,
            }
        })
        .collect()
}

/// The theme list: the built-in looks, then the theme files' themes, and
/// the theme set even when its file is gone.
fn theme_choices(lang: &'static Language, state: &AppState) -> Vec<Choice> {
    let choice = |value: &str, label: &str| Choice { value: value.to_string(), label: lang.tr(label).to_string() };
    let mut out = vec![
        choice("system", "System"),
        choice("light", "Light"),
        choice("dark", "Dark"),
        choice("highContrast", "High contrast"),
    ];
    for t in &state.services.themes.themes {
        out.push(Choice { value: format!("custom:{}", t.name.as_str()), label: t.name.as_str().to_string() });
    }
    if let Some(Theme::Custom(name)) = state.settings.as_ref().map(|s| &s.theme) {
        let value = format!("custom:{}", name.as_str());
        if !out.iter().any(|c| c.value == value) {
            out.push(Choice { value, label: name.as_str().to_string() });
        }
    }
    out
}

/// The adapters Options lists: each with its addresses, or as not
/// connected, and the chosen one first when it is not there now, so the
/// list still shows what downloads are bound to.
fn adapter_rows(
    lang: &'static Language,
    adapters: &[udm_domain::network::NetworkAdapter],
    chosen: Option<&udm_domain::network::AdapterChoice>,
) -> Vec<AdapterRow> {
    let not_connected = |name: &str| fill(lang.tr("{adapter} (not connected)"), &[("adapter", name)]);
    let mut rows: Vec<AdapterRow> = adapters
        .iter()
        .map(|a| {
            // Another connection that took the chosen adapter's name is not
            // it: downloads treat it as down, and so does the list.
            let other = chosen.is_some_and(|c| c.id == a.id && !c.is(a));
            let up = a.usable() && !other;
            AdapterRow {
                id: a.id.to_string(),
                name: a.name.clone(),
                label: if up { format!("{} ({})", a.name, a.addresses_text()) } else { not_connected(&a.name) },
                up,
                renewable: other && a.usable(),
            }
        })
        .collect();
    if let Some(c) = chosen.filter(|c| !adapters.iter().any(|a| a.id == c.id)) {
        rows.insert(
            0,
            AdapterRow {
                id: c.id.to_string(),
                name: c.label().to_string(),
                label: not_connected(c.label()),
                up: false,
                renewable: false,
            },
        );
    }
    rows
}

/// Options' view with what the daemon and the workers reported.
pub fn fill_view(ctx: &Ctx, state: &AppState, view: &mut OptionsView) {
    let lang = ctx.lang;
    let s = &state.services;
    view.tools = tool_rows(lang, s);
    view.tool_note = tool_note(lang, s);
    view.debrid.checking = matches!(s.debrid, DebridState::Checking(_));
    view.debrid.check = debrid_note(ctx, s);
    view.remote = remote_view(ctx, state);
    view.browsers = s.browsers.as_ref().map(|b| browser_rows(lang, b));
    view.chrome_note = s
        .browsers
        .as_ref()
        .filter(|b| !b.chrome_readers.is_empty())
        .map(|b| {
            fill(lang.tr("{browsers} use Google Chrome's setting."), &[("browsers", &b.chrome_readers.join(", "))])
        })
        .unwrap_or_default();
    view.browser_note = s.browser_note.clone();
    let chosen = state.settings.as_ref().and_then(|s| s.network_adapter.as_ref());
    match &s.adapters {
        None => {}
        Some(Ok(adapters)) => view.network.adapters = Some(adapter_rows(lang, adapters, chosen)),
        Some(Err(e)) => {
            view.network.adapters = Some(adapter_rows(lang, &[], chosen));
            view.network.adapters_error = Some(crate::errors::text::network(lang, e));
        }
    }
    view.creation_time = state.system.creation_time;
    view.portable = state.system.portable;
    view.themes = theme_choices(lang, state);
    view.theme_notes = s
        .themes
        .broken
        .iter()
        .map(|b| {
            fill(
                lang.tr("{file} is not a theme: {why}"),
                &[("file", &b.file), ("why", &crate::errors::text::theme_file(lang, &b.why))],
            )
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply, UiMessage};
    use std::time::Instant;

    fn ctx() -> Ctx {
        Ctx {
            lang: crate::i18n::resolve("en", &[]),
            offset: 0,
            today: String::new(),
            now: Instant::now(),
            unix: 1_700_000_000,
            downloads: std::path::PathBuf::from("/home/u/Downloads"),
            flags: true,
        }
    }

    #[test]
    fn the_adapters_list_shows_addresses_and_keeps_the_chosen_one() {
        use udm_domain::network::{AdapterChoice, AdapterId, NetworkAdapter, NetworkError};
        let adapter = |id: &str, addresses: &[&str], up: bool| NetworkAdapter {
            id: AdapterId::parse(id).unwrap(),
            name: format!("{id} adapter"),
            addresses: addresses.iter().map(|a| a.parse().unwrap()).collect(),
            subnets: Vec::new(),
            up,
        };
        let mut s = AppState::default();
        let settings = udm_domain::settings::Settings {
            network_adapter: Some(AdapterChoice {
                id: AdapterId::parse("tun9").unwrap(),
                name: "Old VPN".into(),
                subnets: Vec::new(),
            }),
            ..udm_domain::settings::Settings::default()
        };
        s.settings = Some(settings.clone());
        let mut view = crate::options::view(ctx().lang, &settings);
        fill_view(&ctx(), &s, &mut view);
        assert_eq!(view.network.adapters, None, "still being read");
        let read = vec![adapter("eth0", &["192.168.1.5", "fe80::1"], true), adapter("wg0", &["10.2.0.2"], false)];
        apply(&mut s, UiMessage::AdaptersRead(Ok(read)));
        fill_view(&ctx(), &s, &mut view);
        let rows = view.network.adapters.clone().unwrap();
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Old VPN (not connected)", "eth0 adapter (192.168.1.5, fe80::1)", "wg0 adapter (not connected)"]
        );
        assert_eq!(rows.iter().map(|r| r.up).collect::<Vec<_>>(), [false, true, false]);
        apply(&mut s, UiMessage::AdaptersRead(Err(NetworkError::Adapters("no access".into()))));
        fill_view(&ctx(), &s, &mut view);
        assert_eq!(view.network.adapters.map(|r| r.len()), Some(1), "the chosen one stays listed");
        assert_eq!(view.network.adapters_error.as_deref(), Some("The network adapters could not be read: no access"));
    }

    #[test]
    fn another_connection_with_the_chosen_name_can_be_chosen_again() {
        use udm_domain::network::{AdapterChoice, AdapterId, NetworkAdapter, Subnet};
        let ip = |a: &str| a.parse().unwrap();
        let chosen = AdapterChoice {
            id: AdapterId::parse("utun3").unwrap(),
            name: "utun3".into(),
            subnets: vec![Subnet::new(ip("10.8.0.0"), 24).unwrap()],
        };
        let now = NetworkAdapter {
            id: AdapterId::parse("utun3").unwrap(),
            name: "utun3".into(),
            addresses: vec![ip("172.20.1.2")],
            subnets: vec![Subnet::new(ip("172.20.1.2"), 24).unwrap()],
            up: true,
        };
        let rows = adapter_rows(ctx().lang, std::slice::from_ref(&now), Some(&chosen));
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].up && rows[0].renewable, "{:?}", rows[0]);
        assert_eq!(rows[0].label, "utun3 (not connected)");
        let same = AdapterChoice::of(&now);
        let rows = adapter_rows(ctx().lang, &[now], Some(&same));
        assert!(rows[0].up && !rows[0].renewable);
    }

    #[test]
    fn an_install_reports_its_tool() {
        let mut s = AppState::default();
        s.services.tool_busy.push(Tool::YtDlp);
        let tools = vec![ToolState {
            tool: Tool::YtDlp,
            path: Some("/t/yt-dlp".into()),
            installed: Some("2026.11.01".into()),
            installable: true,
            command: String::new(),
        }];
        let event = Event::Tools { tools, done: Some(Tool::YtDlp), error: None };
        apply(&mut s, UiMessage::Daemon { event: Box::new(event), at: Instant::now(), unix: 0 });
        assert!(s.services.tool_busy.is_empty());
        let mut view = crate::options::view(ctx().lang, &udm_domain::settings::Settings::default());
        fill_view(&ctx(), &s, &mut view);
        assert_eq!(view.tools[0].status, "Installed by UDM (2026.11.01): /t/yt-dlp");
        assert_eq!(view.tools[0].action, "Update yt-dlp");
        assert_eq!(view.tool_note.map(|n| n.text).as_deref(), Some("yt-dlp is installed."));
    }

    #[test]
    fn check_account_never_waits_for_a_daemon_that_went() {
        use udm_domain::debrid::{DebridError, DebridService};
        let daemon = |s: &mut AppState, event: Event| {
            apply(s, UiMessage::Daemon { event: Box::new(event), at: Instant::now(), unix: 0 })
        };
        let shown = |s: &AppState| {
            let mut view = crate::options::view(ctx().lang, &udm_domain::settings::Settings::default());
            fill_view(&ctx(), s, &mut view);
            (view.debrid.checking, view.debrid.check.map(|b| (b.text, b.error)))
        };
        let mut s = AppState::default();
        apply(&mut s, UiMessage::DebridChecking(DebridService::AllDebrid));
        assert_eq!(shown(&s), (true, None));
        daemon(&mut s, Event::Lost(udm_application::daemon::Lost::Closed));
        let unanswered = "UDM's background service did not answer. Try again.".to_string();
        assert_eq!(shown(&s), (false, Some((unanswered.clone(), true))));
        apply(&mut s, UiMessage::DebridChecking(DebridService::AllDebrid));
        daemon(&mut s, Event::Error { error: Failure::InvalidMessage("expected value".into()) });
        assert_eq!(shown(&s), (false, Some((unanswered, true))));
        let check = DebridCheck {
            service: DebridService::AllDebrid,
            outcome: Err(DebridError::KeyForOther(DebridService::RealDebrid)),
        };
        daemon(&mut s, Event::Debrid { check });
        let refused = "AllDebrid: The stored key is for Real-Debrid. Type a key for this service.".to_string();
        assert_eq!(shown(&s), (false, Some((refused, true))));
        // An error that is not about a check leaves the answer alone.
        daemon(&mut s, Event::Error { error: Failure::Other("x".into()) });
        assert!(shown(&s).1.is_some());
        // Options opened again starts without the old answer.
        apply(&mut s, UiMessage::OptionsOpened);
        assert_eq!(shown(&s), (false, None));
    }

    #[test]
    fn a_refused_account_shows_in_the_main_window_and_options() {
        use udm_domain::debrid::{DebridError, DebridService, Refused};
        let mut s = AppState::default();
        let refused = Refused { service: DebridService::AllDebrid, error: DebridError::BadKey };
        let event = Event::DebridRefused { refused };
        apply(&mut s, UiMessage::Daemon { event: Box::new(event), at: Instant::now(), unix: 0 });
        let notice = s.notice.clone().unwrap();
        assert!(notice.error);
        assert_eq!(
            notice.text,
            "AllDebrid refused the debrid account: The service refused the API key, or no key is stored. \
             Downloads go on without it."
        );
        let mut view = crate::options::view(ctx().lang, &udm_domain::settings::Settings::default());
        fill_view(&ctx(), &s, &mut view);
        assert!(view.debrid.check.is_some_and(|b| b.error && b.text.starts_with("AllDebrid: ")));
    }

    #[test]
    fn a_pairing_code_comes_with_its_qr_code() {
        let (modules, side) = qr("https://192.168.1.5:8765/#pair=ABCD").unwrap();
        assert_eq!(modules.len() as u32, side * side);
        assert!(side >= 21);
    }

    #[test]
    fn custom_themes_join_the_list() {
        let s = AppState {
            settings: Some(udm_domain::settings::Settings {
                theme: Theme::Custom(udm_domain::theme::ThemeName::parse("Gone").unwrap()),
                ..Default::default()
            }),
            ..AppState::default()
        };
        let choices = theme_choices(ctx().lang, &s);
        assert_eq!(choices.last().map(|c| c.value.as_str()), Some("custom:Gone"));
    }

    #[test]
    fn the_missing_tools_are_offered_until_answered_and_closed() {
        let lang = crate::i18n::resolve("en", &[]);
        let tool = |tool, path: Option<&str>, installable| ToolState {
            tool,
            path: path.map(str::to_string),
            installed: None,
            installable,
            command: "sudo apt install ffmpeg".into(),
        };
        let daemon =
            |s: &mut AppState, e| apply(s, UiMessage::Daemon { event: Box::new(e), at: Instant::now(), unix: 0 });
        let mut s = AppState::default();
        daemon(&mut s, Event::Connected);
        assert_eq!(missing_tools(lang, &s), None, "the daemon has not said yet");
        let tools = vec![
            tool(Tool::YtDlp, None, true),
            tool(Tool::Ffmpeg, None, false),
            tool(Tool::GalleryDl, Some("/usr/bin/gallery-dl"), true),
        ];
        daemon(&mut s, Event::Tools { tools, done: None, error: None });
        let offer = missing_tools(lang, &s).unwrap();
        assert_eq!(offer.text, "Some downloads need tools UDM did not find: yt-dlp, ffmpeg.");
        assert!(offer.ask && offer.strip && !offer.busy);
        assert_eq!(offer.tools.iter().map(|t| t.tool).collect::<Vec<_>>(), [ToolKey::YtDlp, ToolKey::Ffmpeg]);
        assert!(offer.tools[1].hint.contains("sudo apt install ffmpeg"));

        s.layout_rest.insert(TOOLS_ASKED.into(), true.into());
        s.layout_rest.insert(TOOLS_DISMISSED.into(), missing_names(&s.services).into());
        let offer = missing_tools(lang, &s).unwrap();
        assert!(!offer.ask && !offer.strip);
        // Another tool going missing brings the strip back.
        let tools =
            vec![tool(Tool::YtDlp, None, true), tool(Tool::Ffmpeg, None, false), tool(Tool::GalleryDl, None, true)];
        daemon(&mut s, Event::Tools { tools, done: None, error: None });
        assert!(missing_tools(lang, &s).unwrap().strip);

        apply(&mut s, UiMessage::ToolInstalling(Tool::YtDlp));
        let offer = missing_tools(lang, &s).unwrap();
        assert!(offer.busy);
        assert_eq!(offer.busy_text, "Installing...", "nothing heard yet");
        assert_eq!(offer.tools[0].progress, None);
        daemon(&mut s, Event::Lost(udm_application::daemon::Lost::Closed));
        assert_eq!(missing_tools(lang, &s), None);
        assert!(s.services.tool_busy.is_empty() && s.services.tool_progress.is_empty(), "the installs went too");
    }

    #[test]
    fn an_install_shows_how_far_its_download_is() {
        let lang = ctx().lang;
        let tool = |tool, path: Option<&str>, installable| ToolState {
            tool,
            path: path.map(str::to_string),
            installed: None,
            installable,
            command: String::new(),
        };
        let daemon =
            |s: &mut AppState, e| apply(s, UiMessage::Daemon { event: Box::new(e), at: Instant::now(), unix: 0 });
        let mut s = AppState::default();
        daemon(&mut s, Event::Connected);
        let tools = vec![tool(Tool::YtDlp, None, true), tool(Tool::GalleryDl, None, true)];
        daemon(&mut s, Event::Tools { tools, done: None, error: None });
        let mib = |n: u64| ByteCount::new(n * 1024 * 1024);
        // Progress for an install the window did not start is not shown.
        daemon(&mut s, Event::ToolProgress { tool: Tool::YtDlp, done: mib(1), total: Some(mib(4)) });
        assert!(s.services.tool_progress.is_empty());

        apply(&mut s, UiMessage::ToolInstalling(Tool::YtDlp));
        apply(&mut s, UiMessage::ToolInstalling(Tool::GalleryDl));
        daemon(&mut s, Event::ToolProgress { tool: Tool::YtDlp, done: mib(1), total: Some(mib(4)) });
        let offer = missing_tools(lang, &s).unwrap();
        let progress = offer.tools[0].progress.clone().unwrap();
        assert_eq!(progress.text, "1.0 MB of 4.0 MB (25%)");
        assert_eq!(progress.percent, Some(25));
        assert_eq!(offer.busy_text, "Installing... 25%");
        let mut view = crate::options::view(lang, &udm_domain::settings::Settings::default());
        fill_view(&ctx(), &s, &mut view);
        assert_eq!(view.tools.iter().find(|t| t.tool == ToolKey::YtDlp).unwrap().progress, Some(progress));

        // A server that gave no size: bytes only, and the strip counts what it can.
        daemon(&mut s, Event::ToolProgress { tool: Tool::GalleryDl, done: mib(2), total: None });
        let offer = missing_tools(lang, &s).unwrap();
        let gallery = offer.tools[1].progress.clone().unwrap();
        assert_eq!((gallery.text.as_str(), gallery.percent), ("2.0 MB downloaded", None));
        assert_eq!(offer.busy_text, "Installing... 25%");

        // All of it came: it is being checked and put in place.
        daemon(&mut s, Event::ToolProgress { tool: Tool::YtDlp, done: mib(4), total: Some(mib(4)) });
        let done = missing_tools(lang, &s).unwrap().tools[0].progress.clone().unwrap();
        assert_eq!((done.text.as_str(), done.percent), ("Checking and installing...", Some(100)));

        // The install's answer ends it.
        let tools = vec![tool(Tool::YtDlp, Some("/t/yt-dlp"), true), tool(Tool::GalleryDl, None, true)];
        daemon(&mut s, Event::Tools { tools, done: Some(Tool::YtDlp), error: None });
        assert!(s.services.tool_progress.iter().all(|(t, _, _)| *t != Tool::YtDlp));
        daemon(&mut s, Event::ToolProgress { tool: Tool::YtDlp, done: mib(4), total: Some(mib(4)) });
        assert!(s.services.tool_progress.iter().all(|(t, _, _)| *t != Tool::YtDlp), "a late one is dropped");
    }
}
