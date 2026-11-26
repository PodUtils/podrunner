//! The `NO_PROXY` of a runner (T-265, T-593). The official runner reads
//! `no_proxy` (else `NO_PROXY`) at its start when a proxy variable is set.
//! It splits the value at each comma, trims each entry, and splits the
//! entry at its first `:` into a host and a port, with no empty part. An
//! entry of colons only (`::`, `:`) then has no host, and the runner
//! stops with `ArgumentNullException` before its first request
//! (`RunnerWebProxy`, actions/runner 2.338.2).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

/// Whether the official runner cannot read the entry `entry` of a
/// `no_proxy` list: after the trim, it has colons only.
pub(crate) fn runner_cannot_read(entry: &str) -> bool {
    let e = entry.trim();
    !e.is_empty() && e.chars().all(|c| c == ':')
}

/// The `NO_PROXY` of a job: the loopback, then the entries of each variable
/// of `pod_proxy` that `vars` sets, once each, in their order. Both
/// variables join, as a tool of the job can read either one. The second
/// value is each entry that the runner cannot read: the list drops it.
pub(crate) fn job_list(vars: &BTreeMap<String, String>) -> (String, Vec<String>) {
    let mut list = vec!["127.0.0.1".to_string(), "localhost".to_string(), "::1".to_string()];
    let mut dropped = Vec::new();
    for k in pod_proxy::env::NO_PROXY_VARS_UPPER_FIRST {
        for e in vars.get(k).into_iter().flat_map(|v| v.split(',')).map(str::trim).filter(|e| !e.is_empty()) {
            match runner_cannot_read(e) {
                true if !dropped.iter().any(|d| d == e) => dropped.push(e.to_string()),
                true => {}
                false => list.push(e.to_string()),
            }
        }
    }
    let mut seen = BTreeSet::new();
    list.retain(|v| seen.insert(v.clone()));
    (list.join(","), dropped)
}

/// The entries of the `NO_PROXY` variables of `vars` that a job does not
/// get: `doctor --run` names them as `run` does (T-635).
pub fn dropped(vars: &BTreeMap<String, String>) -> Vec<String> {
    job_list(vars).1
}

/// The entries that this process named on stderr.
static NAMED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Name each entry of `dropped` on stderr, once for each process: a run
/// has many jobs.
pub(crate) fn name_dropped(dropped: &[String]) {
    let mut named = NAMED.lock().unwrap_or_else(|e| e.into_inner());
    for e in dropped {
        if named.insert(e.clone()) {
            eprintln!(
                "podrunner: NO_PROXY: the job does not get the entry {e:?}: it has no host, and the official runner stops at its start on it"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{job_list, runner_cannot_read};

    fn list(pairs: &[(&str, &str)]) -> (String, Vec<String>) {
        job_list(&pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    const LOOP: &str = "127.0.0.1,localhost,::1";

    /// The variables, the `NO_PROXY` of the job, and the dropped entries.
    type Row<'a> = (&'a [(&'a str, &'a str)], String, &'a [&'a str]);

    /// T-593: each entry that the runner 2.338.2 refused, measured with
    /// `Runner.Listener --version`, and each entry that it took.
    #[test]
    fn no_proxy_drops_each_entry_that_the_runner_cannot_read() {
        for bad in ["::", ":", ":::", " :: ", "\t::\t", "\u{a0}::\u{a0}"] {
            assert!(runner_cannot_read(bad), "{bad:?}");
        }
        for good in ["::1", "[::]", "[::1]", "::/0", ":80", "a:80", "x:", ":x", "::ffff:1.2.3.4", ".", "*", "", " "] {
            assert!(!runner_cannot_read(good), "{good:?}");
        }
    }

    /// T-593: the table of values. Each other entry keeps its order.
    #[test]
    fn the_no_proxy_of_a_job_keeps_each_other_entry_in_its_order() {
        let (none, colons): (&[&str], &[&str]) = (&[], &["::"]);
        let table: &[Row] = &[
            (&[], LOOP.into(), none),
            (&[("NO_PROXY", "")], LOOP.into(), none),
            (&[("no_proxy", "::")], LOOP.into(), colons),
            (&[("no_proxy", " :: ")], LOOP.into(), colons),
            (&[("NO_PROXY", "a,,b, ,c")], format!("{LOOP},a,b,c"), none),
            (&[("NO_PROXY", "z.example,::,10.0.0.0/8")], format!("{LOOP},z.example,10.0.0.0/8"), colons),
            (
                &[("NO_PROXY", "::,[::],::/0,:80,a:80,.,*,[::1]:22"), ("no_proxy", "::, b ,::")],
                format!("{LOOP},[::],::/0,:80,a:80,.,*,[::1]:22,b"),
                colons,
            ),
        ];
        for (vars, want, dropped) in table {
            let (got, got_dropped) = list(vars);
            assert_eq!(&got, want, "{vars:?}");
            assert_eq!(got_dropped, *dropped, "{vars:?}");
        }
        let (_, both) = list(&[("no_proxy", ":,:::,::")]);
        assert_eq!(both, [":", ":::", "::"], "each kind once, in its order");
    }

    /// T-593: the clients of podrunner (`pod_proxy`, the fetch of the
    /// actions) read such an entry as the address `::` only: it sends no
    /// host past the proxy.
    #[test]
    fn pod_proxy_takes_no_host_off_the_proxy_for_such_an_entry() {
        for bad in ["::", ":", ":::", " :: "] {
            for host in ["github.com", "127.0.0.2", "10.1.2.3", "::2", "example.com"] {
                assert!(!pod_proxy::env::no_proxy_matches(bad, host), "{bad:?} {host}");
            }
            let vars = [("https_proxy", "http://127.0.0.1:9"), ("no_proxy", bad)];
            let env = pod_proxy::Environment::read(&|n| {
                vars.iter().find(|(k, _)| *k == n).map(|(_, v)| std::ffi::OsString::from(*v))
            });
            assert!(env.proxy_for("github.com", &[]).unwrap().is_some(), "{bad:?}");
        }
    }
}
