//! An unclosed verbatim run strips its trailing whitespace at whatever ends
//! it, and a forced-span closer ends it as much as the block end does
//! (markup-carve/carve#2051).

use carve::{parse, render_carve, render_html};

fn html(src: &str) -> String {
    render_html(&parse(src)).expect("renders")
}

#[test]
fn a_forced_span_closer_strips_the_run_it_ends() {
    assert_eq!(html("{~` ~}\n"), "<p><s><code></code></s></p>");
    assert_eq!(html("{~`a  ~}\n"), "<p><s><code>a</code></s></p>");
}

#[test]
fn a_tab_is_stripped_too() {
    assert_eq!(html("{~`a\t~}\n"), "<p><s><code>a</code></s></p>");
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
    assert_eq!(html("{~` a\n~}\n"), "<p><s><code> a</code></s></p>");
}

#[test]
fn a_closed_span_inside_a_forced_span_is_untouched() {
    // CONTROL. The closed span keeps its own single-space strip.
    assert_eq!(html("{~` a `~}\n"), "<p><s><code>a</code></s></p>");
    assert_eq!(html("{~`x`~}\n"), "<p><s><code>x</code></s></p>");
}

/// The strip leaves an EMPTY code span, whose only spelling is a backtick run
/// its container ends. Inside an emphasis only the braced closer ends it: the
/// bare `~` is swallowed by the open run.
#[test]
fn an_emphasis_ending_in_an_empty_code_span_writes_the_braced_closer() {
    for (src, want) in [
        ("{~` ~}\n", "{~``~}\n"),
        ("a {~` ~} b\n", "a {~``~} b\n"),
        ("{*`  *}\n", "{*``*}\n"),
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
