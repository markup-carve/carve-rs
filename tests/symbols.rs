use carve::{BlockNode, InlineNode, Options};

fn symbol_count(source: &str) -> usize {
    carve::parse(source)
        .children
        .iter()
        .map(|block| match block {
            BlockNode::Paragraph(p) => p
                .children
                .iter()
                .filter(|node| matches!(node, InlineNode::Symbol(_)))
                .count(),
            _ => 0,
        })
        .sum()
}

#[test]
fn symbol_requires_leading_word_boundary() {
    // The guard is what keeps a time, a ratio, or any word-glued colon run from
    // becoming a symbol once a map is active -- `word:+-:` is the same shape as
    // `10:30:` and must behave the same.
    for source in ["a:b:c", "10:30: x", "word:rocket:", "word:+-:"] {
        assert_eq!(symbol_count(source), 0, "{source}");
    }
    for source in ["(:tada:)", "start :rocket:"] {
        assert_eq!(symbol_count(source), 1, "{source}");
    }
}

#[test]
fn symbol_first_char_is_alnum_plus_or_minus() {
    assert_eq!(symbol_count(":1up:"), 1);
    // The reaction shortcodes parse; a leading `_` does not (it would steal
    // from underline), and a symbol beats the typographic `+-` substitution.
    assert_eq!(symbol_count(":+1:"), 1);
    assert_eq!(symbol_count(":-1:"), 1);
    assert_eq!(symbol_count(":+-:"), 1);
    assert_eq!(symbol_count(":_x:"), 0);
}

#[test]
fn symbol_beats_smart_typography_but_only_where_it_opens() {
    let opts = Options::new().with_symbol("+-", "SYM");
    assert_eq!(
        carve::to_html_with_options("Tol :+-: here", &opts),
        "<p>Tol SYM here</p>"
    );
    // No symbol opens: the typographic form applies as usual.
    assert_eq!(
        carve::to_html_with_options("a +- b and word:+-:", &opts),
        "<p>a \u{b1} b and word:\u{b1}:</p>"
    );
}

#[test]
fn symbol_attrs_wrap_html_output() {
    let mapped = Options::new().with_symbol("rocket", "X");
    assert_eq!(
        carve::to_html_with_options(":rocket:{.big}", &mapped),
        "<p><span class=\"big\">X</span></p>"
    );
    assert_eq!(
        carve::to_html(":rocket:{.big}"),
        "<p><span class=\"big\">:rocket:</span></p>"
    );
    assert_eq!(carve::to_html_with_options(":rocket:", &mapped), "<p>X</p>");
    assert_eq!(carve::to_html(":rocket:"), "<p>:rocket:</p>");
}

#[test]
fn symbol_map_value_is_raw_and_literal_is_escaped() {
    let mapped = Options::new().with_symbol("rocket", "<b>X</b>");
    assert_eq!(
        carve::to_html_with_options(":rocket:", &mapped),
        "<p><b>X</b></p>"
    );
    assert_eq!(carve::to_html(":r<:"), "<p>:r&lt;:</p>");
}

#[test]
fn symbol_attrs_round_trip_in_carve_renderer() {
    assert_eq!(carve::to_carve(":rocket:{.big}"), ":rocket:{.big}\n");
}

#[test]
fn colon_inline_extension_still_wins() {
    assert_eq!(symbol_count(":kbd[Ctrl]"), 0);
    assert_eq!(carve::to_html(":kbd[Ctrl]"), "<p><kbd>Ctrl</kbd></p>");
}
