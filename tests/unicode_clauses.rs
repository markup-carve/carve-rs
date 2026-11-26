use carve::chars::{is_control_or_format, is_joining, is_number, is_word};

#[test]
fn c2_i_127_unicode_18_categories_decide_inline_words() {
    assert!(is_word('\u{3d000}'));
    assert!(is_word('\u{30a0c}'));
    assert_eq!(
        carve::render("𽀀*𰨌*", &carve::Options::default()),
        "<p>𽀀*𰨌*</p>"
    );
}

#[test]
fn c2_i_127_uses_pinned_number_mark_and_format_categories() {
    assert!(is_number('①'));
    assert!(is_joining('\u{0301}'));
    assert!(is_control_or_format('\u{200b}'));
    assert!(!is_word('\u{10ffff}'));
}
