//! CARVE-P2-035 measures attribute-run continuations at source columns in quotes.

#[test]
fn shallow_lazy_attribute_run_is_literal_at_each_quote_depth() {
    for (source, expected) in [
        (
            "> q\n> {.k\ntail\n> #x}\n",
            "<blockquote><p>q\n{.k\ntail\n#x}</p></blockquote>",
        ),
        (
            "> > q\n> > {.k\ntail\n> > #x}\n",
            "<blockquote>\n  <blockquote><p>q\n{.k\ntail\n#x}</p></blockquote>\n</blockquote>",
        ),
    ] {
        assert_eq!(carve::to_html(source), expected, "source: {source:?}");
    }
}

#[test]
fn a_deeper_continuation_keeps_the_attribute_run() {
    let source = "> q\n> {.k\n>   #x}\n> tail\n";
    assert_eq!(
        carve::to_html(source),
        "<blockquote>\n  <p>q</p>\n  <p class=\"k\" id=\"x\">tail</p>\n</blockquote>"
    );
}

#[test]
fn a_lazy_line_one_source_column_deeper_does_not_refuse() {
    let source = "> q\n> {.k\n tail\n>  #x}\n";
    let expected = "<blockquote><p>q\n{.k\ntail\n<span class=\"tag\"><strong>#x</strong></span>}</p></blockquote>";
    assert_eq!(carve::to_html(source), expected);
    assert_eq!(
        carve::to_html_with_options(source, &carve::Options::default().with_positions(true)),
        expected
    );
}

#[test]
fn the_opening_braces_authored_indent_is_the_comparison_column() {
    for (source, expected) in [
        (
            "> q\n>  {.k\n tail\n>  #x}\n",
            "<blockquote><p>q\n{.k\ntail\n#x}</p></blockquote>",
        ),
        (
            "> q\n>  {.k\n  tail\n>   #x}\n",
            "<blockquote><p>q\n{.k\ntail\n<span class=\"tag\"><strong>#x</strong></span>}</p></blockquote>",
        ),
    ] {
        assert_eq!(carve::to_html(source), expected);
        assert_eq!(
            carve::to_html_with_options(source, &carve::Options::default().with_positions(true)),
            expected
        );
    }
}
