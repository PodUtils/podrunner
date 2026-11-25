//! The view of `/proc` that the tests need.

use std::fs;

/// True while the process runs. A zombie has ended: its parent has not read
/// its status.
pub fn alive(pid: u32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        // The state follows the name in parentheses; the name can hold ')'.
        Ok(stat) => stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().next()) != Some("Z"),
        Err(_) => false,
    }
}

/// The state of a process that a test finds alive: its stat, the Name,
/// State and PPid of its status, and its wchan. A failure then shows why
/// the process lives (an exit that frees memory, a wait in the kernel).
pub fn describe(pid: u32) -> String {
    let read = |f: &str| fs::read_to_string(format!("/proc/{pid}/{f}")).unwrap_or_else(|e| format!("({e})"));
    let status: Vec<String> = read("status")
        .lines()
        .filter(|l| ["Name:", "State:", "PPid:"].iter().any(|k| l.starts_with(k)))
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    format!("stat: {}; {}; wchan: {}", read("stat").trim(), status.join("; "), read("wchan").trim())
}

/// True when the argv or the environment of the process holds `text`. A
/// process that is gone, or whose files cannot be read, holds nothing; the
/// caller checks that it can read them (a positive control).
pub fn holds_text(pid: u32, text: &str) -> bool {
    ["cmdline", "environ"].iter().any(|f| {
        fs::read(format!("/proc/{pid}/{f}")).is_ok_and(|bytes| bytes.windows(text.len()).any(|w| w == text.as_bytes()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_alive_and_shows_its_argv() {
        let me = std::process::id();
        assert!(alive(me));
        let argv0 = std::env::args().next().unwrap();
        assert!(holds_text(me, &argv0), "a positive control: the argv of this test");
        assert!(!holds_text(me, "podrunner-text-that-is-nowhere-0x51"));
        assert!(!alive(u32::MAX));
        let me = describe(me);
        assert!(me.contains("State:") && me.contains("PPid:") && me.contains("wchan:"), "{me}");
    }
}
