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
fn an_action_by_a_tag_and_a_container_are_refused_by_name() {
    let uses = "on: push\njobs:\n  build:\n    runs-on: x\n    steps:\n      - uses: o/act@v4\n";
    let mut s = LocalSource::new("l", uses, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    s.next(10, Duration::ZERO).unwrap();
    let e = s.broker_job(&JobId::new("build.0").unwrap()).unwrap_err();
    assert!(e.contains("o/act@v4") && e.contains("full commit id"), "{e}");
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
    for bad in ["docker://alpine", "o/r", "o@0123456789abcdef0123456789abcdef01234567", "o/r@main"] {
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
