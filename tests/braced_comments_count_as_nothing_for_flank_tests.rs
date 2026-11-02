//! A braced comment counts as nothing for every flank test (markup-carve/carve#4719,
//! superseding ruling): the hyphen run's flag test and the bare emphasis guards
//! read past it, as quote direction already did.

use carve::{djot_to_carve, to_html};

#[test]
fn flank_tests_read_past_braced_comments() {
    for (source, expected) in [
        ("{%%}--x\n", "<p>--x</p>"),
        ("a{%%}*b*\n", "<p>a*b*</p>"),
        ("a {%%}--x\n", "<p>a --x</p>"),
        ("a--{%%}x\n", "<p>a–x</p>"),
        ("x --{% c %}\n", "<p>x –</p>"),
        ("a{% c %}{%%}/b/\n", "<p>a/b/</p>"),
        ("*a*{%%}b\n", "<p>*a*b</p>"),
        ("*a{%%}*\n", "<p><strong>a</strong></p>"),
        ("*{%%}a*\n", "<p><strong>a</strong></p>"),
        ("*{%%} a*\n", "<p>* a*</p>"),
        ("_{%%}/x/\n", "<p>_<em>x</em></p>"),
        ("a{%%}/*b*/\n", "<p>a/<strong>b</strong>/</p>"),
        ("/*a*/{%%}b\n", "<p>/<strong>a</strong>/b</p>"),
        ("\\{%%}*b*\n", "<p>{%%}<strong>b</strong></p>"),
        ("`{%%}`*b*\n", "<p><code>{%%}</code><strong>b</strong></p>"),
        ("\"a-\"{%%}\n", "<p>“a-”</p>"),
    ] {
        assert_eq!(to_html(source).trim(), expected, "{source:?}");
    }
}

#[test]
fn djot_braces_a_span_beside_an_empty_attribute_block() {
    for (source, expected) in [
        ("*a*{}*b*\n", "{*a*}{%%}{*b*}\n"),
        ("*a*{} b\n", "{*a*}{%%} b\n"),
        ("a {}*b*\n", "a {%%}{*b*}\n"),
    ] {
        let carve = djot_to_carve(source);
        assert_eq!(carve, expected, "{source:?}");
        assert_eq!(
            to_html(&carve).matches("<strong>").count(),
            source.matches('*').count() / 2,
            "{source:?}"
        );
    }
}
