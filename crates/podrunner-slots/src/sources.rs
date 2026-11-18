//! The keys of a `github-scaleset` source: the App (its id, its
//! installation and its key file), or a fine-grained token of the
//! organization (`token_file`, T-174), never both.

use podrunner_source::JobSource;

use crate::config::SourceConfig;

/// One source instance.
pub struct Source {
    pub cfg: SourceConfig,
    pub source: Box<dyn JobSource>,
    /// The source revoked podrunner: it gets no capacity.
    pub revoked: bool,
}

pub(crate) fn check_scaleset(s: &SourceConfig) -> Result<(), String> {
    let app = [
        ("installation_id", s.installation_id.is_some()),
        ("app_id", s.app_id.is_some()),
        ("key_file", s.key_file.is_some()),
    ];
    if s.token_file.is_some()
        && let Some((key, _)) = app.iter().find(|(_, set)| *set)
    {
        return Err(format!("[[source]] {:?}: token_file and {key}: the App or a token, not both", s.name));
    }
    let mut need = vec![
        ("sidecar", s.sidecar.is_some()),
        ("config_url", s.config_url.is_some()),
        ("scale_set", s.scale_set.is_some()),
    ];
    if s.token_file.is_none() {
        need.extend(app);
    }
    match need.iter().find(|(_, set)| !*set) {
        Some((key, _)) => Err(format!("[[source]] {:?}: a github-scaleset source needs {key}", s.name)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    fn file(keys: &str) -> Result<Config, String> {
        Config::parse(&format!(
            "host = \"h\"\n[trust]\norganizations = [{{ login = \"o\" }}]\n[[source]]\nname = \"s\"\n\
             kind = \"github-scaleset\"\ntier = \"trusted\"\narch = \"x86_64\"\nsidecar = \"/bin/true\"\n\
             config_url = \"https://github.com/o\"\nscale_set = \"s\"\n{keys}[pin]\n"
        ))
    }

    #[test]
    fn a_scaleset_source_has_the_app_or_a_token() {
        assert!(file("installation_id = 1\napp_id = \"1\"\nkey_file = \"/k\"\n").is_ok());
        assert!(file("token_file = \"/t\"\n").is_ok());
        assert!(file("").unwrap_err().contains("needs installation_id"));
        let both = file("token_file = \"/t\"\nkey_file = \"/k\"\n").unwrap_err();
        assert!(both.contains("not both"), "{both}");
    }
}
