#![no_main]
//! The writer gates: `parse(fmt(x)) == parse(x)` (C2-W-002) and
//! `fmt(fmt(x)) == fmt(x)` (C2-W-003), except where the first pass reports a
//! loss or a refusal (C2-W-131, C2-W-133).

use carve::api::{Carve, WriteOptions};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > carve_fuzz::MAX_INPUT || carve_fuzz::skip_fmt_smoke_input(data) {
        return;
    }
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    let carve = Carve::new();
    let opts = WriteOptions::default();
    let Ok(once) = carve.format_with_diagnostics(input, &opts) else {
        return;
    };
    if once.diagnostics.iter().any(|d| {
        matches!(
            d.code.as_str(),
            "structure-unspellable" | "field-unspellable" | "writer-refused"
        )
    }) {
        return;
    }
    let once = once.output;
    assert!(
        carve_fuzz::bare_tree(&once) == carve_fuzz::bare_tree(input),
        "C2-W-002: fmt output reads back as another tree\n--- input\n{input}\n--- fmt\n{once}"
    );
    let twice = carve
        .format(once.as_str(), &opts)
        .expect("formatting formatted output succeeds");
    assert!(
        twice == once,
        "C2-W-003: fmt is not stable\n--- input\n{input}\n--- once\n{once}--- twice\n{twice}"
    );
});
