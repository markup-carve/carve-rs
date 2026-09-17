//! An unclosed backtick run ended at a forced or editorial closer drops the
//! trailing whitespace it drops at the end of a block, line breaks included.
//! In a line block a break is content (ruling markup-carve/carve#2089,
//! markup-carve/carve-rs#1748).

fn html(source: &str) -> String {
    let parsed = carve::render_html(&carve::parse(source)).unwrap();
    assert_eq!(carve::to_html(source), parsed, "the two routes disagree");
    parsed.trim_end().to_string()
}

#[test]
fn a_forced_closer_after_a_break() {
    assert_eq!(
        html("x{*`a\n*}\n"),
        "<p>x<strong><code>a</code></strong></p>"
    );
}

#[test]
fn an_editorial_closer_after_a_break() {
    assert_eq!(html("x{+`a\n+}\n"), "<p>x<ins><code>a</code></ins></p>");
}

#[test]
fn several_breaks_and_spaces_go_together() {
    assert_eq!(
        html("x{*`a  \n  *}\n"),
        "<p>x<strong><code>a</code></strong></p>"
    );
}

/// In a line block the break is content, so it stays.
#[test]
fn a_line_block_keeps_the_break() {
    assert_eq!(
        html("::: |\nx{*`a\n*}\n:::\n"),
        "<div class=\"line-block\">\n  <p>x<strong><code>a\n</code></strong></p>\n</div>"
    );
}

/// Control: content before the trailing whitespace is untouched.
#[test]
fn the_content_itself_is_kept() {
    assert_eq!(
        html("x{*`a b\n*}\n"),
        "<p>x<strong><code>a b</code></strong></p>"
    );
}

/// Control: a closed span keeps every byte between its runs.
#[test]
fn a_closed_span_keeps_its_break() {
    assert_eq!(
        html("x{*`a\n`*}\n"),
        "<p>x<strong><code>a\n</code></strong></p>"
    );
}
