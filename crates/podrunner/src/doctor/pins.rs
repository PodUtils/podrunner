//! The rows of `doctor` for the pin list: the runner of each pin, its age
//! and its window (T-499), the disk of its image (T-551) and the memory of
//! a guest that holds it (T-573) with the space of its job (T-689).

use podrunner_slots::config::Config;
use serde_json::{Value, json};

use crate::incus_kind::HOST_DISK_TENTHS;
use crate::pin_size::gb;
use crate::text::display;

/// The version of the official runner that `images/runner/runner.toml` of
/// this build pins.
pub(super) const RUNNER_OF_THIS_BUILD: &str = {
    // The first line `version = "..."` of the file.
    const TEXT: &str = include_str!("../../../../images/runner/runner.toml");
    const_version(TEXT)
};

const fn const_version(text: &'static str) -> &'static str {
    let b = text.as_bytes();
    let key = b"\nversion = \"";
    let mut i = 0;
    while i + key.len() < b.len() {
        let mut k = 0;
        while k < key.len() && b[i + k] == key[k] {
            k += 1;
        }
        if k == key.len() {
            let start = i + k;
            let mut end = start;
            while end < b.len() && b[end] != b'"' {
                end += 1;
            }
            let (_, rest) = text.split_at(start);
            let (v, _) = rest.split_at(end - start);
            return v;
        }
        i += 1;
    }
    "unknown"
}

pub(super) fn now_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The last days of the window of a pin, in which `doctor` warns (T-499).
const NEAR_END_S: u64 = 7 * 24 * 3600;

/// One row for each pinned payload: its runner version, its age and its
/// window (the age limit of the pin, which keeps the runner inside the
/// window of updates of GitHub), and `ok`, `near-end` or `stale`. With
/// `unpacked_bytes`, the size of the image, with an incus kind the host
/// disk that the image needs (podvm's measure, T-551), and with a microvm
/// kind the memory of a guest that boots it and keeps the reserve of its
/// job (podvm's rule, T-573, T-689), and the free root that a job gets in a
/// guest of the `mem` of the kind.
pub(super) fn rows(cfg: &Config, now: u64) -> Vec<Value> {
    let pins = &cfg.pin;
    let incus = cfg.unit.contains_key("incus");
    let microvm = cfg.unit.get("microvm").map(|u| u.mem.as_deref().unwrap_or(crate::microvm_kind::DEFAULT_MEM));
    // `check` refuses a `mem` that is not a size: no space of a job then.
    let kind_mem = microvm.and_then(|m| podrunner_unit::limits::size_bytes(m).ok());
    pins.payload
        .iter()
        .map(|p| {
            let limit = p.max_age_s.map_or(pins.max_age_s, |m| m.min(pins.max_age_s));
            let age = now.saturating_sub(p.built_at_s);
            let state = if p.stale(pins, now) {
                "stale"
            } else if age.saturating_add(NEAR_END_S) >= limit {
                "near-end"
            } else {
                "ok"
            };
            let host = p.unpacked_bytes.filter(|_| incus).map(|b| b.saturating_mul(HOST_DISK_TENTHS) / 10);
            let mem = p.unpacked_bytes.filter(|_| microvm.is_some()).map(crate::microvm_kind::mem_needed);
            let job = p.unpacked_bytes.zip(kind_mem).map(|(s, m)| crate::microvm_kind::job_space(m, s));
            json!({
                "arch": p.arch, "reference": p.reference, "runner_version": p.runner_version,
                "age_s": age, "limit_s": limit, "state": state,
                "unpacked_bytes": p.unpacked_bytes, "incus_host_bytes": host, "microvm_mem_bytes": mem,
                "microvm_job_bytes": job,
            })
        })
        .collect()
}

/// The text line of each row of [`rows`].
pub(super) fn lines(v: &Value) -> String {
    let mut out = String::new();
    for p in v["pins"].as_array().into_iter().flatten() {
        let days = |f: &str| p[f].as_u64().unwrap_or(0) / (24 * 3600);
        let version = display(p["runner_version"].as_str().unwrap_or("not named"));
        let what = match p["state"].as_str() {
            Some("stale") => ": stale, no job starts on it: build and pin a new image",
            Some("near-end") => ": near the end of its window: build and pin a new image",
            _ => "",
        };
        let size = p["unpacked_bytes"].as_u64().map(|b| format!(", {} unpacked", gb(b))).unwrap_or_default();
        let host = p["incus_host_bytes"]
            .as_u64()
            .map(|b| format!(", the incus kind needs about {} of host disk", gb(b)))
            .unwrap_or_default();
        let mem = p["microvm_mem_bytes"]
            .as_u64()
            .map(|b| format!(", the microvm kind needs mem of {} or more", gb(b)))
            .unwrap_or_default();
        let job = p["microvm_job_bytes"]
            .as_u64()
            .map(|b| format!(", a job gets about {} of root in a guest of the kind", gb(b)))
            .unwrap_or_default();
        out.push_str(&format!(
            "pin {}: runner {version}, {} of {} days{size}{host}{mem}{job}{what}\n",
            display(p["arch"].as_str().unwrap_or("?")),
            days("age_s"),
            days("limit_s"),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    /// T-573: with a microvm kind, `doctor` names the memory of a guest that
    /// boots the image (13/4 times it plus 512 MiB), and the refusal of `check`
    /// with its fix.
    #[test]
    fn doctor_names_the_memory_of_a_pinned_image() {
        let dir = std::env::temp_dir().join(format!("podrunner-doctor-mem-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("podrunner.toml");
        let pin = |size: u64| {
            format!(
                "host = \"h\"\n[unit.microvm]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\nmem = \"4G\"\n\
                 [pin]\n[[pin.payload]]\narch = \"{}\"\nreference = \"runner@sha256:{}\"\nbuilt_at_s = 1\n\
                 unpacked_bytes = {size}\n",
                std::env::consts::ARCH,
                "1".repeat(64)
            )
        };
        std::fs::write(&file, pin(3_000_000_000)).unwrap();
        let v = super::super::report_with(Some(&file), json!([])).unwrap();
        assert_eq!(v["pins"][0]["microvm_mem_bytes"], json!(10_286_870_912u64), "{v}");
        let t = super::super::text(&v);
        assert!(t.contains(", 3.0 GB unpacked, the microvm kind needs mem of 10.3 GB or more"), "{t}");
        // 3 GiB of root at 4G is less than the image: no space (T-689).
        assert_eq!(v["pins"][0]["microvm_job_bytes"], json!(221_225_472u64), "{v}");
        assert!(t.contains(", a job gets about 0.2 GB of root in a guest of the kind"), "{t}");
        assert!(t.contains("missing: [unit.microvm] mem: the pin runner@sha256:"), "{t}");
        assert!(t.contains("fix: set mem = \"10G\" or more"), "{t}");
        std::fs::write(&file, pin(1_000_000_000)).unwrap();
        let v = super::super::report_with(Some(&file), json!([])).unwrap();
        assert!(!v["missing"].to_string().contains("[unit.microvm] mem"), "{v}");
        // 0.75 × 4 GiB less the image; the memory for the reserve of 2 GiB.
        assert_eq!(v["pins"][0]["microvm_job_bytes"], json!(2_221_225_472u64), "{v}");
        assert_eq!(v["pins"][0]["microvm_mem_bytes"], json!(4_196_644_864u64), "{v}");
        // T-689: an image that boots at 4G, but leaves its job less than
        // 2 GiB, is missing with the fix.
        std::fs::write(&file, pin(1_100_000_000)).unwrap();
        let v = super::super::report_with(Some(&file), json!([])).unwrap();
        let t = super::super::text(&v);
        assert!(t.contains("missing: [unit.microvm] mem: the pin runner@sha256:"), "{t}");
        assert!(t.contains("MiB for a job, less than 2048 MiB") && t.contains("mem = \"5G\""), "{t}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
