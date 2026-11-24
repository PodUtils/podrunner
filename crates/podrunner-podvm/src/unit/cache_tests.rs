//! T-630: a tar of the image cache that does not load and is not whole (a
//! cut save, a truncated or a changed file) goes, and the start saves the
//! image again once. A fake podvm saves a copy of `src.tar`, and its load
//! takes only a copy of `good.tar`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use podrunner_schema::UnitId;
use podrunner_unit::{Prepared, StartSpec, UnitDriver};

use super::PodvmUnit;

const PIN: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

fn sha(b: &[u8]) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, b);
    d.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// An image tar as a release sorts it: the blobs (a config, a layer of
/// 200 KiB and the manifest), then `index.json`. Its manifest digest.
pub(super) fn write_image(path: &Path) -> String {
    let config = b"{}".to_vec();
    let layer: Vec<u8> = (0..200 * 1024u32).map(|i| (i % 251) as u8).collect();
    let manifest = format!(
        r#"{{"config":{{"digest":"sha256:{}"}},"layers":[{{"digest":"sha256:{}"}}]}}"#,
        sha(&config),
        sha(&layer)
    )
    .into_bytes();
    let index = format!(r#"{{"schemaVersion":2,"manifests":[{{"digest":"sha256:{}"}}]}}"#, sha(&manifest));
    let mut b = tar::Builder::new(std::fs::File::create(path).unwrap());
    let mut add = |name: String, data: &[u8]| {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, name, data).unwrap();
    };
    for blob in [&config, &layer, &manifest] {
        add(format!("blobs/sha256/{}", sha(blob)), blob);
    }
    add("index.json".into(), index.as_bytes());
    b.into_inner().unwrap();
    format!("sha256:{}", sha(&manifest))
}

fn fake(d: &Path, manifest: &str) -> PathBuf {
    let body = format!(
        r#"#!/bin/sh
echo "$*" >> {d}/calls
S=$PODVM_STORE
arg() {{ k=$1; shift; while [ $# -gt 0 ]; do [ "$1" = "$k" ] && {{ echo "$2"; return; }}; shift; done; }}
case "$1" in
probe) echo '{{"podvm":"0.2.0-beta.37","rung":"userland"}}' ;;
save) cp {d}/src.tar "$3" ;;
load) [ ! -f {d}/xfsz ] || kill -s XFSZ $$
  [ ! -f {d}/other ] || {{ rm {d}/other; cp {d}/good.tar "$3.o"; mv "$3.o" "$3"; echo "Error: blob: digest mismatch" >&2; exit 1; }}
  [ ! -f {d}/no-space ] || {{ echo "Error: no space left on device" >&2; exit 1; }}
  cmp -s "$3" {d}/good.tar || {{ echo "Error: blob: digest mismatch" >&2; exit 1; }}
  echo "runner@{manifest}" >> "$S/images.txt" ;;
images) cat "$S/images.txt" 2>/dev/null ;;
extract) mkdir -p "$S/rootfs/img/rootfs"; echo "$S/rootfs/img/rootfs" ;;
run) n=$(arg --name "$@"); mkdir -p "$S/trees/$n"; cp -R "$S/rootfs/img/rootfs" "$S/trees/$n/rootfs" ;;
inspect)
  if [ "$2" = --format ]; then echo "$S/trees/$4/rootfs"; else echo '[{{"State":{{"Status":"running"}}}}]'; fi ;;
wait) sleep 2; echo 0 ;;
rm) rm -rf "$S/trees/$3" ;;
esac
exit 0
"#,
        d = d.display()
    );
    let bin = d.join("podvm");
    std::fs::write(&bin, body).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn spec() -> StartSpec {
    StartSpec {
        unit: UnitId::parse("u-1").unwrap(),
        image: Prepared { digest: PIN.into(), platform: "linux/amd64".into() },
        argv: vec!["true".into()],
        env: Default::default(),
        credential: podrunner_unit::conformance::canary_credential(),
        sockets: None,
        mounts: Vec::new(),
        limits: Default::default(),
    }
}

/// A driver with a cache of the user, after a `prepare` that saved the tar.
fn prepared(d: &Path) -> (PodvmUnit, PathBuf) {
    let manifest = write_image(&d.join("good.tar"));
    std::fs::copy(d.join("good.tar"), d.join("src.tar")).unwrap();
    let unit = PodvmUnit::new(crate::Podvm { binary: fake(d, &manifest) }, &d.join("state"), d)
        .unwrap()
        .with_image_cache(&d.join("cache"));
    unit.prepare(&format!("runner@{PIN}"), "linux/amd64", false).unwrap();
    let tar = d.join("cache").join(format!("{}.tar", PIN.trim_start_matches("sha256:")));
    assert!(std::fs::read(&tar).unwrap() == std::fs::read(d.join("good.tar")).unwrap());
    (unit, tar)
}

fn count(d: &Path, verb: &str) -> usize {
    std::fs::read_to_string(d.join("calls")).unwrap().lines().filter(|l| l.starts_with(verb)).count()
}

/// A changed byte in a blob and a truncated tar: the start saves the image
/// again and runs.
#[test]
fn a_tar_of_the_cache_that_is_not_whole_is_saved_again() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    for cut in [false, true] {
        let d = crate::scratch::Scratch::new("t630-again");
        let (unit, tar) = prepared(&d);
        let mut bytes = std::fs::read(&tar).unwrap();
        match cut {
            true => bytes.truncate(bytes.len() / 2),
            false => bytes[1024 + 10 * 1024] ^= 0xff,
        }
        std::fs::write(&tar, &bytes).unwrap();
        unit.start(&spec()).unwrap_or_else(|e| panic!("cut {cut}: {e}"));
        assert_eq!(count(&d, "save "), 2, "cut {cut}");
        assert!(std::fs::read(&tar).unwrap() == std::fs::read(d.join("good.tar")).unwrap(), "cut {cut}");
        let _ = unit.destroy(&spec().unit);
    }
}

/// A save that gives a tar that is not whole again: the error names the
/// tar and the fix. A whole tar whose load fails keeps the error of the
/// load, and the tar stays.
#[test]
fn a_tar_that_stays_bad_names_the_file_and_the_fix() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let d = crate::scratch::Scratch::new("t630-bad");
    let (unit, tar) = prepared(&d);
    let mut bytes = std::fs::read(d.join("good.tar")).unwrap();
    bytes.truncate(bytes.len() / 2);
    std::fs::write(d.join("src.tar"), &bytes).unwrap();
    std::fs::write(&tar, &bytes).unwrap();
    let e = unit.start(&spec()).unwrap_err();
    assert!(
        e.detail.contains("of the image cache is not whole (")
            && e.detail.ends_with(&format!("fix: rm {}", tar.display())),
        "{e}"
    );
    assert!(e.detail.contains("names no manifest; it was saved again"), "{e}");
    let _ = unit.destroy(&spec().unit);

    let d = crate::scratch::Scratch::new("t630-space");
    let (unit, tar) = prepared(&d);
    std::fs::write(d.join("no-space"), "").unwrap();
    let e = unit.start(&spec()).unwrap_err();
    assert!(e.detail.contains("no space left on device") && !e.detail.contains("rm "), "{e}");
    assert_eq!(count(&d, "save "), 1);
    assert!(tar.is_file());
    let _ = unit.destroy(&spec().unit);
}

/// T-632: a podvm that a signal ends gives the signal in the cause, and
/// SIGXFSZ gives the limit and its fix; the tar of the cache stays.
#[test]
fn a_load_that_sigxfsz_ends_names_the_limit_and_the_fix() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let d = crate::scratch::Scratch::new("t632-xfsz");
    let (unit, tar) = prepared(&d);
    std::fs::write(d.join("xfsz"), "").unwrap();
    let e = unit.start(&spec()).unwrap_err();
    assert!(e.detail.starts_with("podvm load: ended by SIGXFSZ"), "{e}");
    assert!(e.detail.contains("ulimit -f") && e.detail.contains("fix: ulimit -f unlimited"), "{e}");
    assert!(!e.detail.contains("exit -1"), "{e}");
    assert!(tar.is_file());
    let _ = unit.destroy(&spec().unit);
}

/// Two runs that share the cache: while the load of this run fails on a
/// tar that is not whole, another run saves the tar again. This run loads
/// the new tar; it does not remove it and does not save it again.
#[test]
fn a_tar_that_another_run_saved_again_is_loaded_not_removed() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let d = crate::scratch::Scratch::new("t630-race");
    let (unit, tar) = prepared(&d);
    let mut bytes = std::fs::read(&tar).unwrap();
    bytes[1024 + 10 * 1024] ^= 0xff;
    std::fs::write(&tar, &bytes).unwrap();
    std::fs::write(d.join("other"), "").unwrap();
    unit.start(&spec()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(count(&d, "save "), 1);
    assert!(std::fs::read(&tar).unwrap() == std::fs::read(d.join("good.tar")).unwrap());
    let _ = unit.destroy(&spec().unit);
}
