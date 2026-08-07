fn h(s: &str) -> String {
    carve::to_html(s).trim().to_string()
}

#[test]
fn full_reference_resolves_to_image_with_title() {
    assert_eq!(
        h("![alt][ref]\n\n[ref]: /img.png \"cap\""),
        "<img src=\"/img.png\" alt=\"alt\" title=\"cap\">"
    );
}

#[test]
fn collapsed_uses_alt_as_label() {
    assert_eq!(
        h("![alt][]\n\n[alt]: /i.png \"t\""),
        "<img src=\"/i.png\" alt=\"alt\" title=\"t\">"
    );
}

#[test]
fn full_form_allows_empty_alt() {
    assert_eq!(h("![][ref]\n\n[ref]: /u"), "<img src=\"/u\" alt=\"\">");
}

#[test]
fn trailing_attributes_apply() {
    assert_eq!(
        h("![alt][ref]{.c #i}\n\n[ref]: /i.png"),
        "<img src=\"/i.png\" alt=\"alt\" class=\"c\" id=\"i\">"
    );
}

#[test]
fn alt_is_raw_text() {
    assert_eq!(
        h("![a *b* c][ref]\n\n[ref]: /i.png"),
        "<img src=\"/i.png\" alt=\"a *b* c\">"
    );
}

#[test]
fn nested_brackets_in_alt() {
    assert_eq!(
        h("![a [b] c][ref]\n\n[ref]: /u"),
        "<img src=\"/u\" alt=\"a [b] c\">"
    );
}

#[test]
fn unresolved_reference_is_literal() {
    assert_eq!(h("![alt][nope]"), "<p>![alt][nope]</p>");
}

#[test]
fn labels_are_case_sensitive() {
    assert_eq!(h("![a][REF]\n\n[ref]: /u"), "<p>![a][REF]</p>");
}

#[test]
fn shortcut_is_not_a_reference_image() {
    assert_eq!(h("![alt]\n\n[alt]: /i.png"), "<p>![alt]</p>");
}

#[test]
fn inline_image_wins_over_reference() {
    assert_eq!(
        h("![alt](/inline.png)\n\n[alt]: /ref.png"),
        "<img src=\"/inline.png\" alt=\"alt\">"
    );
}

#[test]
fn stays_inline_with_surrounding_text() {
    assert_eq!(
        h("x ![a][ref]\n\n[ref]: /u"),
        "<p>x <img src=\"/u\" alt=\"a\"></p>"
    );
}

#[test]
fn reference_link_and_image_coexist() {
    assert_eq!(
        h("[alt][ref] and ![alt][ref]\n\n[ref]: /u"),
        "<p><a href=\"/u\">alt</a> and <img src=\"/u\" alt=\"alt\"></p>"
    );
}

#[test]
fn resolved_reference_image_with_caption_is_figure() {
    assert_eq!(
        h("![a][r]\n^ cap\n\n[r]: /u"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>"
    );
}

#[test]
fn reference_figure_caption_keeps_markup() {
    assert_eq!(
        h("![a][r]\n^ *b* c\n\n[r]: /u"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption><strong>b</strong> c</figcaption>\n</figure>"
    );
}

#[test]
fn collapsed_reference_image_with_caption_is_figure() {
    assert_eq!(
        h("![a][]\n^ cap\n\n[a]: /u"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>"
    );
}

#[test]
fn unresolved_reference_image_with_caption_stays_literal() {
    assert_eq!(h("![a][nope]\n^ cap"), "<p>![a][nope]\n^ cap</p>");
}

#[test]
fn leading_text_before_reference_image_is_not_a_figure() {
    assert_eq!(
        h("x ![a][r]\n^ cap\n\n[r]: /u"),
        "<p>x <img src=\"/u\" alt=\"a\">\n^ cap</p>"
    );
}

// Grammar §1722 I3: a bare image is not a block of its own; it stays inline in a
// paragraph, rendering as a bare block image only when it stands alone.
#[test]
fn bare_image_plus_text_is_one_paragraph() {
    assert_eq!(
        h("![a](/u)\nmore"),
        "<p><img src=\"/u\" alt=\"a\">\nmore</p>"
    );
}

#[test]
fn two_bare_images_are_one_paragraph() {
    assert_eq!(
        h("![a](/u)\n![b](/u)"),
        "<p><img src=\"/u\" alt=\"a\">\n<img src=\"/u\" alt=\"b\"></p>"
    );
}

#[test]
fn bare_image_plus_list_marker_folds() {
    assert_eq!(h("![a](/u)\n- x"), "<p><img src=\"/u\" alt=\"a\">\n- x</p>");
}

#[test]
fn bare_image_alone_is_block() {
    assert_eq!(h("![a](/u)"), "<img src=\"/u\" alt=\"a\">");
}

#[test]
fn bare_image_before_interrupter_stays_standalone() {
    assert_eq!(
        h("![a](/u)\n# H"),
        "<img src=\"/u\" alt=\"a\">\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
}

// The caption delimiter mirrors a heading's first line (§4/§553): `^` + one-or-
// more literal SPACES (not a tab) + non-empty content. `^ ` alone, `^\t…`, or a
// `^ ` whose content only appears on a later folded line is NOT a caption.
#[test]
fn empty_caption_is_not_a_caption() {
    assert_eq!(h("![a](/u)\n^ "), "<p><img src=\"/u\" alt=\"a\">\n^</p>");
}

#[test]
fn caption_with_content_only_on_a_later_line_is_not_a_caption() {
    // The delimiter line falls back to paragraph text, and its trailing space
    // is dropped there like any other content line's (PART 2 NO TRAILING
    // WHITESPACE, carve#926). What this case pins - that the line is not a
    // caption - is unchanged.
    assert_eq!(
        h("![a](/u)\n^ \nmore"),
        "<p><img src=\"/u\" alt=\"a\">\n^\nmore</p>"
    );
}

#[test]
fn tab_after_caret_is_not_a_caption_delimiter() {
    assert_eq!(
        h("![a](/u)\n^\tx"),
        "<p><img src=\"/u\" alt=\"a\">\n^\tx</p>"
    );
}

#[test]
fn extra_leading_spaces_after_caret_fold_into_the_delimiter() {
    assert_eq!(
        h("![a](/u)\n^  x"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>x</figcaption>\n</figure>"
    );
}

#[test]
fn reference_image_empty_caption_is_not_promoted() {
    assert_eq!(
        h("![a][r]\n^ \n\n[r]: /u"),
        "<p><img src=\"/u\" alt=\"a\">\n^</p>"
    );
}

#[test]
fn reference_image_caption_of_inline_markup_is_a_figure() {
    assert_eq!(
        h("![a][r]\n^ *b* c\n\n[r]: /u"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption><strong>b</strong> c</figcaption>\n</figure>"
    );
}

// A non-breaking space (U+00A0) is caption content: "content" excludes only
// ASCII whitespace, so `^ \u{00a0}` is a caption, matching the parser's NBSP
// handling elsewhere and carve-php's byte-mode \S.
#[test]
fn non_breaking_space_is_caption_content() {
    assert_eq!(
        h("![a](/u)\n^ \u{00a0}"),
        "<figure>\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>&nbsp;</figcaption>\n</figure>"
    );
}

// A leading block-attribute line (`{#id}`) before a sole block image lands on
// the promoted bare `<img>` (§15) -- consistent with an inline `![…](…){#id}`
// and with a sole image rendering bare (no `<p>` wrapper). A following caption
// still puts the id on the `<figure>`.
#[test]
fn leading_attr_line_on_direct_block_image() {
    assert_eq!(h("{#f}\n![a](/u)"), "<img src=\"/u\" alt=\"a\" id=\"f\">");
}

#[test]
fn leading_attr_line_on_reference_block_image() {
    assert_eq!(
        h("{#f}\n![a][r]\n\n[r]: /u"),
        "<img src=\"/u\" alt=\"a\" id=\"f\">"
    );
}

#[test]
fn leading_attr_line_merges_with_image_own_attrs() {
    assert_eq!(
        h("{#f}\n![a][r]{.c}\n\n[r]: /u"),
        "<img src=\"/u\" alt=\"a\" id=\"f\" class=\"c\">"
    );
}

#[test]
fn leading_attr_line_with_caption_stays_on_figure() {
    assert_eq!(
        h("{#f}\n![a](/u)\n^ cap"),
        "<figure id=\"f\">\n  <img src=\"/u\" alt=\"a\">\n  <figcaption>cap</figcaption>\n</figure>"
    );
}
