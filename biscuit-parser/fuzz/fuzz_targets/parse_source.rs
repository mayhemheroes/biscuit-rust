#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    _ = biscuit_parser::parser::parse_source(data);
});
