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
    let got = actrc(text, ".actrc", true, false, &mut notes);
    // A secret reads the environment of the host: not for an untrusted project.
    assert_eq!(got, s(&["--var", "A=1", "--env", "B=2"]));
    assert_eq!(notes.len(), 4, "{notes:?}");
    assert!(notes[0].contains("-P is not a flag"), "{notes:?}");
    assert!(notes[1].contains("--secret reaches the host") && notes[2].contains("NAME=VALUE"), "{notes:?}");
    assert!(notes[3].contains("--secret-file reaches the host"), "{notes:?}");
    let got = actrc("-s TOKEN\n", ".actrc", true, true, &mut Vec::new());
    assert_eq!(got, s(&["--secret", "TOKEN"]), "a trusted project");
    let got = actrc("--secret-file .secrets\n", "~/.actrc", false, true, &mut Vec::new());
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
