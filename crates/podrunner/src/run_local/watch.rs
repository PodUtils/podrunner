//! `podrunner run --watch` and `--schedule` (T-142). `--watch` runs the
//! workflow, then again after each change of the tree (`--checkout DIR`, or
//! the current directory) or of the workflow file; a change while a run is
//! in progress cancels that run, as a signal does (T-141), and starts a new
//! one. `--schedule` waits for the next time of the crons of `on.schedule`
//! (UTC, as GitHub) and runs with the event `schedule`, again and again.
//! SIGINT or SIGTERM ends the loop.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pod_actions_workflow::{Kind, Node};
use pod_core::failure::Class;
use podrunner_local::cancel;
use podrunner_local::cron::{Cron, civil};

use super::{fail, run};
use crate::cli::Out;

/// How often the tree is read, and how long a change must rest.
const POLL: Duration = Duration::from_millis(500);

/// The crons of `on.schedule`.
pub(super) fn crons(on: &Node) -> Result<Vec<Cron>, String> {
    let Kind::Map(entries) = &on.kind else { return Ok(Vec::new()) };
    let Some((_, Node { kind: Kind::Seq(items), .. })) = entries.iter().find(|(k, _)| k.name == "schedule") else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in items {
        if let Kind::Map(e) = &item.kind
            && let Some((_, Node { kind: Kind::String(c), .. })) = e.iter().find(|(k, _)| k.name == "cron")
        {
            out.push(Cron::parse(c).map_err(|e| format!("line {}: {e}", item.mark.line))?);
        }
    }
    Ok(out)
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn utc(t: i64) -> String {
    let (y, m, d) = civil(t.div_euclid(86_400));
    let s = t.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}Z", s / 3600, s % 3600 / 60)
}

/// The stamp of the tree and of the workflow file.
fn stamp(tree: &Path, file: &Path) -> u64 {
    let f = std::fs::metadata(file).and_then(|m| m.modified()).ok();
    podrunner_local::tree::stamp(tree)
        ^ f.map_or(0, |t| t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64))
}

/// Wait until the stamp changes and rests for one poll; false at a signal.
fn wait_change(tree: &Path, file: &Path, from: u64) -> bool {
    loop {
        if cancel::requested() {
            return false;
        }
        std::thread::sleep(POLL);
        let now = stamp(tree, file);
        if now != from {
            std::thread::sleep(POLL);
            if stamp(tree, file) == now {
                return true;
            }
        }
    }
}

/// Run `args` (the options of `run` with `--watch` or `--schedule`).
pub fn repeat(args: &[String]) -> Out {
    let watch = args.iter().any(|a| a == "--watch");
    let schedule = args.iter().any(|a| a == "--schedule");
    if watch && schedule {
        return fail(Class::Usage, "--watch and --schedule exclude each other");
    }
    let mut rest: Vec<String> = args.iter().filter(|a| *a != "--watch" && *a != "--schedule").cloned().collect();
    if schedule {
        rest.extend(["--event".into(), "schedule".into()]);
    }
    let Some(file) = rest.first().filter(|f| !f.starts_with('-')).map(PathBuf::from) else {
        return fail(Class::Usage, format!("run needs a workflow FILE\n{}", super::USAGE));
    };
    let tree = rest
        .iter()
        .position(|a| a == "--checkout")
        .and_then(|i| rest.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
    cancel::install();
    let mut last = Out { code: 0, stdout: String::new(), stderr: String::new() };
    loop {
        if schedule {
            let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()));
            let next = text
                .and_then(|t| pod_actions_workflow::parse(&t).map_err(|e| e.to_string()))
                .and_then(|p| crons(&p.workflow.on))
                .map(|c| c.iter().filter_map(|c| c.next(now())).min());
            let at = match next {
                Ok(Some(at)) => at,
                Ok(None) => return fail(Class::Config, format!("{}: on.schedule has no cron time", file.display())),
                Err(e) => return fail(Class::Config, format!("{}: {e}", file.display())),
            };
            eprintln!("podrunner: the next run of the schedule is at {}", utc(at));
            while now() < at {
                if cancel::requested() {
                    return last;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        let before = stamp(&tree, &file);
        // A change while the run is in progress cancels it.
        let (changed, done) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let watcher = watch.then(|| {
            let (tree, file, changed, done) = (tree.clone(), file.clone(), changed.clone(), done.clone());
            std::thread::spawn(move || {
                while !done.load(Ordering::SeqCst) {
                    std::thread::sleep(POLL);
                    if stamp(&tree, &file) != before {
                        changed.store(true, Ordering::SeqCst);
                        cancel::request();
                        return;
                    }
                }
            })
        });
        last = run(&rest);
        done.store(true, Ordering::SeqCst);
        if let Some(w) = watcher {
            let _ = w.join();
        }
        // Each run prints its own lines; the caller prints none again.
        print!("{}", last.stdout);
        eprint!("{}", last.stderr);
        last.stdout.clear();
        last.stderr.clear();
        if cancel::requested() && !changed.load(Ordering::SeqCst) {
            return last;
        }
        cancel::reset();
        if watch {
            if changed.load(Ordering::SeqCst) {
                eprintln!("podrunner: the tree changed; the run starts again");
            } else if !wait_change(&tree, &file, before) {
                return last;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crons_of_on_schedule_and_a_change_of_the_tree() {
        let text = "on:\n  schedule:\n    - cron: '*/30 * * * *'\n    - cron: '0 5 * * 1'\n  push:\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
        let on = pod_actions_workflow::parse(text).unwrap().workflow.on;
        let c = crons(&on).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].next(0), Some(1800));
        assert_eq!(utc(1800), "1970-01-01T00:30Z");
        let bad = pod_actions_workflow::parse(&text.replace("*/30", "61")).unwrap().workflow.on;
        assert!(crons(&bad).unwrap_err().contains("line 3"), "{:?}", crons(&bad));
        let dir = std::env::temp_dir().join(format!("podrunner-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("w.yml");
        std::fs::write(&file, "x").unwrap();
        let before = stamp(&dir, &file);
        assert_eq!(stamp(&dir, &file), before);
        std::fs::write(dir.join("new.txt"), "y").unwrap();
        assert_ne!(stamp(&dir, &file), before, "a new file changes the stamp");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
