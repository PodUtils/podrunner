//! A run of the podrunner binary on the official runner with a `no_proxy`
//! entry that the runner cannot read (T-593). It has a test binary of its
//! own: the run sets the environment of the child process only, and a run
//! in the process of `tests/local.rs` holds the lock of the runner
//! directory until that process ends.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test local_proxy -- --ignored

/// T-593: with a proxy and the entry `::` in `no_proxy` and `NO_PROXY`, the
/// runner starts and each job runs its steps. The job does not get the
/// entry, and stderr names it once for the run.
#[test]
#[ignore]
fn a_no_proxy_entry_with_no_host_does_not_stop_the_runner() {
    let dir = std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR");
    let scratch = std::env::temp_dir().join(format!("podrunner-local-no-proxy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let flow = scratch.join("flow.yml");
    std::fs::write(
        &flow,
        "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - run: echo \"no_proxy=$no_proxy\"\n      \
         - run: case \",$no_proxy,$NO_PROXY,\" in *,::,*) exit 1;; esac\n  \
         b:\n    needs: a\n    runs-on: x\n    steps:\n      - run: echo b\n",
    )
    .unwrap();
    // A closed port of the loopback: a request that goes to the proxy fails.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let proxy = format!("http://127.0.0.1:{port}");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_podrunner"))
        .current_dir(&scratch)
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HOME", &scratch)
        .env("https_proxy", &proxy)
        .env("HTTPS_PROXY", &proxy)
        .env("no_proxy", "::")
        .env("NO_PROXY", " :: ,intra.example")
        .args(["run", "flow.yml", "--runner-dir", &dir, "-q", "--no-config"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    println!("{stdout}{stderr}");
    assert!(!stderr.contains("ArgumentNullException"), "{stderr}");
    assert!(out.status.success(), "{stdout}{stderr}");
    let ok = stdout.lines().filter(|l| l.starts_with("ok")).count();
    assert_eq!(ok, 2, "{stdout}");
    let named = stderr.lines().filter(|l| l.starts_with("podrunner: NO_PROXY:") && l.contains("\"::\"")).count();
    assert_eq!(named, 1, "one line for the run: {stderr}");
    let _ = std::fs::remove_dir_all(&scratch);
}
