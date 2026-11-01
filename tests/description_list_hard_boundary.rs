use carve::{to_carve, to_html};

const TWO_LISTS: &str =
    "<dl>\n  <dt>t</dt>\n  <dd>x</dd>\n</dl>\n<dl>\n  <dt>u</dt>\n  <dd>v</dd>\n</dl>";

#[test]
fn three_blanks_separate_description_lists_and_format_canonically() {
    for blanks in [3, 4, 5] {
        let source = format!(":: t\n: x{}:: u\n: v\n", "\n".repeat(blanks + 1));
        assert_eq!(to_html(&source), TWO_LISTS, "{blanks} blank lines");
        let formatted = to_carve(&source);
        assert_eq!(formatted, ":: t\n: x\n\n\n\n:: u\n: v\n");
        assert_eq!(to_carve(&formatted), formatted);
    }
}

#[test]
fn two_blanks_keep_a_single_description_list() {
    let source = ":: t\n: x\n\n\n:: u\n: v\n";
    assert_eq!(to_html(source).matches("<dl>").count(), 1);
}

#[test]
fn three_blanks_before_a_description_marker_leave_it_without_a_term() {
    let source = ":: t\n: x\n\n\n\n: y\n";
    let html = to_html(source);
    assert_eq!(html.matches("<dl>").count(), 1);
    assert!(html.ends_with("<p>: y</p>"), "{html}");
}

#[test]
fn three_blanks_before_indented_content_keep_the_description_open() {
    let source = ":: t\n: x\n\n\n\n  continued\n";
    let html = to_html(source);
    assert_eq!(html.matches("<dl>").count(), 1);
    assert!(html.contains("<p>continued</p>"), "{html}");
}
