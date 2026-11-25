//! The fakes pass their conformance suites, and each planted defect fails
//! the test of its number.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use pod_core::clock::{Clock, ManualClock};
use podrunner_schema::{
    Boundary, Capability, ContainerRoute, CredentialStore, EgressHold, Fact, JobId, LimitMode, PlainName, Principal,
    SecretDelivery, TreeKill, UnitId, UnitIds,
};
use podrunner_source::conformance as sc;
use podrunner_source::{Event, JobSource, SourceCode};
use podrunner_unit::UnitDriver;
use podrunner_unit::conformance::{self as uc, Behaviour, Outcome};

use crate::*;

/// A directory of its own for each fixture.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "podrunner-fake-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn capability(container: ContainerRoute) -> Capability {
    Capability {
        schema: 1,
        kind: "fake".into(),
        arch: "x86_64".into(),
        boundary: Boundary::Redirect,
        boundary_proof: None,
        principal: Principal::Shared,
        container,
        net: "none".into(),
        secret_delivery: vec![SecretDelivery::Files],
        credential_store: CredentialStore::PrivateDir,
        mount_modes: vec!["rw".into()],
        rootfs_fresh: Fact::Yes,
        tree_kill: TreeKill::Unverified,
        limits: BTreeMap::from([("wall".into(), LimitMode::Enforced)]),
        egress_hold: EgressHold::None,
        kvm: Fact::No,
    }
}

struct UnitFixture {
    unit: FakeUnit,
    ids: UnitIds,
    _dir: TempDir,
}

impl UnitFixture {
    fn new(container: ContainerRoute, plant: Option<UnitPlant>) -> UnitFixture {
        let dir = TempDir::new();
        let mut unit = FakeUnit::new(&dir.0, capability(container), "linux/amd64");
        if let Some(p) = plant {
            unit = unit.with_plant(p);
        }
        UnitFixture { unit, ids: UnitIds::default(), _dir: dir }
    }
}

impl uc::Fixture for UnitFixture {
    fn driver(&self) -> &dyn UnitDriver {
        &self.unit
    }
    fn image(&self) -> String {
        format!("stand-in@sha256:{}", "1".repeat(64))
    }
    fn platform(&self) -> String {
        "linux/amd64".into()
    }
    fn foreign_platform(&self) -> String {
        "linux/s390x".into()
    }
    fn payload(&self, b: Behaviour) -> Vec<String> {
        payload(b)
    }
    fn unit(&self) -> UnitId {
        self.ids.next()
    }
    fn state_dir(&self) -> Option<PathBuf> {
        Some(self._dir.0.clone())
    }
}

fn need_setsid() -> bool {
    std::process::Command::new("sh").args(["-c", "command -v setsid"]).output().is_ok_and(|o| o.status.success())
}

#[test]
fn the_fake_unit_passes_each_test() {
    assert!(need_setsid(), "the tests need setsid on the host");
    for (route, skipped) in [(ContainerRoute::None, &[14, 20, 27][..]), (ContainerRoute::Hooks, &[13, 27][..])] {
        let fx = UnitFixture::new(route, None);
        // The tests that see each process run in `tests/alone.rs`.
        let parallel: Vec<u32> = uc::tests().into_iter().map(|t| t.0).filter(|n| !uc::ALONE.contains(n)).collect();
        for r in uc::run(&fx, &parallel) {
            if skipped.contains(&r.number) {
                assert!(matches!(r.outcome, Outcome::Skip(_)), "{r}");
            } else {
                assert_eq!(r.outcome, Outcome::Pass, "{route:?}: {r}");
            }
        }
    }
}

/// T-042: a confined fake passes each test, the test 27 of the `confined`
/// boundary included. With no Landlock on the host, `confined` is refused.
#[test]
fn a_confined_fake_unit_passes_each_test() {
    let mut fx = UnitFixture::new(ContainerRoute::Hooks, None);
    let unit = std::mem::replace(&mut fx.unit, FakeUnit::new(&fx._dir.0, capability(ContainerRoute::None), "x"));
    fx.unit = match unit.confined() {
        Ok(u) => u,
        Err(why) => {
            assert!(podrunner_unit::confine::check().is_err(), "{why}");
            return;
        }
    };
    let parallel: Vec<u32> = uc::tests().into_iter().map(|t| t.0).filter(|n| !uc::ALONE.contains(n)).collect();
    for r in uc::run(&fx, &parallel) {
        if r.number == 13 {
            assert!(matches!(r.outcome, Outcome::Skip(_)), "{r}");
        } else {
            assert_eq!(r.outcome, Outcome::Pass, "confined: {r}");
        }
    }
}

#[test]
fn each_unit_plant_fails_its_test() {
    use UnitPlant::*;
    let plants = [
        (1, ProbeDrifts),
        (2, AcceptTag),
        (3, KillTopOnly),
        (4, AcceptForeign),
        (5, SharedRoot),
        (6, LooseMode),
        (7, CredentialInEnv),
        (8, WrongCode),
        (9, DeafSignal),
        (10, NoKill),
        (11, KeepFiles),
        (12, UnknownResidue),
        (13, ContainerWithNoRoute),
        (14, AcceptStdin),
        (15, DestroyOnce),
        (16, StartTwice),
        (17, FalseTreeKill),
        (18, ProbeWritesFile),
        (19, SharedHosts),
        (20, UntaggedContainer),
        (21, CredentialCopy),
        (22, ProofCanary),
        (25, TagOnly),
        (26, ShimDeathIgnored),
        (27, ClaimsConfined),
    ];
    assert_eq!(plants.len() + uc::ALONE.len(), uc::tests().len());
    for (n, plant) in plants {
        let route = if n == 14 || n == 20 { ContainerRoute::Hooks } else { ContainerRoute::None };
        let fx = UnitFixture::new(route, Some(plant));
        let r = uc::run(&fx, &[n]).remove(0);
        assert!(matches!(r.outcome, Outcome::Fail(_)), "{plant:?} passes test {n}: {r}");
    }
}

fn offer_line(at: u64, job: &str) -> String {
    format!(
        r#"{{"at_s":{at},"offer":{{"job_id":"{job}","source":"fake-1","labels":["x"],"arch":"x86_64","wait_deadline_s":60}}}}"#
    )
}

fn script() -> Vec<Step> {
    let text = [offer_line(0, "j-1"), offer_line(0, "j-2"), offer_line(0, "j-3")].join("\n");
    parse_script(&text).unwrap()
}

struct SourceFixture {
    plant: Option<SourcePlant>,
    lax_wire: bool,
}

impl sc::Fixture for SourceFixture {
    fn fresh(&self) -> Box<dyn JobSource> {
        let s = FakeSource::new("fake-1", Arc::new(ManualClock::new()), script());
        Box::new(match self.plant {
            Some(p) => s.with_plant(p),
            None => s,
        })
    }
    fn exchange(&self, line: &[u8]) -> String {
        if self.lax_wire {
            return podrunner_source::wire::reply_line(&podrunner_source::wire::Reply::Done {});
        }
        podrunner_source::serve_line(self.fresh().as_mut(), line)
    }
}

#[test]
fn the_fake_source_passes_each_test() {
    let results = sc::run(&SourceFixture { plant: None, lax_wire: false }, &[]);
    assert!(sc::not_passed(&results).is_empty(), "{results:#?}");
}

#[test]
fn each_source_plant_fails_its_test() {
    use SourcePlant::*;
    let plants = [
        (1, Some(NoName)),
        (2, Some(IgnoreZeroCapacity)),
        (3, Some(IgnoreCapacity)),
        (4, Some(AckAnything)),
        (5, Some(ClaimTwice)),
        (6, Some(MintUnclaimed)),
        (7, Some(RefuseReport)),
        (8, Some(RefuseAudit)),
        (9, Some(KeepWithdrawn)),
        (10, Some(KeepReleased)),
        (11, None),
        (12, None),
        (13, Some(AssignedUnclaimed)),
        (14, Some(RefuseHost)),
        (15, Some(RefuseCancelled)),
    ];
    for (n, plant) in plants {
        let fx = SourceFixture { plant, lax_wire: plant.is_none() };
        let r = sc::run(&fx, &[n]).remove(0);
        assert!(r.failure.is_some(), "{plant:?} passes test {n}: {r}");
    }
}

#[test]
fn the_script_follows_the_manual_clock() {
    let clock = Arc::new(ManualClock::new());
    let start = clock.now();
    let text = format!(
        "# a comment\n{}\n{{\"at_s\":10,\"cancel\":\"j-1\"}}\n{{\"at_s\":0,\"mint_error\":{{\"job_id\":\"j-1\",\"code\":\"credential-unavailable\"}}}}\n",
        offer_line(5, "j-1")
    );
    let mut s = FakeSource::new("fake-1", clock.clone(), parse_script(&text).unwrap());
    assert!(s.next(1, Duration::ZERO).unwrap().is_empty(), "no offer before 5 s");
    clock.advance(Duration::from_secs(5));
    let job = JobId::new("j-1").unwrap();
    assert!(matches!(&s.next(1, Duration::ZERO).unwrap()[..], [Event::Offer { offer }] if offer.job_id == job));
    s.ack(&job).unwrap();
    s.claim(&job, "r").unwrap();
    assert_eq!(s.mint(&job, "r").unwrap_err().code, SourceCode::CredentialUnavailable);
    assert_eq!(s.mint(&job, "r").unwrap().files.len(), 3, "the scripted error comes once");
    clock.advance(Duration::from_secs(5));
    assert!(matches!(&s.next(0, Duration::ZERO).unwrap()[..], [Event::Cancel { job_id }] if *job_id == job));
    assert_eq!(clock.now() - start, Duration::from_secs(10));
}

#[test]
fn a_bad_script_line_is_named() {
    let e = parse_script("\n{\"at_s\":1,\"teleport\":\"j\"}").unwrap_err();
    assert!(e.starts_with("line 2:"), "{e}");
    assert!(parse_script(r#"{"at_s":0,"cancel":"../x"}"#).is_err());
}

#[test]
fn the_canned_credential_names_no_secret_of_a_host() {
    let c = canned_credential(&JobId::new("j-1").unwrap(), "runner-u-1");
    assert_eq!(
        c.files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        [".runner", ".credentials", ".credentials_rsaparams"]
    );
    assert!(c.files.iter().all(|f| f.name_is_plain()));
}

/// T-493: the fake keeps a job of the owner of the offer, with the facts of
/// the real job, and ends a job of another owner.
#[test]
fn assigned_keeps_a_job_of_the_owner_and_ends_another() {
    use podrunner_source::{Assigned, RealJob};
    let line = r#"{"at_s":0,"offer":{"job_id":"j-1","source":"fake-1","labels":["x"],"arch":"x86_64","wait_deadline_s":60,"trust_inputs":{"event":"push","head_is_fork":"no","actor_class":"member","repo_visibility":"private","owner":"example-org","repo":"app","repo_id":7,"run_id":1,"ref":"refs/heads/main","workflow_ref":"w"}}}"#;
    let mut s = FakeSource::new("fake-1", Arc::new(ManualClock::new()), parse_script(line).unwrap());
    s.next(1, Duration::ZERO).unwrap();
    let job = JobId::new("j-1").unwrap();
    let owner = "example-org";
    s.ack(&job).unwrap();
    s.claim(&job, "r").unwrap();
    let real = |repository: String, fork: bool| RealJob {
        run_id: 99,
        job_name: "test".into(),
        event: "pull_request".into(),
        repository,
        head_repository: None,
        head_is_fork: Some(fork),
        actor: "a".into(),
        job_id: None,
    };
    match s.assigned(&job, &real(format!("{owner}/other"), true)).unwrap() {
        Assigned::Keep(t) => {
            assert_eq!((t.repo.as_str(), t.run_id), ("other", 99));
            assert_eq!(t.head_is_fork, podrunner_schema::Fact::Yes);
            // The ref and the id of the offer are not facts of the real job.
            assert_eq!((t.git_ref.as_str(), t.repo_id), ("", None));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(s.assigned(&job, &real("someone-else/x".into(), false)).unwrap(), Assigned::End { .. }));
}

/// The fake does not keep the fork fact of the offer for a real job that
/// gives none: the offer can be of another run and another event. A head
/// repository of the real job tells the fact by its name.
#[test]
fn assigned_keeps_no_fork_fact_of_the_offer() {
    use podrunner_schema::Fact;
    use podrunner_source::{Assigned, RealJob};
    let line = r#"{"at_s":0,"offer":{"job_id":"j-1","source":"fake-1","labels":["x"],"arch":"x86_64","wait_deadline_s":60,"trust_inputs":{"event":"push","head_is_fork":"no","actor_class":"member","repo_visibility":"private","owner":"example-org","repo":"app","run_id":1,"ref":"refs/heads/main","workflow_ref":"w"}}}"#;
    let mut s = FakeSource::new("fake-1", Arc::new(ManualClock::new()), parse_script(line).unwrap());
    s.next(1, Duration::ZERO).unwrap();
    let job = JobId::new("j-1").unwrap();
    s.ack(&job).unwrap();
    s.claim(&job, "r").unwrap();
    let real = |event: &str, head: Option<&str>, fork: Option<bool>| RealJob {
        run_id: 99,
        job_name: "test".into(),
        event: event.into(),
        repository: "example-org/app".into(),
        head_repository: head.map(String::from),
        head_is_fork: fork,
        actor: "a".into(),
        job_id: None,
    };
    for (why, r, want) in [
        ("another event, no fact", real("pull_request_target", None, None), Fact::Unknown),
        ("the same event, no fact", real("push", None, None), Fact::Unknown),
        ("a head of another name", real("pull_request_target", Some("up/app"), None), Fact::Yes),
        ("the base in another case", real("pull_request_target", Some("Example-Org/App"), None), Fact::No),
        ("an empty head", real("pull_request", Some(""), None), Fact::Unknown),
        ("the fact of the hook", real("pull_request_target", Some("up/app"), Some(true)), Fact::Yes),
        ("the fact of the hook, no head", real("pull_request", None, Some(false)), Fact::No),
    ] {
        match s.assigned(&job, &r).unwrap() {
            Assigned::Keep(t) => assert_eq!(t.head_is_fork, want, "{why}"),
            other => panic!("{why}: {other:?}"),
        }
    }
}
