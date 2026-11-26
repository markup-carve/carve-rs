#![no_main]
//! Safe output (C2-H-160, C2-H-161): with raw HTML off, the HTML a browser
//! builds from the output holds no script-bearing element, no event handler
//! attribute and no URL attribute whose scheme is on the default deny list.
//! Rendered through a host with every standard extension.

use std::sync::OnceLock;

use carve::api::Carve;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    static HOST: OnceLock<Carve> = OnceLock::new();
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    if rest.len() > carve_fuzz::MAX_INPUT {
        return;
    }
    let carve = HOST.get_or_init(|| carve_fuzz::extension_host(true));
    let mut opts = carve_fuzz::options(sel);
    opts.raw_html = false;
    let Ok(rendered) = carve.render_with_diagnostics(rest, &opts) else {
        return;
    };
    if let Some(problem) = carve_fuzz::unsafe_html(&rendered.html) {
        panic!(
            "unsafe output: {problem}\n--- input\n{}\n--- html\n{}",
            String::from_utf8_lossy(rest),
            rendered.html
        );
    }
});
