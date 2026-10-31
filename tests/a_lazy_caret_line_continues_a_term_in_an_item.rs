// carve-rs#3626: a term takes no caption, so a caret line below the item's
// content column is a lazy line of the term (PART 0 B-term-keep).
#[test]
fn a_caret_line_below_the_item_column_folds_into_the_term() {
    for source in ["- e\n  :: m\n^ p\n", "- e\n  :: m\n ^ p\n", "- :: m\n^ p\n", "1. e\n   :: m\n^ p\n"] {
        let html = carve::to_html(source);
        assert!(html.contains("<dt>m\n^ p</dt>"), "{source:?}\n{html}");
    }
}

#[test]
fn a_caret_line_at_the_term_column_still_ends_the_term() {
    for source in ["- e\n  :: m\n  ^ p\n", ":: m\n^ p\n"] {
        let html = carve::to_html(source);
        assert!(html.contains("<dt>m</dt>"), "{source:?}\n{html}");
    }
}
