//! A comparison's `=`, a literal directive's tokens and a `%%` at the start of
//! insert or delete content.

fn body(source: &str) -> String {
    carve::to_html(source).trim().to_owned()
}

#[test]
fn a_comparison_equals_sign_closes_no_bare_highlight() {
    for (source, expected) in [
        ("=a <= b=", "<p><mark>a ≤ b</mark></p>"),
        ("=a >= b=", "<p><mark>a ≥ b</mark></p>"),
        ("=a != b=", "<p><mark>a ≠ b</mark></p>"),
        ("=a !== b=", "<p><mark>a ≠</mark> b=</p>"),
        ("=a <== b=", "<p><mark>a &lt;</mark>= b=</p>"),
        ("=a \\<= b=", "<p><mark>a &lt;</mark> b=</p>"),
    ] {
        assert_eq!(body(source), expected, "{source:?}");
    }
}

#[test]
fn a_token_inside_a_literal_directive_opens_nothing_around_it() {
    for (source, expected) in [
        (
            "*{{ `@lines:1 }}ter*",
            "<p><strong>{{ `@lines:1 }}ter</strong></p>",
        ),
        ("_{{ `@lines:1 }}_", "<p><u>{{ `@lines:1 }}</u></p>"),
        (
            "*{{ x.txt @lines:0-2 }} b*",
            "<p><strong>{{ x.txt @lines:0-2 }} b</strong></p>",
        ),
    ] {
        assert_eq!(body(source), expected, "{source:?}");
    }
}

#[test]
fn a_comment_marker_opening_insert_or_delete_content_is_text() {
    for (source, expected) in [
        ("{+%% b+}", "<p><ins>%% b</ins></p>"),
        ("{-%% b-}", "<p><del>%% b</del></p>"),
        ("{~%% a~>b~}", "<p><del>%% a</del><ins>b</ins></p>"),
        ("{~a~>%% b~}", "<p><del>a</del><ins>%% b</ins></p>"),
    ] {
        assert_eq!(body(source), expected, "{source:?}");
    }
    assert_eq!(body("{+a %% b+}"), "<p><ins>a</ins></p>");
}
