//! `/` around a content that opens and closes with `*` is the combined
//! bold-italic spelling, which re-parses with the nesting inverted. The outer
//! marker takes braces (markup-carve/carve-rs#1660).

fn carve(src: &str) -> String {
    carve::to_carve(src)
}

fn round_trips(src: &str) -> bool {
    carve::to_html(&carve(src)) == carve::to_html(src)
}

#[test]
fn an_italic_wrapping_a_strong_braces_the_outer_marker() {
    assert_eq!(carve("{/{*x*}/}\n"), "{/*x*/}\n");
    assert!(round_trips("{/{*x*}/}\n"));
}

#[test]
fn the_test_is_on_the_bytes_rather_than_on_one_child() {
    // Two strongs with text between them write the same hazard.
    assert_eq!(carve("{/{*x*} y {*z*}/}\n"), "{/*x* y *z*/}\n");
    assert!(round_trips("{/{*x*} y {*z*}/}\n"));
}

#[test]
fn a_combined_bold_italic_keeps_its_bare_spelling() {
    assert_eq!(carve("/*x*/\n"), "/*x*/\n");
    assert!(round_trips("/*x*/\n"));
}

#[test]
fn a_strong_wrapping_an_italic_is_untouched() {
    assert_eq!(carve("{*{/x/}*}\n"), "*/x/*\n");
    assert!(round_trips("{*{/x/}*}\n"));
}

#[test]
fn a_strike_wrapping_a_strong_is_untouched() {
    // The hazard is the `/` marker around a strong, not any marker around one.
    assert_eq!(carve("{~{*x*}~}\n"), "~*x*~\n");
    assert!(round_trips("{~{*x*}~}\n"));
}

#[test]
fn an_italic_whose_content_only_starts_with_the_strong_marker_is_untouched() {
    assert_eq!(carve("{/{*x*} y/}\n"), "/*x* y/\n");
    assert!(round_trips("{/{*x*} y/}\n"));
}
