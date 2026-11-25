//! The unit conformance suite on a real podvm, with a stand-in payload
//! (busybox). Ignored by default: CI has no podvm. Run:
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_PODVM_IMAGE=busybox@sha256:HEX \
//!   [PODRUNNER_PODVM_CONFINE=1] cargo test -p podrunner-podvm --test conformance -- --ignored
//!
//! STORE is a podvm store that holds the image (`podvm pull`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use podrunner_podvm::{Podvm, PodvmUnit};
use podrunner_schema::{UnitId, UnitIds};
use podrunner_unit::conformance::{self, Behaviour, Fixture, Outcome};
use podrunner_unit::{Exit, StartSpec, UnitDriver};

struct Real {
    unit: PodvmUnit,
    image: String,
    ids: UnitIds,
    state: PathBuf,
}

fn sh(s: &str) -> Vec<String> {
    vec!["sh".into(), "-c".into(), s.into()]
}

impl Fixture for Real {
    fn driver(&self) -> &dyn UnitDriver {
        &self.unit
    }
    fn image(&self) -> String {
        self.image.clone()
    }
    fn platform(&self) -> String {
        "linux/amd64".into()
    }
    fn foreign_platform(&self) -> String {
        "linux/s390x".into()
    }
    fn payload(&self, b: Behaviour) -> Vec<String> {
        match b {
            Behaviour::Exit(n) => sh(&format!("exit {n}")),
            Behaviour::Sleep => vec!["sleep".into(), "300".into()],
            Behaviour::Grandchild => sh("setsid sh -c 'sleep 300 & wait' & wait"),
            Behaviour::Marker => sh(&format!("echo x > {}; exec sleep 300", conformance::MARKER)),
            Behaviour::Escape => sh(&podrunner_fake_escape("")),
            // The pid file comes before the kill: from podvm beta.4 the launcher
            // of `run -d` sets a death signal, so the shell ends with it. The
            // kill waits for the go file: a launcher killed before it reports
            // the start fails `run -d` (exit 125).
            Behaviour::KillParent => sh(&podrunner_fake_escape(conformance::KILL_PARENT_AFTER_GO)),
            Behaviour::ReadOthers => sh(&format!(
                "cat ../*/{m} > {s} 2>/dev/null; echo own > {o}.part; cat {o}.part > {o}; exec sleep 300",
                m = conformance::MARKER,
                s = conformance::SEEN,
                o = conformance::OWN
            )),
        }
    }
    fn unit(&self) -> UnitId {
        self.ids.next()
    }
    /// The whole state: the tree of each unit is in the store of the
    /// units (`STATE/store`, T-545), not in its unit directory.
    fn state_dir(&self) -> Option<PathBuf> {
        Some(self.state.clone())
    }
}

/// The script of the payloads with an orphan (as `podrunner_fake::escape`).
fn podrunner_fake_escape(after: &str) -> String {
    // The pid file comes first; `after` comes after it.
    // Shell builtins only: a small image has no `mv`.
    let f = conformance::ESCAPED;
    format!("(setsid env -i sleep 300 & echo $! > {f}.part); read p < {f}.part; echo $p > {f}; {after}exec sleep 300")
}

/// A push writes a cache and a second unit can read it with a read-only
/// grant. The host keeps only the first unit's output.
fn cache_mounts(unit: &PodvmUnit, state: &std::path::Path, image: &str) {
    assert!(unit.probe().unwrap().mount_modes.iter().any(|mode| mode == "ro"));
    let dir = state.join("cache-proof");
    std::fs::create_dir_all(&dir).unwrap();
    let prepared = unit.prepare(image, "linux/amd64", false).unwrap();
    let mounted = |id: &str, target: &str, script: &str| {
        let unit_id = UnitId::parse(id).unwrap();
        let spec = StartSpec {
            unit: unit_id.clone(),
            image: prepared.clone(),
            argv: sh(script),
            env: Default::default(),
            credential: Default::default(),
            sockets: None,
            mounts: vec![(dir.clone(), target.to_string())],
            limits: Default::default(),
        };
        unit.start(&spec).unwrap();
        let exit = unit.wait(&unit_id, std::time::Duration::from_secs(120)).unwrap();
        let log = unit.unit_logs(&unit_id).unwrap_or_default();
        let residue = unit.destroy(&unit_id).unwrap();
        assert!(residue.is_clean(), "{residue:?}");
        assert_eq!(exit, Some(Exit::Code(0)), "{}", String::from_utf8_lossy(&log));
    };
    mounted("u-996", "/podrunner-cache", "printf from-push > /podrunner-cache/from-push");
    mounted(
        "u-997",
        "/podrunner-cache:ro",
        r#"test "$(cat /podrunner-cache/from-push)" = from-push || exit 10
            if (printf from-pr > /podrunner-cache/from-pr) 2>/dev/null; then exit 11; fi"#,
    );
    assert_eq!(std::fs::read_to_string(dir.join("from-push")).unwrap(), "from-push");
    assert!(!dir.join("from-pr").exists());
    std::fs::remove_dir_all(&dir).unwrap();
    println!("cache proof: RW persisted and the RO unit read without changing the host");
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[test]
#[ignore]
fn the_podvm_unit_passes_its_tests() {
    static N: AtomicU64 = AtomicU64::new(0);
    let state = std::env::temp_dir().join(format!(
        "podrunner-podvm-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let podvm = Podvm { binary: PathBuf::from(env("PODRUNNER_PODVM")) };
    let mut unit = PodvmUnit::new(podvm, &state, &PathBuf::from(env("PODRUNNER_PODVM_IMAGES"))).unwrap();
    let image = env("PODRUNNER_PODVM_IMAGE");
    // T-079: with PODRUNNER_PODVM_CONFINE=1 each unit runs under the
    // confine layer of podvm, proven first; test 27 then runs too.
    if std::env::var_os("PODRUNNER_PODVM_CONFINE").is_some() {
        unit = unit.with_confine();
        let proven = unit.verify_confine(&image, UnitId::parse("u-998").unwrap()).unwrap();
        assert!(proven, "the confine layer is not proven: {:?}", unit.confine_why());
        assert_eq!(unit.probe().unwrap().boundary, podrunner_schema::Boundary::Confined);
        if std::env::var_os("PODRUNNER_PODVM_CACHE_PROOF").is_some() {
            cache_mounts(&unit, &state, &image);
            let _ = std::fs::remove_dir_all(&state);
            return;
        }
    }
    // The self-test of the tree kill; test 17 then holds the claim to test 3.
    assert!(unit.verify_tree_kill(&image, UnitId::parse("u-999").unwrap()).unwrap(), "the tree kill is not proven");
    assert_eq!(unit.probe().unwrap().tree_kill, podrunner_schema::TreeKill::Verified);
    let fx: std::sync::Arc<dyn Fixture + Send + Sync> =
        std::sync::Arc::new(Real { unit, image, ids: UnitIds::default(), state: state.clone() });
    // PODRUNNER_CONFORMANCE_ONLY=26,3 runs only those tests: a loop of one
    // test finds a race.
    let only: Vec<u32> = std::env::var("PODRUNNER_CONFORMANCE_ONLY")
        .map(|v| v.split(',').map(|n| n.trim().parse().expect("PODRUNNER_CONFORMANCE_ONLY: a number")).collect())
        .unwrap_or_default();
    // One test at a time, each printed when it ends, each in a time limit:
    // a hang fails its own test and the suite goes on (T-099).
    let mut results = Vec::new();
    for (n, name) in conformance::tests().into_iter().filter(|(n, _)| only.is_empty() || only.contains(n)) {
        println!("unit {n:2} {name}: running");
        let r = conformance::run_bounded(fx.clone(), n, std::time::Duration::from_secs(300)).expect("a test");
        println!("{r}");
        results.push(r);
    }
    // The container tests (13 or 14, 20 and 24) skip by the container route
    // of the rung.
    let failed: Vec<_> = results.iter().filter(|r| matches!(r.outcome, Outcome::Fail(_))).map(|r| r.number).collect();
    let _ = std::fs::remove_dir_all(&state);
    assert!(failed.is_empty(), "failed: {failed:?}");
}
