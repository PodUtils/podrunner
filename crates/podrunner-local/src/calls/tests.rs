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
