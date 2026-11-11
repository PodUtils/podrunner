//! The fuzz target `hook_http`: `podrunner_fuzz::targets::hook_http` says what it checks.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| podrunner_fuzz::targets::hook_http(data));
