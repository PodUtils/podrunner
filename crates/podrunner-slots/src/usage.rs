//! The use of each unit (T-108, githome's G-06): the CPU time and the peak
//! resident memory of its processes on the host, for `cpu_s` and
//! `peak_rss_mib` of the report.
//!
//! slots reads `/proc/PID/stat` and `/proc/PID/status` of each process of a
//! unit (`observe`) at each tick. The CPU time of a process is the highest
//! `utime + stime` that a tick saw, so a process that ended still counts
//! with its last sample. The peak memory is the highest sum of `VmRSS` of
//! one tick. A kind whose processes are not on the host (`incus`) measures
//! the unit itself (`UnitDriver::usage`, T-483): each value, or `unknown`
//! with its reason. For a guest of the `microvm` kind
//! the process is the emulator, which holds the memory of the guest.

use std::collections::BTreeMap;

use podrunner_schema::Residual;

use crate::Slots;
use crate::state::State;

/// What slots saw of the processes of one unit.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    /// The highest `utime + stime` of each process, in clock ticks.
    ticks: BTreeMap<u32, u64>,
    /// The highest sum of the resident memory of one sample, in KiB.
    peak_kib: u64,
    /// A sample saw a process of the unit.
    seen: bool,
    /// The use that the kind measured itself (T-483); it holds over the
    /// samples.
    kind: Option<podrunner_unit::UnitUsage>,
}

impl Usage {
    /// Take one sample of the processes `pids`.
    pub fn sample(&mut self, pids: &[u32]) {
        let mut rss = 0;
        for &pid in pids {
            let Some((t, kib)) = read(pid) else { continue };
            self.seen = true;
            let e = self.ticks.entry(pid).or_default();
            *e = (*e).max(t);
            rss += kib;
        }
        self.peak_kib = self.peak_kib.max(rss);
    }

    /// The use that the kind of the unit measured (an incus instance).
    pub fn of_kind(&mut self, u: podrunner_unit::UnitUsage) {
        self.kind = Some(u);
    }

    /// `cpu_s` and `peak_rss_mib` of the report: the use of the kind, else
    /// the samples; `unknown` with its reason when neither has a value.
    pub fn report(&self) -> [Measure; 2] {
        if let Some(k) = &self.kind {
            return [measure(&k.cpu_s), measure(&k.peak_memory_bytes.clone().map(|b| b.div_ceil(1 << 20)))];
        }
        if !self.seen {
            let why = || Err("no sample saw a process of the unit on the host".to_string());
            return [measure(&why()), measure(&why())];
        }
        let hz = clock_ticks();
        let ticks: u64 = self.ticks.values().sum();
        [(Residual::Count(ticks.div_ceil(hz)), None), (Residual::Count(self.peak_kib.div_ceil(1024)), None)]
    }
}

/// A measured field of the report, and the reason when it is `unknown`.
pub type Measure = (Residual, Option<String>);

fn measure(v: &Result<u64, String>) -> Measure {
    match v {
        Ok(n) => (Residual::Count(*n), None),
        Err(why) => (Residual::Unknown(podrunner_schema::report::Unknown::Unknown), Some(why.clone())),
    }
}

/// The clock ticks of a second (`_SC_CLK_TCK`).
fn clock_ticks() -> u64 {
    // SAFETY: sysconf reads a constant of the system.
    let n = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    u64::try_from(n).ok().filter(|n| *n > 0).unwrap_or(100)
}

/// `utime + stime` (clock ticks) and `VmRSS` (KiB) of `pid`.
fn read(pid: u32) -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The name of the process is in parentheses and can hold a space:
    // the fields after it start at the last ')'.
    let rest = &stat[stat.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    // After the name: state (index 0), ..., utime (11), stime (12).
    let ticks = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let kib = status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    Some((ticks, kib))
}

impl Slots {
    /// One sample of each unit that runs.
    pub(crate) fn sample_usage(&mut self) {
        let running: Vec<_> = self
            .jobs
            .iter()
            .filter(|(_, j)| matches!(j.state, State::Started | State::Running))
            .filter_map(|(k, j)| Some((k.clone(), j.unit.clone()?, j.placement.as_ref()?.kind)))
            .collect();
        for (key, unit, kind) in running {
            let pids = self.kinds[kind].driver.observe(&unit).map(|o| o.pids).unwrap_or_default();
            if let Some(j) = self.jobs.get_mut(&key) {
                j.usage.sample(&pids);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    /// T-483: the use that a kind measured holds over the samples, in MiB
    /// rounded up; a field that the kind could not measure is `unknown`
    /// with the reason of the kind.
    #[test]
    fn the_use_of_the_kind_holds() {
        let mut u = Usage::default();
        u.of_kind(podrunner_unit::UnitUsage { cpu_s: Ok(13), peak_memory_bytes: Ok((100 << 20) + 1) });
        assert_eq!(u.report(), [(Residual::Count(13), None), (Residual::Count(101), None)]);
        u.of_kind(podrunner_unit::UnitUsage { cpu_s: Ok(2), peak_memory_bytes: Err("no peak".into()) });
        let unknown = Residual::Unknown(podrunner_schema::report::Unknown::Unknown);
        assert_eq!(u.report(), [(Residual::Count(2), None), (unknown, Some("no peak".into()))]);
    }

    use super::*;

    #[test]
    fn a_process_that_spins_has_cpu_time_and_memory() {
        let mut child = std::process::Command::new("sh")
            .args(["-c", "i=0; while [ $i -lt 3000000 ]; do i=$((i+1)); done"])
            .spawn()
            .unwrap();
        let mut u = Usage::default();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while child.try_wait().unwrap().is_none() && std::time::Instant::now() < end {
            u.sample(&[child.id()]);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
        let [(cpu, _), (rss, _)] = u.report();
        assert!(matches!(cpu, Residual::Count(n) if n >= 1), "{cpu:?}");
        assert!(matches!(rss, Residual::Count(n) if n >= 1), "{rss:?}");
    }

    #[test]
    fn no_process_is_unknown_never_zero() {
        let mut u = Usage::default();
        u.sample(&[]);
        let [(cpu, cpu_why), (rss, rss_why)] = u.report();
        assert!(matches!(cpu, Residual::Unknown(_)) && matches!(rss, Residual::Unknown(_)));
        // `unknown` always has its reason (T-497).
        assert!(cpu_why.is_some() && rss_why.is_some());
    }
}
