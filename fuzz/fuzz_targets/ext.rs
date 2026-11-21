#![no_main]
//! Render through a host with every standard extension enabled.

use std::sync::OnceLock;

use carve::api::Carve;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    static HOSTS: OnceLock<[Carve; 2]> = OnceLock::new();
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    if rest.len() > carve_fuzz::MAX_INPUT {
        return;
    }
    let hosts = HOSTS.get_or_init(|| {
        [
            carve_fuzz::extension_host(true),
            carve_fuzz::extension_host(false),
        ]
    });
    let carve = &hosts[usize::from(sel & 1)];
    let opts = carve_fuzz::options(sel);
    if let Ok(tree) = carve.parse(rest, &opts) {
        let _ = carve.render_tree_with_diagnostics(&tree, &opts);
    }
    let _ = carve.render_with_diagnostics(rest, &opts);
});
