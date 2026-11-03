//! A `%%%` span on a description's lead line (`: %%%`) leaves nothing, so the
//! line below the item or quote that ends it has no paragraph to fold into
//! (markup-carve/carve#4998). Expected HTML is the spec model at carve
//! `b426bf87`.

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the two parse paths disagree on {src:?}"
    );
    facade
}

const ITEM: &str = concat!(
    "<ul>\n",
    "  <li>a\n",
    "    <dl>\n",
    "      <dt>u</dt>\n",
    "      <dd></dd>\n",
    "    </dl>\n",
    "  </li>\n",
    "</ul>\n",
    "<p>zz</p>",
);

#[test]
fn a_payload_line_does_not_hand_the_item_a_lazy_claim() {
    assert_eq!(both_paths("- a\n  :: u\n  : %%%\n    n\nzz\n"), ITEM);
}

#[test]
fn a_list_marker_payload_does_not_hand_the_item_a_lazy_claim() {
    assert_eq!(both_paths("- a\n  :: u\n  : %%%\n    - n\nzz\n"), ITEM);
}

#[test]
fn a_quoted_span_does_not_hand_the_quote_a_lazy_claim() {
    assert_eq!(
        both_paths("> - a\n>   :: u\n>   : %%%\n>     n\nzz\n"),
        concat!(
            "<blockquote>\n",
            "  <ul>\n",
            "    <li>a\n",
            "      <dl>\n",
            "        <dt>u</dt>\n",
            "        <dd></dd>\n",
            "      </dl>\n",
            "    </li>\n",
            "  </ul>\n",
            "</blockquote>\n",
            "<p>zz</p>",
        )
    );
}

#[test]
fn a_blank_in_a_nested_span_does_not_hand_the_outer_item_the_line() {
    assert_eq!(
        both_paths("- a\n  - b\n    :: u\n    : %%%\n\n      n\n zz\n"),
        concat!(
            "<ul>\n",
            "  <li>a\n",
            "    <ul>\n",
            "      <li>b\n",
            "        <dl>\n",
            "          <dt>u</dt>\n",
            "          <dd></dd>\n",
            "        </dl>\n",
            "      </li>\n",
            "    </ul>\n",
            "  </li>\n",
            "</ul>\n",
            "<p>zz</p>",
        )
    );
}

#[test]
fn a_span_after_a_lead_paragraph_is_not_a_lead_span() {
    assert_eq!(
        both_paths("- a\n  :: u\n  : x\n    %%%\n    n\n    %%%\nzz\n"),
        concat!(
            "<ul>\n",
            "  <li>a\n",
            "    <dl>\n",
            "      <dt>u</dt>\n",
            "      <dd>x</dd>\n",
            "    </dl>\n",
            "  </li>\n",
            "</ul>\n",
            "<p>zz</p>",
        )
    );
}
