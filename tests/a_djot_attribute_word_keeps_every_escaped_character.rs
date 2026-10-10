//! An attribute attaches to the whole non-ASCII-whitespace run, escaped
//! characters included: the boundary is whitespace in the source, not
//! punctuation and not an escape.
//!
//! djot.js splits the run at an escape, contradicting the definition in its own
//! `test/attributes.test`, so it is not the authority for these shapes and the
//! whole-word reading is: markup-carve/carve#2848, jgm/djot#417. Every
//! expectation below is byte-identical to carve-js and carve-php.

fn converted(source: &str) -> (String, String) {
    let carve = carve::djot_to_carve(source);
    let html = carve::to_html(&carve).trim().to_string();
    (carve, html)
}

fn check(source: &str, expected_carve: &str, expected_html: &str) {
    let (carve, html) = converted(source);
    assert_eq!(carve, expected_carve, "carve for {source:?}");
    assert_eq!(html, expected_html, "html for {source:?}");
}

#[test]
fn an_attribute_word_keeps_every_escaped_character() {
    let mut moved = Vec::new();
    for (source, carve, html) in [
        (
            "a\\{b{.c}",
            "[a\\{b]{.c}",
            "<p><span class=\"c\">a{b</span></p>",
        ),
        (
            "a\\}b{.c}",
            "[a\\}b]{.c}",
            "<p><span class=\"c\">a}b</span></p>",
        ),
        (
            "\\{a\\_b\\*c\\}d{.c}",
            "[\\{a\\_b\\*c\\}d]{.c}",
            "<p><span class=\"c\">{a_b*c}d</span></p>",
        ),
        (
            "x\\_y \\{z\\}{.c}",
            "x\\_y [\\{z\\}]{.c}",
            "<p>x_y <span class=\"c\">{z}</span></p>",
        ),
        (
            "a\\{{.c}",
            "[a\\{]{.c}",
            "<p><span class=\"c\">a{</span></p>",
        ),
        (
            "a\\}{.c}",
            "[a\\}]{.c}",
            "<p><span class=\"c\">a}</span></p>",
        ),
        (
            "a\\{b\\}c{.c}",
            "[a\\{b\\}c]{.c}",
            "<p><span class=\"c\">a{b}c</span></p>",
        ),
        (
            "a\\{b\\_c{.c}",
            "[a\\{b\\_c]{.c}",
            "<p><span class=\"c\">a{b_c</span></p>",
        ),
        (
            "\\{\\}{.c}",
            "[\\{\\}]{.c}",
            "<p><span class=\"c\">{}</span></p>",
        ),
        (
            "a \\{b{.c}",
            "a [\\{b]{.c}",
            "<p>a <span class=\"c\">{b</span></p>",
        ),
        (
            "a\\]b{.c}",
            "[a\\]b]{.c}",
            "<p><span class=\"c\">a]b</span></p>",
        ),
        (
            "a\\\"b{.c}",
            "[a\\\"b]{.c}",
            "<p><span class=\"c\">a\"b</span></p>",
        ),
        (
            "a\\`b{.c}",
            "[a\\`b]{.c}",
            "<p><span class=\"c\">a`b</span></p>",
        ),
        (
            "w\\{x\\}{.d}",
            "[w\\{x\\}]{.d}",
            "<p><span class=\"d\">w{x}</span></p>",
        ),
        (
            "a\\[^x]{.c}",
            "[a\\[^x\\]]{.c}",
            "<p><span class=\"c\">a[^x]</span></p>",
        ),
        (
            "\\[^x\\]{.c}",
            "[\\[^x\\]]{.c}",
            "<p><span class=\"c\">[^x]</span></p>",
        ),
        (
            "w\\[^x\\]{.c}",
            "[w\\[^x\\]]{.c}",
            "<p><span class=\"c\">w[^x]</span></p>",
        ),
    ] {
        let (got_carve, got_html) = converted(source);
        if got_carve != carve || got_html != html {
            moved.push(format!(
                "{source:?}\n  carve want {carve:?}\n  carve  got {got_carve:?}\n   html want {html:?}\n   html  got {got_html:?}"
            ));
        }
    }
    assert!(
        moved.is_empty(),
        "{} shapes wrong:\n{}",
        moved.len(),
        moved.join("\n")
    );
}

/// Escaped LITERAL WHITESPACE is still a boundary: the run ends at it, so the
/// attribute takes `b` alone. Pinned deliberately, in all three engines.
#[test]
fn an_escaped_literal_space_stays_a_word_boundary() {
    check(
        "a\\ b{.c}",
        "a\\ [b]{.c}",
        "<p>a&nbsp;<span class=\"c\">b</span></p>",
    );
}

/// The ticket's own example was already correct, so a measurement of it alone
/// would have found nothing to do.
#[test]
fn an_escaped_run_opening_at_position_zero_was_already_whole() {
    check(
        "\\{a\\_b}{.c}",
        "[\\{a\\_b}]{.c}",
        "<p><span class=\"c\">{a_b}</span></p>",
    );
}

#[test]
fn an_escaped_emphasis_marker_inside_the_run_was_already_whole() {
    check(
        "foo\\*bar{.c}",
        "[foo\\*bar]{.c}",
        "<p><span class=\"c\">foo*bar</span></p>",
    );
}

#[test]
fn a_run_starting_after_a_space_was_already_whole() {
    check(
        "a \\}b{.c}",
        "a [\\}b]{.c}",
        "<p>a <span class=\"c\">}b</span></p>",
    );
}

#[test]
fn an_unescaped_brace_pair_inside_the_run_is_attributed_whole() {
    check(
        "w{x}{.c}",
        "[w\\{x}]{.c}",
        "<p><span class=\"c\">w{x}</span></p>",
    );
}

#[test]
fn a_run_with_no_escape_at_all_is_attributed_whole() {
    check(
        "word{.c}",
        "[word]{.c}",
        "<p><span class=\"c\">word</span></p>",
    );
}
