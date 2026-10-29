use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_matched_comment_below_the_child_column_keeps_the_list_item() {
    for (source, expected) in [
        (
            ":: t\n: - para\n %%%\n c\n %%%\n    tail\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>para</li>\n    </ul>\n  </dd>\n</dl>\n<p>tail</p>",
        ),
        (
            ":: t\n: - para\n   %%%\n   c\n   %%%\n    tail\n",
            "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>para</li>\n    </ul>\n    <p>tail</p>\n  </dd>\n</dl>",
        ),
        (
            "[^c]: 1. para\n   %%%\n   c\n   %%%\n         tail\nsee [^c]\n",
            "",
        ),
    ] {
        for html in [
            to_html(source),
            to_html_with_options(source, &Options::default().with_positions(true)),
        ] {
            assert_eq!(html, expected, "{source:?}");
        }
    }
}

#[test]
fn a_matched_fence_between_item_columns_ends_the_inner_item() {
    let source = "- 1. para\n    %%%\n    c\n    %%%\n     tail\n";
    let expected = "<ol>\n      <li>para</li>\n    </ol>\n    tail";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        assert!(html.contains(expected), "{html}");
    }
}

#[test]
fn a_matched_comment_at_the_document_column_ends_the_item() {
    let html = to_html("- para\n%%%\nc\n%%%\ntail\n");
    assert!(html.contains("</ul>\n<p>tail</p>"), "{html}");
}
