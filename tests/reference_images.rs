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
