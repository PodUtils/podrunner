//! The fuzz target `slots_config`: `podrunner_fuzz::targets::slots_config` says what it checks.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| podrunner_fuzz::targets::slots_config(data));
