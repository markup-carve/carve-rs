#![no_main]
//! Render through a host with every standard extension enabled.

use std::sync::OnceLock;

use carve::api::{Carve, extensions};
use libfuzzer_sys::fuzz_target;

const STANDARD: &[&str] = &[
    "carve.cite",
    "carve.codeGroup",
    "carve.details",
    "carve.fencedRender.mermaid",
    "carve.imgFence.image",
    "carve.imgFence.img",
    "carve.mathBlock",
    "carve.spoiler",
    "carve.spoilerBlock",
    "carve.tab",
    "carve.tabs",
    "carve.wiki",
];

fn host(renderers: bool) -> Carve {
    let mut carve = Carve::new();
    for name in STANDARD {
        let _ = carve.enable_standard(name, renderers);
    }
    for bundle in [
        extensions::glossary(),
        extensions::list_table(),
        extensions::code_callouts(),
        extensions::semantic_span(),
    ] {
        let _ = carve.register(bundle);
    }
    carve
}

fuzz_target!(|data: &[u8]| {
    static HOSTS: OnceLock<[Carve; 2]> = OnceLock::new();
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    if rest.len() > carve_fuzz::MAX_INPUT {
        return;
    }
    let hosts = HOSTS.get_or_init(|| [host(true), host(false)]);
    let carve = &hosts[usize::from(sel & 1)];
    let opts = carve_fuzz::options(sel);
    if let Ok(tree) = carve.parse(rest, &opts) {
        let _ = carve.render_tree_with_diagnostics(&tree, &opts);
    }
    let _ = carve.render_with_diagnostics(rest, &opts);
});
