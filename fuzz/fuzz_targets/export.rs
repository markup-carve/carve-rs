#![no_main]
//! The export writers: Markdown, plain text and ANSI (first byte).

use carve::api::{self, Carve, Options, WriteOptions};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    if rest.len() > carve_fuzz::MAX_INPUT {
        return;
    }
    let Ok(tree) = api::parse(rest, &Options::default()) else {
        return;
    };
    let target = ["markdown", "plain", "ansi"][usize::from(sel % 3)];
    let opts = WriteOptions {
        render: carve_fuzz::options(sel),
        ..WriteOptions::default()
    };
    let _ = Carve::new().write_to(target, &tree, &opts);
});
