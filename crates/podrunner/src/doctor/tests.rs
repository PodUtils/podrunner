use super::*;
use podrunner_schema::Tier;

#[test]
fn a_pin_names_its_runner_its_age_and_the_end_of_its_window() {
    let day = 24 * 3600;
    let cfg = Config::parse(&format!(
        "host = \"h\"\n[pin]\n[[pin.payload]]\narch = \"x86_64\"\nreference = \"runner@sha256:{d}\"\n\
         built_at_s = {a}\nrunner_version = \"2.338.2\"\n[[pin.payload]]\narch = \"aarch64\"\n\
         reference = \"runner@sha256:{d}\"\nbuilt_at_s = {b}\n[[pin.payload]]\narch = \"x86_64\"\n\
         reference = \"runner@sha256:{e}\"\nbuilt_at_s = {c}\n",
        d = "1".repeat(64),
        e = "2".repeat(64),
        a = 100 * day - 2 * day,
        b = 100 * day - 25 * day,
        c = 100 * day - 31 * day,
    ))
    .unwrap();
    let rows = pins::rows(&cfg, 100 * day);
    let states: Vec<&str> = rows.iter().map(|r| r["state"].as_str().unwrap()).collect();
    assert_eq!(states, ["ok", "near-end", "stale"]);
    let text = text(&json!({ "pins": rows, "runner_of_this_build": pins::RUNNER_OF_THIS_BUILD }));
    assert!(text.contains("pin x86_64: runner 2.338.2, 2 of 30 days\n"), "{text}");
    assert!(text.contains("pin aarch64: runner not named, 25 of 30 days: near the end of its window"), "{text}");
    assert!(text.contains("pin x86_64: runner not named, 31 of 30 days: stale"), "{text}");
    let built = pins::RUNNER_OF_THIS_BUILD;
    assert!(built.starts_with("2."), "{built}");
    assert!(text.contains(&format!("runner of this build: {built}\n")), "{text}");
}

/// T-551: doctor names the size of a pinned image, the host disk of the
/// incus kind for it, and, as `check`, an image over the root disk.
#[test]
fn doctor_names_the_disk_of_a_pinned_image() {
    let dir = crate::scratch::Scratch::new("doctor-disk");
    let file = dir.join("podrunner.toml");
    let pin = |size: u64| {
        format!(
            "host = \"h\"\n[unit.incus]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\nroot_quota = true\n\
             disk_max_bytes = 40000000000\n[pin]\n[[pin.payload]]\narch = \"{}\"\n\
             reference = \"hosted@sha256:{}\"\nbuilt_at_s = 1\nunpacked_bytes = {size}\n",
            std::env::consts::ARCH,
            "1".repeat(64)
        )
    };
    std::fs::write(&file, pin(55_000_000_000)).unwrap();
    let v = report_with(Some(&file), json!([])).unwrap();
    assert_eq!(v["pins"][0]["unpacked_bytes"], json!(55_000_000_000u64), "{v}");
    assert_eq!(v["pins"][0]["incus_host_bytes"], json!(187_000_000_000u64), "{v}");
    let t = text(&v);
    assert!(t.contains(", 55.0 GB unpacked, the incus kind needs about 187.0 GB of host disk"), "{t}");
    assert!(t.contains("missing: [unit.incus] disk_max_bytes: the pin hosted@sha256:"), "{t}");
    assert!(t.contains("fix: set disk_max_bytes above 55.0 GB"), "{t}");
    std::fs::write(&file, pin(30_000_000_000)).unwrap();
    let v = report_with(Some(&file), json!([])).unwrap();
    assert!(!v["missing"].to_string().contains("disk_max_bytes"), "{v}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_tool_is_named() {
    let r = read_podnet_hold(Path::new("podrunner-no-such-tool")).unwrap_err();
    assert!(r.starts_with("podrunner-no-such-tool: not run"), "{r}");
}

#[test]
fn doctor_keeps_the_podnet_report_when_no_backend_can_run() {
    let dir = crate::scratch::Scratch::new("doctor-podnet");
    let file = dir.join("podrunner.toml");
    let podvm = dir.join("podvm");
    let podnet = dir.join("podnet");
    podrunner_unit::program_file::write(
        &podvm,
        "#!/bin/sh\necho '{\"podvm\":\"0.2.0-beta.14\",\"rung\":\"userland\"}'\n",
    )
    .unwrap();
    std::fs::write(
        &file,
        format!(
            "host = \"h\"\n[unit.podvm]\nslots = 1\nbinary = \"{}\"\nimages = \"{}\"\npodnet = \"{}\"\n",
            podvm.display(),
            dir.display(),
            podnet.display()
        ),
    )
    .unwrap();
    let hold = json!({
        "schema": 1,
        "verdicts": { "any": "fail" },
        "hold": { "netns": { "level": "none" }, "seccomp": { "level": "none" } }
    });
    for (status, stdout, want, missing) in [
        (3, hold.to_string(), &hold, None),
        (3, "no JSON".into(), &Value::Null, Some("not JSON")),
        (78, hold.to_string(), &Value::Null, Some("exit 78")),
    ] {
        let script = format!(
            "#!/bin/sh\n[ \"$*\" = 'pod doctor --json' ] || exit 64\nprintf '%s\\n' '{stdout}'\nexit {status}\n"
        );
        podrunner_unit::program_file::write(&podnet, script).unwrap();
        let report = report_with(Some(&file), json!([])).unwrap();
        assert_eq!(&report["hold"], want);
        assert!(report["kinds"][0].get("capability").is_none(), "{report}");
        let text = text(&report);
        assert!(text.contains("kind podvm: not served on this host:"), "{text}");
        if let Some(why) = missing {
            assert!(
                text.lines()
                    .any(|line| line.starts_with("missing: the hold of [unit.podvm] podnet:") && line.ends_with(why)),
                "{text}"
            );
        } else {
            assert!(text.contains("hold netns: none\n"), "{text}");
            assert!(text.contains("hold seccomp: none\n"), "{text}");
            assert!(text.contains("podnet pod: the seccomp hold is none"), "{text}");
        }
    }
}

/// T-113: the guest kinds give their record through their probe.
#[test]
fn doctor_knows_the_microvm_and_incus_kinds() {
    let dir = crate::scratch::Scratch::new("doctor-guest");
    // A fake podvm whose probe names a version that each kind takes.
    let fake = dir.join("podvm");
    podrunner_unit::program_file::write(
        &fake,
        "#!/bin/sh\necho '{\"podvm\":\"0.2.0-beta.14\",\"rung\":\"userland\"}'\n",
    )
    .unwrap();
    let u = |extra: &str| -> podrunner_slots::config::UnitConfig {
        toml::from_str(&format!("slots = 1\nbinary = \"{}\"\nimages = \"{}\"\n{extra}", fake.display(), dir.display()))
            .unwrap()
    };
    // T-629: with no /dev/kvm, no microvm kind.
    match crate::tiers::kvm() {
        Ok(()) => {
            let m = capability("microvm", &u("")).unwrap();
            assert_eq!((m.kind.as_str(), m.boundary), ("microvm", podrunner_schema::Boundary::Guest));
        }
        Err(_) => assert!(capability("microvm", &u("")).unwrap_err().starts_with("/dev/kvm: ")),
    }
    // T-534: no incusd, no incus kind.
    let incusd = dir.join("incus.socket");
    let _ = std::fs::remove_file(&incusd);
    assert!(capability_at("incus", &u(""), &incusd, &Default::default()).unwrap_err().starts_with("incusd: "));
    let _listen = std::os::unix::net::UnixListener::bind(&incusd).unwrap();
    let i = capability_at("incus", &u(""), &incusd, &Default::default()).unwrap();
    assert_eq!((i.kind.as_str(), i.boundary), ("incus", podrunner_schema::Boundary::Confined));
    assert!(
        capability_at("incus", &u("podnet = \"/bin/true\""), &incusd, &Default::default())
            .unwrap_err()
            .contains("podnet")
    );
    // T-181: the podvm kind with podnet gives the hold of `podnet pod
    // doctor`, as `serve` does; with none, `unknown`.
    let podnet = dir.join("podnet");
    podrunner_unit::program_file::write(
        &podnet,
        "#!/bin/sh\necho '{\"seccomp\":{\"level\":\"advisory\"},\"netns\":{\"level\":\"none\"}}'\n",
    )
    .unwrap();
    let held = capability("podvm", &u(&format!("podnet = \"{}\"", podnet.display()))).unwrap();
    assert_eq!(held.egress_hold, podrunner_schema::EgressHold::Advisory);
    assert_eq!(capability("podvm", &u("")).unwrap().egress_hold, podrunner_schema::EgressHold::Unknown);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn doctor_reads_the_kinds_and_the_tiers_of_the_file() {
    let dir = crate::scratch::Scratch::new("doctor");
    let file = dir.join("podrunner.toml");
    let arch = std::env::consts::ARCH;
    std::fs::write(
        &file,
        format!(
            "host = \"h\"\nallow_fake = true\n[[source]]\nname = \"local\"\nkind = \"fake\"\ntier = \"local\"\n\
             arch = \"{arch}\"\n[[source]]\nname = \"org\"\nkind = \"fake\"\ntier = \"trusted\"\narch = \"{arch}\"\n\
             [unit.fake]\nslots = 1\n[trust]\norganizations = [{{ login = \"example-org\" }}]\n[pin]\n"
        ),
    )
    .unwrap();
    let v = report_with(Some(&file), json!([])).unwrap();
    // The fake kind is `none`: it serves `local`, never `trusted`.
    assert_eq!(tier_of(&v, "local"), Some((Tier::Local, true)));
    assert_eq!(tier_of(&v, "org"), Some((Tier::Trusted, false)));
    // T-194: the line names the shortfall of each kind.
    let missing = v["missing"].to_string();
    assert!(missing.contains("fake: boundary None below Confined"), "{missing}");
    let t = text(&v);
    assert!(t.contains("kind fake: boundary none"), "{t}");
    assert!(t.contains("source org: tier trusted, not served"), "{t}");
    assert!(t.contains("missing: [[source]] org"), "{t}");
    // With no file, doctor says what it did not read.
    let bare = report_with(None, json!([])).unwrap();
    assert!(text(&bare).contains("missing: --config"), "{}", text(&bare));
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-504: a scale-set source with `allow_personal_account` names the
/// extra permission of the App; with the key off the list is empty.
#[test]
fn doctor_names_the_extra_permission_of_a_personal_account() {
    let dir = crate::scratch::Scratch::new("doctor-personal");
    let file = dir.join("podrunner.toml");
    let arch = std::env::consts::ARCH;
    for (key, want) in
        [("", false), ("allow_personal_account = false\n", false), ("allow_personal_account = true\n", true)]
    {
        std::fs::write(
            &file,
            format!(
                "host = \"h\"\n[[source]]\nname = \"alice-app\"\nkind = \"github-scaleset\"\ntier = \"trusted\"\n\
                 arch = \"{arch}\"\nsidecar = \"/bin/true\"\nconfig_url = \"https://github.com/alice/app\"\n\
                 scale_set = \"s\"\ntoken_file = \"/t\"\n{key}[trust]\nusers = [{{ login = \"alice\" }}]\n[pin]\n"
            ),
        )
        .unwrap();
        let v = report_with(Some(&file), json!([])).unwrap();
        let row = &v["sources"][0];
        let named =
            text(&v).contains("source alice-app: tier trusted, not served; the App needs Administration: write");
        let list = if want { json!(["Administration: write"]) } else { json!([]) };
        assert_eq!((&row["extra_permissions"], named), (&list, want), "{key}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
