//! PART 9R R3 matches a term in RENDERED TEXT at word boundaries. The container
//! the text sits in does not change that.
//!
//! This engine dropped the expansion inside a SPAN - ordinary or compact
//! semantic - because `apply_abbreviations_inline` matched `Emphasis`, `Link`
//! and `Extension` and let a `Span` fall to the catch-all arm, so its children
//! were never walked. PART 9 section 10 then made `[HTML]{kbd}` a documented
//! feature, which put the silent loss inside a construct the docs teach.
//!
//! carve-js held the OPPOSITE hole - it expanded inside a span and dropped
//! inside `:name[…]` - and carve-php was right on every row, so two engines
//! carried opposite defects for months (carve#1151). Nothing caught it: the
//! corpus pinned exactly one case here, the explicit-`abbr` row all three
//! agreed on, leaving every neighbouring row unpinned.

fn html(body: &str) -> String {
    carve::to_html(&format!("*[HTML]: Long Form\n\n{body}\n"))
}

const EXPANDED: &str = "<abbr title=\"Long Form\">HTML</abbr>";

#[test]
fn expands_inside_an_ordinary_span() {
    assert_eq!(
        html("The [HTML]{.x} key."),
        format!("<p>The <span class=\"x\">{EXPANDED}</span> key.</p>")
    );
}

#[test]
fn expands_inside_a_compact_semantic_span() {
    assert_eq!(
        html("The [HTML]{kbd} key."),
        format!("<p>The <kbd>{EXPANDED}</kbd> key.</p>")
    );
}

#[test]
fn controls_emphasis_and_a_link_already_agreed_and_must_keep_agreeing() {
    // These pin that the containers above are not being special-cased in one
    // direction: a fix that only widened the span arm leaves these untouched,
    // which is what tells a regression test apart from a control.
    assert_eq!(
        html("Both *HTML* and [HTML](/u)."),
        format!("<p>Both <strong>{EXPANDED}</strong> and <a href=\"/u\">{EXPANDED}</a>.</p>")
    );
}

#[test]
fn an_explicit_abbr_attribute_still_wins_over_the_definition() {
    // carve#1127: the authored expansion is the exception, and a walk that
    // reaches further must not start applying the definition on top of it.
    assert_eq!(
        html("The [HTML]{abbr=\"Custom\"} key."),
        "<p>The <abbr title=\"Custom\">HTML</abbr> key.</p>"
    );
}
