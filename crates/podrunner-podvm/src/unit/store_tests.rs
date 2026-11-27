//! T-545: the units of one image share one store where the rung binds a
//! volume. A fake podvm keeps a line of each call, copies the extracted
//! image into a tree of each container at `run`, as podvm does, and writes
//! a run report with two OOM kills at `--report`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use podrunner_schema::UnitId;
use podrunner_unit::{ContainerMount, Count, Prepared, StartSpec, UnitCode, UnitDriver};

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

/// A fake podvm in `d` on the rung `rung`. `inspect` names the tree of the
/// container, or the extracted image when `guest` (as a guest tier runs in
/// it).
fn fake(d: &Path, rung: &str, guest: bool) -> PathBuf {
    let tree = if guest { "$S/rootfs/img/rootfs" } else { "$S/trees/$n/rootfs" };
    let body = format!(
        r#"#!/bin/sh
echo "$*" >> {log}
S=$PODVM_STORE
arg() {{ k=$1; shift; while [ $# -gt 0 ]; do [ "$1" = "$k" ] && {{ echo "$2"; return; }}; shift; done; }}
image() {{ mkdir -p "$S/rootfs/img/rootfs/etc"; }}
case "$1" in
probe) echo '{{"podvm":"0.2.0-beta.37","rung":"{rung}"}}' ;;
load) echo "runner@{MANIFEST}" >> "$S/images.txt" ;;
images) [ "$2" = --format ] && cat "$S/images.txt" 2>/dev/null ;;
extract) image; echo "$S/rootfs/img/rootfs" ;;
run) image; n=$(arg --name "$@"); mkdir -p "$S/trees/$n"; cp -R "$S/rootfs/img/rootfs" "$S/trees/$n/rootfs"
  r=$(arg --report "$@"); [ -n "$r" ] && echo '{{"detached":true,"oom_kills":2}}' > "$r" ;;
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
fn driver(name: &str, rung: &str, guest: bool) -> (PodvmUnit, PathBuf) {
    let d = std::env::temp_dir().join(format!("podrunner-podvm-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("state/images")).unwrap();
    write_tar(&d.join(format!("state/images/{}.tar", PIN.trim_start_matches("sha256:"))), MANIFEST);
    let unit = PodvmUnit::new(crate::Podvm { binary: fake(&d, rung, guest) }, &d.join("state"), &d).unwrap();
    (unit, d)
}

fn mode(p: &Path) -> u32 {
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

/// Two units of one image on a rung with volumes: one load into one store,
/// a tree of its own for each unit, the credential files in a volume of
/// the unit and in no tree and not in the extracted image, and the OOM
/// kills of the run report.
#[test]
fn the_units_of_one_image_load_it_once_and_each_has_a_tree_of_its_own() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (unit, d) = driver("shared", "userland", false);
    let (one, two) = (spec(1), spec(2));
    unit.start(&one).unwrap();
    unit.start(&two).unwrap();
    let calls = std::fs::read_to_string(d.join("calls")).unwrap();
    assert_eq!(calls.lines().filter(|l| l.starts_with("load ")).count(), 1, "{calls}");
    let store = d.join("state/store");
    for s in [&one, &two] {
        let dir = d.join("state/units").join(s.unit.as_str());
        let creds = dir.join("runner-root");
        // The unit runs the pinned image by the digest of its manifest, with
        // its credential files as a volume.
        let volume = format!("-v {}:/podrunner/.runner ", creds.display());
        let run = calls.lines().find(|l| l.contains(&format!("--name {} ", s.unit))).unwrap();
        assert!(run.starts_with("run -d ") && run.contains(&volume), "{run}");
        assert!(run.contains(&format!("runner@{MANIFEST} true")), "{run}");
        let o = unit.observe(&s.unit).unwrap();
        assert_eq!(o.runner_root.as_deref(), Some(creds.as_path()));
        assert!(o.unit_root.unwrap().starts_with(store.join("trees").join(s.unit.as_str())));
        assert_eq!(mode(&creds.join(".credentials")), 0o600);
        assert!(!store.join("trees").join(s.unit.as_str()).join("rootfs/podrunner/.runner").exists());
        assert_eq!(unit.oom_kills(&s.unit), Some(2), "the run report of the unit");
    }
    assert!(!store.join("rootfs/img/rootfs/podrunner").exists(), "a credential in the extracted image");
    unit.destroy(&one.unit).unwrap();
    let residue = unit.destroy(&two.unit).unwrap();
    assert!(!store.join("trees/u-2").exists() && residue.files == Count::Known(0), "{residue:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// A rung that binds no volume keeps a store for each unit: the credential
/// files go into its own extracted image, the tree takes them, and the run
/// report gives the OOM kills.
#[test]
fn a_rung_with_no_volume_keeps_a_store_for_each_unit() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (unit, d) = driver("own", "chroot", false);
    let s = spec(1);
    unit.start(&s).unwrap();
    let store = d.join("state/units/u-1/store");
    let calls = std::fs::read_to_string(d.join("calls")).unwrap();
    assert!(!calls.contains(":/podrunner/.runner"), "{calls}");
    let root = unit.observe(&s.unit).unwrap().runner_root.unwrap();
    assert_eq!(root, store.join("trees/u-1/rootfs/podrunner/.runner"));
    assert_eq!(mode(&root.join(".credentials")), 0o600);
    assert_eq!(unit.oom_kills(&s.unit), Some(2));
    let residue = unit.destroy(&s.unit).unwrap();
    assert!(!d.join("state/units/u-1").exists() && residue.files == Count::Known(0), "{residue:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// In the store of the units, a root that is not a tree of the unit fails
/// the start.
#[test]
fn a_unit_with_no_tree_of_its_own_does_not_start() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (unit, d) = driver("guest", "userland", true);
    let s = spec(1);
    let e = unit.start(&s).unwrap_err();
    assert_eq!(e.code, UnitCode::UnitStartFailed, "{e:?}");
    let image = d.join("state/store/rootfs/img/rootfs");
    assert!(!image.join("podrunner").exists(), "a credential in the extracted image");
    let _ = unit.destroy(&s.unit);
    let _ = std::fs::remove_dir_all(&d);
}

/// The confine layer lets the unit write its working directory, which
/// podvm makes for `-w`: the extracted image need not have it.
#[test]
fn the_confine_layer_names_the_working_directory_that_the_image_lacks() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (unit, d) = driver("confine", "userland", false);
    let unit = unit.with_confine();
    let image = d.join("image");
    std::fs::create_dir_all(image.join("tmp")).unwrap();
    let args = unit.confine_args(&image, None, &[]);
    assert_eq!(args, ["--podvm-confine", "read=/;write=/podrunner,/tmp;exec=/podrunner,/tmp"]);
    let _ = std::fs::remove_dir_all(&d);
}

/// T-480: a rung with volumes mounts the cache of a job, `:ro` as it is;
/// the confine layer lets the unit write a read-write cache and execute
/// from each cache. A rung with no volume mounts none.
#[test]
fn the_cache_of_a_job_is_a_volume_and_the_confine_layer_names_it() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (unit, d) = driver("cache", "userland", false);
    assert!(unit.host_mounts());
    let host = d.join("cache/data");
    let s = StartSpec { mounts: vec![(host.clone(), "/podrunner-cache:ro".into())], ..spec(1) };
    unit.start(&s).unwrap();
    let calls = std::fs::read_to_string(d.join("calls")).unwrap();
    assert!(calls.contains(&format!("-v {}:/podrunner-cache:ro ", host.display())), "{calls}");
    let _ = unit.destroy(&s.unit);
    let unit = unit.with_confine();
    let image = d.join("image");
    std::fs::create_dir_all(image.join("tmp")).unwrap();
    let ro = unit.confine_args(&image, None, &s.mounts);
    assert_eq!(ro, ["--podvm-confine", "read=/;write=/podrunner,/tmp;exec=/podrunner,/tmp,/podrunner-cache"]);
    let rw = unit.confine_args(&image, Some("/podrunner/sockets"), &[(host, "/podrunner-cache".into())]);
    let own = "/podrunner,/tmp,/podrunner/sockets,/podrunner-cache";
    assert_eq!(rw, ["--podvm-confine".to_string(), format!("read=/;write={own};exec={own}")]);
    let _ = std::fs::remove_dir_all(&d);
    let (chroot, d) = driver("cache-chroot", "chroot", false);
    assert!(!chroot.host_mounts());
    let _ = std::fs::remove_dir_all(&d);
}

/// T-603: a container may write each writable mount and execute in each
/// mount of the job, but not in the CA directory: it holds no program.
#[test]
fn the_ca_directory_of_a_confined_container_is_not_in_exec() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (unit, d) = driver("confine-ca", "namespace", false);
    let m = |target: &str, read_only, exec| ContainerMount {
        source: target.into(),
        target: target.into(),
        read_only,
        exec,
    };
    let mounts = [m("/__w", false, true), m("/__e", true, true), m("/__podrunner", true, false)];
    assert!(unit.container_confine_args(&mounts).is_empty(), "no confine layer");
    let unit = unit.with_confine();
    let args = unit.container_confine_args(&mounts);
    assert_eq!(args, ["--podvm-confine", "read=/;write=/__w;exec=/__w,/__e"]);
    // A container with the CA directory only: no exec= list, as with no mount.
    assert_eq!(unit.container_confine_args(&mounts[2..]), ["--podvm-confine", "read=/"]);
    assert_eq!(unit.container_confine_args(&[]), ["--podvm-confine", "read=/"]);
    let _ = std::fs::remove_dir_all(&d);
}

/// The third review of T-480: a `:ro` volume of the confine self-test that
/// the host cannot make leaves `ro` out of the record; the self-test of
/// the layer still runs and gives a verdict, not an error.
#[test]
fn a_ro_volume_that_cannot_be_made_does_not_stop_the_confine_self_test() {
    let _one = super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (unit, d) = driver("confine-ro", "userland", false);
    let mut unit = unit.with_confine();
    // A file where the volume goes: no directory can be made there.
    std::fs::write(d.join("state/confine-ro"), "x").unwrap();
    let verdict = unit.verify_confine(&format!("runner@{PIN}"), UnitId::parse("u-9").unwrap());
    assert!(matches!(verdict, Ok(false)), "{verdict:?}");
    assert_eq!(unit.probe().unwrap().mount_modes, ["rw"]);
    let _ = std::fs::remove_dir_all(&d);
}
