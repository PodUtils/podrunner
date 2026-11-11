//! `podrunner doctor`: a thin reader of the host checks that other tools
//! own (call C9, DR-15): `pod doctor --json` of podnet (the object `hold`)
//! and `podvm probe --json`. It names each input that is missing; it
//! decides nothing.

use std::process::Command;

use serde_json::Value;

use crate::text::display;

/// Run one tool and read its JSON, or say why there is none.
fn read(tool: &str, args: &[&str]) -> Result<Value, String> {
    let out = Command::new(tool).args(args).output().map_err(|e| format!("{tool}: not run ({})", e.kind()))?;
    if !out.status.success() {
        return Err(format!("{tool} {}: exit {}", args.join(" "), out.status.code().unwrap_or(-1)));
    }
    serde_json::from_slice(&out.stdout).map_err(|_| format!("{tool} {}: not JSON", args.join(" ")))
}

/// The report of `doctor`.
pub fn report() -> String {
    let mut out = String::new();
    match read("pod", &["doctor", "--json"]) {
        Ok(v) => {
            for backend in ["netns", "seccomp"] {
                let h = &v["hold"][backend];
                let level = h["level"].as_str().map(display).unwrap_or_else(|| "missing".into());
                out.push_str(&format!("hold {backend}: {level}\n"));
            }
        }
        Err(why) => out.push_str(&format!("missing: the hold of the host ({})\n", display(&why))),
    }
    match read("podvm", &["probe", "--json"]) {
        Ok(v) => {
            let keys: Vec<String> = v.as_object().map(|m| m.keys().map(|k| display(k)).collect()).unwrap_or_default();
            out.push_str(&format!("podvm probe: {} keys (the driver maps them, GitHub #23)\n", keys.len()));
        }
        Err(why) => out.push_str(&format!("missing: the podvm probe ({})\n", display(&why))),
    }
    out.push_str("missing: the guest self-test (GitHub #36)\n");
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_missing_tool_is_named() {
        let r = super::read("podrunner-no-such-tool", &["--json"]).unwrap_err();
        assert!(r.starts_with("podrunner-no-such-tool: not run"), "{r}");
    }
}
