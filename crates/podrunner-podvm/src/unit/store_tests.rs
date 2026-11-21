//! T-545: the units of one image share one store. A fake podvm keeps a
//! line of each call, and copies the extracted image into a tree of each
//! container at `create` (and at `run`), as podvm does.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use podrunner_schema::UnitId;
use podrunner_unit::{Prepared, StartSpec, UnitCode, UnitDriver};

use super::PodvmUnit;

/// The digest of the pin (an index), and of the manifest in its tar.
const PIN: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const MANIFEST: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";

/// An OCI tar with only `index.json`, which names `manifest`.
pub(super) fn write_tar(path: &Path, manifest: &str) {
    let index = format!(r#"{{"schemaVersion":2,"manifests":[{{"digest":"{manifest}"}}]}}"#);
    let mut b = tar::Builder::new(std::fs::File::create(path).unwrap());
    let mut h = tar::Header::new_gnu();
    h.set_size(index.len() as u64);
    h.set_mode(0o644);
    h.set_cksum();
    b.append_data(&mut h, "index.json", index.as_bytes()).unwrap();
    b.finish().unwrap();
}

/// A fake podvm in `d`. `inspect` names the tree of the container, or the
/// extracted image when `guest` (as a guest tier runs in it).
fn fake(d: &Path, guest: bool) -> PathBuf {
    let tree = if guest { "$S/rootfs/img/rootfs" } else { "$S/trees/$n/rootfs" };
    let body = format!(
        r#"#!/bin/sh
echo "$*" >> {log}
S=$PODVM_STORE
name() {{ while [ $# -gt 0 ]; do [ "$1" = --name ] && {{ echo "$2"; return; }}; shift; done; }}
image() {{ mkdir -p "$S/rootfs/img/rootfs/etc"; }}
case "$1" in
probe) echo '{{"podvm":"0.2.0-beta.37","rung":"userland"}}' ;;
load) echo "runner@{MANIFEST}" >> "$S/images.txt" ;;
images)
  if [ "$2" = --format ]; then cat "$S/images.txt" 2>/dev/null
  elif [ -f "$S/images.txt" ]; then echo 'REPOSITORY TAG'; echo 'runner 2.338.2'; fi ;;
extract) image; echo "$S/rootfs/img/rootfs" ;;
create|run) image; n=$(name "$@"); mkdir -p "$S/trees/$n"; cp -R "$S/rootfs/img/rootfs" "$S/trees/$n/rootfs" ;;
inspect)
  if [ "$2" = --format ]; then n=$4; [ -d "$S/trees/$n" ] || exit 1; echo "{tree}"
  else echo '[{{"State":{{"Status":"running"}}}}]'; fi ;;
wait) sleep 2; echo 0 ;;
rm) rm -rf "$S/trees/$3" ;;
esac
exit 0
"#,
        log = d.join("calls").display()
    );
    let bin = d.join("podvm");
    std::fs::write(&bin, body).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn spec(n: u32) -> StartSpec {
    StartSpec {
        unit: UnitId::parse(&format!("u-{n}")).unwrap(),
        image: Prepared { digest: PIN.into(), platform: "linux/amd64".into() },
        argv: vec!["true".into()],
        env: Default::default(),
        credential: podrunner_unit::conformance::canary_credential(),
        sockets: None,
        mounts: Vec::new(),
        limits: Default::default(),
    }
}

/// A driver with the fake podvm and the tar of the pin in its cache.
fn driver(name: &str, guest: bool) -> (PodvmUnit, PathBuf) {
    let d = std::env::temp_dir().join(format!("podrunner-podvm-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("state/images")).unwrap();
    write_tar(&d.join(format!("state/images/{}.tar", PIN.trim_start_matches("sha256:"))), MANIFEST);
    let unit = PodvmUnit::new(crate::Podvm { binary: fake(&d, guest) }, &d.join("state"), &d).unwrap();
    (unit, d)
}

/// Two units of one image: one load into one store, a tree of its own for
/// each unit with its credential files, and no credential file in the
/// extracted image that the next unit copies.
#[test]
fn the_units_of_one_image_load_it_once_and_each_has_a_tree_of_its_own() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (unit, d) = driver("shared", false);
    let (one, two) = (spec(1), spec(2));
    unit.start(&one).unwrap();
    unit.start(&two).unwrap();
    let calls = std::fs::read_to_string(d.join("calls")).unwrap();
    assert_eq!(calls.lines().filter(|l| l.starts_with("load ")).count(), 1, "{calls}");
    // The unit runs the pinned image by the digest of its manifest.
    assert!(
        calls.lines().any(|l| l.starts_with("create ") && l.contains(&format!("runner@{MANIFEST} true"))),
        "{calls}"
    );
    let store = d.join("state/store");
    let roots: Vec<PathBuf> =
        [&one, &two].iter().map(|s| unit.observe(&s.unit).unwrap().runner_root.unwrap()).collect();
    for (s, root) in [&one, &two].iter().zip(&roots) {
        assert!(root.starts_with(store.join("trees").join(s.unit.as_str())), "{}", root.display());
        let mode = std::fs::metadata(root.join(".credentials")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    assert!(!store.join("rootfs/img/rootfs/podrunner").exists(), "a credential in the extracted image");
    unit.destroy(&one.unit).unwrap();
    let residue = unit.destroy(&two.unit).unwrap();
    assert!(!roots[1].exists() && residue.files == podrunner_unit::Count::Known(0), "{residue:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// A root that is not a tree of the unit fails the start, and no
/// credential file goes into the extracted image.
#[test]
fn a_unit_with_no_tree_of_its_own_does_not_start() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (unit, d) = driver("guest", true);
    let s = spec(1);
    let e = unit.start(&s).unwrap_err();
    assert_eq!(e.code, UnitCode::UnitStartFailed, "{e:?}");
    let image = d.join("state/store/rootfs/img/rootfs");
    assert!(!image.join("podrunner").exists(), "a credential in the extracted image");
    let _ = unit.destroy(&s.unit);
    let _ = std::fs::remove_dir_all(&d);
}

/// The confine layer lets the unit write its working directory, which the
/// driver makes in the tree after `create`: the extracted image need not
/// have it.
#[test]
fn the_confine_layer_names_the_working_directory_that_the_image_lacks() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (unit, d) = driver("confine", false);
    let unit = unit.with_confine();
    let image = d.join("image");
    std::fs::create_dir_all(image.join("tmp")).unwrap();
    let args = unit.confine_args(&image, None);
    assert_eq!(args, ["--podvm-confine", "read=/;write=/podrunner,/tmp;exec=/podrunner,/tmp"]);
    let _ = std::fs::remove_dir_all(&d);
}
