//! A `github-scaleset` source with a fine-grained token of the organization
//! in place of the App (T-174): the sidecar gets the token on its
//! descriptor 3 (`-pat`), never in argv or in the environment.

use std::path::Path;

use pod_core::failure::Class;

use crate::serve::{Failure, fail, private_file};

/// The sidecar of `s`, which names `token_file`.
pub fn scaleset(s: &podrunner_slots::config::SourceConfig) -> Result<podrunner_mint::SidecarSource, Failure> {
    let get = |v: &Option<String>| v.clone().unwrap_or_default();
    let token = private_file(&format!("{}: the token file", s.name), &get(&s.token_file))?;
    let token = String::from_utf8(token)
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && !t.contains(char::is_whitespace))
        .ok_or_else(|| fail(Class::Config, format!("{}: the token file holds no token", s.name)))?;
    let mut args = vec![
        "-config-url".to_string(),
        get(&s.config_url),
        "-api-url".into(),
        podrunner_slots::config_url::api_url(s),
        "-pat".into(),
        "-scale-set".into(),
        get(&s.scale_set),
        "-source".into(),
        s.name.clone(),
        "-arch".into(),
        s.arch.clone(),
    ];
    args.extend(crate::serve::flags(s));
    podrunner_slots::config::check_driver(Path::new(&get(&s.sidecar)))
        .map_err(|e| fail(Class::Config, format!("[[source]] {} sidecar: {e}", s.name)))?;
    Ok(podrunner_mint::SidecarSource::with_token(&get(&s.sidecar), args, token, &format!("sidecar:{}", s.name)))
}
