use carve::lint::{LintOptions, lint};

fn ranges(source: &str, code: &str) -> Vec<(u32, u32)> {
    lint(source, &LintOptions::default())
        .into_iter()
        .filter(|finding| finding.code == code)
        .filter_map(|finding| finding.pos.map(|pos| (pos.start_offset, pos.end_offset)))
        .collect()
}

#[test]
fn item_div_opener_uses_the_marker_column_even_when_it_is_the_first_child() {
    let source = "[^f]: - [ ] ::: box\n\t- x\n";
    assert_eq!(
        ranges(source, "list-item-block-overindented"),
        vec![(12, 19)]
    );
    for source in ["- ::: box\n", "1. ::: box\n"] {
        assert!(
            ranges(source, "list-item-block-overindented").is_empty(),
            "{source:?}"
        );
    }
}

#[test]
fn plus_inside_a_quote_on_an_item_marker_line_is_not_a_continuation() {
    assert!(ranges("    * > > +\n> >\n", "deprecated-continuation-marker").is_empty());
    assert_eq!(
        ranges("* +\n", "deprecated-continuation-marker"),
        vec![(2, 3)]
    );
    assert_eq!(
        ranges("> +\n", "deprecated-continuation-marker"),
        vec![(2, 3)]
    );
    for source in ["- - +\n", "- * +\n"] {
        assert_eq!(
            ranges(source, "deprecated-continuation-marker"),
            vec![(4, 5)],
            "{source:?}"
        );
    }
}

#[test]
fn failed_item_interruption_still_offers_its_whole_spaced_hyphen_run() {
    assert_eq!(
        ranges(":: t\n: d\n  - ----\n", "deprecated-spaced-thematic-break"),
        vec![(11, 17)]
    );
    assert!(ranges(":: t\n: d\n  - ----\n", "thematic-break-spelling").is_empty());
}
