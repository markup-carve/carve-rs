#[test]
fn below_column_description_returns_to_its_outer_body() {
    let hosts = [
        ":: outer\n:  detail\n    :: inner\n :  tail",
        "- a\n  :: outer\n  :  detail\n      :: inner\n   :  tail",
        "> :: outer\n> :  detail\n>     :: inner\n>  :  tail",
        "::: note\n:: outer\n:  detail\n    :: inner\n :  tail\n:::",
        "- a\n  - b\n    :: outer\n    :  detail\n        :: inner\n     :  tail",
    ];
    for source in hosts {
        let html = carve::to_html(source);
        assert!(html.contains("<dt>inner</dt>"), "{source:?}: {html}");
        assert!(html.contains("<p>:  tail</p>"), "{source:?}: {html}");
        assert!(!html.contains("<dd>tail</dd>"), "{source:?}: {html}");
    }
}

#[test]
fn a_past_column_quote_remains_term_text() {
    for (source, expected_quotes) in [
        (":: outer\n:  detail\n     :: inner\n    > quote", 0),
        (
            "- a\n  :: outer\n  :  detail\n       :: inner\n      > quote",
            0,
        ),
        ("> :: outer\n> :  detail\n>      :: inner\n>     > quote", 1),
    ] {
        let html = carve::to_html(source);
        assert!(
            html.contains("<dt>inner\n &gt; quote</dt>"),
            "{source:?}: {html}"
        );
        assert_eq!(
            html.matches("<blockquote>").count(),
            expected_quotes,
            "{source:?}: {html}"
        );
    }
}

#[test]
fn a_quote_in_a_description_keeps_the_outer_items_marker_as_text() {
    let source = "- a\n  :: outer\n  :  detail\n     :: inner\n     > quote\n  - sibling";
    let html = carve::to_html(source);
    assert!(
        html.contains("<blockquote><p>quote\n- sibling</p></blockquote>"),
        "{html}"
    );
}

#[test]
fn a_quote_at_the_nested_terms_column_opens_a_block() {
    let source = ":: outer\n:  detail\n    :: inner\n    > quote";
    let html = carve::to_html(source);
    assert!(html.contains("<dt>inner</dt>"), "{html}");
    assert!(
        html.contains("<blockquote><p>quote</p></blockquote>"),
        "{html}"
    );
}
