#![no_main]
//! The JSON boundary: decode a tree, then render and write it.

use carve::api::{self, Carve, Options, WriteOptions};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > carve_fuzz::MAX_INPUT {
        return;
    }
    let Ok(json) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(tree) = api::decode_tree(json) else {
        return;
    };
    let _ = api::render_tree_with_diagnostics(&tree, &Options::default());
    let _ = Carve::new().write_with_diagnostics(&tree, &WriteOptions::default());
});
