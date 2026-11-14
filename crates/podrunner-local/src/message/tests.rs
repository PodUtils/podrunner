use pod_actions_expr::Value as XValue;
use pod_actions_workflow::Base;
use podrunner_schema::JobId;
use podrunner_source::JobSource;
use serde_json::json;
use std::time::Duration;

use super::*;
use crate::LocalSource;

#[test]
fn a_literal_stays_a_string_and_one_expression_is_a_basic_expression() {
    assert_eq!(token("echo hi").unwrap(), json!("echo hi"));
    assert_eq!(token("${{ matrix.os }}").unwrap(), json!({ "type": 3, "expr": "matrix.os" }));
    // A `$` and a single brace are literal text.
    assert_eq!(token("echo ${HOME} {x}").unwrap(), json!("echo ${HOME} {x}"));
}

#[test]
fn a_text_around_expressions_is_format_with_escaped_literals() {
    let t = token("it's {a} ${{ matrix.os }}-${{ 'x}}y' }}!").unwrap();
    assert_eq!(t["type"], 3);
    assert_eq!(t["expr"], "format('it''s {{a}} {0}-{1}!', (matrix.os), ('x}}y'))");
    // A bad expression is refused with the text.
    let e = token("${{ matrix. }}").unwrap_err();
    assert!(e.contains("matrix."), "{e}");
}

#[test]
fn context_data_has_the_forms_of_the_runner() {
    let v = XValue::from_json("{\"os\":\"a\",\"n\":[1,true,null]}").unwrap();
    assert_eq!(
        context_data(&v),
        json!({ "t": 2, "d": [
            { "k": "os", "v": "a" },
            { "k": "n", "v": { "t": 1, "a": [1.0, true, null] } },
        ] })
    );
}

const JOB: &str = "on: push\nenv:\n  W: w\n  N: 3\ndefaults:\n  run:\n    shell: bash\njobs:\n  build:\n    \
                   runs-on: x\n    strategy:\n      matrix:\n        os: [a]\n    env:\n      OS: ${{ matrix.os }}\n    \
                   outputs:\n      out: ${{ steps.s.outputs.v }}\n    defaults:\n      run:\n        working-directory: sub\n    \
                   steps:\n      - id: s\n        run: echo v=${{ matrix.os }} >> $GITHUB_OUTPUT\n      \
                   - run: echo two\n        if: always()\n        shell: sh\n        continue-on-error: true\n      \
                   - name: Third\n        run: echo three\n        env:\n          S: ${{ steps.s.outputs.v }}\n";

pub(super) fn job_of(text: &str) -> podrunner_broker_fake::Job {
    let mut s = LocalSource::new("l", text, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    s.broker_job(&JobId::new("build.0").unwrap()).unwrap()
}

#[test]
fn a_job_becomes_steps_env_outputs_and_contexts() {
    let job = job_of(JOB);
    let o = &job.overrides;
    let steps = o["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    // Step 1: its id names its context; the script is a format(); the
    // shell of the workflow and the directory of the job are its inputs.
    assert_eq!(steps[0]["contextName"], "s");
    assert_eq!(steps[0]["condition"], "success()");
    let inputs = &steps[0]["inputs"]["map"];
    assert_eq!(inputs[0]["Value"]["expr"], "format('echo v={0} >> $GITHUB_OUTPUT', (matrix.os))");
    assert_eq!(inputs[1], json!({ "Key": "shell", "Value": "bash" }));
    assert_eq!(inputs[2], json!({ "Key": "workingDirectory", "Value": "sub" }));
    // Step 2: no id; its own shell; `always()` keeps no implicit success().
    assert_eq!(steps[1]["contextName"], "__run");
    assert_eq!(steps[1]["condition"], "always()");
    assert_eq!(steps[1]["inputs"]["map"][1]["Value"], "sh");
    assert_eq!(steps[1]["continueOnError"], true);
    // Step 3: the second step with no id; its env reads `steps`.
    assert_eq!(steps[2]["contextName"], "__run_2");
    assert_eq!(steps[2]["displayName"], "Third");
    assert_eq!(steps[2]["environment"]["map"][0]["Value"]["expr"], "steps.s.outputs.v");
    // The env of the workflow, then of the job; a number is itself.
    let env = o["environmentVariables"].as_array().unwrap();
    assert_eq!(env[0]["map"][1], json!({ "Key": "N", "Value": 3.0 }));
    assert_eq!(env[1]["map"][0]["Value"]["expr"], "matrix.os");
    assert_eq!(o["jobOutputs"]["map"][0]["Value"]["expr"], "steps.s.outputs.v");
    assert_eq!(o["contextData"]["matrix"], json!({ "t": 2, "d": [{ "k": "os", "v": "a" }] }));
    assert_eq!(o["contextData"]["strategy"]["t"], 2);
    // The message of the broker carries them.
    let m = podrunner_broker_fake::message::job_message("http://127.0.0.1:1/", 1, &job);
    assert_eq!(m["steps"].as_array().unwrap().len(), 3);
    assert_eq!(m["contextData"]["matrix"]["d"][0]["v"], "a");
    assert_eq!(m["contextData"]["github"]["t"], 2);
}

#[test]
fn an_action_by_a_tag_is_a_reference_and_a_container_is_refused_by_name() {
    let uses = "on: push\njobs:\n  build:\n    runs-on: x\n    steps:\n      - uses: o/act/sub@v4\n";
    let mut s = LocalSource::new("l", uses, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    let job = s.broker_job(&JobId::new("build.0").unwrap()).unwrap();
    let m = podrunner_broker_fake::message::job_message("http://127.0.0.1:1/", 1, &job);
    let r = &m["steps"][0]["reference"];
    assert_eq!((r["name"].as_str(), r["ref"].as_str(), r["path"].as_str()), (Some("o/act"), Some("v4"), Some("sub")));
    let container = "on: push\njobs:\n  build:\n    runs-on: x\n    container: alpine\n    steps:\n      - run: x\n";
    let mut s = LocalSource::new("l", container, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    assert!(s.broker_job(&JobId::new("build.0").unwrap()).unwrap_err().contains("container"));
}

#[test]
fn an_action_pinned_by_commit_is_a_repository_reference_with_its_inputs() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let text = format!(
        "on: push\njobs:\n  build:\n    runs-on: x\n    steps:\n      - uses: o/act@{sha}\n        \
         with:\n          fetch-depth: 0\n          ref: ${{{{ github.sha }}}}\n      - uses: o/act@{sha}\n      \
         - uses: o/r/sub/dir@{sha}\n      - uses: ./local\n      - run: echo\n"
    );
    let job = job_of(&text);
    let steps = job.overrides["steps"].as_array().unwrap().clone();
    assert_eq!(
        steps[0]["reference"],
        json!({ "type": "repository", "repositoryType": "GitHub", "name": "o/act", "ref": sha })
    );
    assert_eq!(steps[0]["contextName"], "__o_act");
    assert_eq!(steps[0]["inputs"]["map"][0], json!({ "Key": "fetch-depth", "Value": 0.0 }));
    assert_eq!(steps[0]["inputs"]["map"][1]["Value"]["expr"], "github.sha");
    assert_eq!(steps[1]["contextName"], "__o_act_2");
    assert_eq!(steps[2]["reference"]["name"], "o/r");
    assert_eq!(steps[2]["reference"]["path"], "sub/dir");
    assert_eq!(steps[3]["reference"], json!({ "type": "repository", "repositoryType": "self", "path": "./local" }));
    assert_eq!(steps[4]["contextName"], "__run");
    for old in ["actions/upload-artifact@v3", "actions/download-artifact@v3.0.2", "actions/upload-artifact@v2"] {
        assert!(action(old, 1).unwrap_err().contains("2025-01-30"), "{old}");
    }
    assert!(action("actions/upload-artifact@v4", 1).is_ok());
    for bad in ["docker://alpine", "o/r", "o@0123456789abcdef0123456789abcdef01234567", "o/r@", "o/r@a b"] {
        assert!(action(bad, 1).is_err(), "{bad}");
    }
}

#[test]
fn actions_checkout_is_the_stand_in_of_the_local_tree() {
    let text = "on: push\njobs:\n  build:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n      \
                - uses: actions/checkout@v4\n        with:\n          path: sub/dir\n          fetch-depth: 0\n";
    let job = job_of(text);
    let steps = job.overrides["steps"].as_array().unwrap().clone();
    assert_eq!(steps[0]["reference"]["type"], "script");
    assert_eq!(steps[0]["contextName"], "__actions_checkout");
    let script = steps[0]["inputs"]["map"][0]["Value"]["expr"].as_str().unwrap();
    assert!(script.contains("{0}_checkout.tar.gz") && script.contains("(github.api_url)"), "{script}");
    assert!(script.contains("cd \"$GITHUB_WORKSPACE\"") && script.contains("gzip -dc | tar -xf -"), "{script}");
    assert!(!script.contains(" -C ") && !script.contains("-xz"), "{script}");
    assert_eq!(steps[1]["contextName"], "__actions_checkout_2");
    let script = steps[1]["inputs"]["map"][0]["Value"]["expr"].as_str().unwrap();
    assert!(script.contains("$GITHUB_WORKSPACE/sub/dir"), "{script}");
    let bad = "on: push\njobs:\n  build:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n        \
               with:\n          path: ../out\n";
    let mut s = LocalSource::new("l", bad, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    assert!(s.broker_job(&JobId::new("build.0").unwrap()).unwrap_err().contains("plain relative path"));
}

#[test]
fn a_pinned_container_services_and_a_docker_step_go_into_the_message() {
    let img = format!("alpine@sha256:{}", "c".repeat(64));
    let text = format!(
        "on: push\njobs:\n  build:\n    runs-on: x\n    container:\n      image: {img}\n      env:\n        A: ${{{{ matrix.x }}}}\n    \
         services:\n      cache:\n        image: {img}\n        ports: [6379]\n    steps:\n      - uses: docker://{img}\n        \
         with:\n          args: echo hi\n"
    );
    let job = job_of(&text);
    let o = &job.overrides;
    assert_eq!(o["jobContainer"]["map"][0], json!({ "Key": "image", "Value": img }));
    assert_eq!(o["jobContainer"]["map"][1]["Value"]["map"][0]["Value"]["expr"], "matrix.x");
    let cache = &o["jobServiceContainers"]["map"][0];
    assert_eq!(cache["Key"], "cache");
    assert_eq!(cache["Value"]["map"][1]["Value"], json!({ "type": 1, "seq": [6379.0] }));
    let step = &o["steps"][0];
    assert_eq!(step["reference"], json!({ "type": "containerRegistry", "image": img }));
    assert_eq!(step["contextName"], "__docker");
    assert_eq!(step["inputs"]["map"][0], json!({ "Key": "args", "Value": "echo hi" }));
    // A tag, a short digest and no image are refused by name.
    for bad in ["alpine:3", "alpine@sha256:abc", "@sha256:cccc"] {
        assert!(pinned(bad).is_err(), "{bad}");
        assert!(action(&format!("docker://{bad}"), 1).is_err(), "{bad}");
    }
    let no_image = "on: push\njobs:\n  build:\n    runs-on: x\n    services:\n      s:\n        ports: [1]\n    steps: [{run: x}]\n";
    let mut s = LocalSource::new("l", no_image, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    assert!(s.broker_job(&JobId::new("build.0").unwrap()).unwrap_err().contains("service s has no image"));
}

/// T-114: each step reads the `github` context of the run, and the broker
/// keeps its `api_url`.
#[test]
fn the_steps_read_the_github_context_of_the_run() {
    let github = XValue::from_json("{\"event_name\":\"pull_request\",\"repository\":\"local/workflow\"}").unwrap();
    let base = Base { github, ..Base::default() };
    let mut s = LocalSource::new("l", JOB, base, "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    let job = s.broker_job(&JobId::new("build.0").unwrap()).unwrap();
    let m = podrunner_broker_fake::message::job_message("http://127.0.0.1:1/", 1, &job);
    let d = m["contextData"]["github"]["d"].as_array().unwrap();
    let get = |k: &str| d.iter().find(|e| e["k"] == k).map(|e| e["v"].clone());
    assert_eq!(get("event_name"), Some(json!("pull_request")));
    assert_eq!(get("repository"), Some(json!("local/workflow")));
    assert_eq!(get("api_url"), Some(json!("http://127.0.0.1:1/")), "the broker keeps its api_url");
}

#[test]
fn the_secrets_and_the_token_are_secret_variables_and_permissions_none_gets_no_real_token() {
    let text = "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n  \
                b:\n    runs-on: x\n    permissions: {}\n    steps: [{run: x}]\n";
    let mut s = LocalSource::new("l", text, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    let vars = |s: &LocalSource, job: &str| {
        let job = s.broker_job(&JobId::new(job).unwrap()).unwrap();
        podrunner_broker_fake::message::job_message("http://127.0.0.1:1/", 1, &job)["variables"].clone()
    };
    let v = vars(&s, "a.0");
    assert_eq!(v["system.github.token"], json!({ "value": podrunner_broker_fake::message::TOKEN, "isSecret": true }));
    s.set_secrets(vec![("API_KEY".into(), "made-up-value".into())], Some("made-up-real".into()));
    let v = vars(&s, "a.0");
    assert_eq!(v["API_KEY"], json!({ "value": "made-up-value", "isSecret": true }));
    assert_eq!(v["system.github.token"]["value"], "made-up-real");
    assert_eq!(v["GITHUB_TOKEN"], json!({ "value": "made-up-real", "isSecret": true }));
    let v = vars(&s, "b.0");
    assert_eq!(v["system.github.token"]["value"], podrunner_broker_fake::message::TOKEN, "permissions: {{}}");
    assert_eq!(v["API_KEY"]["isSecret"], true);
}

#[test]
fn a_checkout_of_another_repository_takes_its_pinned_tree() {
    let text = "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n        with:\n          repository: o/lib\n          ref: v2\n          path: lib\n";
    let base = Base { github: XValue::object([("repository", XValue::String("me/app".into()))]), ..Base::default() };
    let mut s = LocalSource::new("l", text, base, "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    let job = JobId::new("a.0").unwrap();
    assert!(s.broker_job(&job).unwrap_err().contains("not pinned"));
    let sha = "c".repeat(40);
    s.set_pins(BTreeMap::from([("o/lib@v2".to_string(), sha.clone())]));
    let m = podrunner_broker_fake::message::job_message("http://127.0.0.1:1/", 1, &s.broker_job(&job).unwrap());
    let script = m["steps"][0]["inputs"]["map"][0]["Value"].to_string();
    assert!(
        script.contains(&format!("_actions/o_lib/{sha}.tar.gz")) && script.contains("--strip-components=1"),
        "{script}"
    );
    assert!(script.contains("lib"), "{script}");
}

#[test]
fn an_image_by_tag_takes_its_pinned_digest() {
    let text = "on: push\njobs:\n  a:\n    runs-on: x\n    container: busybox:1.37.0\n    services:\n      db:\n        image: redis\n    steps:\n      - uses: docker://busybox:1.37.0\n";
    let mut s = LocalSource::new("l", text, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    let job = JobId::new("a.0").unwrap();
    assert!(s.broker_job(&job).unwrap_err().contains("digest"), "a tag with no pin is refused");
    let (bb, redis) = (format!("busybox@sha256:{}", "a".repeat(64)), format!("redis@sha256:{}", "b".repeat(64)));
    s.set_pins(BTreeMap::from([
        ("docker://busybox:1.37.0".to_string(), bb.clone()),
        ("docker://redis:latest".to_string(), redis.clone()),
    ]));
    let m = podrunner_broker_fake::message::job_message("http://127.0.0.1:1/", 1, &s.broker_job(&job).unwrap());
    let text = m.to_string();
    assert!(text.contains(&bb) && text.contains(&redis), "{text}");
    // The display name keeps the text of the file; the runner pulls the reference.
    assert_eq!(m["steps"][0]["reference"]["image"], bb.as_str());
    assert_eq!(m["jobContainer"], bb.as_str());
}

#[test]
fn the_env_of_the_run_is_under_the_env_of_the_workflow() {
    let text = "on: push\nenv:\n  W: wf\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    let mut s = LocalSource::new("l", text, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    s.set_env(vec![("E".into(), "1".into()), ("W".into(), "run".into())]);
    let m = podrunner_broker_fake::message::job_message(
        "http://127.0.0.1:1/",
        1,
        &s.broker_job(&JobId::new("a.0").unwrap()).unwrap(),
    );
    let env = m["environmentVariables"].as_array().unwrap();
    assert_eq!(env[0]["map"][0], json!({ "Key": "E", "Value": "1" }), "the env of the run comes first");
    assert!(env.len() >= 2, "the workflow's env comes after it and wins: {env:?}");
}
