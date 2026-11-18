//! `config.v1` (T-479): the schema of `podrunner.toml` in `schemas/` is
//! generated from the types of the configuration. A program that writes the
//! file (githome's renderer) checks its output with it;
//! `scripts/check-config-schema.py` validates the example file and the
//! blocks of `docs/configuration.md` against it. `podrunner check` stays
//! the last word: a floor, the pin list and the modes of the files are
//! not in the schema.

use crate::config::Config;

#[test]
fn the_config_schema_is_generated_from_the_types() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/config.v1.json");
    let mut s = schemars::schema_for!(Config);
    s.insert("$id".into(), "https://github.com/PodUtils/podrunner/schemas/config.v1.json".into());
    s.insert(
        "description".into(),
        "The keys of podrunner.toml (docs/configuration.md). `podrunner check --config FILE` is the last word: \
         the floors, the pin list and the modes of the files are not in this schema."
            .into(),
    );
    let text = serde_json::to_string_pretty(&s).unwrap() + "\n";
    if std::env::var_os("PODRUNNER_WRITE_SCHEMAS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    assert!(std::fs::read_to_string(&path).unwrap_or_default() == text, "{} differs from the types", path.display());
}
