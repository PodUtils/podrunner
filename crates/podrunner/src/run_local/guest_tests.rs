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

/// An install of `--serve`: `DATA/bin/podvm` and `DATA/serve.json` with the
/// unpacked bytes of its image, and a run line with that podvm.
fn install(tag: &str, serve: &str) -> (crate::scratch::Scratch, impl Fn(&[&str]) -> Vec<String> + use<>) {
    let dir = crate::scratch::Scratch::new(&format!("guest-{tag}"));
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("serve.json"), serve).unwrap();
    let podvm = dir.join("bin/podvm").display().to_string();
    let line = move |extra: &[&str]| {
        let base = ["w.yml", "--microvm", podvm.as_str(), "--images", "/nonexistent/store", "--image", DIGEST];
        base.iter().chain(extra).map(|s| s.to_string()).collect()
    };
    (dir, line)
}

fn serve_json(image: &str, bytes: &str) -> String {
    format!(r#"{{"schema": 1, "podvm": "/d/bin/podvm", "image": "{image}", "unpacked_bytes": {bytes}}}"#)
}

const MIB: u64 = 1 << 20;

/// T-698: the unpacked bytes come from `serve.json` of the install for the
/// digest of `--image` only.
#[test]
fn the_size_of_an_image_comes_from_the_install() {
    let other = DIGEST.replace("r@sha256:0", "r@sha256:1");
    let renamed = DIGEST.replace("r@", "runner@");
    for (text, want) in [
        (serve_json(DIGEST, "1000"), Some(1000)),
        (serve_json(&renamed, "1000"), Some(1000)),
        (serve_json(&other, "1000"), None),
        (serve_json(DIGEST, "null"), None),
        (serve_json(DIGEST, "0"), None),
        (serve_json(DIGEST, "-1"), None),
        (serve_json(DIGEST, "\"1000\""), None),
        (serve_json("runner:latest", "1000"), None),
        (serve_json(DIGEST, "1000").replace("\"schema\": 1", "\"schema\": 2"), None),
        ("{".into(), None),
        (String::new(), None),
    ] {
        assert_eq!(super::install_bytes(&text, DIGEST), want, "{text}");
    }
    assert_eq!(super::install_bytes(&serve_json(DIGEST, "1000"), "r:latest"), None, "an --image with no digest");
    let (dir, _) = install("path", &serve_json(DIGEST, "1000"));
    assert_eq!(super::image_bytes(&dir.join("bin/podvm"), DIGEST), Some(1000));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/serve.json"), serve_json(DIGEST, "1000")).unwrap();
    assert_eq!(super::image_bytes(&dir.join("lib/podvm"), DIGEST), None, "podvm not in DATA/bin");
    assert_eq!(super::image_bytes("podvm".as_ref(), DIGEST), None, "a bare name");
}

/// T-698: an image that the install knows is checked against `--mem` as a
/// pin of `serve` is: the boot of the image and the reserve of its job.
#[test]
fn a_memory_too_small_for_the_image_is_refused() {
    let size = |m: u64| (m * MIB).to_string();
    // 1416 MiB boots at 5114 MiB, more than 4G.
    let (_d, line) = install("boot", &serve_json(DIGEST, &size(1416)));
    let a = parse(&line(&["--mem", "4G"])).unwrap();
    let e = super::image_size(&a).unwrap_err();
    assert!(e.starts_with(&format!("--mem 4G: the image {DIGEST} unpacks to 1.5 GB, more than a guest of 4G")), "{e}");
    assert!(e.ends_with("fix: --mem 5G or more"), "{e}");
    assert_eq!(super::image_size(&parse(&line(&["--mem", "5G"])).unwrap()), Ok(Some(1416 * MIB)));
    assert_eq!(super::image_size(&parse(&line(&[])).unwrap()), Ok(Some(1416 * MIB)), "the default 8G");
    // 1100 MiB boots at 4087 MiB, but its job keeps 3072 - 1100 MiB.
    let (_d, line) = install("reserve", &serve_json(DIGEST, &size(1100)));
    let e = super::image_size(&parse(&line(&["--mem", "4G"])).unwrap()).unwrap_err();
    assert!(e.contains("keeps 1972 MiB for a job, less than 2048 MiB") && e.ends_with("--mem 5G or more"), "{e}");
    // 1024 MiB: 3072 - 1024 = 2048 MiB, the reserve and no more.
    let (_d, line) = install("edge", &serve_json(DIGEST, &size(1024)));
    assert_eq!(super::image_size(&parse(&line(&["--mem", "4G"])).unwrap()), Ok(Some(1024 * MIB)));
    // Another image of the store: not known, not checked.
    let (_d, line) = install("other", &serve_json(&DIGEST.replace("sha256:0", "sha256:1"), &size(9000)));
    assert_eq!(super::image_size(&parse(&line(&["--mem", "4G"])).unwrap()), Ok(None));
}

/// T-698: `doctor --run` names the size of the image, or says that it is
/// not known, and fails a memory too small for it.
#[test]
fn doctor_names_the_size_of_the_image() {
    let (_d, line) = install("doctor", &serve_json(DIGEST, &(1416 * MIB).to_string()));
    let out = super::super::doctor(&line(&["--mem", "6G"]));
    let want = "ok   mem: 6G (--mem): a job gets about 3.3 GB of root beside the image, which unpacks to 1.5 GB";
    assert!(out.stdout.contains(want), "{}", out.stdout);
    let out = super::super::doctor(&line(&["--mem", "4G"]));
    assert!(out.stdout.contains("FAIL mem: --mem 4G: the image r@sha256:"), "{}", out.stdout);
    let out = super::super::doctor(&self::line(&[]));
    assert!(out.stdout.contains("the unpacked size of the image is not known"), "{}", out.stdout);
}
