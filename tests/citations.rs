use carve::{BlockNode, Citations, InlineNode, Options};

fn h(source: &str) -> String {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    carve::to_html_with_options(source, &options)
        .trim()
        .to_string()
}

fn ha(source: &str) -> String {
    let citations = Citations::author_date();
    let options = Options::new().with_extension(&citations);
    carve::to_html_with_options(source, &options)
        .trim()
        .to_string()
}

fn group(source: &str) -> Option<carve::CitationGroup> {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    let doc = carve::parse_with_options(source, &options);
    let BlockNode::Paragraph(p) = doc.children.first()? else {
        return None;
    };
    p.children.iter().find_map(|node| {
        if let InlineNode::CitationGroup(group) = node {
            Some(group.clone())
        } else {
            None
        }
    })
}

#[test]
fn parses_key_into_citation_group() {
    assert_eq!(group("[@smith2020]").unwrap().items[0].key, "smith2020");
}

#[test]
fn leaves_bare_mention_alone() {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    let doc = carve::parse_with_options("@alice", &options);
    let BlockNode::Paragraph(p) = &doc.children[0] else {
        panic!("expected paragraph");
    };
    assert!(matches!(p.children[0], InlineNode::Mention(_)));
}

#[test]
fn does_not_claim_reference_link_or_plain_brackets() {
    assert!(group("[text][ref]").is_none());
    assert!(group("[just text]").is_none());
    assert!(group("[\\@key]").is_none());
}

#[test]
fn parses_locator_suppress_author_and_multiple_items() {
    let locator = group("[@smith2020, p. 33]").unwrap();
    assert_eq!(locator.items[0].key, "smith2020");
    assert!(locator.items[0].locator.is_some());

    assert!(group("[-@smith2020]").unwrap().items[0].suppress_author);

    let multi = group("[@a; @b]").unwrap();
    assert_eq!(
        multi
            .items
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
}

#[test]
fn drops_definition_paragraph_and_numbers_citation() {
    let out = h("See [@smith2020].\n\n[@smith2020]: Smith, J. (2020). Title.");
    assert!(out.contains("[<a href=\"#ref-smith2020\">1</a>]"));
    assert!(!out.contains("<p>Smith, J. (2020). Title.</p>"));
}

#[test]
fn builds_references_list_with_stable_ids() {
    let out = h("[@a].\n\n[@a]: Entry A.");
    assert!(out.contains("<ol class=\"references\">"));
    assert!(out.contains("<li id=\"ref-a\">Entry A.</li>"));
}

#[test]
fn numbers_by_first_citation_order() {
    let out = h("[@b] then [@a].\n\n[@a]: A.\n\n[@b]: B.");
    assert!(out.contains("href=\"#ref-b\">1</a>"));
    assert!(out.contains("href=\"#ref-a\">2</a>"));
}

#[test]
fn renders_locator_and_prefix_inside_brackets() {
    let out = h("[see @a, p. 3].\n\n[@a]: A.");
    assert!(out.contains("[see <a href=\"#ref-a\">1</a>, p. 3]"));
}

#[test]
fn renders_undefined_key_verbatim() {
    assert!(h("[@nope].").contains("[@nope]"));
}

#[test]
fn mixed_defined_undefined_group_renders_raw_but_lists_defined_key() {
    let out = h("[@a; @missing].\n\n[@a]: A.");
    assert!(out.contains("[@a; @missing]"));
    assert!(out.contains("<li id=\"ref-a\">A.</li>"));
}

#[test]
fn keeps_mentions_and_reference_links_working() {
    assert!(h("@alice").contains("class=\"mention\""));
    let out = h("[text][ref]\n\n[ref]: https://example.com");
    assert!(out.contains("<a href=\"https://example.com\">text</a>"));
}

#[test]
fn collects_consecutive_definition_lines() {
    let out = h("[@a] and [@b].\n\n[@a]: First.\n[@b]: Second.");
    assert!(out.contains("href=\"#ref-a\">1</a>"));
    assert!(out.contains("href=\"#ref-b\">2</a>"));
    assert!(out.contains("<li id=\"ref-a\">First.</li>"));
    assert!(out.contains("<li id=\"ref-b\">Second.</li>"));
}

#[test]
fn author_date_renders_attrs_and_suppresses_author() {
    let out = ha("See [@s].\n\n[@s]: {author=\"Smith\" year=\"2020\"} Smith, J.");
    assert!(out.contains("(<a href=\"#ref-s\">Smith 2020</a>)"));

    let out = ha("[-@s].\n\n[@s]: {author=\"Smith\" year=\"2020\"} S.");
    assert!(out.contains(">2020</a>"));
}

#[test]
fn reused_extension_does_not_leak_state() {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    let _ = carve::to_html_with_options("[@a].\n\n[@a]: First doc A.", &options);
    let out = carve::to_html_with_options("[@a].", &options)
        .trim()
        .to_string();
    assert!(out.contains("[@a]"));
    assert!(!out.contains("href=\"#ref-a\""));
}

#[test]
fn injects_into_explicit_references_block() {
    let out = h("[@a].\n\n::: references\n:::\n\n[@a]: A.");
    assert!(out.contains("<div class=\"references\">\n  <ol class=\"references\">"));
}
