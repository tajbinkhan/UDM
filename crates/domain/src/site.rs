//! Site patterns as the user types them in Options: `example.com`,
//! `*.example.com`, or a login pattern with a path.

/// Whether `host` belongs to the site `pattern`. `example.com` and
/// `*.example.com` both cover example.com and every subdomain; any other `*`
/// matches within the host name. Case and a port in the pattern are ignored.
pub fn site_matches(pattern: &str, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let p = pattern.trim().to_ascii_lowercase();
    let p = match p.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h.to_string(),
        _ => p,
    };
    if p.is_empty() || host.is_empty() {
        return false;
    }
    if let Some(base) = p.strip_prefix("*.") {
        if !base.contains('*') {
            return host == base || host.ends_with(&format!(".{base}"));
        }
    }
    if p.contains('*') {
        return glob(&p, &host);
    }
    host == p || host.ends_with(&format!(".{p}"))
}

/// `*` matches any run of characters; everything else matches itself.
pub fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = text;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(at) => rest = &rest[at + part.len()..],
                None => return false,
            }
        }
    }
    rest.is_empty()
}

/// The host of a URL, lowercase, without port or credentials: the one a
/// connection to it reaches, because saved logins go by this answer. A host
/// the URL parser reads the same way stays as written, so `bücher.example`
/// still matches the site the user typed.
pub fn host_of(url: &str) -> String {
    let written = written_host(url);
    match connected_host(url) {
        Some(host) if !same_host(&written, &host) => host,
        _ => written,
    }
}

/// The host the URL parser finds in `url`, which connections use.
fn connected_host(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    Some(parsed.host_str()?.trim_matches(['[', ']']).to_ascii_lowercase())
}

/// Whether `written` is `connected` as the user would type it.
fn same_host(written: &str, connected: &str) -> bool {
    written == connected || url::Host::parse(written).is_ok_and(|h| h.to_string().eq_ignore_ascii_case(connected))
}

/// The host in `url`'s text, for text that is not a whole URL too.
fn written_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(v6) = host_port.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    host.to_ascii_lowercase()
}

/// A login pattern split into its site and its path (`""` for none):
/// `example.com/private/*` is `example.com` and `/private/*`.
pub fn split_login_pattern(pattern: &str) -> (&str, &str) {
    let p = pattern.trim();
    match p.find('/') {
        Some(i) => (&p[..i], &p[i..]),
        None => (p, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_is_the_one_a_connection_reaches() {
        assert_eq!(host_of(r"ftp://evil.example\@victim.example/a.zip"), "evil.example");
        assert_eq!(host_of(r"https://evil.example:8443\@victim.example/a.zip"), "evil.example");
        // `ftps` is not one of the schemes where a backslash ends the host.
        assert_eq!(host_of(r"ftps://victim.example\@evil.example/a.zip"), "evil.example");
    }

    #[test]
    fn a_host_is_kept_as_written_when_it_is_the_same_host() {
        assert_eq!(host_of("https://user:pw@Files.Example.com:8443/a?b#c"), "files.example.com");
        assert_eq!(host_of("https://bücher.example/a.zip"), "bücher.example");
        assert_eq!(host_of("http://[::1]:8080/"), "::1");
        assert_eq!(host_of("ftps://Files.Example.com/a.zip"), "files.example.com");
        assert_eq!(host_of("files.example.com/a.zip"), "files.example.com");
        assert_eq!(host_of("localhost:8080/a.zip"), "localhost");
    }
}
