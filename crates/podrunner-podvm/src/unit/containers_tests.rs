//! The tests of the containers of a podvm unit (`containers.rs`).

use super::*;

#[test]
fn the_state_comes_from_inspect() {
    let up = br#"x: Extracted
[{"Name":"u-1-c0","State":"running","ExitCode":"-"}]"#;
    assert_eq!(state_of(up).unwrap(), ContainerState { running: true, exit: None });
    let done = br#"[{"Name":"u-1-c0","State":"exited","ExitCode":"3"}]"#;
    assert_eq!(state_of(done).unwrap(), ContainerState { running: false, exit: Some(Exit::Code(3)) });
    assert!(state_of(b"no json").is_err());
}

/// podvm `0.2.0-beta.37` gives `State` as the object of docker, and
/// `ExitCode` as a number or `-` (the output of that release).
#[test]
fn the_state_comes_from_the_docker_object_of_inspect() {
    let up = br#"[{"ExitCode":"-","Name":"c1","State":{"Dead":false,"ExitCode":null,"Pid":20170,"Running":true,"Status":"running"},"Status":"Up"}]"#;
    assert_eq!(state_of(up).unwrap(), ContainerState { running: true, exit: None });
    let done = br#"[{"ExitCode":3,"Name":"c2","State":{"Dead":false,"ExitCode":3,"Pid":0,"Running":false,"Status":"exited"},"Status":"Exited (3)"}]"#;
    assert_eq!(state_of(done).unwrap(), ContainerState { running: false, exit: Some(Exit::Code(3)) });
}

/// T-581: a podvm after `0.2.0-beta.37` gives docker's types (podvm
/// T-2060): `ExitCode` and `Pid` are numbers or null, `Labels` is a
/// map, `Config` is an object. A dead container has no exit code in
/// each form: its end is -1, not a wait with no end.
#[test]
fn the_state_comes_from_the_typed_inspect_of_a_later_podvm() {
    let doc = |state: &str, exit: &str| {
        format!(
            r#"[{{"Id":"a","Name":"u-1-c0","State":{state},"Status":"x","Pid":77,"LauncherPid":70,"ExitCode":{exit},"RootfsPath":"/s/r","Labels":{{"podrunner.unit":"u-1"}},"Complete.Fixups":"-","Config":{{"Env":["A=1"],"Cmd":["tail","-f","/dev/null"],"Image":"img","WorkingDir":"/w","Labels":{{"podrunner.unit":"u-1"}}}},"End":{{"Reason":"-","RunMs":null}}}}]"#
        )
    };
    let up = doc(r#"{"Status":"running","Running":true,"Dead":false,"ExitCode":null,"Pid":77}"#, "null");
    assert_eq!(state_of(up.as_bytes()).unwrap(), ContainerState { running: true, exit: None });
    let done = doc(r#"{"Status":"exited","Running":false,"Dead":false,"ExitCode":3,"Pid":0}"#, "3");
    assert_eq!(state_of(done.as_bytes()).unwrap(), ContainerState { running: false, exit: Some(Exit::Code(3)) });
    let created = doc(r#"{"Status":"created","Running":false,"Dead":false,"ExitCode":null,"Pid":0}"#, "null");
    assert_eq!(state_of(created.as_bytes()).unwrap(), ContainerState { running: false, exit: None });
    let dead = doc(r#"{"Status":"dead","Running":false,"Dead":true,"ExitCode":null,"Pid":0}"#, "null");
    assert_eq!(state_of(dead.as_bytes()).unwrap(), ContainerState { running: false, exit: Some(Exit::Code(-1)) });
    // `0.2.0-beta.37` and an older podvm: a dead container, no code.
    let dead37 =
        br#"[{"ExitCode":"-","Name":"c","State":{"Dead":true,"ExitCode":null,"Running":false,"Status":"dead"}}]"#;
    assert_eq!(state_of(dead37).unwrap().exit, Some(Exit::Code(-1)));
    let dead_word = br#"[{"Name":"c","State":"dead","ExitCode":"-"}]"#;
    assert_eq!(state_of(dead_word).unwrap().exit, Some(Exit::Code(-1)));
}

/// T-581: the output of podvm `0.2.0-beta.38` (`tests/inspect/`,
/// measured as uid 0 on the namespace rung; the store is `/s`): a running,
/// an exited and a dead container (its launcher killed), each field with
/// docker's type. The state, the check of a killed launcher and the
/// refusal of `exec` into an ended container read it.
#[test]
fn the_readers_take_the_output_of_podvm_0_2_0_beta_38() {
    let read = |name: &str| std::fs::read(format!("{}/tests/inspect/{name}.json", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(state_of(&read("running")).unwrap(), ContainerState { running: true, exit: None });
    assert_eq!(state_of(&read("exited")).unwrap(), ContainerState { running: false, exit: Some(Exit::Code(7)) });
    assert_eq!(state_of(&read("dead")).unwrap(), ContainerState { running: false, exit: Some(Exit::Code(-1)) });
    let dead = |name: &str| inspected(&read(name)).is_some_and(|c| is_dead(&c));
    assert!(dead("dead") && !dead("exited") && !dead("running"));
    for state in ["exited", "dead"] {
        let line = format!("podvm exec: container c2 is not running ({state}). Start it with `podvm start c2`\n");
        let e = exec_exit("c2", Some(1), line.as_bytes()).unwrap_err();
        assert_eq!(e.code, UnitCode::UnitStartFailed, "{state}");
    }
}

/// T-581: the check of a killed launcher (`launcher_died`) reads each
/// form of `State`, after a line that podvm writes before the JSON.
#[test]
fn a_dead_launcher_is_seen_in_each_form_of_inspect() {
    let dead = |out: &str| inspected(out.as_bytes()).is_some_and(|c| is_dead(&c));
    assert!(dead(r#"[{"Name":"u-1","State":"dead"}]"#), "the word");
    assert!(dead(r#"[{"Name":"u-1","State":{"Status":"dead","Dead":true,"ExitCode":null}}]"#), "docker's object");
    assert!(dead("u-1: Extracted\n[{\"State\":{\"Status\":\"dead\"},\"Config\":{\"Cmd\":[\"sh\"]}}]"), "a line before");
    assert!(dead(r#"{"State":{"Dead":true}}"#), "one object");
    assert!(!dead(r#"[{"State":{"Status":"exited","Dead":false,"ExitCode":0}}]"#));
    assert!(!dead(r#"[{"State":"running"}]"#));
    assert!(!dead("Error: no such container u-1\n"));
}

/// T-581: a podvm after `0.2.0-beta.37` refuses `exec` into a container
/// that ended with exit 1 (podvm T-2061). That is a stopped container,
/// not a step that exits 1. The exit of a step stays its exit.
#[test]
fn a_refused_exec_into_an_ended_container_is_not_the_exit_of_the_step() {
    let refusal =
        b"podvm exec: container u-1-c0 is not running (exited). Start it with `podvm start u-1-c0`\n".as_slice();
    let e = exec_exit("u-1-c0", Some(1), refusal).unwrap_err();
    assert_eq!(e.code, UnitCode::UnitStartFailed);
    assert!(e.detail.contains("is not running (exited)"), "{}", e.detail);
    let dead = b"podvm exec: container u-1-c0 is not running (dead). Start it with `podvm start u-1-c0`".as_slice();
    assert!(exec_exit("u-1-c0", Some(1), dead).is_err());
    // `0.2.0-beta.37` runs the exec: the step's own exit 1 stays.
    assert_eq!(exec_exit("u-1-c0", Some(1), b"make: *** [all] Error 1\n").unwrap(), Exit::Code(1));
    // The text of another container, or another code, is a step's output.
    assert_eq!(exec_exit("u-1-c1", Some(1), refusal).unwrap(), Exit::Code(1));
    assert_eq!(exec_exit("u-1-c0", Some(0), refusal).unwrap(), Exit::Code(0));
    assert_eq!(exec_exit("u-1-c0", Some(126), b"").unwrap(), Exit::Code(126));
    assert_eq!(exec_exit("u-1-c0", None, b"").unwrap(), Exit::Code(-1));
}

/// T-532, T-403: a container of the namespace rung binds its volumes
/// under the runner directory with no symbolic link; the other rungs
/// get no flag.
#[test]
fn a_container_of_the_namespace_rung_has_the_volume_base() {
    let runner = Path::new("/u/store/trees/t/rootfs/home/runner");
    assert_eq!(
        volume_base_args(Some("namespace"), runner),
        ["--podvm-volume-base", "/u/store/trees/t/rootfs/home/runner"]
    );
    for rung in [Some("supervise"), Some("userland"), None] {
        assert!(volume_base_args(rung, runner).is_empty(), "{rung:?}");
    }
}

/// T-578: on the namespace rung the CA directory of the hooks is a
/// `-v` beside `--podvm-volume-base`, by its host path, out of the
/// runner directory: a directory of the job cannot replace it. A
/// relative source stays under the base.
#[test]
fn the_ca_directory_of_the_hooks_is_a_volume_out_of_the_base() {
    let dir = std::env::temp_dir().join(format!("podrunner-podvm-ca-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (runner, ca) = (dir.join("rootfs/home/runner"), dir.join("egress/u-1.logs/ca"));
    std::fs::create_dir_all(&runner).unwrap();
    std::fs::create_dir_all(&ca).unwrap();
    let mounts = [
        ContainerMount { source: "work".into(), target: "/__w".into(), read_only: false },
        ContainerMount { source: ca.clone(), target: "/__podrunner".into(), read_only: true },
    ];
    let base = volume_base_args(Some("namespace"), &runner);
    assert_eq!(base, ["--podvm-volume-base".to_string(), runner.display().to_string()]);
    let v = volume_args(&runner, &mounts).unwrap();
    assert_eq!(
        v,
        [
            "-v".to_string(),
            format!("{}:/__w", runner.join("work").display()),
            "-v".to_string(),
            format!("{}:/__podrunner:ro", ca.display()),
        ]
    );
    assert!(!ca.starts_with(&runner), "the CA directory is out of the volume base");
    // A missing absolute source is not made here: the host made it.
    let gone = [ContainerMount { source: dir.join("gone"), target: "/x".into(), read_only: true }];
    assert!(volume_args(&runner, &gone).is_ok());
    assert!(!dir.join("gone").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-403: a link in a mount source under the runner directory is
/// refused before podvm runs; a link out of it is not the job's.
#[test]
fn a_linked_source_under_the_runner_directory_is_refused() {
    let dir = std::env::temp_dir().join(format!("podrunner-podvm-nolink-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("runner/_work/_temp")).unwrap();
    std::os::unix::fs::symlink("/", dir.join("runner/_work/_temp/_github_home")).unwrap();
    let runner = dir.join("runner");
    assert!(no_link(&runner, Path::new("_work")).is_ok());
    assert!(no_link(&runner, Path::new("_work/_temp/absent/x")).is_ok());
    for rel in ["_work/_temp/_github_home", "_work/_temp/_github_home/x"] {
        let e = no_link(&runner, Path::new(rel)).unwrap_err();
        assert_eq!(e.code, UnitCode::ContainerOptionRefused, "{rel}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-403: the refusal of podvm `0.2.0-beta.37` for a link under the
/// volume base (measured on the namespace rung) is a refused option;
/// another exit 125 stays a start that failed.
#[test]
fn a_volume_that_podvm_refuses_is_a_refused_option() {
    let out = b"podvm run: -v /u/rootfs/home/runner/_work/_temp/_github_home:/github/home: the volume is refused: \
                its host path holds a symbolic link under its base /u/rootfs/home/runner (--podvm-volume-base); \
                it resolves to /\n";
    let e = run_error(Some(125), out);
    assert_eq!(e.code, UnitCode::ContainerOptionRefused);
    assert!(e.detail.contains("the volume is refused"), "{}", e.detail);
    assert_eq!(run_error(Some(1), out).code, UnitCode::UnitStartFailed);
    assert_eq!(
        run_error(Some(125), b"podvm run: chroot(2) is denied on this machine\n").code,
        UnitCode::UnitStartFailed
    );
}

/// A refusal of podvm keeps its reason; a longer line shows its cut.
#[test]
fn the_last_line_keeps_the_reason_of_podvm() {
    let refusal = "podvm run: chroot(2) is denied on this machine, and no no-chroot family runs this payload: \
                   the loader family declines the payload: this podvm binary carries no musl interposer object, \
                   so the tier is declined; use a podvm build with the musl interposer or a host that allows chroot";
    let out = format!("Extracted\n{refusal}\n\n");
    assert_eq!(last_line(out.as_bytes()), refusal);
    let long = "x".repeat(5000);
    let cut = last_line(long.as_bytes());
    assert!(cut.ends_with(" [cut]") && cut.len() < 1000, "{}", cut.len());
}
