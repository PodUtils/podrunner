//! The limits of memory, CPU and processes of a unit kind (G-12, T-162).
//! A kind enforces the values of its configuration on each unit; it says
//! `enforced` for each limit that it holds, and placement refuses an offer
//! that needs another (`limit-not-enforceable`). A driver applies the
//! lesser of the value of its kind and the value of the offer where it can
//! (T-198, [`Limits::lesser`]); `docs/configuration.md` names each kind.

use podrunner_schema::{Capability, LimitMode};

/// The limits of one kind. None: not held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    pub memory_bytes: Option<u64>,
    pub cpus: Option<u32>,
    pub pids: Option<u32>,
    /// The size of the writable disk of a unit, in bytes, where the kind
    /// can give one (the root disk of an incus instance, T-482). `record`
    /// leaves it to the kind.
    pub disk_bytes: Option<u64>,
}

impl Limits {
    /// Each held limit as `enforced` in the record of the kind.
    pub fn record(&self, cap: &mut Capability) {
        for (name, held) in
            [("memory", self.memory_bytes.is_some()), ("cpu", self.cpus.is_some()), ("pids", self.pids.is_some())]
        {
            if held {
                cap.limits.insert(name.into(), LimitMode::Enforced);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Limits::default()
    }

    /// The lesser of each limit of `self` (the kind) and of `job` (the
    /// offer, T-198); a limit that one side has alone is kept.
    pub fn lesser(&self, job: &Limits) -> Limits {
        fn min<T: Ord + Copy>(a: Option<T>, b: Option<T>) -> Option<T> {
            match (a, b) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            }
        }
        Limits {
            memory_bytes: min(self.memory_bytes, job.memory_bytes),
            cpus: min(self.cpus, job.cpus),
            pids: min(self.pids, job.pids),
            disk_bytes: min(self.disk_bytes, job.disk_bytes),
        }
    }

    /// The limits of an offer (`memory` in bytes, `cpu` in milli-cores,
    /// `pids`): a CPU count rounds up.
    pub fn of_offer(limits: &std::collections::BTreeMap<String, u64>) -> Limits {
        Limits {
            memory_bytes: limits.get("memory").copied().filter(|v| *v > 0),
            cpus: limits.get("cpu").copied().filter(|v| *v > 0).map(|m| m.div_ceil(1000) as u32),
            pids: limits.get("pids").copied().filter(|v| *v > 0).map(|p| p.min(u32::MAX.into()) as u32),
            disk_bytes: limits.get("disk").copied().filter(|v| *v > 0),
        }
    }
}

/// A size: whole bytes, or a number with `K`, `M`, `G` or `T` (powers of
/// 1024), as `--podvm-mem` takes it.
pub fn size_bytes(text: &str) -> Result<u64, String> {
    let t = text.trim();
    let (num, mult) = match t.char_indices().last() {
        Some((i, c)) if c.is_ascii_alphabetic() => {
            let m = match c.to_ascii_uppercase() {
                'K' => 1u64 << 10,
                'M' => 1 << 20,
                'G' => 1 << 30,
                'T' => 1 << 40,
                _ => return Err(format!("{text:?}: the unit is not K, M, G or T")),
            };
            (&t[..i], m)
        }
        _ => (t, 1),
    };
    let n: u64 = num.parse().map_err(|_| format!("{text:?} is not a size"))?;
    let bytes = n.checked_mul(mult).ok_or_else(|| format!("{text:?} is too large"))?;
    if bytes == 0 {
        return Err(format!("{text:?} is zero"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lesser_of_the_kind_and_the_job() {
        let kind = Limits { memory_bytes: Some(8 << 30), cpus: Some(4), pids: None, disk_bytes: None };
        let offer: std::collections::BTreeMap<String, u64> =
            [("memory".into(), 12u64 << 30), ("cpu".into(), 1500), ("pids".into(), 512)].into();
        let job = Limits::of_offer(&offer);
        assert_eq!(job, Limits { memory_bytes: Some(12 << 30), cpus: Some(2), pids: Some(512), disk_bytes: None });
        assert_eq!(
            kind.lesser(&job),
            Limits { memory_bytes: Some(8 << 30), cpus: Some(2), pids: Some(512), disk_bytes: None }
        );
        assert_eq!(kind.lesser(&Limits::default()), kind);
    }

    #[test]
    fn a_size_takes_bytes_and_the_binary_units() {
        assert_eq!(size_bytes("4G"), Ok(4 << 30));
        assert_eq!(size_bytes("512m"), Ok(512 << 20));
        assert_eq!(size_bytes("1048576"), Ok(1 << 20));
        for bad in ["", "0", "4X", "G", "-1G", "99999999999T"] {
            assert!(size_bytes(bad).is_err(), "{bad:?}");
        }
    }
}
