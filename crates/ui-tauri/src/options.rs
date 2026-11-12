//! The Options window's draft: made from the settings, and turned back into
//! settings when the window is saved. Settings the window does not show
//! keep their values. Plain functions, tested without Tauri.

use udm_domain::id::QueueId;
use udm_domain::settings::{
    default_capture_types, parse_extensions, CaptureKey, Category, Duplicate, InvalidSettings, Notify, PanelView,
    ProxyMode, Scanner, ServerLimit, Settings, SiteLogin, SocksVersion, Theme, BROWSER_MENU_ITEMS, CONNECTIONS_RANGE,
    GENERAL, SERVER_CONNECTIONS_RANGE,
};
use udm_domain::theme::ThemeName;

use crate::contract::{
    CategoryDraft, Choice, DuplicateChoice, KeysDraft, LoginDraft, OptionsDraft, OptionsView, ProxyChoice, ProxyDraft,
    ServerLimitDraft, SoundDraft, VideoTypeDraft,
};
use crate::i18n::{Language, LANGUAGES};

/// Why the window's settings cannot be saved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OptionsError {
    #[error(transparent)]
    Invalid(#[from] InvalidSettings),
    #[error("Type a name for each category.")]
    CategoryName,
    #[error("There are two categories named \"{0}\".")]
    CategoryTwice(String),
    #[error("\"{0}\" is not a theme.")]
    Theme(String),
}

fn keys(k: CaptureKey) -> KeysDraft {
    KeysDraft { on: !k.is_none(), alt: k.alt, shift: k.shift, ctrl: k.ctrl, insert: k.insert, delete: k.delete }
}

fn key_of(d: KeysDraft) -> CaptureKey {
    if !d.on {
        return CaptureKey::NONE;
    }
    CaptureKey { alt: d.alt, shift: d.shift, ctrl: d.ctrl, insert: d.insert, delete: d.delete }
}

/// `host:port` as its host and port; a port only when it is digits.
fn split_address(text: &str) -> (String, String) {
    let text = text.trim();
    match text.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) && !host.ends_with(':') => {
            (host.to_string(), port.to_string())
        }
        _ => (text.to_string(), String::new()),
    }
}

fn join_address(host: &str, port: &str) -> String {
    let (host, port) = (host.trim(), port.trim());
    match (host.is_empty(), port.is_empty()) {
        (true, _) => String::new(),
        (false, true) => host.to_string(),
        (false, false) => format!("{host}:{port}"),
    }
}

fn words(list: &[String]) -> String {
    list.join(" ")
}

/// Sites separated by spaces, commas or lines, lowercase.
fn sites(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        let s = s.trim().to_ascii_lowercase();
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

fn theme_key(t: &Theme) -> String {
    match t {
        Theme::System => "system".into(),
        Theme::Light => "light".into(),
        Theme::Dark => "dark".into(),
        Theme::HighContrast => "highContrast".into(),
        Theme::Custom(name) => format!("custom:{}", name.as_str()),
    }
}

fn theme_of(key: &str) -> Result<Theme, OptionsError> {
    Ok(match key {
        "system" => Theme::System,
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        "highContrast" => Theme::HighContrast,
        other => match other.strip_prefix("custom:").map(ThemeName::parse) {
            Some(Ok(name)) => Theme::Custom(name),
            _ => return Err(OptionsError::Theme(other.to_string())),
        },
    })
}

/// The draft the window starts from.
pub fn draft(s: &Settings) -> OptionsDraft {
    let proxy = |address: &str, user: &str| {
        let (host, port) = split_address(address);
        ProxyDraft { on: !host.is_empty(), host, port, user: user.to_string(), password: String::new() }
    };
    let shown = |hidden: &[String]| {
        BROWSER_MENU_ITEMS
            .iter()
            .filter(|(id, _)| !hidden.iter().any(|h| h == id))
            .map(|(id, _)| id.to_string())
            .collect()
    };
    let mut video_types: Vec<VideoTypeDraft> = udm_domain::settings::default_video_types()
        .into_iter()
        .map(|ext| VideoTypeDraft { on: s.video_types.contains(&ext), ext })
        .collect();
    for t in &s.video_types {
        if !video_types.iter().any(|v| &v.ext == t) {
            video_types.push(VideoTypeDraft { ext: t.clone(), on: true });
        }
    }
    OptionsDraft {
        start_with_system: s.start_with_windows,
        tray_icon: s.tray_icon,
        close_to_tray: s.close_to_tray,
        minimize_to_tray: s.minimize_to_tray,
        keep_awake: s.keep_awake,
        check_updates: s.check_updates,
        clipboard_monitor: s.clipboard_monitor,
        clipboard_start: s.clipboard_start,
        capture_private: s.capture_private,
        theme: theme_key(&s.theme),
        language: s.language.clone(),
        skip_key: keys(s.capture_skip_key),
        force_key: keys(s.capture_force_key),
        keys_left_click: s.capture_keys_left_click,
        force_skips_pages: s.capture_force_skips_pages,
        menu_chromium: shown(&s.browser_menu_hidden_chromium),
        menu_firefox: shown(&s.browser_menu_hidden_firefox),
        video_button: s.video_button,
        video_mini: s.video_view == PanelView::Mini,
        video_types,
        video_auto_capture: s.video_auto_capture,
        video_protected_note: s.video_protected_note,
        capture_types: words(&s.capture_types).to_uppercase(),
        capture_excluded_sites: words(&s.capture_excluded_sites),
        capture_excluded_urls: s.capture_excluded_urls.clone(),
        offer_exceptions: s.offer_exceptions,
        download_dir: s.download_dir.clone().unwrap_or_default(),
        general_remember_folder: s.general_remember_folder,
        categories: s
            .categories
            .iter()
            .filter(|c| c.name != GENERAL)
            .map(|c| CategoryDraft {
                original: Some(c.name.clone()),
                name: c.name.clone(),
                extensions: words(&c.extensions),
                sites_on: !c.sites.is_empty(),
                sites: words(&c.sites),
                folder: c.folder.clone().unwrap_or_default(),
                remember_folder: c.remember_folder,
            })
            .collect(),
        server_file_date: s.server_file_date,
        temp_dir: s.temp_dir.clone().unwrap_or_default(),
        ask_where_to_save: s.ask_where_to_save,
        add_only: s.add_only,
        done_dialog: s.notify == Notify::Dialog,
        start_while_asking: s.start_while_asking,
        later_panel: s.later_queue.is_none(),
        list_queue_panel: s.list_queue_panel,
        resume_ignores_date: s.resume_ignores_date,
        if_duplicate: match s.if_duplicate {
            Duplicate::Ask => DuplicateChoice::Ask,
            Duplicate::NewName => DuplicateChoice::NewName,
            Duplicate::Overwrite => DuplicateChoice::Overwrite,
            Duplicate::Existing => DuplicateChoice::Existing,
        },
        user_agent: s.manual_user_agent.clone(),
        progress_on_start: s.progress_on_start,
        progress_speed_tab: s.progress_speed_tab,
        progress_done_tab: s.progress_done_tab,
        progress_hide_buttons: s.progress_hide_buttons,
        progress_start_details: s.progress_start_details,
        scan_after_download: s.scan_after_download,
        scanner_custom: s.scanner == Scanner::Custom,
        scanner_command: s.scanner_command.clone(),
        connections: s.connections as u32,
        server_limits: s
            .server_limits
            .iter()
            .map(|l| ServerLimitDraft { site: l.site.clone(), max: l.max as u32 })
            .collect(),
        quota_enabled: s.quota_enabled,
        quota_mb: s.quota_mb.min(u64::from(u32::MAX)) as u32,
        quota_hours: s.quota_hours.min(u64::from(u32::MAX)) as u32,
        quota_warn: s.quota_warn,
        proxy_mode: match s.proxy_mode {
            ProxyMode::None => ProxyChoice::None,
            ProxyMode::System => ProxyChoice::System,
            ProxyMode::Script => ProxyChoice::Script,
            ProxyMode::Manual => ProxyChoice::Manual,
        },
        browser_proxy: s.use_browser_proxy,
        pac_url: s.proxy_pac_url.clone(),
        proxies: vec![
            proxy(&s.proxy_http, &s.proxy_username),
            proxy(&s.proxy_https, &s.proxy_https_username),
            proxy(&s.proxy_ftp, &s.proxy_ftp_username),
            proxy(&s.proxy_socks5, &s.proxy_socks_username),
        ],
        socks4: s.proxy_socks_version == SocksVersion::V4,
        passive_ftp: !s.ftp_active,
        logins: s
            .site_logins
            .iter()
            .map(|l| LoginDraft {
                site: l.site.clone(),
                user: l.username.clone(),
                password: String::new(),
                stored: true,
            })
            .collect(),
        sounds: vec![
            SoundDraft { on: s.sound_on_done, file: s.sound_done_file.clone() },
            SoundDraft { on: s.sound_on_failed, file: s.sound_failed_file.clone() },
            SoundDraft { on: s.sound_on_queue_started, file: s.sound_queue_started_file.clone() },
            SoundDraft { on: s.sound_on_queue_done, file: s.sound_queue_file.clone() },
        ],
    }
}

/// The Options window's view.
pub fn view(lang: &'static Language, s: &Settings) -> OptionsView {
    let choice = |value: &str, label: &str| Choice { value: value.to_string(), label: lang.tr(label).to_string() };
    let mut themes = vec![
        choice("system", "System"),
        choice("light", "Light"),
        choice("dark", "Dark"),
        choice("highContrast", "High contrast"),
    ];
    if let Theme::Custom(name) = &s.theme {
        themes.push(Choice { value: theme_key(&s.theme), label: name.as_str().to_string() });
    }
    let mut languages = vec![choice("", "System default")];
    languages.extend(LANGUAGES.iter().map(|l| Choice { value: l.code.to_string(), label: l.name.to_string() }));
    let span = |(lo, hi): (usize, usize)| {
        [1u32, 2, 4, 8, 16, 24, 32].into_iter().filter(move |n| (lo..=hi).contains(&(*n as usize))).collect()
    };
    OptionsView {
        draft: draft(s),
        themes,
        languages,
        menu_items: BROWSER_MENU_ITEMS.iter().map(|(id, label)| choice(id, label)).collect(),
        default_capture_types: words(&default_capture_types()).to_uppercase(),
        default_excluded_sites: words(&Settings::default().capture_excluded_sites),
        connections: span(CONNECTIONS_RANGE),
        server_connections: span(SERVER_CONNECTIONS_RANGE),
    }
}

/// `current` with the draft's values, checked as the daemon checks them.
pub fn settings(d: &OptionsDraft, current: &Settings) -> Result<Settings, OptionsError> {
    let optional = |text: &str| Some(text.trim().to_string()).filter(|t| !t.is_empty());
    let mut s = current.clone();
    s.start_with_windows = d.start_with_system;
    s.tray_icon = d.tray_icon;
    s.close_to_tray = d.close_to_tray;
    s.minimize_to_tray = d.minimize_to_tray;
    s.keep_awake = d.keep_awake;
    s.check_updates = d.check_updates;
    s.clipboard_monitor = d.clipboard_monitor;
    s.clipboard_start = d.clipboard_start;
    s.capture_private = d.capture_private;
    s.theme = theme_of(&d.theme)?;
    s.language = d.language.trim().to_string();
    s.capture_skip_key = key_of(d.skip_key);
    s.capture_force_key = key_of(d.force_key);
    s.capture_keys_left_click = d.keys_left_click;
    s.capture_force_skips_pages = d.force_skips_pages;
    let hidden = |shown: &[String]| {
        BROWSER_MENU_ITEMS
            .iter()
            .filter(|(id, _)| !shown.iter().any(|s| s == id))
            .map(|(id, _)| id.to_string())
            .collect()
    };
    s.browser_menu_hidden_chromium = hidden(&d.menu_chromium);
    s.browser_menu_hidden_firefox = hidden(&d.menu_firefox);
    s.video_button = d.video_button;
    s.video_view = if d.video_mini { PanelView::Mini } else { PanelView::Full };
    s.video_types = d.video_types.iter().filter(|v| v.on).map(|v| v.ext.trim().to_ascii_lowercase()).collect();
    s.video_auto_capture = d.video_auto_capture;
    s.video_protected_note = d.video_protected_note;
    s.capture_types = parse_extensions(&d.capture_types);
    s.capture_excluded_sites = sites(&d.capture_excluded_sites);
    s.capture_excluded_urls =
        d.capture_excluded_urls.iter().map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).collect();
    s.offer_exceptions = d.offer_exceptions;
    s.download_dir = optional(&d.download_dir);
    s.general_remember_folder = d.general_remember_folder;
    if !d.general_remember_folder {
        s.general_last_folder = None;
    }
    // A General entry, which the window does not list, stays as it is.
    let mut categories: Vec<Category> = current.categories.iter().filter(|c| c.name == GENERAL).cloned().collect();
    for c in &d.categories {
        let name = c.name.trim().to_string();
        if name.is_empty() {
            return Err(OptionsError::CategoryName);
        }
        if categories.iter().any(|k| k.name.eq_ignore_ascii_case(&name)) || name.eq_ignore_ascii_case(GENERAL) {
            return Err(OptionsError::CategoryTwice(name));
        }
        let before = c.original.as_deref().and_then(|o| current.category(o));
        categories.push(Category {
            name,
            extensions: parse_extensions(&c.extensions),
            folder: optional(&c.folder),
            sites: if c.sites_on { sites(&c.sites) } else { Vec::new() },
            remember_folder: c.remember_folder,
            last_folder: before.and_then(|b| b.last_folder.clone()).filter(|_| c.remember_folder),
        });
    }
    s.categories = categories;
    s.server_file_date = d.server_file_date;
    s.temp_dir = optional(&d.temp_dir);
    s.ask_where_to_save = d.ask_where_to_save;
    s.add_only = d.add_only;
    s.notify = match (d.done_dialog, current.notify) {
        (true, _) => Notify::Dialog,
        (false, Notify::Dialog) => Notify::Toast,
        (false, other) => other,
    };
    s.start_while_asking = d.start_while_asking;
    s.later_queue = match (d.later_panel, current.later_queue) {
        (true, _) => None,
        (false, Some(q)) => Some(q),
        (false, None) => Some(QueueId::MAIN),
    };
    s.list_queue_panel = d.list_queue_panel;
    s.resume_ignores_date = d.resume_ignores_date;
    s.if_duplicate = match d.if_duplicate {
        DuplicateChoice::Ask => Duplicate::Ask,
        DuplicateChoice::NewName => Duplicate::NewName,
        DuplicateChoice::Overwrite => Duplicate::Overwrite,
        DuplicateChoice::Existing => Duplicate::Existing,
    };
    s.manual_user_agent = d.user_agent.trim().to_string();
    s.progress_on_start = d.progress_on_start;
    s.progress_speed_tab = d.progress_speed_tab;
    s.progress_done_tab = d.progress_done_tab;
    s.progress_hide_buttons = d.progress_hide_buttons;
    s.progress_start_details = d.progress_start_details;
    s.scan_after_download = d.scan_after_download;
    s.scanner = if d.scanner_custom { Scanner::Custom } else { Scanner::Defender };
    s.scanner_command = d.scanner_command.trim().to_string();
    s.connections = d.connections as usize;
    s.server_limits = d
        .server_limits
        .iter()
        .map(|l| ServerLimit { site: l.site.trim().to_ascii_lowercase(), max: l.max as usize })
        .collect();
    s.quota_enabled = d.quota_enabled;
    s.quota_mb = u64::from(d.quota_mb);
    s.quota_hours = u64::from(d.quota_hours);
    s.quota_warn = d.quota_warn;
    s.proxy_mode = match d.proxy_mode {
        ProxyChoice::None => ProxyMode::None,
        ProxyChoice::System => ProxyMode::System,
        ProxyChoice::Script => ProxyMode::Script,
        ProxyChoice::Manual => ProxyMode::Manual,
    };
    s.use_browser_proxy = d.browser_proxy;
    s.proxy_pac_url = d.pac_url.trim().to_string();
    // The HTTP proxy's server stands in for HTTPS and FTP when they are on
    // without one of their own, as the tab's protocol boxes mean.
    let empty = ProxyDraft {
        on: false,
        host: String::new(),
        port: String::new(),
        user: String::new(),
        password: String::new(),
    };
    let entry = |i: usize| d.proxies.get(i).unwrap_or(&empty);
    let http = entry(0);
    let filled = |p: &ProxyDraft| if p.on && p.host.trim().is_empty() { http.clone() } else { p.clone() };
    let address = |p: &ProxyDraft| if p.on { join_address(&p.host, &p.port) } else { String::new() };
    let password = |p: &ProxyDraft| Some(p.password.clone()).filter(|pw| !pw.is_empty());
    let (https, ftp, socks) = (filled(entry(1)), filled(entry(2)), entry(3).clone());
    s.proxy_http = address(http);
    s.proxy_username = http.user.trim().to_string();
    s.proxy_password = password(http);
    s.proxy_https = address(&https);
    s.proxy_https_username = https.user.trim().to_string();
    s.proxy_https_password = password(&https);
    s.proxy_ftp = address(&ftp);
    s.proxy_ftp_username = ftp.user.trim().to_string();
    s.proxy_ftp_password = password(&ftp);
    s.proxy_socks5 = address(&socks);
    s.proxy_socks_username = socks.user.trim().to_string();
    s.proxy_socks_password = password(&socks);
    s.proxy_socks_version = if d.socks4 { SocksVersion::V4 } else { SocksVersion::V5 };
    s.ftp_active = !d.passive_ftp;
    s.site_logins = d
        .logins
        .iter()
        .filter(|l| !l.site.trim().is_empty())
        .map(|l| {
            let plain = current
                .site_logins
                .iter()
                .find(|o| o.site.eq_ignore_ascii_case(l.site.trim()))
                .is_some_and(|o| o.plain);
            SiteLogin {
                site: l.site.trim().to_ascii_lowercase(),
                username: l.user.trim().to_string(),
                password: Some(l.password.clone()).filter(|p| !p.is_empty()),
                plain,
            }
        })
        .collect();
    let sound = |i: usize| d.sounds.get(i).cloned().unwrap_or(SoundDraft { on: false, file: String::new() });
    let (done, failed, started, finished) = (sound(0), sound(1), sound(2), sound(3));
    s.sound_on_done = done.on;
    s.sound_done_file = done.file.trim().to_string();
    s.sound_on_failed = failed.on;
    s.sound_failed_file = failed.file.trim().to_string();
    s.sound_on_queue_started = started.on;
    s.sound_queue_started_file = started.file.trim().to_string();
    s.sound_on_queue_done = finished.on;
    s.sound_queue_file = finished.file.trim().to_string();
    s.validate()?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::resolve;

    #[test]
    fn an_unchanged_draft_keeps_the_settings() {
        let s = Settings::default();
        assert_eq!(settings(&draft(&s), &s).unwrap(), s);
    }

    #[test]
    fn changes_reach_the_settings() {
        let s = Settings::default();
        let mut d = draft(&s);
        d.theme = "dark".into();
        d.connections = 4;
        d.done_dialog = true;
        d.capture_types = "ZIP  .Rar 7z".into();
        d.proxy_mode = ProxyChoice::Manual;
        d.proxies[0] = ProxyDraft {
            on: true,
            host: "proxy".into(),
            port: "3128".into(),
            user: "me".into(),
            password: "pw".into(),
        };
        d.proxies[1].on = true;
        let out = settings(&d, &s).unwrap();
        assert_eq!(out.theme, Theme::Dark);
        assert_eq!(out.connections, 4);
        assert_eq!(out.notify, Notify::Dialog);
        assert_eq!(out.capture_types, ["zip", "rar", "7z"]);
        assert_eq!((out.proxy_http.as_str(), out.proxy_https.as_str()), ("proxy:3128", "proxy:3128"));
        assert_eq!(out.proxy_password.as_deref(), Some("pw"));
    }

    #[test]
    fn categories_keep_their_last_folder_when_renamed() {
        let mut s = Settings::default();
        s.categories[0].remember_folder = true;
        s.categories[0].last_folder = Some("/tmp/x".into());
        let mut d = draft(&s);
        d.categories[0].name = "Archives".into();
        let out = settings(&d, &s).unwrap();
        assert_eq!(out.categories[0].name, "Archives");
        assert_eq!(out.categories[0].last_folder.as_deref(), Some("/tmp/x"));
        d.categories[1].name = "archives".into();
        assert_eq!(settings(&d, &s), Err(OptionsError::CategoryTwice("archives".into())));
    }

    #[test]
    fn the_settings_checks_apply() {
        let s = Settings::default();
        let mut d = draft(&s);
        d.proxy_mode = ProxyChoice::Script;
        d.pac_url = "not a url".into();
        assert_eq!(settings(&d, &s), Err(OptionsError::Invalid(InvalidSettings::ScriptNotWeb)));
    }

    #[test]
    fn addresses_split_into_host_and_port() {
        assert_eq!(split_address("proxy.lan:8080"), ("proxy.lan".into(), "8080".into()));
        assert_eq!(split_address("proxy.lan"), ("proxy.lan".into(), String::new()));
        assert_eq!(join_address(" h ", ""), "h");
    }

    #[test]
    fn the_view_lists_choices() {
        let v = view(resolve("en", &[]), &Settings::default());
        assert_eq!(v.themes.len(), 4);
        assert_eq!(v.languages[0].value, "");
        assert!(v.connections.contains(&8) && !v.connections.contains(&32));
    }
}
