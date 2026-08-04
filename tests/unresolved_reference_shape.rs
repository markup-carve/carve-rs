fn h(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

fn paragraph_children(doc: &carve::Document) -> &[carve::InlineNode] {
    let [carve::BlockNode::Paragraph(p)] = doc.children.as_slice() else {
        panic!("expected one paragraph, got {:#?}", doc.children);
    };
    &p.children
}

fn only_link(source: &str) -> carve::Link {
    let doc = carve::parse(source);
    let [carve::InlineNode::Link(link)] = paragraph_children(&doc) else {
        panic!("expected one link, got {:#?}", paragraph_children(&doc));
    };
    link.clone()
}

fn only_image(source: &str) -> carve::Image {
    let doc = carve::parse(source);
    let [carve::InlineNode::Image(image)] = paragraph_children(&doc) else {
        panic!("expected one image, got {:#?}", paragraph_children(&doc));
    };
    image.clone()
}

fn text_value(node: &carve::InlineNode) -> &str {
    let carve::InlineNode::Text(text) = node else {
        panic!("expected text, got {node:#?}");
    };
    &text.value
}

#[test]
fn unresolved_full_reference_link_stays_a_link_node() {
    let link = only_link("[missing][nope]");

    assert_eq!(link.href, "");
    assert_eq!(link.ref_label.as_deref(), Some("nope"));
    assert_eq!(link.raw_ref.as_deref(), Some("[missing][nope]"));
    assert_eq!(link.children.len(), 1);
    assert_eq!(text_value(&link.children[0]), "missing");
    assert_eq!(h("[missing][nope]"), "<p>[missing][nope]</p>");
}

#[test]
fn unresolved_collapsed_reference_derives_label_from_text() {
    let link = only_link("[a][]");

    assert_eq!(link.ref_label.as_deref(), Some("a"));
    assert_eq!(link.raw_ref.as_deref(), Some("[a][]"));
    assert_eq!(h("[a][]"), "<p>[a][]</p>");
}

#[test]
fn unresolved_reference_preserves_authored_label_case() {
    let link = only_link("[a][NoPe]");

    assert_eq!(link.ref_label.as_deref(), Some("NoPe"));
    assert_eq!(h("[a][NoPe]"), "<p>[a][NoPe]</p>");
}

#[test]
fn unresolved_reference_keeps_parsed_link_children() {
    let link = only_link("[*x*][nope]");

    let [carve::InlineNode::Emphasis(strong)] = link.children.as_slice() else {
        panic!("expected strong child, got {:#?}", link.children);
    };
    assert_eq!(strong.kind, carve::EmphasisKind::Strong);
    assert_eq!(h("[*x*][nope]"), "<p>[*x*][nope]</p>");
}

#[test]
fn unresolved_reference_raw_ref_spans_trailing_attrs() {
    let link = only_link("[a][nope]{#i .c}");

    assert_eq!(link.ref_label.as_deref(), Some("nope"));
    assert_eq!(link.raw_ref.as_deref(), Some("[a][nope]{#i .c}"));
    let attrs = link.attrs.as_ref().expect("attrs");
    assert_eq!(attrs.id.as_deref(), Some("i"));
    assert_eq!(attrs.classes, ["c"]);
    assert_eq!(h("[a][nope]{#i .c}"), "<p>[a][nope]{#i .c}</p>");
}

#[test]
fn unresolved_full_reference_image_stays_an_image_node() {
    let image = only_image("![alt][nope]");

    assert_eq!(image.src, "");
    assert_eq!(image.alt, "alt");
    assert_eq!(image.ref_label.as_deref(), Some("nope"));
    assert_eq!(image.raw_ref.as_deref(), Some("![alt][nope]"));
    assert_eq!(h("![alt][nope]"), "<p>![alt][nope]</p>");
}

#[test]
fn unresolved_collapsed_reference_image_derives_label_from_alt() {
    let image = only_image("![a][]");

    assert_eq!(image.ref_label.as_deref(), Some("a"));
    assert_eq!(image.raw_ref.as_deref(), Some("![a][]"));
    assert_eq!(h("![a][]"), "<p>![a][]</p>");
}

#[test]
fn adjacent_brackets_after_unresolved_reference_stay_text() {
    let doc = carve::parse("[a][b][c]");
    let [carve::InlineNode::Link(link), carve::InlineNode::Text(text)] = paragraph_children(&doc)
    else {
        panic!(
            "expected link plus text, got {:#?}",
            paragraph_children(&doc)
        );
    };

    assert_eq!(link.raw_ref.as_deref(), Some("[a][b]"));
    assert_eq!(text.value, "[c]");
    assert_eq!(h("[a][b][c]"), "<p>[a][b][c]</p>");
}

#[test]
fn unresolved_reference_source_is_html_escaped() {
    let link = only_link("[a&b][no<pe>]");

    assert_eq!(link.ref_label.as_deref(), Some("no<pe>"));
    assert_eq!(link.raw_ref.as_deref(), Some("[a&b][no<pe>]"));
    assert_eq!(h("[a&b][no<pe>]"), "<p>[a&amp;b][no&lt;pe&gt;]</p>");
}

#[test]
fn unresolved_link_inside_link_is_not_unwrapped() {
    let doc = carve::parse("[[x][missing]](/z)");
    let [carve::InlineNode::Link(outer)] = paragraph_children(&doc) else {
        panic!("expected outer link, got {:#?}", paragraph_children(&doc));
    };
    let [carve::InlineNode::Link(inner)] = outer.children.as_slice() else {
        panic!("expected unresolved inner link, got {:#?}", outer.children);
    };

    assert_eq!(outer.href, "/z");
    assert_eq!(inner.ref_label.as_deref(), Some("missing"));
    assert_eq!(inner.raw_ref.as_deref(), Some("[x][missing]"));
    assert_eq!(
        h("[[x][missing]](/z)"),
        "<p><a href=\"/z\">[x][missing]</a></p>"
    );
}

/// PART 12 §3a's second half: A RESOLVED REFERENCE KEEPS ITS DESTINATION - the
/// authored `ref` and `raw_ref` survive BESIDE `href`, the same way §5 has
/// footnote numbering added alongside rather than in place of the reference.
///
/// This asserted the pair was CLEARED on a successful resolve, which made
/// `[gs][]` and `[gs](/start)` the same tree - the distinction the clause
/// exists to protect (carve#597).
#[test]
fn explicit_definition_resolves_and_keeps_reference_metadata() {
    let doc = carve::parse("See [gs][] here.\n\n[gs]: /start");
    let [carve::InlineNode::Text(before), carve::InlineNode::Link(link), carve::InlineNode::Text(after)] =
        paragraph_children(&doc)
    else {
        panic!(
            "expected text/link/text, got {:#?}",
            paragraph_children(&doc)
        );
    };

    assert_eq!(before.value, "See ");
    assert_eq!(link.href, "/start");
    assert_eq!(link.ref_label.as_deref(), Some("gs"));
    assert_eq!(link.raw_ref.as_deref(), Some("[gs][]"));
    assert_eq!(after.value, " here.");
    assert_eq!(
        h("See [gs][] here.\n\n[gs]: /start"),
        "<p>See <a href=\"/start\">gs</a> here.</p>"
    );
}

#[test]
fn non_html_writers_render_unresolved_reference_source() {
    let source = "a ![alt][nope] and [b][no] x";

    assert_eq!(
        carve::to_markdown(source),
        "a !\\[alt\\]\\[nope\\] and \\[b\\]\\[no\\] x\n"
    );
    assert_eq!(
        carve::to_plain_text(source),
        "a ![alt][nope] and [b][no] x\n"
    );
    assert_eq!(carve::to_ansi(source), "a ![alt][nope] and [b][no] x\n");
    assert_eq!(carve::to_carve(source), "a ![alt][nope] and [b][no] x\n");
}

#[test]
fn unresolved_reference_image_does_not_promote_to_block_image() {
    assert_eq!(h("![a](/x)"), "<img src=\"/x\" alt=\"a\">");
    assert_eq!(h("![a][]"), "<p>![a][]</p>");
}

#[test]
fn unresolved_reference_json_round_trips() {
    for source in ["![alt][nope]", "[missing][nope]"] {
        let json = carve::to_json(&carve::parse(source));
        let decoded = carve::from_json(&json).expect("decode unresolved reference JSON");
        assert_eq!(carve::to_json(&decoded), json);
    }
}
