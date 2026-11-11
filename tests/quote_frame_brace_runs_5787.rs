//! carve#5787: every line of a block brace run is measured after the quote
//! prefixes it has, however wide its outer quote prefix is written.

use carve::to_html;

#[test]
fn a_comment_run_past_its_brace_after_a_narrower_outer_quote_prefix_is_hidden() {
    assert_eq!(
        to_html("  > - {% a\n>    b %}\n").trim(),
        "<blockquote>\n  <ul>\n    <li></li>\n  </ul>\n</blockquote>"
    );
    assert_eq!(
        to_html("  > {% a\n>  b\n>  z %}\n").trim(),
        "<blockquote>\n\n</blockquote>"
    );
}

#[test]
fn a_comment_run_at_its_brace_after_the_outer_quote_prefix_is_text() {
    assert_eq!(
        to_html("  > - {% a\n>   b %}\n").trim(),
        "<blockquote>\n  <ul>\n    <li>{% a\nb %}</li>\n  </ul>\n</blockquote>"
    );
    assert_eq!(
        to_html("  > {% a\n> b %}\n").trim(),
        "<blockquote><p>{% a\nb %}</p></blockquote>"
    );
}

#[test]
fn an_attribute_run_after_a_narrower_outer_quote_prefix_continues() {
    assert_eq!(
        to_html("  > {#i\n>  .y}\n> p\n").trim(),
        "<blockquote><p id=\"i\" class=\"y\">p</p></blockquote>"
    );
}

#[test]
fn a_quote_behind_item_markers_frames_its_comment_run() {
    assert_eq!(
        to_html("- > {% a\n  >  b\n  >  z %}\n").trim(),
        "<ul>\n  <li>\n    <blockquote>\n\n    </blockquote>\n  </li>\n</ul>"
    );
    assert_eq!(
        to_html("> - > {% a\n>   >  b %}\n").trim(),
        "<blockquote>\n  <ul>\n    <li>\n      <blockquote>\n\n      </blockquote>\n    </li>\n  </ul>\n</blockquote>"
    );
}
