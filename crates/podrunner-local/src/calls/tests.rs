use std::time::Duration;

use pod_actions_workflow::{Base, JobResult};
use podrunner_source::{Event, JobSource};

use super::*;
use crate::LocalSource;

const CALLER: &str = "on: push\njobs:\n  call:\n    uses: ./.github/workflows/called.yml\n    with:\n      name: podrunner\n    secrets:\n      TOKEN: ${{ secrets.API_KEY }}\n  after:\n    needs: call\n    runs-on: x\n    steps: [{run: \"echo ${{ needs.call.outputs.out }}\"}]\n";

const CALLED: &str = "on:\n  workflow_call:\n    inputs:\n      name:\n        type: string\n        required: true\n    secrets:\n      TOKEN:\n        required: true\n    outputs:\n      out:\n        value: ${{ jobs.build.outputs.v }}\njobs:\n  build:\n    runs-on: x\n    outputs:\n      v: built-${{ inputs.name }}\n    steps: [{run: \"echo ${{ inputs.name }}\"}]\n";

fn offers(s: &mut LocalSource) -> Vec<JobId> {
    s.next(10, Duration::ZERO)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e {
            Event::Offer { offer } => Some(offer.job_id),
            _ => None,
        })
        .collect()
}

#[test]
fn a_local_call_runs_the_jobs_of_its_workflow_with_its_inputs_and_secrets() {
    let root = std::env::temp_dir().join(format!("podrunner-calls-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
    std::fs::write(root.join(".github/workflows/called.yml"), CALLED).unwrap();
    let texts = Arc::new(Texts::new(root.clone(), None, BTreeMap::new()));
    let mut s =
        LocalSource::new_with("l", CALLER, Base::default(), "x86_64", Box::new(|_, _| unreachable!()), Some(texts))
            .unwrap();
    s.set_secrets(vec![("API_KEY".into(), "made-up-api-key".into()), ("OTHER".into(), "made-up-other".into())], None);
    let first = offers(&mut s);
    assert_eq!(first.iter().map(JobId::as_str).collect::<Vec<_>>(), ["call__build.0"]);
    let b = s.broker_job(&first[0]).unwrap();
    assert_eq!(b.secrets, [("TOKEN".to_string(), "made-up-api-key".to_string())], "only the passed secret");
    let m = podrunner_broker_fake::message::job_message("http://127.0.0.1:1/", 1, &b);
    assert!(m["contextData"]["inputs"].to_string().contains("podrunner"), "{}", m["contextData"]["inputs"]);
    s.complete(&first[0], JobResult::Success, vec![("v".into(), "built-podrunner".into())]).unwrap();
    let next = offers(&mut s);
    assert_eq!(next.iter().map(JobId::as_str).collect::<Vec<_>>(), ["after.0"]);
    let m = podrunner_broker_fake::message::job_message("http://127.0.0.1:1/", 2, &s.broker_job(&next[0]).unwrap());
    assert!(m["contextData"]["needs"].to_string().contains("built-podrunner"), "{}", m["contextData"]["needs"]);
    s.complete(&next[0], JobResult::Success, vec![]).unwrap();
    assert!(s.is_finished());
    let _ = std::fs::remove_dir_all(&root);
}

/// `--job` of a job that needs a call selects the jobs of the call, and
/// `--job` of the call selects them too (T-271).
#[test]
fn a_job_filter_selects_the_jobs_of_a_call() {
    let root = std::env::temp_dir().join(format!("podrunner-calls-filter-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
    std::fs::write(root.join(".github/workflows/called.yml"), CALLED).unwrap();
    for pick in ["after", "call"] {
        let texts = Arc::new(Texts::new(root.clone(), None, BTreeMap::new()));
        let mut s =
            LocalSource::new_with("l", CALLER, Base::default(), "x86_64", Box::new(|_, _| unreachable!()), Some(texts))
                .unwrap();
        s.set_secrets(vec![("API_KEY".into(), "made-up-api-key".into())], None);
        s.set_filter(&[pick.into()], &[]).unwrap();
        let first = offers(&mut s);
        assert_eq!(first.iter().map(JobId::as_str).collect::<Vec<_>>(), ["call__build.0"], "--job {pick}");
        assert_eq!(s.not_selected(&first[0]), None, "--job {pick}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_ids_and_the_secrets_of_calls() {
    assert_eq!(cell_id("call/build", 0).unwrap().as_str(), "call__build.0");
    assert_eq!(cell_id("call[1]/build", 2).unwrap().as_str(), "call-1__build.2");
    let run = vec![("A".to_string(), "1".to_string())];
    assert_eq!(values(&Secrets::Run, &run), run, "inherit gives each secret of the run");
    let passed = Secrets::Passed { from: Box::new(Secrets::Run), items: vec![("B".into(), "${{ secrets.A }}".into())] };
    assert_eq!(values(&passed, &run), [("B".to_string(), "1".to_string())]);
    assert_eq!(
        split("o/r/.github/workflows/x.yml@v1"),
        Some(("o/r".into(), ".github/workflows/x.yml".into(), "v1".into()))
    );
    let t = Texts::new(std::env::temp_dir(), None, BTreeMap::new());
    assert!(t.text("o/r/.github/workflows/x.yml@v1", None).unwrap_err().contains("not pinned"));
}

/// T-416: a called job takes the `permissions` of its caller, and its own
/// can only lower them, as on GitHub.
#[test]
fn a_called_job_takes_the_permissions_of_its_caller() {
    let root = std::env::temp_dir().join(format!("podrunner-calls-perm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
    let called =
        |perm: &str| format!("on: workflow_call\n{perm}jobs:\n  build:\n    runs-on: x\n    steps: [{{run: x}}]\n");
    let caller = |perm: &str, matrix: &str| {
        format!("on: push\njobs:\n  call:\n{matrix}{perm}    uses: ./.github/workflows/called.yml\n")
    };
    let id = "    permissions: {id-token: write}\n";
    let matrix = "    strategy: {matrix: {n: [1, 2]}}\n";
    // (caller, called, an id-token, the made-up token)
    for (up, down, mx, ask, token) in [
        (id, "", "", true, true),
        (id, "", matrix, true, true),
        ("", "", "", false, true),
        ("", "permissions: {id-token: write}\n", "", false, true),
        (id, "permissions: {contents: read}\n", "", false, true),
        (id, "permissions: write-all\n", "", true, true),
        ("    permissions: {}\n", "", "", false, false),
        (id, "permissions: {}\n", "", false, false),
    ] {
        std::fs::write(root.join(".github/workflows/called.yml"), called(down)).unwrap();
        let texts = Arc::new(Texts::new(root.clone(), None, BTreeMap::new()));
        let text = caller(up, mx);
        let mut s =
            LocalSource::new_with("l", &text, Base::default(), "x86_64", Box::new(|_, _| unreachable!()), Some(texts))
                .unwrap();
        s.set_secrets(vec![], Some("made-up-run-token".into()));
        for job in offers(&mut s) {
            let b = s.broker_job(&job).unwrap();
            let case = format!("{job}: caller {up:?}, called {down:?}");
            assert_eq!(b.id_token.is_some(), ask, "{case}");
            assert_eq!(b.token.is_some(), token, "{case}");
        }
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// T-434: the id-token of a called job names its own workflow in
/// `job_workflow_ref`, and the caller in `workflow_ref`; a `./` call in a
/// workflow of another repository reads and names that repository.
#[test]
fn the_id_token_of_a_called_job_names_its_workflow() {
    use std::io::Write;
    let root = std::env::temp_dir().join(format!("podrunner-calls-jwr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
    let job = |name: &str| {
        format!("  {name}:\n    runs-on: x\n    permissions: {{id-token: write}}\n    steps: [{{run: x}}]\n")
    };
    std::fs::write(root.join(".github/workflows/local.yml"), format!("on: workflow_call\njobs:\n{}", job("l")))
        .unwrap();
    // The store: o/lib at v1, with a workflow that calls a `./` workflow of
    // its own repository.
    let commit = "c".repeat(40);
    let store = root.join("store");
    std::fs::create_dir_all(&store).unwrap();
    let files = [
        (".github/workflows/outer.yml", "on: workflow_call\njobs:\n  in:\n    permissions: {id-token: write}\n    uses: ./.github/workflows/inner.yml\n".to_string()),
        (".github/workflows/inner.yml", format!("on: workflow_call\njobs:\n{}", job("i"))),
    ];
    let archive = store.join(podrunner_broker_fake::actions::archive_name("o/lib", &commit));
    std::fs::create_dir_all(archive.parent().unwrap()).unwrap();
    let mut b = tar::Builder::new(flate2::write::GzEncoder::new(
        std::fs::File::create(&archive).unwrap(),
        flate2::Compression::default(),
    ));
    for (path, text) in &files {
        let mut h = tar::Header::new_gnu();
        h.set_size(text.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, format!("lib-top/{path}"), text.as_bytes()).unwrap();
    }
    b.into_inner().unwrap().finish().unwrap().flush().unwrap();
    let pins = BTreeMap::from([("o/lib@v1".to_string(), commit.clone())]);
    let texts = Arc::new(Texts::new(root.clone(), Some(store), pins.clone()));
    let text = "on: push\npermissions: {id-token: write}\njobs:\n  loc:\n    uses: ./.github/workflows/local.yml\n  \
                rem:\n    uses: o/lib/.github/workflows/outer.yml@v1\n";
    let s = |v: &str| XValue::String(v.into());
    let github = XValue::object([
        ("repository", s("me/app")),
        ("ref", s("refs/heads/main")),
        ("sha", s("abc")),
        ("workflow_ref", s("me/app/.github/workflows/ci.yml@refs/heads/main")),
    ]);
    let mut src = LocalSource::new_with(
        "l",
        text,
        Base { github, ..Base::default() },
        "x86_64",
        Box::new(|_, _| unreachable!()),
        Some(texts),
    )
    .unwrap();
    src.set_pins(pins);
    let mut got = Vec::new();
    for j in offers(&mut src) {
        let c = src.broker_job(&j).unwrap().id_token.unwrap();
        assert_eq!(c["workflow_ref"], "me/app/.github/workflows/ci.yml@refs/heads/main", "{j}");
        got.push((j.as_str().to_string(), c["job_workflow_ref"].clone(), c["job_workflow_sha"].clone()));
    }
    got.sort_by(|a, b| a.0.cmp(&b.0));
    let want = [
        ("loc__l.0", "me/app/.github/workflows/local.yml@refs/heads/main", "abc"),
        ("rem__in__i.0", "o/lib/.github/workflows/inner.yml@v1", commit.as_str()),
    ];
    let want: Vec<_> =
        want.iter().map(|(j, r, c)| (j.to_string(), serde_json::json!(r), serde_json::json!(c))).collect();
    assert_eq!(got, want);
    let _ = std::fs::remove_dir_all(&root);
}
