//! T-515: the OOM kills of a podvm unit. A fake podvm writes a run report
//! with the end of the run, and with `oom_kills` only for a run with
//! `--memory`, as podvm counts the kills of the leaf of such a run only.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use podrunner_schema::{LimitMode, UnitId};
use podrunner_unit::limits::Limits;
use podrunner_unit::{Prepared, StartSpec, UnitDriver};
use serde_json::{Value, json};

use super::super::{PodvmUnit, limits, store_tests::write_tar};

const PIN: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";
const MANIFEST: &str = "sha256:4444444444444444444444444444444444444444444444444444444444444444";

/// The edges of the field: no field (an old podvm, a run with no leaf),
/// `null`, the `-` of a value that podvm does not know, a text, 0, the
/// largest count, a count past `u64`, a negative count and a fraction.
#[test]
fn the_count_of_a_report_is_a_whole_number_or_none() {
    for (report, count) in [
        (json!({"exit_code": 137}), None),
        (json!({"oom_kills": null}), None),
        (json!({"oom_kills": "-"}), None),
        (json!({"oom_kills": "2"}), None),
        (json!({"oom_kills": 0}), Some(0)),
        (json!({"oom_kills": 3}), Some(3)),
        (json!({"oom_kills": u64::MAX}), Some(u64::MAX)),
        (serde_json::from_str::<Value>(r#"{"oom_kills": 18446744073709551616}"#).unwrap(), None),
        (json!({"oom_kills": -1}), None),
        (json!({"oom_kills": 1.5}), None),
    ] {
        assert_eq!(super::count_of(&report), count, "{report}");
    }
}

/// A report with no end yet: the read of a unit with a leaf waits for the
/// end (the launcher writes the report after the record that `podvm wait`
/// reads); a unit with no leaf reads once; the wait has a bound. Each read
/// is an event of the test: no real time.
#[test]
fn the_count_waits_for_the_end_of_the_report() {
    let start = json!({"detached": true, "exit_code": null});
    let end = json!({"detached": true, "exit_code": 137, "oom_kills": 1});
    let run = |wait: bool, tries: u32, ends_at: usize| {
        let (mut reads, mut pauses) = (0, 0);
        let n = super::settle(
            || {
                reads += 1;
                Some(if reads >= ends_at { end.clone() } else { start.clone() })
            },
            wait,
            tries,
            || pauses += 1,
        );
        (n, reads, pauses)
    };
    assert_eq!(run(true, 100, 3), (Some(1), 3, 2), "the end at the third read");
    assert_eq!(run(true, 100, 1), (Some(1), 1, 0), "the end at once: no pause");
    assert_eq!(run(false, 100, 3), (None, 1, 0), "no leaf: one read");
    assert_eq!(run(true, 4, usize::MAX), (None, 5, 4), "the bound");
    // No report yet is a report with no end.
    let (mut reads, mut pauses) = (0, 0);
    let read = || {
        reads += 1;
        (reads > 2).then(|| end.clone())
    };
    assert_eq!(super::settle(read, true, 100, || pauses += 1), Some(1));
    assert_eq!((reads, pauses), (3, 2));
}

/// A fake podvm in `d`: each call goes to `d/calls`; `run` writes the end
/// of the run into its report at once, with one OOM kill where the run has
/// `--memory`.
fn fake(d: &Path) -> PathBuf {
    let body = format!(
        r#"#!/bin/sh
echo "$*" >> {log}
S=$PODVM_STORE
arg() {{ k=$1; shift; while [ $# -gt 0 ]; do [ "$1" = "$k" ] && {{ echo "$2"; return; }}; shift; done; }}
image() {{ mkdir -p "$S/rootfs/img/rootfs/etc"; }}
case "$1" in
probe) echo '{{"podvm":"0.2.0-beta.40","rung":"userland"}}' ;;
load) echo "runner@{MANIFEST}" >> "$S/images.txt" ;;
images) [ "$2" = --format ] && cat "$S/images.txt" 2>/dev/null ;;
extract) image; echo "$S/rootfs/img/rootfs" ;;
run) image; n=$(arg --name "$@"); mkdir -p "$S/trees/$n"; cp -R "$S/rootfs/img/rootfs" "$S/trees/$n/rootfs"
  oom=; [ -n "$(arg --memory "$@")" ] && oom=',"oom_kills":1'
  r=$(arg --report "$@"); [ -n "$r" ] && echo "{{\"detached\":true,\"exit_code\":137$oom}}" > "$r" ;;
inspect)
  if [ "$2" = --format ]; then n=$4; [ -d "$S/trees/$n" ] || exit 1; echo "$S/trees/$n/rootfs"
  else echo '[{{"State":{{"Status":"exited"}}}}]'; fi ;;
wait) echo 137 ;;
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

/// A unit of 256 MiB on a host whose cgroup is delegated with the
/// `memory` controller, or not (`delegated`): the `run -d` of the unit,
/// its OOM kills, and the record of the kind.
fn unit_of_256_mib(name: &str, delegated: bool) -> (String, Option<u64>, LimitMode) {
    let d = crate::scratch::Scratch::new(&format!("podvm-oom-{name}"));
    std::fs::create_dir_all(d.join("state/images")).unwrap();
    write_tar(&d.join(format!("state/images/{}.tar", PIN.trim_start_matches("sha256:"))), MANIFEST);
    let podvm = crate::Podvm { binary: fake(&d) };
    let verdict = if delegated { Ok(()) } else { Err("no memory controller".to_string()) };
    limits::assume(&podvm, "memory", verdict);
    limits::assume(&podvm, "pids", Err("no pids controller".into()));
    limits::assume(&podvm, "cpu", Err("no cpu controller".into()));
    let unit = PodvmUnit::new(podvm, &d.join("state"), &d).unwrap();
    let mode = unit.probe().unwrap().limit(limits::MEMORY_PER_UNIT);
    let s = StartSpec {
        unit: UnitId::parse("u-1").unwrap(),
        image: Prepared { digest: PIN.into(), platform: "linux/amd64".into() },
        argv: vec!["true".into()],
        env: Default::default(),
        credential: podrunner_unit::conformance::canary_credential(),
        sockets: None,
        mounts: Vec::new(),
        limits: Limits { memory_bytes: Some(256 << 20), ..Limits::default() },
    };
    unit.start(&s).unwrap();
    let oom = unit.oom_kills(&s.unit);
    let _ = unit.destroy(&s.unit);
    let calls = std::fs::read_to_string(d.join("calls")).unwrap();
    let run = calls.lines().find(|l| l.starts_with("run -d ")).unwrap().to_string();
    (run, oom, mode)
}

/// T-515: where the cgroup is delegated, the memory of a unit is
/// `memory.max` of its leaf (`--memory`), not RLIMIT_AS, so a payload past
/// it is ended by the OOM killer and the run report counts the kill; slots
/// then ends the job with `limit`. Elsewhere RLIMIT_AS holds and no count
/// comes.
#[test]
fn a_unit_with_its_memory_in_its_leaf_gives_the_oom_kills_of_its_run() {
    let _one = super::super::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
    let (run, oom, mode) = unit_of_256_mib("leaf", true);
    assert!(run.contains(" --memory 268435456 ") && !run.contains("as="), "{run}");
    assert_eq!(oom, Some(1), "{run}");
    assert_eq!(mode, LimitMode::Enforced);
    let (run, oom, mode) = unit_of_256_mib("rlimit", false);
    assert!(run.contains(" --ulimit as=268435456 ") && !run.contains("--memory"), "{run}");
    assert_eq!(oom, None, "{run}");
    assert_eq!(mode, LimitMode::None);
}
