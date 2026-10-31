// CARVE-P2-021 item 4 and CARVE-P2-020: a lone image on an item's marker line
// takes one caption line; a further `^ ` line is its own paragraph, and an
// over-indented continuation keeps its extra indentation.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn a_second_caption_line_is_its_own_paragraph() {
    assert_eq!(
        html("- ![a](u)\n  ^ cap\n  ^ cap\n"),
        "<ul>\n  <li>\n    <figure>\n      <img src=\"u\" alt=\"a\">\n      <figcaption>cap</figcaption>\n    </figure>\n    ^ cap\n  </li>\n</ul>"
    );
    assert_eq!(
        html("1. ![a](u)\n   ^ cap\n   more\n   ^ cap\n"),
        "<ol>\n  <li>\n    <figure>\n      <img src=\"u\" alt=\"a\">\n      <figcaption>cap\nmore</figcaption>\n    </figure>\n    ^ cap\n  </li>\n</ol>"
    );
}

#[test]
fn an_unresolved_reference_image_keeps_the_paragraph_boundary() {
    assert_eq!(
        html("- ![a][r]\n  ^ cap\n  ^ cap\n"),
        "<ul>\n  <li>![a][r]\n^ cap\n    ^ cap\n  </li>\n</ul>"
    );
}

#[test]
fn an_over_indented_caption_continuation_keeps_its_indentation() {
    assert_eq!(
        html("- ![a](u)\n  ^ cap\n   plain\n"),
        "<ul>\n  <li>\n    <figure>\n      <img src=\"u\" alt=\"a\">\n      <figcaption>cap\n plain</figcaption>\n    </figure>\n  </li>\n</ul>"
    );
    assert_eq!(
        html("- ![a][r]\n  ^ cap\n   plain\n"),
        "<ul>\n  <li>![a][r]\n^ cap\nplain</li>\n</ul>"
    );
}
