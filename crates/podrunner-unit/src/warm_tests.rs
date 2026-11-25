use std::collections::BTreeMap;

use super::*;
use crate::conformance::{CANARY, canary_credential};

/// Run the script of `env_sh` in `/bin/sh` and give back each variable of
/// `names` and each word of the argv, as the shell sees them.
fn round_trip(script: &str, names: &[&str]) -> Vec<String> {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = std::env::temp_dir().join(format!("podrunner-env-sh-{}-{n}", std::process::id()));
    std::fs::write(&file, script).unwrap();
    let vars: String = names.iter().map(|k| format!("printf '%s\\0' \"${k}\"; ")).collect();
    let out = std::process::Command::new("/bin/sh")
        .args(["-c", &format!(". \"$1\"; {vars}for a; do printf '%s\\0' \"$a\"; done"), "sh"])
        .arg(&file)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&file);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    let mut words: Vec<String> = text.split('\0').map(String::from).collect();
    assert_eq!(words.pop().as_deref(), Some(""), "a NUL ends each word");
    words
}

#[test]
fn the_shell_gets_each_value_and_each_word_as_it_is() {
    let values = [
        "",
        "a b",
        "it's",
        "'",
        "\"q\"",
        "line\nnext",
        "$HOME",
        "`id`",
        "${{ secrets.X }}",
        "-e",
        "\\n",
        "Ünïcode",
        "a;b|c&d",
        "*",
    ];
    let env: BTreeMap<String, String> =
        values.iter().enumerate().map(|(i, v)| (format!("V_{i}"), v.to_string())).collect();
    let argv: Vec<String> = values.iter().map(|v| v.to_string()).collect();
    let script = env_sh(&env, &argv).unwrap();
    let names: Vec<String> = env.keys().cloned().collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let got = round_trip(&script, &names);
    let want: Vec<String> = env.values().cloned().chain(argv.iter().cloned()).collect();
    assert_eq!(got, want);
    // No argv: no word.
    assert!(round_trip(&env_sh(&BTreeMap::new(), &[]).unwrap(), &[]).is_empty());
}

#[test]
fn a_name_that_is_not_plain_or_a_nul_is_refused() {
    for bad in ["", "1A", "A-B", "A B", "A=B", "A.B", "Ä", "$A"] {
        let env = BTreeMap::from([(bad.to_string(), "v".to_string())]);
        assert_eq!(env_sh(&env, &[]).unwrap_err().code, UnitCode::UnitStartFailed, "{bad:?}");
    }
    for ok in ["A", "_", "a_1", "PODRUNNER_UNIT"] {
        env_sh(&BTreeMap::from([(ok.to_string(), "v".to_string())]), &[]).unwrap();
    }
    let env = BTreeMap::from([("A".to_string(), "a\0b".to_string())]);
    assert!(env_sh(&env, &[]).is_err());
    assert!(env_sh(&BTreeMap::new(), &["a\0b".to_string()]).is_err());
}

#[test]
fn the_warm_half_of_a_start_holds_nothing_of_the_job() {
    let spec = StartSpec {
        unit: UnitId::parse("u-1").unwrap(),
        image: Prepared { digest: format!("sha256:{}", "a".repeat(64)), platform: "linux/amd64".into() },
        argv: vec!["run.sh".into()],
        env: BTreeMap::from([
            ("PODRUNNER_UNIT".to_string(), "u-1".to_string()),
            ("GITHUB_TOKEN_NAME".to_string(), "job".to_string()),
        ]),
        credential: canary_credential(),
        sockets: Some("/s/u-1".into()),
        mounts: Vec::new(),
        limits: Limits { pids: Some(9), ..Limits::default() },
    };
    let (warm, claim) = split(&spec).unwrap();
    assert_eq!(warm.env, BTreeMap::from([("PODRUNNER_UNIT".to_string(), "u-1".to_string())]));
    assert!(!format!("{warm:?}").contains(CANARY) && !format!("{warm:?}").contains("run.sh"));
    assert_eq!((claim.argv, claim.env, claim.limits), (spec.argv.clone(), spec.env.clone(), spec.limits));
    let mounted = StartSpec { mounts: vec![("/c".into(), "/podrunner-cache".into())], ..spec };
    assert_eq!(split(&mounted).unwrap_err().code, UnitCode::Unsupported);
}
