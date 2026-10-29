//! A list marker on a continuation line does not interrupt a paragraph, so the
//! writer leaves it bare inside a code span (CARVE-P11-007), and pads it only
//! where the line would otherwise end an item or a description.

use carve::{to_carve, to_html};

fn round_trips(source: &str) -> String {
    let written = to_carve(source);
    assert_eq!(to_html(&written), to_html(source), "{source:?}");
    assert_eq!(to_carve(&written), written, "{source:?}");
    written
}

#[test]
fn a_list_marker_in_a_paragraph_code_span_is_written_bare() {
    for marker in ["- x", "* x", "1. x", "1) x", "a. x", "I. x"] {
        for prefix in ["", "a "] {
            let source = format!("{prefix}`b\n{marker}`\n");
            assert_eq!(round_trips(&source), source);
        }
        let quoted = format!("> `b\n> {marker}`\n");
        assert_eq!(round_trips(&quoted), quoted);
    }
    let in_item_quote = "- > a `b\n  > 1. x`\n";
    assert_eq!(round_trips(in_item_quote), in_item_quote);
    assert_eq!(to_carve("`\n1. x\n"), "`\n1. x`\n");
}

#[test]
fn a_line_below_an_item_or_description_keeps_its_guard() {
    for marker in ["1. x", "a) x"] {
        for quote in ["", "> "] {
            let source = format!("{quote}- lead\n{quote}  a `b\n{quote} {marker}`\n");
            assert_eq!(round_trips(&source), source);
        }
    }
    for quote in ["", "> "] {
        let blank = quote.trim_end();
        assert_eq!(
            round_trips(&format!("{quote}- lead\n{quote}  a `b\n{quote} - x`\n")),
            format!("{quote}- lead\n{quote}  a `b`\n{quote}- x``\n")
        );
        assert_eq!(
            round_trips(&format!("{quote}- lead\n{quote}  a `b\n{quote} - [ ] x`\n")),
            format!("{quote}- lead\n{quote}  a `b`\n{blank}\n{quote}- [ ] x``\n")
        );
    }
    assert_eq!(round_trips(":: t\n: a `b\n- x`\n"), ":: t\n: a `b\n - x`\n");
    assert_eq!(round_trips("`a\n > x`\n"), "`a`\n\n> x``\n");
}

#[test]
fn a_colon_run_with_content_is_bare_inside_a_code_span() {
    for line in [":::}", ":::~", ":::t"] {
        let source = format!("`\n{line}");
        assert_eq!(round_trips(&source), format!("`\n{line}`\n"));
    }
}
