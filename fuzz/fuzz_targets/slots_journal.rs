//! The fuzz target `slots_journal`: `podrunner_fuzz::targets::slots_journal` says what it checks.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| podrunner_fuzz::targets::slots_journal(data));
