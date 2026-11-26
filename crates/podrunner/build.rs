//! Give the binary the commit of its tree and the version of its release,
//! for `--version`.

use std::process::Command;

fn main() {
    // CI sets PODRUNNER_COMMIT; a local build asks git; a tree with no git
    // gives "unknown".
    let commit = std::env::var("PODRUNNER_COMMIT").ok().filter(|c| !c.is_empty()).or_else(|| {
        let out = Command::new("git").args(["rev-parse", "--short=12", "HEAD"]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    });
    println!("cargo:rustc-env=PODRUNNER_COMMIT={}", commit.unwrap_or_else(|| "unknown".into()));
    println!("cargo:rerun-if-env-changed=PODRUNNER_COMMIT");
    // The date of the man page (mandoc warns on a header with no date): the
    // reproducible-build time, else the commit time. The value is empty
    // when there is none.
    let epoch = std::env::var("SOURCE_DATE_EPOCH").ok().filter(|e| !e.is_empty()).or_else(|| {
        let out = Command::new("git").args(["log", "-1", "--format=%ct", "HEAD"]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    });
    println!("cargo:rustc-env=PODRUNNER_BUILD_EPOCH={}", epoch.unwrap_or_default());
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    // The release sets PODRUNNER_VERSION to the version of its tag
    // (`0.1.0-beta.6`); it must name the version of Cargo.toml (T-173).
    let pkg = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let version = match std::env::var("PODRUNNER_VERSION").ok().filter(|v| !v.is_empty()) {
        Some(v) if v == pkg || v.starts_with(&format!("{pkg}-")) => v,
        Some(v) => panic!("PODRUNNER_VERSION {v} does not name the version {pkg} of Cargo.toml"),
        None => pkg,
    };
    // The help links to the documents of the tag of a release, and to main
    // for a local build (T-323 F9).
    let tagged = std::env::var("PODRUNNER_VERSION").is_ok_and(|v| !v.is_empty());
    println!("cargo:rustc-env=PODRUNNER_DOC_REF={}", if tagged { format!("v{version}") } else { "main".into() });
    println!("cargo:rustc-env=PODRUNNER_VERSION={version}");
    println!("cargo:rerun-if-env-changed=PODRUNNER_VERSION");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
}
