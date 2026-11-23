//! The CPU time and the peak memory of an instance (T-483), from
//! `podvm inspect --format '{{.CpuSeconds}} {{.PeakMemoryBytes}}'` of the
//! incus tier (podvm `0.2.0-beta.33` and later). podvm prints `-` for a
//! value that incusd did not give. A podvm before `0.2.0-beta.33` refuses
//! the template, so `inspect` fails.

use podrunner_unit::UnitUsage;

/// The template that `inspect` renders.
pub(crate) const FORMAT: &str = "{{.CpuSeconds}} {{.PeakMemoryBytes}}";

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
