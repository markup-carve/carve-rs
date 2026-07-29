//! A blank line inside a marker-line item does not end its sub-list.
//!
//! `- - A` opens a sub-list on the marker line. When its first item is loose,
//! the blank separating that item's blocks is followed by a sibling marker at
//! the sub-list's own column - which is SHALLOWER than the indented block above
//! it, but still inside the item. The collector compared the following line's
//! indent against the first collected block's indent rather than against the
//! item's content column, ended the block there, and the sibling started a
//! second list (carve-rs#301).
//!
//! Reached through `carve fmt`: the writer emits the blank that the source did
//! not have, so formatting a document changed what it rendered as - both PART 11
//! section 1 invariants at once. carve-js and carve-php read all of these as one
//! list, and are the oracle here.

fn ul_count(html: &str) -> usize {
    html.matches("<ul>").count()
}

#[test]
fn a_sibling_after_a_blank_stays_in_the_marker_line_sublist() {
    // The separator's own indentation is irrelevant - all three spellings are a
    // blank line, and all three used to split.
    for separator in ["", "  ", "    "] {
        let src = format!("- - A\n\n    second\n{separator}\n  - B\n");
        let html = carve::to_html(&src);
        assert_eq!(
            ul_count(&html),
            2,
            "separator {separator:?} split the sub-list:\n{html}"
        );
        assert!(
            html.contains("<p>B</p>"),
            "separator {separator:?} left B tight:\n{html}"
        );
    }
}

#[test]
fn formatting_the_corpus_case_preserves_what_it_renders() {
    // to_html(fmt(x)) == to_html(x), and fmt is idempotent.
    let src = "- - A\n\n    second\n  - B\n";
    let once = carve::to_carve(src);
    assert_eq!(
        carve::to_html(&once),
        carve::to_html(src),
        "formatting changed the rendered document"
    );
    assert_eq!(carve::to_carve(&once), once, "fmt is not idempotent");
}

#[test]
fn ordinary_nesting_is_unchanged() {
    // The non-marker-line form always worked; it must keep working.
    let html = carve::to_html("- x\n  - A\n\n    second\n  - B\n");
    assert_eq!(ul_count(&html), 2);
}

#[test]
fn a_real_dedent_still_ends_the_block() {
    // The rule this loosens exists so a dedent landing below a sub-list closes
    // it rather than folding in. A line BELOW the content column still must.
    let html = carve::to_html("- - A\n\n    second\n\nafter\n");
    assert!(
        html.contains("<p>after</p>"),
        "the dedented paragraph was absorbed into the list:\n{html}"
    );
}
