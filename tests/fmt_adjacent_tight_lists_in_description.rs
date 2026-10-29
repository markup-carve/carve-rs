use carve::{parse, render_carve, to_html};

#[test]
fn adjacent_tight_lists_stay_in_a_description_body() {
    for (first, second, tail) in [
        ("- a", "- c", "\t"),
        ("* alpha", "* beta", "  "),
        ("1. first", "1. second", ""),
    ] {
        let source = format!(":: term\n: {first}\n\n   {second}{tail}\n");
        let written = render_carve(&parse(&source)).unwrap();
        assert_eq!(to_html(&written), to_html(&source), "{source:?}");
        assert_eq!(
            render_carve(&parse(&written)).unwrap(),
            written,
            "{source:?}"
        );
        // Rule A (carve#3885): the second marker sits at the body column.
        assert!(
            written.contains("\n\n  ") && !written.contains("\n\n   "),
            "{written:?}"
        );
    }
}

#[test]
fn shifted_lists_keep_all_items_and_nested_children() {
    for (first, body) in [
        ("- a", "- c\n   - d"),
        ("1. a", "1. c\n   2. d"),
        ("9. a", "9. c\n   10. d"),
        ("- a", "- c\n     - nested"),
        ("- a", "- c\n\n    - e"),
        ("- a", "- c\n\n   - d"),
    ] {
        let source = format!(":: term\n: {first}\n\n   {body}\n");
        let written = render_carve(&parse(&source)).unwrap();
        assert_eq!(to_html(&written), to_html(&source), "{source:?}");
        assert_eq!(
            render_carve(&parse(&written)).unwrap(),
            written,
            "{source:?}"
        );
    }
}
