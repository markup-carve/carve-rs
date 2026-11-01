use carve::{to_carve, to_html};

#[test]
fn a_marker_line_child_inside_a_div_keeps_its_following_blocks() {
    let source = include_str!(
        "spec/tests/corpus/743-a-later-term-after-a-shifted-separator-opens-in-the-outer-item-3.crv"
    );
    let expected = include_str!(
        "spec/tests/corpus/743-a-later-term-after-a-shifted-separator-opens-in-the-outer-item-3.html"
    );
    let formatted = to_carve(source);
    assert_eq!(to_html(source).trim(), expected.trim());
    assert_eq!(to_html(&formatted), to_html(source));
    assert_eq!(to_carve(&formatted), formatted);
}
