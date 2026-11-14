//! The tool cache of `--toolcache DIR` in a unit (T-138).

/// Where a unit sees the tool cache of `--toolcache` (T-138).
pub const TOOLCACHE_MOUNT: &str = "/podrunner-toolcache";

/// The argv of a unit with a tool cache: the tool cache of the runner
/// (`_work/_tool`) gets a link for each file of the read-only mount, so a
/// `setup-*` action finds each tool and adds a new one beside them. A
/// read-only mount at the tool cache path itself breaks `setup-*`.
pub fn toolcache_argv(run_sh: &str, run_root: &str) -> Vec<String> {
    let script = format!("mkdir -p \"$1\" && cp -as {TOOLCACHE_MOUNT}/. \"$1\"/ && exec \"$2\"");
    ["/bin/sh", "-c", &script, "podrunner", &format!("{run_root}/_work/_tool"), run_sh].map(String::from).to_vec()
}
