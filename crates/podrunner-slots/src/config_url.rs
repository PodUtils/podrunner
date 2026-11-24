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
    if host.is_empty() || !host.bytes().all(|c| c.is_ascii_alphanumeric() || b".-:[]".contains(&c)) {
        return Err(not());
    }
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    if segments.len() > 2 || segments[0].eq_ignore_ascii_case("enterprises") || !segments.iter().all(|s| name_ok(s)) {
        return Err(not());
    }
    if let [_, repo] = segments.as_slice()
        && repo.len() > 4
        && repo[repo.len() - 4..].eq_ignore_ascii_case(".git")
    {
        return Err(format!("config_url {url:?}: give the URL of the repository with no .git"));
    }
    Ok(Parts { scheme, host, segments })
}

/// The REST API that `actions/scaleset` sends its calls to for a config
/// URL (`gitHubAPIURL` of its `config.go`): `api.HOST` on github.com and on
/// `*.ghe.com`, else `HOST/api/v3` (GHES). The sidecar inherits the
/// environment, so `GITHUB_ACTIONS_FORCE_GHES` counts here too.
fn api_of(p: &Parts) -> String {
    let (scheme, host) = (p.scheme.to_ascii_lowercase(), p.host);
    let hosted = std::env::var_os("GITHUB_ACTIONS_FORCE_GHES").is_none()
        && (["github.com", "www.github.com", "github.localhost"].iter().any(|h| host.eq_ignore_ascii_case(h))
            || host.ends_with(".ghe.com"));
    match hosted {
        true if host.eq_ignore_ascii_case("www.github.com") => format!("{scheme}://api.github.com"),
        true => format!("{scheme}://api.{host}"),
        false => format!("{scheme}://{host}/api/v3"),
    }
}

/// The `-api-url` of the sidecar: `api_url`, else the API of `config_url`.
pub fn api_url(s: &SourceConfig) -> String {
    let url = s.config_url.as_deref().unwrap_or_default();
    match (&s.api_url, parts(url)) {
        (Some(a), _) => a.clone(),
        (None, Ok(p)) => api_of(&p),
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
    let want = api_of(&p);
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
}
