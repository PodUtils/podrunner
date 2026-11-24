//! The `config_url` of a `github-scaleset` source, read as the sidecar and
//! `actions/scaleset` read it (call C3, T-504): the URL of an organization,
//! or of a repository of a personal account. `check` refuses here what the
//! sidecar would refuse at each start, so a bad URL is one refusal (78),
//! not a loop of restarts (69).

use crate::config::SourceConfig;

/// The parts of a config URL: the scheme, the host and the path segments
/// (an owner, and a repository).
struct Parts<'a> {
    scheme: &'a str,
    host: &'a str,
    segments: Vec<&'a str>,
}

/// A login or a repository name of GitHub: each other byte (an escape, a
/// space, an expression) is no name.
fn name_ok(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

/// A host that Go's `url.Parse` (of the sidecar and of `actions/scaleset`)
/// and a URL parser of the WHATWG both take: a name with `_` or a name
/// that is not ASCII (an IDN) is a host of GHES; an escape, a space and
/// the other marks are not. A port is digits.
fn host_ok(host: &str) -> bool {
    let (name, port) = match host.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((ip, port))
                if !ip.is_empty() && ip.bytes().all(|c| c.is_ascii_hexdigit() || c == b':' || c == b'.') =>
            {
                (ip, port)
            }
            _ => return false,
        },
        None => match host.rfind(':') {
            Some(i) => (&host[..i], &host[i..]),
            None => (host, ""),
        },
    };
    let port_ok = port.is_empty()
        || port.strip_prefix(':').is_some_and(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()));
    !name.is_empty()
        && port_ok
        && name.bytes().all(|c| !c.is_ascii() || c.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:".contains(&c))
}

fn parts(url: &str) -> Result<Parts<'_>, String> {
    let not = || format!("config_url {url:?} is not the URL of an organization or of a repository");
    // A query, a fragment or a user passes a split of the path; the scale
    // set then fails at each start.
    if url.contains(['?', '#']) {
        return Err(format!("config_url {url:?}: give the URL with no query, no fragment and no user"));
    }
    // actions/scaleset trims the slashes of each end first.
    let (scheme, rest) = url.trim_matches('/').split_once("://").ok_or_else(not)?;
    if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
        return Err(not());
    }
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if host.contains('@') {
        return Err(format!("config_url {url:?}: give the URL with no query, no fragment and no user"));
    }
    if !host_ok(host) {
        return Err(not());
    }
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    if segments.len() > 2 || segments[0].eq_ignore_ascii_case("enterprises") || !segments.iter().all(|s| name_ok(s)) {
        return Err(not());
    }
    // The last part is the repository, or the organization when it is
    // the only part: the scale set refuses a `.git` on each (T-641).
    let last = segments[segments.len() - 1];
    if last.len() >= 4 && last[last.len() - 4..].eq_ignore_ascii_case(".git") {
        let of = if segments.len() == 2 { "repository" } else { "organization" };
        return Err(format!("config_url {url:?}: give the URL of the {of} with no .git"));
    }
    Ok(Parts { scheme, host, segments })
}

/// `GITHUB_ACTIONS_FORCE_GHES` of this process: set, also empty, is GHES
/// (`os.LookupEnv` of `actions/scaleset`). The sidecar inherits the
/// environment of `serve`, so the two agree when both read it there.
fn force_ghes() -> bool {
    std::env::var_os("GITHUB_ACTIONS_FORCE_GHES").is_some()
}

/// The REST API that `actions/scaleset` sends its calls to for a config
/// URL (`gitHubAPIURL` of its `config.go`): `api.HOST` on github.com and on
/// `*.ghe.com`, else `HOST/api/v3` (GHES, or each host with `ghes`).
fn api_of(p: &Parts, ghes: bool) -> String {
    let (scheme, host) = (p.scheme.to_ascii_lowercase(), p.host);
    let hosted = !ghes
        && (["github.com", "www.github.com", "github.localhost"].iter().any(|h| host.eq_ignore_ascii_case(h))
            || host.ends_with(".ghe.com"));
    match hosted {
        true if host.eq_ignore_ascii_case("www.github.com") => format!("{scheme}://api.github.com"),
        true => format!("{scheme}://api.{host}"),
        false => format!("{scheme}://{host}/api/v3"),
    }
}

/// The `-api-url` of the sidecar: `api_url`, else the API of `config_url`.
/// A slash at the end goes: `check` compares the API with none.
pub fn api_url(s: &SourceConfig) -> String {
    let url = s.config_url.as_deref().unwrap_or_default();
    match (&s.api_url, parts(url)) {
        (Some(a), _) => a.trim_end_matches('/').to_string(),
        (None, Ok(p)) => api_of(&p, force_ghes()),
        (None, Err(_)) => "https://api.github.com".into(),
    }
}

/// The checks of `config_url` and `api_url` of a `github-scaleset` source.
pub fn check(s: &SourceConfig) -> Result<(), String> {
    let url = s.config_url.as_deref().unwrap_or_default();
    let p = parts(url).map_err(|e| format!("[[source]] {:?}: {e}", s.name))?;
    if s.allow_personal_account && p.segments.len() == 1 {
        return Err(format!(
            "[[source]] {:?}: allow_personal_account needs the URL of a repository of a personal account \
             (https://github.com/OWNER/REPO), not config_url {url:?}: the scale set of a personal account is of \
             one repository",
            s.name
        ));
    }
    let want = api_of(&p, force_ghes());
    if let Some(a) = &s.api_url
        && !a.trim_end_matches('/').eq_ignore_ascii_case(&want)
    {
        return Err(format!(
            "[[source]] {:?}: api_url {a:?} is not the API of config_url {url:?}: actions/scaleset sends its calls \
             to {want:?}, and the sidecar sends its lookups to api_url; remove api_url or set it to {want:?}",
            s.name
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(url: &str, api: Option<&str>) -> SourceConfig {
        let mut v = serde_json::json!({
            "name": "s", "kind": "github-scaleset", "tier": "trusted", "arch": "x86_64", "config_url": url,
        });
        if let Some(a) = api {
            v["api_url"] = a.into();
        }
        serde_json::from_value(v).unwrap()
    }

    /// F2 of the T-504 review: with no `api_url`, the sidecar gets the API
    /// that actions/scaleset derives from `config_url`.
    #[test]
    fn the_api_of_a_config_url_is_the_api_of_actions_scaleset() {
        for (url, want) in [
            ("https://github.com/o", "https://api.github.com"),
            ("https://GitHub.com/o/r/", "https://api.GitHub.com"),
            ("https://www.github.com/o", "https://api.github.com"),
            ("https://tenant.ghe.com/o", "https://api.tenant.ghe.com"),
            ("https://tenant.GHE.COM/o", "https://tenant.GHE.COM/api/v3"),
            ("https://ghes.example/o", "https://ghes.example/api/v3"),
            ("http://127.0.0.1:8080/example-org", "http://127.0.0.1:8080/api/v3"),
            ("https://github.localhost/o", "https://api.github.localhost"),
        ] {
            assert_eq!(api_url(&source(url, None)), want, "{url}");
            assert!(check(&source(url, Some(want))).is_ok(), "{url}");
            assert!(check(&source(url, Some(&format!("{want}/")))).is_ok(), "{url}");
        }
        assert_eq!(api_url(&source("https://github.com/o", Some("https://x"))), "https://x");
        let err = check(&source("https://ghes.example/o", Some("https://api.github.com"))).unwrap_err();
        assert!(err.contains("\"https://ghes.example/api/v3\""), "{err}");
    }

    /// The class of a refusal of `config_url`, as the Go test names it.
    fn refusal(r: &Result<Parts<'_>, String>) -> &'static str {
        match r {
            Ok(_) => "",
            Err(e) if e.contains("no query") => "query",
            Err(e) if e.contains("no .git") => "git",
            Err(e) if e.contains("is not the URL") => "not",
            Err(_) => "other",
        }
    }

    /// F of the second T-504 review: `parts` and the API agree with the
    /// table that the sidecar reads too (`tools/config-url-cases.json`).
    #[test]
    fn the_shared_table_of_config_urls() {
        let table: serde_json::Value =
            serde_json::from_str(include_str!("../../../tools/config-url-cases.json")).unwrap();
        let cases = table["cases"].as_array().unwrap();
        assert!(cases.len() > 30);
        let mut wrong = Vec::new();
        for c in cases {
            let url = c["url"].as_str().unwrap();
            let r = parts(url);
            if refusal(&r) != c["refuse"].as_str().unwrap() {
                wrong.push(format!("{url}: {:?}", r.as_ref().err()));
                continue;
            }
            let Ok(p) = r else { continue };
            if p.segments.len() as u64 != c["parts"].as_u64().unwrap() {
                wrong.push(format!("{url}: {} parts", p.segments.len()));
            }
            for (ghes, key) in [(false, "api"), (true, "ghes_api")] {
                if api_of(&p, ghes) != c[key].as_str().unwrap() {
                    wrong.push(format!("{url}: {key} {}", api_of(&p, ghes)));
                }
            }
            // check takes the URL with no api_url; an api_url with a slash
            // at its end is that API, and the sidecar gets it with none.
            let api = format!("{}/", api_of(&p, false));
            let s = source(url, Some(&api));
            if check(&s).is_err() || api_url(&s) != api_of(&p, false) {
                wrong.push(format!("{url}: check {:?}, api_url {}", check(&s), api_url(&s)));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
