//! A matched `%%%` span is one extent (CARVE-P0-005): its first exact-width line
//! closes it wherever that line sits, and it keeps a list open like `%%`.

use carve::to_html;

fn html(source: &str) -> String {
    to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_closed_span_between_items_keeps_the_list() {
    let one_list = "<ul> <li>a</li> <li>s</li> </ul>";
    assert_eq!(html("- a\n%%%\nx\n%%%\n- s\n"), one_list);
    assert_eq!(html("- a\n%%%\nx\n%%%\n%% c\n- s\n"), one_list);
}

#[test]
fn a_span_followed_by_something_else_still_ends_the_list() {
    assert_eq!(
        html("- a\n%%%\nx\n%%%\nb\n"),
        "<ul> <li>a</li> </ul> <p>b</p>"
    );
}

#[test]
fn a_nested_marker_line_opener_takes_the_first_closer() {
    assert_eq!(
        html(". - %%%\n%%%\nx\n%%%\n"),
        "<ol> <li> <ul> <li></li> </ul> </li> </ol> <p>x</p>"
    );
}
