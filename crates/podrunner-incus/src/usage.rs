//! The CPU time and the peak memory of an instance (T-483), from
//! `podvm inspect --format '... {{.CpuSeconds}} {{.PeakMemoryBytes}}'` of
//! the incus tier (podvm `0.2.0-beta.33` and later). podvm prints `-` for a
//! value that incusd did not give. A podvm before `0.2.0-beta.33` refuses
//! the template, so `inspect` fails.
//!
//! The instance stops when its command ends (`guest-hold --run` of podvm
//! is its PID 1), and incusd gives no CPU time and no peak of a stopped
//! instance: podvm prints `-` for each. So each read of the state while
//! the instance runs is also a sample, and the driver keeps the use that
//! the samples saw ([`Kept`]).

use podrunner_unit::UnitUsage;

/// The template of one read of the state: the status, the exit code, the
/// use and the OOM kills. incusd gives no metrics of a stopped instance
/// either, so podvm prints `-` for its `OomKills` too.
pub(crate) const STATE: &str = "{{.Status}} {{.ExitCode}} {{.CpuSeconds}} {{.PeakMemoryBytes}} {{.OomKills}}";
/// The template of a podvm before `0.2.0-beta.33`: no fields of the use.
pub(crate) const STATUS: &str = "{{.Status}} {{.ExitCode}}";

/// One read of the state of an instance.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct State {
    pub(crate) status: String,
    pub(crate) code: Option<i32>,
    /// The use, or why there is none.
    pub(crate) usage: UnitUsage,
    /// The OOM kills of the instance, where incusd gave a count.
    pub(crate) oom_kills: Option<u64>,
}

impl State {
    pub(crate) fn running(&self) -> bool {
        self.status == "Running"
    }
}

/// The state from the output of `inspect` with [`STATE`] (`with_use`) or
/// [`STATUS`]; `None` when it has no status.
pub(crate) fn state(text: &str, with_use: bool) -> Option<State> {
    let mut words = text.split_whitespace();
    let status = words.next()?.to_string();
    // A running instance has `-` for its exit code.
    let code = words.next().and_then(|c| c.parse().ok());
    let rest: Vec<&str> = words.collect();
    let usage = if with_use { parse(&rest.iter().take(2).copied().collect::<Vec<_>>().join(" ")) } else { failed() };
    let oom_kills = rest.get(2).and_then(|n| n.parse().ok()).filter(|_| with_use);
    Some(State { status, code, usage, oom_kills })
}

/// The reason of a field that no read of a running instance saw.
const NO_SAMPLE: &str = "the instance ended before a read of its use";

/// The use that the reads of the state of one instance saw: the highest
/// CPU time and peak (both only grow while the instance runs), or the
/// reason of the last read of the running instance that gave none.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Kept(UnitUsage, Option<u64>);

impl Default for Kept {
    fn default() -> Kept {
        Kept(UnitUsage { cpu_s: Err(NO_SAMPLE.into()), peak_memory_bytes: Err(NO_SAMPLE.into()) }, None)
    }
}

impl Kept {
    /// Take one read. A value always counts (a podvm that keeps the use of
    /// a stopped instance gives the last one); a reason counts only while
    /// the instance runs and no value is kept, so the `-` of a stopped
    /// instance erases nothing.
    pub(crate) fn take(&mut self, s: &State) {
        fn one(kept: &mut Result<u64, String>, new: &Result<u64, String>, running: bool) {
            match (&*kept, new) {
                (Ok(k), Ok(v)) => *kept = Ok((*k).max(*v)),
                (Err(_), Ok(v)) => *kept = Ok(*v),
                (Err(_), Err(why)) if running => *kept = Err(why.clone()),
                _ => {}
            }
        }
        one(&mut self.0.cpu_s, &s.usage.cpu_s, s.running());
        one(&mut self.0.peak_memory_bytes, &s.usage.peak_memory_bytes, s.running());
        self.1 = self.1.max(s.oom_kills);
    }

    pub(crate) fn usage(&self) -> UnitUsage {
        self.0.clone()
    }

    /// The highest count of OOM kills that a read saw (T-515).
    pub(crate) fn oom_kills(&self) -> Option<u64> {
        self.1
    }
}

/// The use of an instance from the output of `inspect` with [`FORMAT`]:
/// each field is its value, or why there is none.
pub(crate) fn parse(text: &str) -> UnitUsage {
    let mut words = text.split_whitespace();
    let cpu = words.next();
    let peak = words.next();
    UnitUsage {
        cpu_s: number(cpu, "CpuSeconds").map(|s| s.ceil() as u64),
        // podvm prints an integer; a float form is taken too.
        peak_memory_bytes: number(peak, "PeakMemoryBytes").map(|b| b.ceil() as u64),
    }
}

/// The use of an instance when `inspect` failed (a gone instance, or a
/// podvm before `0.2.0-beta.33`). The reason names no output of podvm: a
/// report can go to a source that is not the operator's.
pub(crate) fn failed() -> UnitUsage {
    let why = "podvm inspect of the instance failed (podvm 0.2.0-beta.33 or later gives the use)".to_string();
    UnitUsage { cpu_s: Err(why.clone()), peak_memory_bytes: Err(why) }
}

/// A finite value of 0 or more of the field `name`, or why it is not one.
fn number(word: Option<&str>, name: &str) -> Result<f64, String> {
    match word {
        None => Err(format!("podvm inspect gave no {name}")),
        Some("-") => Err(format!("incusd gave no {name} of the instance")),
        Some(w) => match w.parse::<f64>() {
            Ok(v) if v.is_finite() && v >= 0.0 => Ok(v),
            Ok(_) => Err(format!("podvm inspect gave a {name} that is negative or not finite")),
            Err(_) => Err(format!("podvm inspect gave a {name} that is not a number")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> State {
        state(text, true).unwrap()
    }

    #[test]
    fn a_read_of_the_state_gives_the_status_the_code_and_the_use() {
        let s = read("Running - 2.500 4096\n");
        assert_eq!((s.status.as_str(), s.code, s.running()), ("Running", None, true));
        assert_eq!(s.usage, UnitUsage { cpu_s: Ok(3), peak_memory_bytes: Ok(4096) });
        let s = read("Stopped 7 - - -");
        assert_eq!((s.code, s.running(), s.oom_kills), (Some(7), false, None));
        assert!(s.usage.cpu_s.is_err());
        assert_eq!(read("Running - 1 2 3").oom_kills, Some(3));
        // The template of an old podvm: the reason names the version.
        let s = state("Running -", false).unwrap();
        assert!(s.usage.cpu_s.is_err_and(|e| e.contains("0.2.0-beta.33")));
        assert_eq!(state(" \n", true), None);
    }

    #[test]
    fn the_kept_use_is_the_highest_of_the_reads_of_a_running_instance() {
        let mut k = Kept::default();
        for text in ["Running - - -", "Running - 1.5 5000", "Running - 2.25 3000"] {
            k.take(&read(text));
        }
        assert_eq!(k.usage(), UnitUsage { cpu_s: Ok(3), peak_memory_bytes: Ok(5000) });
        // The `-` of the stopped instance erases nothing.
        k.take(&read("Stopped 0 - -"));
        assert_eq!(k.usage(), UnitUsage { cpu_s: Ok(3), peak_memory_bytes: Ok(5000) });
        // A podvm that keeps the use of a stopped instance gives the last.
        k.take(&read("Stopped 0 3.5 6000"));
        assert_eq!(k.usage(), UnitUsage { cpu_s: Ok(4), peak_memory_bytes: Ok(6000) });
        // The OOM kills: the highest count; a `-` erases none.
        assert_eq!(k.oom_kills(), None);
        for text in ["Running - 1 1 0", "Running - 1 1 2", "Stopped 0 - - -"] {
            k.take(&read(text));
        }
        assert_eq!(k.oom_kills(), Some(2));
    }

    #[test]
    fn no_read_with_a_value_gives_a_reason() {
        let mut k = Kept::default();
        k.take(&read("Stopped 0 - -"));
        assert_eq!(k.usage().cpu_s, Err(NO_SAMPLE.into()), "no read of the running instance");
        k.take(&read("Running - - 12"));
        assert_eq!(k.usage().cpu_s, Err("incusd gave no CpuSeconds of the instance".into()));
        assert_eq!(k.usage().peak_memory_bytes, Ok(12));
        k.take(&read("Stopped 0 - -"));
        assert_eq!(k.usage().cpu_s, Err("incusd gave no CpuSeconds of the instance".into()), "kept");
    }

    #[test]
    fn both_values_are_read_and_rounded_up() {
        let u = parse("12.250 104857601\n");
        assert_eq!(u, UnitUsage { cpu_s: Ok(13), peak_memory_bytes: Ok(104857601) });
        assert_eq!(parse("3.000 0"), UnitUsage { cpu_s: Ok(3), peak_memory_bytes: Ok(0) });
        // A float form of the bytes (as Go prints a large number).
        assert_eq!(parse("0 1.5e+08").peak_memory_bytes, Ok(150_000_000));
    }

    #[test]
    fn a_dash_of_incusd_is_unknown_with_a_reason() {
        let u = parse("- -");
        assert_eq!(u.cpu_s, Err("incusd gave no CpuSeconds of the instance".into()));
        assert_eq!(u.peak_memory_bytes, Err("incusd gave no PeakMemoryBytes of the instance".into()));
        // One field known, one not: each stands alone.
        let u = parse("4.5 -");
        assert_eq!(u.cpu_s, Ok(5));
        assert!(u.peak_memory_bytes.is_err());
    }

    #[test]
    fn an_absent_field_is_unknown_with_a_reason() {
        let u = parse("");
        assert_eq!(u.cpu_s, Err("podvm inspect gave no CpuSeconds".into()));
        assert_eq!(u.peak_memory_bytes, Err("podvm inspect gave no PeakMemoryBytes".into()));
        let u = parse("2.0");
        assert_eq!(u.cpu_s, Ok(2));
        assert_eq!(u.peak_memory_bytes, Err("podvm inspect gave no PeakMemoryBytes".into()));
    }

    #[test]
    fn a_negative_or_non_number_value_is_unknown_with_a_reason() {
        for bad in ["-1.5 -7", "NaN inf", "-inf 1e999"] {
            let u = parse(bad);
            assert!(u.cpu_s.as_ref().is_err_and(|e| e.contains("negative or not finite")), "{bad}: {u:?}");
            assert!(u.peak_memory_bytes.as_ref().is_err_and(|e| e.contains("negative or not finite")), "{bad}: {u:?}");
        }
        // Go's text of a missing key, and other words.
        let u = parse("<no value>");
        assert_eq!(u.cpu_s, Err("podvm inspect gave a CpuSeconds that is not a number".into()));
        assert_eq!(u.peak_memory_bytes, Err("podvm inspect gave a PeakMemoryBytes that is not a number".into()));
        assert_eq!(
            parse("1 12MiB").peak_memory_bytes,
            Err("podvm inspect gave a PeakMemoryBytes that is not a number".into())
        );
    }

    #[test]
    fn a_failed_inspect_gives_unknown_with_a_reason() {
        let u = failed();
        assert!(u.cpu_s.is_err() && u.peak_memory_bytes.is_err(), "{u:?}");
    }
}
