//! An unclosed verbatim run takes the rest of the block, a forced-span closer
//! included (markup-carve/carve#3973 X5), and strips nothing there.

use carve::{parse, render_carve, render_html};

fn html(src: &str) -> String {
    render_html(&parse(src)).expect("renders")
}

#[test]
fn a_forced_span_closer_does_not_end_the_run() {
    assert_eq!(html("{~` ~}\n"), "<p>{~<code> ~}</code></p>");
    assert_eq!(html("{~`a  ~}\n"), "<p>{~<code>a  ~}</code></p>");
}

#[test]
fn a_tab_before_the_closer_is_content_too() {
    assert_eq!(html("{~`a\t~}\n"), "<p>{~<code>a\t~}</code></p>");
}

#[test]
fn a_newline_is_content_only_in_a_line_block() {
    // Corpus 380's shape. Outside a line block the break goes with the spaces
    // (ruling B on markup-carve/carve#2089, markup-carve/carve-rs#1748).
    assert!(
        html("::: |\n`\n%%\n:::\n").contains("<code>\n</code>"),
        "{}",
        html("::: |\n`\n%%\n:::\n")
    );
    assert_eq!(html("{~` a\n~}\n"), "<p>{~<code> a\n~}</code></p>");
}

#[test]
fn a_closed_span_inside_a_forced_span_is_untouched() {
    // CONTROL. The closed span keeps its own single-space strip.
    assert_eq!(html("{~` a `~}\n"), "<p><s><code>a</code></s></p>");
    assert_eq!(html("{~`x`~}\n"), "<p><s><code>x</code></s></p>");
}

/// The pair has no closer, so its opener is text and the writer closes the
/// run it reads.
#[test]
fn an_unclosed_run_in_a_braced_pair_is_written_as_text_and_a_closed_run() {
    for (src, want) in [
        ("{~` ~}\n", "{~` ~}`\n"),
        ("a {~` ~} b\n", "a {~` ~} b`\n"),
        ("{*`  *}\n", "{*`  *}`\n"),
    ] {
        let doc = parse(src);
        let written = render_carve(&doc).expect("writes");
        assert_eq!(written, want, "{src:?}");
        assert_eq!(
            render_html(&parse(&written)).unwrap(),
            render_html(&doc).unwrap(),
            "{src:?} round trip"
        );
    }
}

#[test]
fn an_emphasis_ending_in_a_filled_code_span_stays_bare() {
    // CONTROL. A filled span closes itself, so nothing needs the braces.
    assert_eq!(render_carve(&parse("{~`a`~}\n")).unwrap(), "~`a`~\n");
}
