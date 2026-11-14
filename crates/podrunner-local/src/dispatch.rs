//! The executor of several runner images (T-129): each label of `runs-on`
//! that `--platform LABEL=TARGET` names takes its own executor, and a job
//! with no such label takes the default. A job whose labels match nothing,
//! with no default, fails by name: GitHub would queue it for ever. A label
//! of Windows or macOS fails by name unless a route names it: the units are
//! Linux.

use std::time::Duration;

use podrunner_broker_fake::Job;
use podrunner_schema::JobId;

use crate::drive::{End, Executor};

/// The routes, the default, and the route of the next job.
pub struct Dispatch {
    pub routes: Vec<(String, Box<dyn Executor>)>,
    pub default: Option<Box<dyn Executor>>,
    next: Option<usize>,
}

impl Dispatch {
    pub fn new(routes: Vec<(String, Box<dyn Executor>)>, default: Option<Box<dyn Executor>>) -> Dispatch {
        Dispatch { routes, default, next: None }
    }

    /// The route of `labels`: the index of a route, or `None` for the default.
    pub fn route(&self, labels: &[String]) -> Result<Option<usize>, String> {
        for l in labels {
            if let Some(i) = self.routes.iter().position(|(r, _)| r.eq_ignore_ascii_case(l)) {
                return Ok(Some(i));
            }
        }
        let foreign = labels.iter().find(|l| {
            let l = l.to_ascii_lowercase();
            l.starts_with("windows") || l.starts_with("macos")
        });
        if let Some(l) = foreign {
            return Err(format!("runs-on {l:?}: a unit is Linux; name a --platform for the label to run it there"));
        }
        if self.default.is_none() {
            let names: Vec<&str> = self.routes.iter().map(|(r, _)| r.as_str()).collect();
            return Err(format!("runs-on {labels:?}: no --platform names a label of it (the platforms: {names:?})"));
        }
        Ok(None)
    }
}

impl Executor for Dispatch {
    fn select(&mut self, labels: &[String]) -> Result<(), String> {
        self.next = self.route(labels)?;
        Ok(())
    }

    fn run(&mut self, job: &JobId, message: Job, limit: Duration) -> Result<End, String> {
        let exec = match self.next.take() {
            Some(i) => &mut self.routes[i].1,
            None => self.default.as_mut().ok_or("no executor for the job")?,
        };
        exec.run(job, message, limit)
    }

    fn take_logs(&mut self) -> Vec<(JobId, String)> {
        let mut out: Vec<(JobId, String)> = self.routes.iter_mut().flat_map(|(_, e)| e.take_logs()).collect();
        if let Some(d) = &mut self.default {
            out.extend(d.take_logs());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use pod_actions_workflow::JobResult;

    use super::*;

    /// An executor that gives its name as the output of each job.
    struct Named(&'static str);

    impl Executor for Named {
        fn run(&mut self, _job: &JobId, _m: Job, _l: Duration) -> Result<End, String> {
            Ok(End { result: JobResult::Success, outputs: vec![("by".into(), self.0.into())] })
        }
    }

    fn by(d: &mut Dispatch, labels: &[&str]) -> Result<String, String> {
        let labels: Vec<String> = labels.iter().map(|s| s.to_string()).collect();
        d.select(&labels)?;
        let end = d.run(&JobId::new("j.0").unwrap(), Job::scripts("j", &[]), Duration::ZERO)?;
        Ok(end.outputs[0].1.clone())
    }

    #[test]
    fn each_job_takes_the_route_of_its_first_label_that_has_one() {
        let routes: Vec<(String, Box<dyn Executor>)> =
            vec![("big".into(), Box::new(Named("big"))), ("host".into(), Box::new(Named("host")))];
        let mut d = Dispatch::new(routes, Some(Box::new(Named("default"))));
        assert_eq!(by(&mut d, &["self-hosted", "Big"]).unwrap(), "big");
        assert_eq!(by(&mut d, &["host", "big"]).unwrap(), "host", "the first label that has a route");
        assert_eq!(by(&mut d, &["ubuntu-latest"]).unwrap(), "default");
        assert!(by(&mut d, &["windows-latest"]).unwrap_err().contains("Linux"));
        let mut none = Dispatch::new(vec![("big".into(), Box::new(Named("big")))], None);
        assert!(by(&mut none, &["ubuntu-latest"]).unwrap_err().contains("no --platform"));
        let mut mac = Dispatch::new(vec![("macos-14".into(), Box::new(Named("mac")))], None);
        assert_eq!(by(&mut mac, &["macos-14"]).unwrap(), "mac", "a route that names it runs it");
    }
}
