//! C2-H-043 (carve#6762): the `&nbsp;` runs a line block writes for its
//! whitespace import back as spaces; an authored U+00A0 the renderer never
//! writes for whitespace stays.

use carve::api::{self, ImportOptions};

fn import(body: &str) -> String {
    let html =
        format!("<div class=\"line-block\" data-carve-line-block>\n  <p>{body}</p>\n</div>\n");
    api::import_document(&html, &ImportOptions::default())
        .unwrap()
        .source
}

fn nbsp(n: usize) -> String {
    "&nbsp;".repeat(n)
}

#[test]
fn cc_2143_lead_and_interior_runs() {
    let body = format!(
        "tab gap<br>\nwide{}gap<br>\n{}lead<br>\nwide{}two<br>\na b",
        nbsp(8),
        nbsp(4),
        nbsp(2)
    );
    assert_eq!(
        import(&body),
        "::: |\ntab gap\nwide        gap\n    lead\nwide  two\na b\n:::\n"
    );
}

#[test]
fn cc_2144_run_inside_emphasis() {
    assert_eq!(
        import("<em>a&nbsp;&nbsp;b</em> c"),
        "::: |\n/a  b/ c\n:::\n"
    );
}

#[test]
fn cc_2145_code_keeps_its_text() {
    assert_eq!(
        import("<code>a  b\tc</code>&nbsp;&nbsp;d"),
        "::: |\n`a  b\tc`  d\n:::\n"
    );
}

#[test]
fn one_column_lead_is_a_space() {
    assert_eq!(import("&nbsp;a<br>\n&nbsp;b"), "::: |\n a\n b\n:::\n");
}

#[test]
fn authored_no_break_spaces_stay() {
    for (body, line) in [
        ("a&nbsp;b", "a\u{a0}b"),
        ("a&nbsp; b", "a\u{a0} b"),
        ("&nbsp; a", "\u{a0} a"),
        ("a&nbsp;&nbsp; b", "a\u{a0}\u{a0} b"),
        ("<em>&nbsp;a</em>", "{/\u{a0}a/}"),
    ] {
        assert_eq!(import(body), format!("::: |\n{line}\n:::\n"), "{body}");
    }
}
