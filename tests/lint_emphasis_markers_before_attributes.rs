//! `emphasis-marker-before-attributes` covers each of `*`, `/`, `_`, `~` and `=` that is text and
//! is followed by an attribute block (C2-I-028, C2-I-036).

use carve::lint::{LintOptions, lint};

#[test]
fn every_marker_of_the_row_is_reported() {
    for ch in ['*', '/', '_', '~', '='] {
        let source = format!("a{ch}b{ch}{{lang=de}}\n");
        let found: Vec<_> = lint(&source, &LintOptions::default())
            .into_iter()
            .filter(|f| f.code == "emphasis-marker-before-attributes")
            .filter_map(|f| f.pos.map(|p| p.start_offset))
            .collect();
        assert_eq!(found, [3], "{source:?}");
    }
}
