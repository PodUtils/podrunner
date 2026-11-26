//! The man page of the binary under the renderers (T-645): mandoc warns
//! on a `.TH` with no date, and the page of a release is the page of
//! `podrunner man --roff`.

use std::process::{Command, Stdio};

use pod_manual::roff;

use super::{manual, page_date};

/// The build knows a date (a git checkout or `SOURCE_DATE_EPOCH`): its
/// script gives an empty value when it does not.
fn dated_build() -> bool {
    option_env!("PODRUNNER_BUILD_EPOCH") != Some("")
}

/// The date of the `.TH` line of a page: its first quoted word.
fn date_of(page: &str) -> &str {
    page.lines().next().and_then(|l| l.split('"').nth(1)).unwrap_or("")
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10 && b.iter().enumerate().all(|(i, c)| if matches!(i, 4 | 7) { *c == b'-' } else { c.is_ascii_digit() })
}

#[test]
fn the_page_of_the_binary_has_a_date() {
    let page = roff::render(manual());
    assert!(page.starts_with(".TH PODRUNNER 1 "), "{}", page.lines().next().unwrap_or(""));
    if dated_build() {
        assert!(is_date(date_of(&page)), "mandoc needs a date: {}", page.lines().next().unwrap_or(""));
    }
}

#[test]
fn the_date_of_the_page_is_the_date_of_the_build() {
    // The edges: no value, no number, the epoch, a day and the second
    // before it.
    assert_eq!(page_date(""), "");
    assert_eq!(page_date("none"), "");
    assert_eq!(page_date("-1"), "");
    assert_eq!(page_date("0"), "1970-01-01");
    assert_eq!(page_date("1795651200"), "2026-11-26");
    assert_eq!(page_date("1795651199"), "2026-11-25");
    assert_eq!(page_date(" 1795651200\n"), "2026-11-26");
}

/// Run a renderer on a page; `None` when it is not installed.
fn render(tool: &str, args: &[&str], page: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("podrunner-page-{}-{tool}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let file = dir.join("page.1");
    let out = std::fs::write(&file, page)
        .ok()
        .and_then(|()| Command::new(tool).args(args).arg(&file).stdin(Stdio::null()).output().ok());
    let _ = std::fs::remove_dir_all(&dir);
    let out = out?;
    Some(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

/// With mandoc and groff on the PATH, the page gives no warning. The gate
/// installs both and runs `check-man.sh` of podcrates too.
#[test]
fn the_page_has_no_warning_of_mandoc_or_groff() {
    let page = roff::render(manual());
    let lint = ["-Tlint", "-W", "warning"];
    if let Some(out) = render("mandoc", &lint, &page) {
        assert!(!dated_build() || out.trim().is_empty(), "mandoc warns: {out}");
        // The planted defect: a page with no date is a warning.
        let planted = page.replacen(&format!("\"{}\"", date_of(&page)), "\"\"", 1);
        let out = render("mandoc", &lint, &planted).unwrap_or_default();
        assert!(out.contains("missing date"), "mandoc says nothing of a missing date: {out}");
    }
    if let Some(err) = render("groff", &["-man", "-Tascii", "-ww", "-z"], &page) {
        assert!(err.trim().is_empty(), "groff warns: {err}");
    }
}
