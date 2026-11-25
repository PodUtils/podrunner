//! T-480: the mode, the scope, the table `[cache]` and the bound of the
//! directory, at the edges of each input.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use podrunner_schema::{JobOffer, Tier};
use serde_json::{Value, json};

use super::store::{self, Mode, Uses};
use super::{CacheConfig, Mounts, decide, scope};

/// A kind that mounts a host directory and holds `:ro`.
const BOTH: Mounts = Mounts { host: true, ro: true };

fn offer(with: Value) -> JobOffer {
    let step: Value = serde_json::from_str(&crate::tests::offer(0, "j-1", with)).unwrap();
    serde_json::from_value(step["offer"].clone()).unwrap()
}

fn cfg(text: &str) -> Result<CacheConfig, String> {
    #[derive(serde::Deserialize)]
    struct File {
        cache: CacheConfig,
    }
    toml::from_str::<File>(&format!("[cache]\n{text}")).map(|f| f.cache).map_err(|e| e.to_string())
}

fn mode(tier: Tier, with: Value) -> Result<(Mode, String), &'static str> {
    decide(&cfg("").unwrap(), tier, &offer(with), BOTH).map_err(|n| n.why)
}

#[test]
fn only_a_trusted_job_of_a_writing_event_on_a_write_ref_writes() {
    let push = mode(Tier::Trusted, json!({}));
    assert_eq!(push, Ok((Mode::Rw, "default".into())));
    assert_eq!(mode(Tier::Local, json!({})).map(|d| d.0), Ok(Mode::Rw));
    for event in ["workflow_dispatch", "repository_dispatch", "schedule"] {
        assert_eq!(mode(Tier::Trusted, json!({ "trust_inputs": { "event": event } })).map(|d| d.0), Ok(Mode::Rw));
    }
    // The default branch, read by an event that an outsider can start:
    // read only, as on GitHub. Also when the offer asks for `rw`.
    let rw = json!({ "key": "default", "mode": "rw" });
    for event in ["pull_request_target", "workflow_run", "issue_comment", "pull_request", "merge_group", "other"] {
        let got = mode(Tier::Trusted, json!({ "trust_inputs": { "event": event }, "cache": rw }));
        assert_eq!(got.map(|d| d.0), Ok(Mode::Ro), "{event}");
    }
    // Another branch reads; the source can say `rw` for a push to it.
    let branch = json!({ "trust_inputs": { "ref": "refs/heads/feature" } });
    assert_eq!(mode(Tier::Trusted, branch).map(|d| d.0), Ok(Mode::Ro));
    let said = json!({ "trust_inputs": { "ref": "refs/heads/trunk" }, "cache": rw });
    assert_eq!(mode(Tier::Trusted, said).map(|d| d.0), Ok(Mode::Rw));
    // A head of a fork never writes, also in a tier that a rule did not
    // set (`local`) and with `rw` in its offer.
    for fork in ["yes", "unknown"] {
        let push = json!({ "trust_inputs": { "head_is_fork": fork }, "cache": rw });
        assert_eq!(mode(Tier::Local, push.clone()).map(|d| d.0), Ok(Mode::Ro), "{fork}");
        assert_eq!(mode(Tier::Trusted, push).map(|d| d.0), Ok(Mode::Ro), "{fork}");
    }
    // A ref in another case is another ref.
    assert_eq!(mode(Tier::Trusted, json!({ "trust_inputs": { "ref": "refs/heads/MAIN" } })).map(|d| d.0), Ok(Mode::Ro));
}

#[test]
fn an_untrusted_job_a_kind_with_no_mounts_and_a_bad_key_get_no_cache() {
    let rw = json!({ "cache": { "key": "default", "mode": "rw" } });
    assert!(mode(Tier::Untrusted, rw.clone()).unwrap_err().contains("untrusted"));
    let no_mount = Mounts { host: false, ro: true };
    assert_eq!(decide(&cfg("").unwrap(), Tier::Trusted, &offer(rw.clone()), no_mount).unwrap_err().code, "no-mount");
    // P2-C of the review: a kind that does not prove `:ro` gives a job
    // that would read no cache; a writer keeps `rw`.
    let rw_only = Mounts { host: true, ro: false };
    let pr = offer(json!({ "trust_inputs": { "event": "pull_request" }, "cache": { "key": "k", "mode": "rw" } }));
    assert_eq!(decide(&cfg("").unwrap(), Tier::Trusted, &pr, rw_only).unwrap_err().code, "no-ro-mount");
    let asked_ro = offer(json!({ "cache": { "key": "k", "mode": "ro" } }));
    assert_eq!(decide(&cfg("").unwrap(), Tier::Trusted, &asked_ro, rw_only).unwrap_err().code, "no-ro-mount");
    assert_eq!(decide(&cfg("").unwrap(), Tier::Trusted, &offer(json!({})), rw_only).map(|d| d.0), Ok(Mode::Rw));
    let none = json!({ "cache": { "key": "k", "mode": "none" } });
    assert!(mode(Tier::Trusted, none).is_err());
    let ro = json!({ "cache": { "key": "k", "mode": "ro" } });
    assert_eq!(mode(Tier::Trusted, ro), Ok((Mode::Ro, "k".into())));
    let key = |k: &str| mode(Tier::Trusted, json!({ "cache": { "key": k, "mode": "rw" } })).map(|d| d.1);
    for bad in ["", "a\nb", "a\u{0}b", &"k".repeat(129)] {
        assert!(key(bad).is_err(), "{bad:?}");
    }
    // A key is only bytes of the hash of the scope: a path or an
    // expression is a key like another.
    for good in [&"k".repeat(128), "..", ".", "../../etc", "a/b", "${{ github.ref }}", "Ünï"] {
        assert_eq!(key(good).as_deref(), Ok(good), "{good:?}");
    }
}

#[test]
fn a_scope_is_one_source_forge_repository_tier_and_key() {
    let o = offer(json!({}));
    let id = |s: &str, o: &JobOffer, t: Tier, k: &str| scope(s, o, t, k).0;
    let base = id("a", &o, Tier::Trusted, "k");
    assert_eq!(base.len(), 32);
    assert!(base.bytes().all(|b| b.is_ascii_hexdigit()));
    let other_repo = offer(json!({ "trust_inputs": { "repo": "lib" } }));
    let other_owner = offer(json!({ "trust_inputs": { "owner": "other-org" } }));
    let other_forge = offer(json!({ "trust_inputs": { "forge": "git.example.org" } }));
    for other in [
        id("b", &o, Tier::Trusted, "k"),
        id("a", &other_repo, Tier::Trusted, "k"),
        id("a", &other_owner, Tier::Trusted, "k"),
        id("a", &other_forge, Tier::Trusted, "k"),
        id("a", &o, Tier::Untrusted, "k"),
        id("a", &o, Tier::Local, "k"),
        id("a", &o, Tier::Trusted, "k2"),
        // The length of each part is in the bytes: no shift of a byte
        // from one part to the next gives the same scope.
        id("a", &offer(json!({ "trust_inputs": { "owner": "example-or", "repo": "gapp" } })), Tier::Trusted, "k"),
    ] {
        assert_ne!(other, base);
    }
    // A repository made again with the same name has another id.
    let with_id = |n: u64| offer(json!({ "trust_inputs": { "repo_id": n } }));
    let (one, two) = (id("a", &with_id(1), Tier::Trusted, "k"), id("a", &with_id(2), Tier::Trusted, "k"));
    assert!(one != two && one != base && two != base);
    // GitHub has no case in the names of an owner, a repository and a host.
    let upper = offer(json!({ "trust_inputs": { "owner": "Example-Org", "repo": "App", "forge": "GitHub.com" } }));
    assert_eq!(id("a", &upper, Tier::Trusted, "k"), base);
    assert!(scope("a", &o, Tier::Trusted, "k").1.contains("\"repo\":\"app\""));
}

#[test]
fn the_table_refuses_each_bad_value() {
    let c = cfg("").unwrap();
    assert_eq!((c.max_bytes, c.total_bytes, c.max_age_s), (10 << 30, 50 << 30, 7 * 24 * 3600));
    assert_eq!((c.write_refs, c.key, c.dir), (vec!["refs/heads/main".to_string()], "default".into(), None));
    for bad in [
        "dir = \"\"",
        "dir = \"cache\"",
        "dir = \"./cache\"",
        "dir = \"/var/../cache\"",
        "dir = \"/var/./cache\"",
        "dir = \"/a:b\"",
        "max_bytes = 0",
        "max_age_s = 0",
        "max_bytes = 10\ntotal_bytes = 9",
        "write_refs = [\"main\"]",
        "write_refs = [\"refs/heads/a b\"]",
        "key = \"\"",
        "env = { PODRUNNER_CACHE = \"x\" }",
        "env = { github_token = \"x\" }",
        "env = { ACTIONS_RUNTIME_TOKEN = \"x\" }",
        "env = { PATH = \"bin\" }",
        // A variable that loads code or settings into each process of the
        // job: a cache is not a place for them.
        "env = { LD_PRELOAD = \"x\" }",
        "env = { ld_library_path = \"x\" }",
        "env = { HOME = \"x\" }",
        "env = { BASH_ENV = \"x\" }",
        "env = { ENV = \"x\" }",
        "env = { NODE_OPTIONS = \"x\" }",
        "env = { JAVA_TOOL_OPTIONS = \"x\" }",
        // A variable that makes a tool load its settings or code from the
        // cache (the second review of T-480): only a known cache variable.
        "env = { XDG_CONFIG_HOME = \"x\" }",
        "env = { XDG_DATA_HOME = \"x\" }",
        "env = { PYTHONUSERBASE = \"x\" }",
        "env = { GEM_PATH = \"x\" }",
        "env = { GEM_HOME = \"x\" }",
        "env = { CARGO_HOME = \"x\" }",
        "env = { RUSTUP_HOME = \"x\" }",
        "env = { npm_config_userconfig = \"x\" }",
        "env = { npm_config_prefix = \"x\" }",
        "env = { npm_config_globalconfig = \"x\" }",
        "env = { GOPATH = \"x\" }",
        "env = { GOFLAGS = \"x\" }",
        "env = { MAVEN_OPTS = \"x\" }",
        "env = { GRADLE_OPTS = \"x\" }",
        "env = { GRADLE_USER_HOME = \"x\" }",
        // ccache reads `ccache.conf` in its directory: `prefix_command`
        // runs a command.
        "env = { CCACHE_DIR = \"x\" }",
        "env = { NPM_CONFIG_CACHE_X = \"x\" }",
        "env = { runner_tool_cache = \"x\" }",
        "env = { RUNNER_TEMP = \"x\" }",
        "env = { 1X = \"x\" }",
        "env = { \"A-B\" = \"x\" }",
        "env = { \"\" = \"x\" }",
        "env = { X = \"\" }",
        "env = { X = \".\" }",
        "env = { X = \"..\" }",
        "env = { X = \"a/../b\" }",
        "env = { X = \"a/./b\" }",
        "env = { X = \"a//b\" }",
        "env = { X = \"a/\" }",
        "env = { X = \"/abs\" }",
        "other = 1",
    ] {
        assert!(cfg(bad).is_err(), "{bad}");
    }
    let good = "dir = \"/var/cache/podrunner\"\nmax_bytes = 1\ntotal_bytes = 1\nmax_age_s = 1\nwrite_refs = []\n\
                env = { npm_config_cache = \"npm\", PLAYWRIGHT_BROWSERS_PATH = \"ms/playwright\", RUNNER_TOOL_CACHE = \"tool\" }";
    let c = cfg(good).unwrap();
    assert_eq!(c.env.len(), 3);
    // With no write ref, only a source that says `rw` gives a writer.
    assert_eq!(decide(&c, Tier::Trusted, &offer(json!({})), BOTH).map(|d| d.0), Ok(Mode::Ro));
    // Each cache variable of githome's G-20 is taken.
    let g20 = "env = { npm_config_cache = \"npm\", PNPM_STORE_DIR = \"pnpm\", BUN_INSTALL_CACHE_DIR = \"bun\", \
               PLAYWRIGHT_BROWSERS_PATH = \"pw\", RUNNER_TOOL_CACHE = \"tool\" }";
    assert_eq!(cfg(g20).unwrap().env.len(), 5);
    for name in super::ENV_NAMES {
        assert!(cfg(&format!("env = {{ {name} = \"x\" }}")).is_ok(), "{name}");
    }
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("podrunner-cache-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_cache_counts_the_bytes_of_the_run_and_follows_no_link() {
    let d = tmp("open");
    let root = d.join("cache");
    let outside = d.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("big"), vec![0u8; 4096]).unwrap();
    let id = "0123456789abcdef0123456789abcdef";
    let (data, _) = store::open(&root, id, "s\n", Mode::Rw, ["npm"].into_iter()).unwrap();
    std::fs::write(data.join("old"), "12345").unwrap();
    // A job of an earlier run put links where the paths of `env` go.
    std::os::unix::fs::symlink(&outside, data.join("pnpm")).unwrap();
    std::os::unix::fs::symlink(outside.join("big"), data.join("link-to-big")).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let (again, since) = store::open(&root, id, "s\n", Mode::Rw, ["npm", "pnpm/store", "a/b"].into_iter()).unwrap();
    assert_eq!(again, data);
    assert!(data.join("a/b").is_dir() && data.join("npm").is_dir());
    assert!(!outside.join("store").exists(), "the host made a directory through a link");
    std::fs::write(data.join("npm/new"), "123").unwrap();
    assert_eq!(store::measure(&data, since), (8, 3), "a link is not followed");
    assert_eq!(std::fs::metadata(root.join(id)).unwrap().permissions().mode() & 0o777, 0o700);
    // A new cache has its count: no bound walks a cache of `ro` jobs.
    let fresh = "00112233445566778899aabbccddeeff";
    store::open(&root, fresh, "s\n", Mode::Ro, std::iter::empty()).unwrap();
    assert_eq!(std::fs::read_to_string(root.join(fresh).join("bytes")).unwrap(), "0");
    // The data of a scope that is a link is refused.
    let linked = "fedcba9876543210fedcba9876543210";
    std::os::unix::fs::symlink(&outside, root.join(linked)).unwrap();
    assert!(store::open(&root, linked, "s\n", Mode::Rw, std::iter::empty()).is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_bound_removes_whole_caches_and_keeps_the_used_ones() {
    let d = tmp("bound");
    let root = d.join("cache");
    let ids = ["a", "b", "c", "d"].map(|c| c.repeat(32));
    for (i, id) in ids.iter().enumerate() {
        let (data, _) = store::open(&root, id, "s\n", Mode::Rw, std::iter::empty()).unwrap();
        std::fs::write(data.join("f"), vec![0u8; 100]).unwrap();
        std::thread::sleep(Duration::from_millis(15 * (i as u64 + 1)));
    }
    // A job took the write bit of a directory away.
    let (data, _) = store::open(&root, &ids[0], "s\n", Mode::Rw, ["ro"].into_iter()).unwrap();
    std::fs::write(data.join("ro/f"), "x").unwrap();
    std::fs::set_permissions(data.join("ro"), std::fs::Permissions::from_mode(0o555)).unwrap();
    let mut c = cfg("").unwrap();
    (c.max_bytes, c.total_bytes) = (150, 250);
    let n = |id: &str| store::size(&root.join(id).join("data"));
    // The tick renames each cache of the plan; the thread removes it.
    let bound = |used: &[&str], just: &str, n: u64, now: SystemTime| {
        let uses = uses_of(used, just);
        let gone = store::bound(&root, &c, &uses, just, 0, n, now);
        for (id, _) in &gone {
            store::remove(&store::evict(&root, id).unwrap()).unwrap();
        }
        gone
    };
    // `a` was used last; `b` is the oldest, `c` is in use.
    let used = [ids[2].as_str()];
    let gone = bound(&used, &ids[0], n(&ids[0]), SystemTime::now());
    let names: Vec<_> = gone.iter().map(|(id, why)| (&id[..1], *why)).collect();
    assert_eq!(names, [("b", "above [cache] total_bytes"), ("d", "above [cache] total_bytes")]);
    assert!(root.join(&ids[0]).is_dir() && root.join(&ids[2]).is_dir());
    // Above `max_bytes`, the cache of the job is emptied whole.
    std::fs::write(root.join(&ids[0]).join("data/g"), vec![0u8; 100]).unwrap();
    let gone = bound(&used, &ids[0], n(&ids[0]), SystemTime::now());
    assert_eq!(gone, [(ids[0].clone(), "above [cache] max_bytes")]);
    assert!(!root.join(&ids[0]).exists());
    // A cache with no use in `max_age_s` goes; one in use stays.
    let late = SystemTime::now() + Duration::from_secs(c.max_age_s + 10);
    let (data, _) = store::open(&root, &ids[1], "s\n", Mode::Rw, std::iter::empty()).unwrap();
    drop(data);
    let gone = bound(&used, &ids[1], n(&ids[1]), late);
    assert_eq!(gone, [(ids[1].clone(), "no use in [cache] max_age_s")]);
    assert!(root.join(&ids[2]).is_dir());
    // The thread of the removals takes each trash that a stop left.
    let left = store::evict(&root, &ids[2]).unwrap();
    assert_eq!(store::trash(&root), [left]);
    let _ = std::fs::remove_dir_all(&d);
}

/// The scopes `used`, and `just`, which its ending job holds.
fn uses_of(used: &[&str], just: &str) -> Mutex<Uses> {
    let mut u = Uses::default();
    let mut held: BTreeMap<String, u32> = used.iter().map(|id| (id.to_string(), 1)).collect();
    *held.entry(just.to_string()).or_default() += 1;
    u.set_held(held);
    Mutex::new(u)
}

/// A P3 of the second review of T-480: the thread writes no count of a
/// scope that a job took after its walk began (a new `rw` job removed the
/// count), and empties no cache that another job holds.
#[test]
fn the_bound_writes_no_count_of_a_scope_that_a_job_took() {
    let d = tmp("race");
    let root = d.join("cache");
    let (a, b) = ("a".repeat(32), "b".repeat(32));
    for id in [&a, &b] {
        let (data, _) = store::open(&root, id, "s\n", Mode::Rw, std::iter::empty()).unwrap();
        std::fs::write(data.join("f"), vec![0u8; 100]).unwrap();
    }
    let mut c = cfg("").unwrap();
    (c.max_bytes, c.total_bytes) = (50, 1000);
    let count = |id: &str| std::fs::read_to_string(root.join(id).join("bytes")).ok();
    // A new `rw` job took `a` after the end of the job of `a` was sent.
    let uses = uses_of(&[], &a);
    let stamp = uses.lock().unwrap().stamp(&a);
    uses.lock().unwrap().take(&a);
    let gone = store::bound(&root, &c, &uses, &a, stamp, 100, SystemTime::now());
    assert!(gone.is_empty(), "{gone:?}");
    assert_eq!(count(&a), None, "a count of a cache that a job writes");
    // With no other holder, the count is written; `b` gets its own.
    let uses = uses_of(&[], &a);
    (c.max_bytes, c.total_bytes) = (1000, 1000);
    store::bound(&root, &c, &uses, &a, stamp, 100, SystemTime::now());
    assert_eq!((count(&a).as_deref(), count(&b).as_deref()), (Some("100"), Some("100")));
    // A scope in use gets no count from a walk.
    std::fs::remove_file(root.join(&b).join("bytes")).unwrap();
    store::bound(&root, &c, &uses_of(&[&b], &a), &a, stamp, 100, SystemTime::now());
    assert_eq!(count(&b), None);
    let _ = std::fs::remove_dir_all(&d);
}

/// A trash that another removal took first is gone: no false line
/// "the removal failed".
#[test]
fn the_removal_of_a_trash_that_is_gone_is_done() {
    let d = tmp("gone");
    assert!(store::remove(&d.join("trash-x")).is_ok());
    let _ = std::fs::remove_dir_all(&d);
}

/// The owner gets the bits of a directory back by its own name: a link
/// that a job put there is not followed.
#[test]
fn the_removal_follows_no_link() {
    let d = tmp("nofollow");
    let outside = d.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o750)).unwrap();
    std::os::unix::fs::symlink(&outside, d.join("link")).unwrap();
    store::open_up(&d.join("link"));
    assert_eq!(std::fs::metadata(&outside).unwrap().permissions().mode() & 0o777, 0o750);
    // A directory with no bit at all gets them back.
    let shut = d.join("shut");
    std::fs::create_dir(&shut).unwrap();
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
    store::open_up(&shut);
    assert_eq!(std::fs::metadata(&shut).unwrap().permissions().mode() & 0o777, 0o700);
    let _ = std::fs::remove_dir_all(&d);
}

/// P2-A of the second review of T-480: a job of another run keeps a `ro`
/// cache of its scope only when the hook gives the event of its facts and
/// no head of a fork; it never keeps `rw`.
#[test]
fn a_job_of_another_run_keeps_only_a_ro_cache_of_its_event_with_no_fork() {
    let c = cfg("").unwrap();
    let pr = offer(json!({ "trust_inputs": { "event": "pull_request", "ref": "refs/pull/7/merge" } }));
    let facts = |event: &str| {
        let mut t = pr.trust_inputs.clone();
        t.run_id = 2;
        t.event = serde_json::from_value(json!(event)).unwrap();
        t
    };
    let real = |event: &str, fork: Option<bool>| podrunner_source::RealJob {
        run_id: 2,
        job_name: "test".into(),
        event: event.into(),
        repository: "example-org/app".into(),
        head_repository: None,
        head_is_fork: fork,
        actor: "a".into(),
        job_id: None,
        git_ref: None,
        repo_id: None,
    };
    let grant = |mode: Mode, t: &podrunner_schema::offer::TrustInputs| store::Grant {
        id: super::scope_of("s", t, Tier::Trusted, "default").0,
        root: "/r".into(),
        data: "/r/d".into(),
        mode,
        since: SystemTime::now(),
        written: None,
    };
    let holds = |g: &store::Grant, r: &podrunner_source::RealJob, f: &podrunner_schema::offer::TrustInputs, m| {
        super::holds(&c, g, "s", &pr, r, f, Tier::Trusted, m)
    };
    let ro = grant(Mode::Ro, &pr.trust_inputs);
    let f = facts("pull_request");
    assert!(holds(&ro, &real("pull_request", Some(false)), &f, BOTH));
    assert!(holds(&ro, &real("pull_request", None), &f, BOTH), "the hook does not know the head");
    assert!(!holds(&ro, &real("pull_request", Some(true)), &f, BOTH), "a head of a fork");
    assert!(!holds(&ro, &real("pull_request_target", Some(false)), &f, BOTH), "another event than the facts");
    assert!(!holds(&ro, &real("pull_request", Some(false)), &f, Mounts { host: true, ro: false }));
    // A `rw` grant never holds: the source can give the ref of the offer.
    let push = offer(json!({}));
    let mut pf = push.trust_inputs.clone();
    pf.run_id = 4;
    let rw = grant(Mode::Rw, &push.trust_inputs);
    assert!(!super::holds(&c, &rw, "s", &push, &real("push", Some(false)), &pf, Tier::Trusted, BOTH));
}
