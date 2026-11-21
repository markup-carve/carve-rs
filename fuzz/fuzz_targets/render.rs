#![no_main]
//! HTML rendering under options picked by the first byte.

use carve::api;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    if rest.len() > carve_fuzz::MAX_INPUT {
        return;
    }
    let opts = carve_fuzz::options(sel);
    let _ = api::render_with_diagnostics(rest, &opts);
});
