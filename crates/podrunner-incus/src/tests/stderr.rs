//! COORD-124: an error of podvm keeps whole lines of its stderr. podvm
//! `0.2.0-beta.39` names the free space of a full pool and the need of the
//! image on its last line.

use super::*;

/// The refusal of an image that cannot fit, as PodUtils/podvm#652 gives it.
const REFUSAL: &str = "podvm run: incus tier: the storage pool `default` at /var/lib/incus/storage-pools/default/containers has 330.3 MiB free and the root of the instance needs at least 358.7 MiB: incusd would fill it and fail the copy (TODO/machine.md T-2078, PodUtils/podrunner#346); free space there, or give the default profile a root disk on a pool with room";

/// The line after an answer of incusd over the bound of podvm.
const OVER: &str = "podvm run: incus tier: the answer of incusd was over podvm's bound of 64.0 MiB: where a pool fills, incusd's error holds a line of tar for each file that it could not write (measured), and incusd then removes what it wrote; the storage pool `default` at /var/lib/incus/storage-pools/default/containers has 330.3 MiB free and 102380 free inodes now, and the root of the instance needs at least 8.7 MiB and 401207 inodes (TODO/machine.md T-2078, PodUtils/podrunner#346)";

#[test]
fn an_error_keeps_whole_lines_of_stderr() {
    let first = "podvm run: incus tier: POST /1.0/instances: the response body is larger than allowed";
    let got = tail(format!("warning: a line before\n{first}\n{OVER}\n").as_bytes());
    assert!(got.starts_with("warning: a line before\n"), "{got}");
    assert!(got.ends_with(OVER), "{got}");
    // Each line that does not fit goes whole.
    let long = "x".repeat(900);
    assert_eq!(tail(format!("{long}\n{OVER}").as_bytes()), OVER);
}

#[test]
fn a_long_last_line_keeps_its_end_at_a_character() {
    // A file name of tar in the error of incusd can have any character.
    let line = format!("{}x", "é".repeat(700));
    let got = tail(line.as_bytes());
    assert!(got.len() <= 1024 && got.ends_with("éx"), "{}", got.len());
    assert!(line.ends_with(&got));
}

#[test]
fn a_refused_image_names_the_free_space_and_the_need() {
    let (u, d) = driver("full", "exit 0");
    fs::write(d.join("podvm.refuse"), format!("{REFUSAL}\n")).unwrap();
    let s = spec("u-1");
    let e = u.start(&s).unwrap_err();
    assert_eq!(e.code, UnitCode::UnitStartFailed);
    assert!(e.to_string().contains(&format!("podvm run -d: {REFUSAL}")), "{e}");
    assert!(!u.unit_dir(&s.unit).exists(), "the start removes the directory of the unit");
    let _ = fs::remove_dir_all(&d);
}
