//! T-690: `--mem SIZE` of `run --microvm`.

use super::super::args::parse;

const DIGEST: &str = "r@sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn line(extra: &[&str]) -> Vec<String> {
    let base = ["w.yml", "--microvm", "/nonexistent/podvm", "--images", "/nonexistent/store", "--image", DIGEST];
    base.iter().chain(extra).map(|s| s.to_string()).collect()
}

/// The size of `--mem` goes to the `mem` of the kind, which gives the
/// `--podvm-mem` of each guest; no `--mem` keeps the default of the kind.
#[test]
fn the_memory_of_a_guest_goes_to_the_kind() {
    let a = parse(&line(&["--mem", "4G"])).unwrap();
    assert_eq!(a.mem.as_deref(), Some("4G"));
    let u = super::unit(&a, "/b".as_ref(), "/i".as_ref());
    assert_eq!(u.mem.as_deref(), Some("4G"));
    let a = parse(&line(&[])).unwrap();
    assert_eq!(super::unit(&a, "/b".as_ref(), "/i".as_ref()).mem, None, "the default of the kind, 8G");
    // A value in bytes, and the smallest memory that keeps the reserve of a
    // job: ceil(4/3 × 2 GiB) = 2863311531 bytes.
    assert!(parse(&line(&["--mem", "2863311531"])).is_ok());
    assert!(parse(&line(&["--mem", "2731M"])).is_ok());
}

/// Each refusal names `--mem` and its cause, as `[unit.microvm] mem`.
#[test]
fn a_bad_memory_of_a_guest_is_refused() {
    for (value, why) in [
        ("0", "--mem \"0\" is zero"),
        ("lots", "--mem \"lots\": the unit is not K, M, G or T"),
        ("8.5G", "--mem \"8.5G\" is not a size"),
        ("8", "--mem 8: a guest of 8 keeps 0 MiB"),
        ("8GB", "--mem \"8GB\": the unit is not K, M, G or T"),
        ("-1G", "is not a size"),
        ("99999999999T", "--mem \"99999999999T\" is too large"),
        (
            "2G",
            "--mem 2G: a guest of 2G keeps 1536 MiB of root for a job, less than 2048 MiB with no image; fix: --mem 3G or more",
        ),
        ("2730M", "keeps 2047 MiB of root"),
        ("2863311530", "less than 2048 MiB"),
    ] {
        let e = parse(&line(&["--mem", value])).unwrap_err();
        assert!(e.contains(why), "{value}: {e}");
    }
    // Only the microvm kind takes it.
    for kind in [&["--incus", "/p"][..], &["--podvm", "/p"], &["--runner-dir", "/r"], &[]] {
        let a: Vec<String> = ["w.yml", "--mem", "4G"].iter().chain(kind).map(|s| s.to_string()).collect();
        let e = parse(&a).unwrap_err();
        assert!(e.contains("--mem needs --microvm"), "{kind:?}: {e}");
    }
}

/// `doctor --run` names the memory of a guest and the root of its job.
#[test]
fn doctor_names_the_memory_of_a_guest() {
    let out = super::super::doctor(&line(&["--mem", "4G"]));
    assert!(
        out.stdout.contains("ok   mem: 4G (--mem): a job gets about 3.2 GB of root less the unpacked image"),
        "{}",
        out.stdout
    );
    let out = super::super::doctor(&line(&[]));
    assert!(out.stdout.contains("ok   mem: 8G (the default): a job gets about 6.4 GB of root"), "{}", out.stdout);
    let out = super::super::doctor(&line(&["--mem", "2G"]));
    assert!(out.stdout.contains("FAIL options: --mem 2G: a guest of 2G keeps 1536 MiB"), "{}", out.stdout);
}
