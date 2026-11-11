//! The stable part of the fuzz targets: each target on its seeds and on
//! deterministic mutations of them, with no nightly and no libFuzzer.

use std::path::{Path, PathBuf};

use crate::TARGETS;

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("an entry").file_name().to_string_lossy().trim_end_matches(".rs").to_string())
        .collect();
    names.sort();
    names
}

/// Each target has a binary and seeds; each binary and each directory of
/// seeds is a target.
#[test]
fn each_target_has_a_binary_and_seeds() {
    let mut targets: Vec<String> = TARGETS.iter().map(|(n, _)| n.to_string()).collect();
    targets.sort();
    assert_eq!(names(&here().join("fuzz_targets")), targets);
    assert_eq!(names(&here().join("seeds")), targets);
    let manifest = std::fs::read_to_string(here().join("Cargo.toml")).unwrap();
    for name in &targets {
        assert!(manifest.contains(&format!("[[bin]]\nname = \"{name}\"\n")), "Cargo.toml has no binary {name}");
    }
}

/// Each target on its seeds and on 5000 mutations of them (a fixed
/// xorshift sequence): no panic.
#[test]
fn each_target_survives_mutations() {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for (name, target) in TARGETS {
        let dir = here().join("seeds").join(name);
        let seeds: Vec<Vec<u8>> = names(&dir).iter().map(|n| std::fs::read(dir.join(n)).unwrap()).collect();
        assert!(!seeds.is_empty(), "{name} has no seed");
        for seed in &seeds {
            target(seed);
        }
        for _ in 0..5000 {
            let mut data = seeds[(next() as usize) % seeds.len()].clone();
            for _ in 0..1 + next() % 4 {
                let at = if data.is_empty() { 0 } else { (next() as usize) % data.len() };
                match next() % 3 {
                    0 if !data.is_empty() => data[at] = next() as u8,
                    1 => data.insert(at, next() as u8),
                    _ if !data.is_empty() => {
                        data.remove(at);
                    }
                    _ => {}
                }
            }
            target(&data);
        }
    }
}
