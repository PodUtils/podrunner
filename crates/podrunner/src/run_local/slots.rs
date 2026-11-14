//! The executors of a run and the drive (T-132): one executor, or with
//! `--jobs N` one for each slot, each with its own state directory, so
//! that the jobs of two slots share no unit, file or daemon.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use podrunner_local::LocalSource;
use podrunner_local::drive::{Executor, Line, drive, drive_parallel};
use podrunner_local::exec::Live;
use podrunner_schema::JobId;

use super::{Args, Out, concurrency, inspect, routes};

/// The concurrency groups of the jobs (T-137): the lock directory, the
/// repository and the sink of the notes.
pub(super) type Groups = (PathBuf, String, Arc<Mutex<Vec<String>>>);

/// The logs and the blobs of each job that ran.
type Taken = (Vec<(JobId, String)>, Vec<(JobId, Vec<(String, String)>)>);

/// The executors of the run: `--jobs` of them (one for a dry run), each
/// with the live sink and the groups.
pub(super) fn build(
    a: &Args,
    common: &routes::Common,
    state: &Path,
    live: Option<Live>,
    groups: Option<Groups>,
) -> Result<Vec<Box<dyn Executor>>, Out> {
    let n = if a.dry_run { 1 } else { a.parallel.max(1) };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let dir = if n == 1 { state.to_path_buf() } else { state.join(format!("slot-{i}")) };
        let mut exec = match a.dry_run {
            true => Box::new(inspect::DryRun) as Box<dyn Executor>,
            false => routes::build(a, common, &dir)?,
        };
        if let Some(l) = &live {
            exec.set_live(l.clone());
        }
        if let Some((dir, repo, notes)) = &groups {
            exec = Box::new(concurrency::Grouped {
                inner: exec,
                dir: dir.clone(),
                repo: repo.clone(),
                next: None,
                notes: notes.clone(),
            });
        }
        if !a.breaks.is_empty() {
            exec = Box::new(super::pause::Paused { inner: exec, breaks: a.breaks.clone() });
        }
        out.push(exec);
    }
    Ok(out)
}

/// Drive the run on the executors: one at a time with one, else in
/// parallel. Then the logs and the blobs of each job.
pub(super) fn run(
    source: &mut LocalSource,
    mut slots: Vec<Box<dyn Executor>>,
    each: &mut dyn FnMut(&Line),
) -> (Result<bool, String>, Taken) {
    let (result, mut back) = match slots.len() {
        1 => {
            let r = drive(source, slots[0].as_mut(), each);
            (r, slots)
        }
        _ => match drive_parallel(source, slots, each, &podrunner_local::cancel::SIGNALLED) {
            Ok((ok, back)) => (Ok(ok), back),
            Err(e) => (Err(e), Vec::new()),
        },
    };
    let mut taken: Taken = (Vec::new(), Vec::new());
    for exec in &mut back {
        taken.0.extend(exec.take_logs());
        taken.1.extend(exec.take_blobs());
    }
    (result, taken)
}

#[cfg(test)]
mod tests {
    use super::super::args::parse;

    #[test]
    fn jobs_in_parallel_need_a_unit_kind() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let unit = ["w.yml", "--podvm", "/p", "--images", "/s", "--image", "r@sha256:00"];
        assert_eq!(parse(&args(&[&unit[..], &["--jobs", "4"]].concat())).unwrap().parallel, 4);
        assert_eq!(parse(&args(&unit)).unwrap().parallel, 0, "one at a time by default");
        let e = parse(&args(&["w.yml", "--runner-dir", "/r", "--jobs", "2"])).unwrap_err();
        assert!(e.contains("--jobs N needs a unit kind"), "{e}");
        assert!(parse(&args(&[&unit[..], &["--jobs", "0"]].concat())).unwrap_err().contains("1 or more"));
    }
}
