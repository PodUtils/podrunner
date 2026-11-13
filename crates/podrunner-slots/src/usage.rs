//! The use of each unit (T-108, githome's G-06): the CPU time and the peak
//! resident memory of its processes on the host, for `cpu_s` and
//! `peak_rss_mib` of the report.
//!
//! slots reads `/proc/PID/stat` and `/proc/PID/status` of each process of a
//! unit (`observe`) at each tick. The CPU time of a process is the highest
//! `utime + stime` that a tick saw, so a process that ended still counts
//! with its last sample. The peak memory is the highest sum of `VmRSS` of
//! one tick. A kind whose processes are not on the host (`incus`) shows no
//! process: its report says `unknown`. For a guest of the `microvm` kind
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

    /// `cpu_s` and `peak_rss_mib` of the report: `unknown` when no sample
    /// saw a process of the unit.
    pub fn report(&self) -> (Residual, Residual) {
        if !self.seen {
            let unknown = Residual::Unknown(podrunner_schema::report::Unknown::Unknown);
            return (unknown, unknown);
        }
        let hz = clock_ticks();
        let ticks: u64 = self.ticks.values().sum();
        (Residual::Count(ticks.div_ceil(hz)), Residual::Count(self.peak_kib.div_ceil(1024)))
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
        let (cpu, rss) = u.report();
        assert!(matches!(cpu, Residual::Count(n) if n >= 1), "{cpu:?}");
        assert!(matches!(rss, Residual::Count(n) if n >= 1), "{rss:?}");
    }

    #[test]
    fn no_process_is_unknown_never_zero() {
        let mut u = Usage::default();
        u.sample(&[]);
        let (cpu, rss) = u.report();
        assert!(matches!(cpu, Residual::Unknown(_)) && matches!(rss, Residual::Unknown(_)));
    }
}
