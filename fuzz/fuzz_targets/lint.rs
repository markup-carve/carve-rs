#![no_main]
//! Lint with every rule at its default.

use carve::api::{self, LintOptions};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > carve_fuzz::MAX_INPUT {
        return;
    }
    let _ = api::lint(data, &LintOptions::default());
});
