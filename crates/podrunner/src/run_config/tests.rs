use super::*;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn a_file_gives_flags_and_a_project_file_reaches_no_host_resource() {
    let base = Path::new("/p");
    let text = "event = \"workflow_dispatch\"\nquiet = true\noffline = false\nvar = { A = \"1\" }\njob = [\"a\", \"b\"]\nvar-file = \"ci/vars.env\"\n";
    let got = flags_of(text, "f", base, true, false).unwrap();
    assert_eq!(
        got,
        s(&[
            "--event",
            "workflow_dispatch",
            "--job",
            "a",
            "--job",
            "b",
            "--quiet",
            "--var",
            "A=1",
            "--var-file",
            "/p/ci/vars.env"
        ])
    );
    let e = flags_of("allow = [\"evil.example\"]\n", "f", base, true, false).unwrap_err();
    assert!(e.contains("reaches the host"), "{e}");
    assert_eq!(flags_of("allow = [\"x.example\"]\n", "f", base, true, true).unwrap(), s(&["--allow", "x.example"]));
    assert_eq!(
        flags_of("podvm = \"/usr/bin/podvm\"\n", "f", base, false, true).unwrap(),
        s(&["--podvm", "/usr/bin/podvm"])
    );
    assert!(flags_of("var-file = \"../x\"\n", "f", base, true, false).unwrap_err().contains("stay in the project"));
    assert!(flags_of("var-file = \"/etc/x\"\n", "f", base, true, false).unwrap_err().contains("stay in the project"));
    assert!(flags_of("no-such-flag = 1\n", "f", base, false, true).unwrap_err().contains("not a key of run"));
    assert!(flags_of("dry-run = true\n", "f", base, false, true).unwrap_err().contains("not a key of run"));
    assert!(flags_of("trusted = [\"/p\"]\n", "f", base, true, false).is_err(), "only the user file trusts");
    assert!(flags_of("event = [1.5]\n", "f", base, false, true).is_err());
}

#[test]
fn the_flags_of_an_actrc_that_podrunner_takes() {
    let mut notes = Vec::new();
    let text = "# act\n-P ubuntu-latest=node:16\n--var A=1\n--env=B=2\n-s TOKEN\n-s K=V\n--secret-file .secrets\n";
    let got = actrc(text, ".actrc", Some(std::path::Path::new("/p")), false, &mut notes).unwrap();
    // A secret reads the environment of the host: not for an untrusted project.
    assert_eq!(got, s(&["--var", "A=1", "--env", "B=2"]));
    assert_eq!(notes.len(), 4, "{notes:?}");
    assert!(notes[0].contains("-P of act is not used") && notes[0].contains("--platform LABEL="), "{notes:?}");
    assert!(notes[1].contains("--secret reaches the host") && notes[2].contains("NAME=VALUE"), "{notes:?}");
    assert!(notes[3].contains("--secret-file reaches the host") && notes[3].contains("`trusted`"), "{notes:?}");
    let got = actrc("-s TOKEN\n", ".actrc", Some(std::path::Path::new("/p")), true, &mut Vec::new()).unwrap();
    assert_eq!(got, s(&["--secret", "TOKEN"]), "a trusted project");
    let got = actrc("--secret-file .secrets\n", "~/.actrc", None, true, &mut Vec::new()).unwrap();
    assert_eq!(got, s(&["--secret-file", ".secrets"]));
}

#[test]
fn the_flags_of_the_project_file_go_before_the_command_line() {
    let dir = std::env::temp_dir().join(format!("podrunner-run-config-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".podrunner")).unwrap();
    std::fs::write(dir.join(".podrunner/run.toml"), "event = \"pull_request\"\n").unwrap();
    let d = dir.display().to_string();
    let got = expand(&s(&["w.yml", "--checkout", &d, "--event", "push"])).unwrap();
    let at = |f: &str| got.iter().position(|a| a == f).unwrap();
    assert_eq!(got[0], "w.yml");
    assert!(got.contains(&"pull_request".to_string()));
    assert!(at("pull_request") < got.iter().rposition(|a| a == "push").unwrap(), "the command line wins: {got:?}");
    assert_eq!(expand(&s(&["w.yml", "--checkout", &d, "--no-config"])).unwrap(), s(&["w.yml", "--checkout", &d]));
    std::fs::write(dir.join(".podrunner/run.toml"), "runner-dir = \"/x\"\n").unwrap();
    assert!(expand(&s(&["w.yml", "--checkout", &d])).unwrap_err().contains("reaches the host"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A link in the project that leads out of it is refused; one that stays
/// in it passes (T-275).
#[test]
fn a_link_out_of_the_project_is_refused() {
    let dir = std::env::temp_dir().join(format!("podrunner-config-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = dir.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(dir.join("outside.env"), "A=1").unwrap();
    std::fs::write(project.join("inside.env"), "B=2").unwrap();
    std::os::unix::fs::symlink(dir.join("outside.env"), project.join("out")).unwrap();
    std::os::unix::fs::symlink("inside.env", project.join("in")).unwrap();
    let err = flags_of("env-file = \"out\"\n", "f", &project, true, false).unwrap_err();
    assert!(err.contains("symbolic link"), "{err}");
    assert!(flags_of("env-file = \"in\"\n", "f", &project, true, false).is_ok());
    assert!(flags_of("env-file = \"out\"\n", "f", &project, true, true).is_ok(), "a trusted project may");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `jobs` and `web` are keys of the file of the user; a flag that is not a
/// key is named (T-277).
#[test]
fn jobs_and_web_are_keys_of_the_user_file() {
    let base = std::path::Path::new("/p");
    assert_eq!(flags_of("jobs = 2\n", "f", base, false, true).unwrap(), ["--jobs", "2"]);
    assert_eq!(flags_of("web = \"127.0.0.1:0\"\n", "f", base, false, true).unwrap(), ["--web", "127.0.0.1:0"]);
    assert!(flags_of("web = \"127.0.0.1:0\"\n", "f", base, true, false).is_err(), "not of a project");
    assert!(flags_of("watch = true\n", "f", base, false, true).unwrap_err().contains("but for list"));
}

/// T-292: a file of `.actrc` of an untrusted project stays in the project.
#[test]
fn actrc_of_a_project_keeps_its_files_in_the_project() {
    let p = std::path::Path::new("/p");
    let e = actrc("--env-file ../outside.env\n", ".actrc", Some(p), false, &mut Vec::new()).unwrap_err();
    assert!(e.contains("stay in the project"), "{e}");
    assert!(actrc("--var-file /etc/x\n", ".actrc", Some(p), false, &mut Vec::new()).is_err());
    assert!(actrc("--env-file ../o.env\n", ".actrc", Some(p), true, &mut Vec::new()).is_ok(), "a trusted project may");
}

/// T-303: an executor of the command line replaces the one of the files.
#[test]
fn an_executor_of_the_command_line_replaces_the_one_of_the_files() {
    let file = s(&[
        "--podvm",
        "/b/podvm",
        "--images",
        "/i",
        "--image",
        "r@sha256:0000000000000000000000000000000000000000000000000000000000000000",
        "--quiet",
        "--allow",
        "x.example",
    ]);
    let got = command_line_wins(file.clone(), &s(&["w.yml", "--runner-dir", "/r"]));
    assert_eq!(got, s(&["--quiet", "--allow", "x.example"]));
    let got = command_line_wins(file.clone(), &s(&["w.yml", "--microvm", "/m"]));
    assert_eq!(
        got,
        s(&[
            "--images",
            "/i",
            "--image",
            "r@sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "--quiet",
            "--allow",
            "x.example"
        ])
    );
    assert_eq!(command_line_wins(file.clone(), &s(&["w.yml", "--quiet"])), file, "no executor: the file stays");
}

const DIGEST: &str = "r@sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// T-627, T-303: the executor of the command line drops each key of the
/// files that belongs to another executor.
#[test]
fn the_executor_of_the_command_line_drops_the_keys_of_other_executors() {
    let all = s(&[
        "--podvm",
        "/b/podvm",
        "--podnet",
        "/n",
        "--vnet",
        "/v",
        "--toolcache",
        "/t",
        "--engine",
        "/e",
        "--images",
        "/i",
        "--image",
        DIGEST,
        "--quiet",
    ]);
    // A file with podvm and vnet, and --runner-dir: no key of a unit stays.
    let got = command_line_wins(
        s(&["--podvm", "/b/podvm", "--vnet", "/v", "--toolcache", "/t"]),
        &s(&["--runner-dir", "/r"]),
    );
    assert!(got.is_empty(), "{got:?}");
    let got = command_line_wins(all.clone(), &s(&["w.yml", "--runner-dir", "/r"]));
    assert_eq!(got, s(&["--quiet"]));
    // --microvm and --incus: podnet, vnet and toolcache need podvm.
    for kind in ["--microvm", "--incus"] {
        let got = command_line_wins(all.clone(), &s(&["w.yml", kind, "/m"]));
        assert_eq!(got, s(&["--engine", "/e", "--images", "/i", "--image", DIGEST, "--quiet"]), "{kind}");
    }
    // --podvm: engine needs --microvm or --incus.
    let got = command_line_wins(all.clone(), &s(&["w.yml", "--podvm", "/p"]));
    assert_eq!(
        got,
        s(&["--podnet", "/n", "--vnet", "/v", "--toolcache", "/t", "--images", "/i", "--image", DIGEST, "--quiet"])
    );
}

/// T-627: of two flags that exclude each other, the one of the command
/// line drops the one of the files; a flag with no value does not take the
/// next word.
#[test]
fn a_flag_of_the_command_line_drops_the_file_flag_that_it_excludes() {
    let got = command_line_wins(s(&["--vnet", "/v", "--quiet"]), &s(&["w.yml", "--podnet", "/n"]));
    assert_eq!(got, s(&["--quiet"]));
    let got = command_line_wins(s(&["--offline", "--quiet"]), &s(&["w.yml", "--frozen"]));
    assert_eq!(got, s(&["--quiet"]));
    let got = command_line_wins(s(&["--web", "127.0.0.1:0", "--quiet"]), &s(&["w.yml", "--dry-run"]));
    assert_eq!(got, s(&["--quiet"]));
    let got = command_line_wins(s(&["--podnet", "/n"]), &s(&["w.yml", "--podnet", "/m"]));
    assert_eq!(got, s(&["--podnet", "/n"]), "the same flag: the parser takes the last");
}

/// T-314: `trusted` reads a relative path from the directory of the file.
#[test]
fn a_trusted_path_is_read_from_the_directory_of_the_file() {
    let dir = Path::new("/home/u/.config/podrunner");
    assert_eq!(trusted_path(".", dir, None), dir.join("."));
    assert_eq!(trusted_path("proj", dir, None), dir.join("proj"));
    assert_eq!(trusted_path("~/proj", dir, Some(Path::new("/home/u"))), Path::new("/home/u/proj"));
    assert_eq!(trusted_path("/abs", dir, None), Path::new("/abs"));
}

/// T-627: each flag of run with a key in the model is a key: `vnet` was
/// none, and a relative `podnet` or `vnet` is read from the directory of
/// the file, as `podvm` is.
#[test]
fn the_keys_come_from_the_model() {
    let base = Path::new("/home/u/.config/podrunner");
    for key in ["podnet", "vnet"] {
        let got = flags_of(&format!("{key} = \"bin/podnet\"\n"), "f", base, false, true).unwrap();
        assert_eq!(got, [format!("--{key}"), "/home/u/.config/podrunner/bin/podnet".to_string()]);
        assert!(flags_of(&format!("{key} = \"p\"\n"), "f", base, true, false).is_err(), "{key}: not of a project");
    }
    let e = flags_of("keep-failed = true\n", "f", base, false, true).unwrap_err();
    assert!(e.contains("not a key of run") && e.contains("keep-failed"), "{e}");
}
