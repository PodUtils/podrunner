//! The rows of `doctor` for the pin list: the runner of each pin, its age
//! and its window (T-499), and the disk of its image (T-551).

use podrunner_slots::config::Config;
use serde_json::{Value, json};

use crate::incus_kind::{HOST_DISK_TENTHS, gb};
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
/// `unpacked_bytes`, the size of the image, and with an incus kind the host
/// disk that the image needs (podvm's measure, T-551).
pub(super) fn rows(cfg: &Config, now: u64) -> Vec<Value> {
    let pins = &cfg.pin;
    let incus = cfg.unit.contains_key("incus");
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
            json!({
                "arch": p.arch, "reference": p.reference, "runner_version": p.runner_version,
                "age_s": age, "limit_s": limit, "state": state,
                "unpacked_bytes": p.unpacked_bytes, "incus_host_bytes": host,
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
        out.push_str(&format!(
            "pin {}: runner {version}, {} of {} days{size}{host}{what}\n",
            display(p["arch"].as_str().unwrap_or("?")),
            days("age_s"),
            days("limit_s"),
        ));
    }
    out
}
