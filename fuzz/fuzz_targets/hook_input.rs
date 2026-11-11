//! The fuzz target `hook_input`: `podrunner_fuzz::targets::hook_input` says what it checks.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| podrunner_fuzz::targets::hook_input(data));
