//! The fuzz target `source_wire`: `podrunner_fuzz::targets::source_wire` says what it checks.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| podrunner_fuzz::targets::source_wire(data));
