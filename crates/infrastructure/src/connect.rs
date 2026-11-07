//! The proxy and logins a download connects with (`config_for`), from the
//! settings, the download's own login and the proxy its browser reported.

use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard, PoisonError};

use uidm_domain::download::Download;

use crate::net::{self, Login, NetConfig, ProxyKind, ProxyLogin};
use uidm_domain::settings::{ProxyMode, Settings, SocksVersion};

/// Addresses retried through the browser's proxy after failing through
/// UIDM's own proxy setting; they keep using it.
#[derive(Default)]
pub struct ProxyRetries(Mutex<HashSet<String>>);

impl ProxyRetries {
    /// Only single inserts change the set, so a lock poisoned by a
    /// panicking thread still holds a usable one.
    fn urls(&self) -> MutexGuard<'_, HashSet<String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `url` was retried through the browser's proxy.
    pub fn contains(&self, url: &str) -> bool {
        self.urls().contains(url)
    }

    /// Note that `url` is retried through the browser's proxy. Returns false
    /// when it already was, so each address gets one such retry.
    pub fn insert(&self, url: String) -> bool {
        self.urls().insert(url)
    }
}

/// The proxy and logins for `download`. A download from the browser uses
/// the browser's proxy when the settings say so: always, or once it failed
/// through UIDM's own setting (`retries`).
pub fn config_for(download: &Download, s: &Settings, retries: &ProxyRetries) -> NetConfig {
    let browser_wanted = s.use_browser_proxy && (!s.browser_proxy_after_errors || retries.contains(&download.url));
    // The browser's whole setting when it sent one (its direct
    // connection too, as `Some(None)`), else the fixed proxy an older
    // extension sends.
    let browser = match (&download.browser_proxy, download.proxy.as_deref()) {
        _ if !browser_wanted => None,
        (Some(setting), _) => Some(net::browser_proxy_rules(setting)),
        (None, Some(p)) => Some(Some(net::ProxyRules {
            http: p.strip_prefix("http://").map(str::to_string),
            https: p.strip_prefix("http://").map(str::to_string),
            socks: p.starts_with("socks").then(|| p.to_string()),
            ..net::ProxyRules::default()
        })),
        (None, None) => None,
    };
    let addr = |a: &str| (!a.trim().is_empty()).then(|| a.trim().to_string());
    let proxy_login = |user: &str, kind: ProxyKind| {
        if user.trim().is_empty() {
            return ProxyLogin::default();
        }
        let password = net::proxy_password_path(kind).map(|p| net::load_proxy_password(&p)).unwrap_or_default();
        ProxyLogin::new(user.trim(), &password)
    };
    let proxy = browser.unwrap_or_else(|| match s.proxy_mode {
        ProxyMode::None => None,
        ProxyMode::System => net::system_proxy(),
        ProxyMode::Script => Some(net::ProxyRules {
            pac: Some(crate::pac::PacSource::Url(s.proxy_pac_url.trim().to_string())),
            ..net::ProxyRules::default()
        }),
        ProxyMode::Manual => {
            // An empty HTTPS proxy means the HTTP one, with its login.
            let (https, https_login) = match addr(&s.proxy_https) {
                Some(a) => (Some(a), proxy_login(&s.proxy_https_username, ProxyKind::Https)),
                None => (addr(&s.proxy_http), proxy_login(&s.proxy_username, ProxyKind::Http)),
            };
            Some(net::ProxyRules {
                http: addr(&s.proxy_http),
                https,
                socks: addr(&s.proxy_socks5).map(|a| {
                    let scheme = net::socks_scheme(s.proxy_socks_version == SocksVersion::V5, s.proxy_remote_dns);
                    format!("{scheme}://{a}")
                }),
                // ftp:// goes through `NetConfig::ftp_proxy`, below.
                ftp: None,
                bypass: net::parse_bypass(&s.proxy_bypass),
                http_login: proxy_login(&s.proxy_username, ProxyKind::Http),
                https_login,
                socks_login: proxy_login(&s.proxy_socks_username, ProxyKind::Socks),
                socks_for_https: s.proxy_socks_https,
                socks_for_ftp: s.proxy_socks_ftp,
                pac: None,
            })
        }
    });
    let manual_ftp = (s.proxy_mode == ProxyMode::Manual).then(|| addr(&s.proxy_ftp)).flatten();
    let mut logins: Vec<Login> = s
        .site_logins
        .iter()
        .filter_map(|l| {
            Some(Login {
                site: l.site.trim().to_string(),
                username: l.username.clone(),
                password: net::load_login_password(&l.site)?,
                plain: l.plain,
            })
        })
        .collect();
    // The download's own login takes the place of the site logins for
    // its host, even those for one of its folders.
    let host = url::Url::parse(&download.url).ok().and_then(|u| u.host_str().map(str::to_string));
    if let (Some(l), Some(host)) = (&download.login, host) {
        logins.retain(|s| !uidm_domain::site::site_matches(uidm_domain::site::split_login_pattern(&s.site).0, &host));
        logins.push(Login {
            site: host,
            username: l.username.clone(),
            password: l.password.clone().unwrap_or_default(),
            // Typed for this download, so meant for its address.
            plain: true,
        });
    }
    NetConfig {
        proxy,
        logins,
        ftp_active: s.ftp_active,
        ftp_proxy_login: manual_ftp
            .as_ref()
            .map_or_else(ProxyLogin::default, |_| proxy_login(&s.proxy_ftp_username, ProxyKind::Ftp)),
        ftp_proxy: manual_ftp,
    }
}

impl uidm_domain::ports::ProxyFallback for ProxyRetries {
    fn claim_retry(&self, url: String) -> bool {
        self.insert(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browsers_proxy_can_wait_for_a_failure() {
        let d = Download { url: "https://x/a.zip".into(), proxy: Some("http://bp:3128".into()), ..Download::default() };
        let retries = ProxyRetries::default();
        let through = |s: &Settings| config_for(&d, s, &retries).proxy.and_then(|p| p.proxy_for(&d.url));
        let after = Settings { use_browser_proxy: true, browser_proxy_after_errors: true, ..Settings::default() };
        assert_eq!(through(&after), None, "not before it failed");
        let always = Settings { browser_proxy_after_errors: false, ..after.clone() };
        assert_eq!(through(&always).as_deref(), Some("http://bp:3128"));
        assert!(retries.insert(d.url.clone()));
        assert!(!retries.insert(d.url.clone()), "one retry each");
        assert_eq!(through(&after).as_deref(), Some("http://bp:3128"), "once it failed");
    }

    #[test]
    fn the_browsers_whole_proxy_setting_is_used() {
        use uidm_domain::download::{BrowserProxy, BrowserProxyMode};
        let s = Settings {
            use_browser_proxy: true,
            proxy_mode: ProxyMode::Manual,
            proxy_http: "uidm:8080".into(),
            ..Settings::default()
        };
        let through = |setting: Option<BrowserProxy>, s: &Settings| {
            let d = Download {
                url: "http://x.net/a.zip".into(),
                proxy: Some("http://old:1".into()),
                browser_proxy: setting,
                ..Download::default()
            };
            config_for(&d, s, &ProxyRetries::default()).proxy.and_then(|p| p.proxy_for(&d.url))
        };
        // The browser connects directly: so does the download, whatever
        // UIDM's own setting and the older field say.
        let direct = BrowserProxy::default();
        assert_eq!(through(Some(direct.clone()), &s), None);
        let fixed = BrowserProxy {
            mode: BrowserProxyMode::Fixed,
            http: Some("http://bp:3128".into()),
            bypass: vec!["x.net".into()],
            ..BrowserProxy::default()
        };
        assert_eq!(through(Some(fixed.clone()), &s), None, "the browser's bypass list");
        let fixed = BrowserProxy { bypass: vec![], ..fixed };
        assert_eq!(through(Some(fixed), &s).as_deref(), Some("http://bp:3128"));
        // Older extensions send only the fixed proxy.
        assert_eq!(through(None, &s).as_deref(), Some("http://old:1"));
        // Without the option, UIDM's own setting applies.
        let own = Settings { use_browser_proxy: false, ..s.clone() };
        assert_eq!(through(Some(direct), &own).as_deref(), Some("http://uidm:8080"));
    }

    #[test]
    fn manual_socks_can_carry_https_and_ftp() {
        let s = Settings {
            proxy_mode: ProxyMode::Manual,
            proxy_http: "web:8080".into(),
            proxy_socks5: "sock:1080".into(),
            proxy_socks_https: true,
            ..Settings::default()
        };
        let net = |url: &str, s: &Settings| {
            config_for(&Download { url: url.into(), ..Download::default() }, s, &ProxyRetries::default())
        };
        let via = |url: &str, s: &Settings| net(url, s).proxy.and_then(|p| p.proxy_for(url));
        assert_eq!(via("http://x.net/a.zip", &s).as_deref(), Some("http://web:8080"));
        assert_eq!(via("https://x.net/a.zip", &s).as_deref(), Some("socks5://sock:1080"));
        let off = Settings { proxy_socks_https: false, ..s.clone() };
        assert_eq!(
            via("https://x.net/a.zip", &off).as_deref(),
            Some("http://web:8080"),
            "an empty HTTPS proxy is the HTTP one"
        );

        // ftp:// takes the SOCKS proxy (on by default) unless an FTP proxy
        // is set, or it is turned off.
        let ftp = |s: &Settings| net("ftp://x.net/a.zip", s).proxy.and_then(|p| p.socks_for_ftp("ftp://x.net/a.zip"));
        assert_eq!(ftp(&s).map(|p| p.addr).as_deref(), Some("sock:1080"));
        assert_eq!(ftp(&Settings { proxy_socks_ftp: false, ..s.clone() }), None);
        let with_ftp_proxy = Settings { proxy_ftp: "ftpproxy:21".into(), ..s.clone() };
        assert_eq!(net("ftp://x.net/a.zip", &with_ftp_proxy).ftp_proxy.as_deref(), Some("ftpproxy:21"));
    }
}
