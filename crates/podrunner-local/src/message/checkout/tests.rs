//! T-640: a checkout of the same repository with `ref:` takes the commit
//! of the ref in the local git, not the local tree; a ref that the run
//! did not resolve fails the step with its name.

use std::collections::BTreeMap;
use std::time::Duration;

use pod_actions_expr::Value as XValue;
use pod_actions_workflow::Base;
use podrunner_schema::JobId;
use podrunner_source::JobSource;

use crate::LocalSource;
use crate::tree::refs::key;

/// The inputs of the checkout with `with` and the pins `pins`, or the
/// refusal of the job.
fn script(with: &str, pins: &[(&str, &str)]) -> Result<String, String> {
    let text = format!(
        "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n        with:\n          {with}\n"
    );
    let base = Base { github: XValue::object([("repository", XValue::String("me/app".into()))]), ..Base::default() };
    let mut s = LocalSource::new("l", &text, base, "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    s.set_pins(pins.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>());
    let job = s.broker_job(&JobId::new("a.0").unwrap())?;
    let m = podrunner_broker_fake::message::job_message("http://127.0.0.1:1/", 1, &job);
    Ok(format!("{} {}", m["steps"][0]["displayName"], m["steps"][0]["inputs"]["map"]))
}

#[test]
fn a_ref_of_the_same_repository_takes_its_commit() {
    let sha = "4b5a9ca".to_string() + &"0".repeat(33);
    let main = key("main");
    // The commit of the ref: its tarball, into `path`, with no strip.
    for with in ["ref: main\n          path: other", "repository: me/app\n          ref: main\n          path: other"] {
        let s = script(with, &[(&main, &sha)]).unwrap();
        assert!(s.contains(&format!("_checkout/{sha}.tar.gz")) && s.contains(r#""Key":"dest","Value":"other""#), "{with}: {s}");
        assert!(!s.contains("_checkout.tar.gz") && !s.contains(r#""Key":"strip""#), "{s}");
        assert!(s.contains(&format!("ref main at {sha}")), "the display names it: {s}");
    }
    // The ref is the commit of HEAD: the local tree.
    let s = script("ref: main", &[(&main, "")]).unwrap();
    assert!(s.contains("_checkout.tar.gz"), "{s}");
    // The default, an empty ref and an expression: the local tree.
    for with in ["path: x", "ref: ''", "ref:", "ref: ${{ github.sha }}"] {
        let s = script(with, &[]).unwrap();
        assert!(s.contains("_checkout.tar.gz") && !s.contains(r#""Key":"fail""#), "{with}: {s}");
    }
    // A ref with no commit fails the step with its name and the reason; a
    // ref that the run did not resolve, too. No fetch of the tree.
    for (with, pins, want) in [
        ("ref: nosuch", vec![(key("nosuch"), "!not a commit of the local git".to_string())], "not a commit"),
        ("ref: main", vec![], "the run resolved no commit"),
        ("ref: -x", vec![(key("-x"), sha.clone())], "starts with -"),
        ("ref: --output=/tmp/x", vec![], "starts with -"),
    ] {
        let pins: Vec<(&str, &str)> = pins.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let s = script(with, &pins).unwrap();
        assert!(s.contains(r#""Key":"fail""#) && s.contains(want) && !s.contains("_checkout"), "{with}: {s}");
    }
    // A ref with no quotes that YAML reads as a number is that text.
    let s = script("ref: 1234567", &[(&key("1234567"), &sha)]).unwrap();
    assert!(s.contains(&format!("_checkout/{sha}.tar.gz")), "{s}");
    // A ref that is not text, and the submodules of another commit, are
    // refused.
    assert!(script("ref: true", &[]).unwrap().contains("`ref:` is not text"));
    let e = script("ref: main\n          submodules: true", &[(&main, &sha)]).unwrap_err();
    assert!(e.contains("submodules"), "{e}");
}

/// T-643: the display of the step lists the keys that the stand-in does
/// not take, and no value.
#[test]
fn the_display_names_the_keys_that_the_stand_in_ignores() {
    let s = script("fetch-depth: 0\n          token: s3cr3t\n          path: x", &[]).unwrap();
    assert!(s.contains("the local tree; ignores fetch-depth, token"), "{s}");
    assert!(!s.contains("s3cr3t"), "{s}");
    assert!(!script("path: x", &[]).unwrap().contains("ignores"));
}
