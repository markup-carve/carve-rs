#![no_main]
//! Import of HTML, Markdown, GFM or Djot (first byte), then the canonical
//! writer on the imported tree.

use carve::api::{Carve, ImportAdapter, ImportFormat, ImportOptions, LossOptions, WriteOptions};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    if rest.len() > carve_fuzz::MAX_INPUT {
        return;
    }
    let Ok(input) = std::str::from_utf8(rest) else {
        return;
    };
    let format = match sel & 3 {
        0 => ImportFormat::Html,
        1 => ImportFormat::Markdown,
        2 => ImportFormat::Gfm,
        _ => ImportFormat::Djot,
    };
    let adapters = ImportAdapter::ALL;
    let adapter = adapters[usize::from(sel >> 2) % adapters.len()];
    let opts = ImportOptions {
        raw_html: sel & 0x80 != 0,
        strict_losses: false,
        adapter,
    };
    let carve = Carve::new();
    let Ok(imported) = carve.import_from(input, format, &opts, &LossOptions::default()) else {
        return;
    };
    let _ = carve.write_with_diagnostics(&imported.tree, &WriteOptions::default());
});
