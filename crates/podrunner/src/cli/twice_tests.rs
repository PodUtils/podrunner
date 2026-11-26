//! T-711: a flag that takes one value, given two times, is a usage error
//! (exit 64) that names the flag, for each command of the model. A flag
//! that the model marks as repeating keeps each value.

use super::{opts, run};

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// Each flag of each command that takes one value, given two times.
#[test]
fn a_flag_of_one_value_given_two_times_is_64_for_each_command() {
    let mut seen = 0;
    for cmd in crate::manual::all() {
        for f in cmd.flags.iter().filter(|f| f.value.is_some() && !f.many) {
            let mut a: Vec<String> = cmd.path.split_whitespace().map(str::to_string).collect();
            if a.is_empty() {
                continue;
            }
            let flag = f.spelling();
            a.extend([flag.clone(), "a".into(), flag.clone(), "b".into()]);
            let o = run(&a);
            assert_eq!(o.code, 64, "{a:?}: {}", o.stderr);
            assert!(o.stderr.starts_with(&format!("podrunner: {flag} is given two times")), "{a:?}: {}", o.stderr);
            seen += 1;
        }
    }
    assert!(seen > 20, "{seen}");
}

/// `doctor --run` takes the flags of `run`: the same rule.
#[test]
fn doctor_run_refuses_a_flag_of_one_value_given_two_times() {
    let o = run(&args(&["doctor", "--run", "w.yml", "--event", "push", "--event", "pull_request"]));
    assert_eq!(o.code, 64, "{}", o.stderr);
    assert!(o.stderr.starts_with("podrunner: --event is given two times"), "{}", o.stderr);
}

/// The first case of the report: `config init --host a --host b`.
#[test]
fn config_init_refuses_a_second_host() {
    let o = run(&args(&["config", "init", "--host", "a", "--host", "b"]));
    assert_eq!(o.code, 64, "{}", o.stderr);
    assert!(o.stderr.contains("--host is given two times: it takes one value"), "{}", o.stderr);
}

/// A flag that repeats keeps each value; a value that is the name of the
/// flag is a value, not a second flag.
#[test]
fn a_flag_that_repeats_keeps_each_value() {
    let o = opts("config init", &args(&["--org", "a", "--org", "b", "--user", "c", "--user", "d"])).ok().unwrap();
    let all = |k: &str| o.values.iter().filter(|(n, _)| n == k).map(|(_, v)| v.as_str()).collect::<Vec<_>>();
    assert_eq!((all("--org"), all("--user")), (vec!["a", "b"], vec!["c", "d"]));
    let o = opts("config init", &args(&["--host", "--host"])).ok().unwrap();
    assert_eq!(o.get("--host"), Some("--host".into()));
    // Each repeating flag of `run` passes the check; the parse of `run`
    // keeps each value.
    let mut a = vec!["run".to_string(), "/nonexistent/w.yml".into(), "--no-config".into()];
    for f in crate::manual::find("run").unwrap().flags.iter().filter(|f| f.many) {
        a.extend([f.spelling(), "x=y".into(), f.spelling(), "z=w".into()]);
    }
    let o = run(&a);
    assert!(!o.stderr.contains("given two times"), "{}", o.stderr);
}
