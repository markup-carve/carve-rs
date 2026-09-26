use carve::{to_carve, to_html};

#[test]
fn a_leading_newline_does_not_leave_a_trailing_pad() {
    for source in ["`z` ``\n`\n", "before ``\n`\n", "before ```\n``\n"] {
        let formatted = to_carve(source);
        assert_eq!(
            to_html(&formatted),
            to_html(source),
            "{source:?} -> {formatted:?}"
        );
        assert_eq!(to_carve(&formatted), formatted);
        assert!(!formatted.lines().any(|line| line.ends_with(' ')));
    }
}

#[test]
fn a_closed_span_still_protects_following_content() {
    for source in [
        "before `` `x` `` after\n",
        "before `` `x` ``{.code}\n",
        "before `` x ` `` after\n",
    ] {
        let formatted = to_carve(source);
        assert_eq!(to_html(&formatted), to_html(source));
        assert_eq!(to_carve(&formatted), formatted);
    }
}
