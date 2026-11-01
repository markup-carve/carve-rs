use carve::{to_carve, to_html};

#[test]
fn a_comment_only_payload_keeps_the_nested_items_stanza_open() {
    let source = "- - ::: |\n    %% c\nb\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <div class=\"line-block\">\n          <p><br>\nb</p>\n        </div>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
    assert_eq!(to_html(&to_carve(source)), expected);
}

#[test]
fn a_description_in_an_item_keeps_the_same_stanza() {
    let source = "- :: u\n  : ::: |\n    %% c\nb\n";
    let expected = "<ul>\n  <li>\n    <dl>\n      <dt>u</dt>\n      <dd>\n        <div class=\"line-block\">\n          <p><br>\nb</p>\n        </div>\n      </dd>\n    </dl>\n  </li>\n</ul>";
    assert_eq!(to_html(source), expected);
}

#[test]
fn a_plain_payload_needs_no_later_line_to_keep_its_stanza() {
    let source = "- - ::: |\n    x\nb\n";
    assert!(to_html(source).contains("<p>x<br>\nb</p>"));
}

#[test]
fn an_exact_closer_ends_the_description_stanza() {
    let source = "- :: u\n  : ::: |\n    %% c\n    :::\ntail\n";
    let html = to_html(source);
    assert!(
        html.contains("<div class=\"line-block\">\n          <p></p>\n"),
        "{html}"
    );
    assert!(html.ends_with("</ul>\n<p>tail</p>"), "{html}");
}
