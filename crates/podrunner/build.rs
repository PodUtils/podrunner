//! Give the binary the commit of its tree, for `--version`.

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
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
}
