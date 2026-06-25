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
fn partially_resolved_group_is_fully_verbatim() {
    // A group with any unresolved key renders verbatim (§6.4): the defined key
    // is literal text, not a citation, so it is NOT numbered or listed - no
    // orphan reference entry. Matches carve-js / carve-php (§6.5).
    let out = h("[@a; @missing].\n\n[@a]: A.");
    assert!(out.contains("[@a; @missing]"));
    assert!(!out.contains("id=\"ref-a\""));
    assert!(!out.contains("href=\"#ref-a\""));
    assert!(!out.contains("class=\"references\""));
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

// ----- Tier-3 Bibliography (#199) -------------------------------------------

use carve::{CslDate, CslEntry, CslName};

fn smith() -> CslEntry {
    CslEntry {
        id: "smith2020".to_string(),
        author: Some(vec![CslName {
            family: Some("Smith".to_string()),
            given: Some("John".to_string()),
            literal: None,
        }]),
        issued: Some(CslDate {
            date_parts: Some(vec![vec![2020]]),
            literal: None,
        }),
        title: Some("A Study".to_string()),
    }
}

fn hb(source: &str, bib: Vec<CslEntry>) -> String {
    let citations = Citations::new().with_bibliography(bib);
    let options = Options::new().with_extension(&citations);
    carve::to_html_with_options(source, &options)
        .trim()
        .to_string()
}

fn entry(csl: CslEntry) -> String {
    let out = hb("[@x]", vec![csl]);
    let start = out.find("<li id=\"ref-x\">").unwrap() + "<li id=\"ref-x\">".len();
    let tail = &out[start..];
    let end = tail
        .find(" <a href=\"#cite-x-1\"")
        .or_else(|| tail.find("</li>"))
        .unwrap();
    tail[..end].to_string()
}

#[test]
fn bib_resolves_from_pool_with_backlinks() {
    let out = hb("See [@smith2020].", vec![smith()]);
    assert!(out.contains("<a id=\"cite-smith2020-1\" href=\"#ref-smith2020\">1</a>"));
    assert!(out.contains(
        "<li id=\"ref-smith2020\">Smith, John (2020). A Study. \
         <a href=\"#cite-smith2020-1\" class=\"ref-backref\">\u{21a9}</a></li>"
    ));
}

#[test]
fn bib_in_doc_def_overrides_pool() {
    let out = hb(
        "See [@smith2020].\n\n[@smith2020]: In-doc entry.",
        vec![smith()],
    );
    assert!(out.contains("<li id=\"ref-smith2020\">In-doc entry."));
    assert!(!out.contains("A Study"));
}

#[test]
fn bib_one_backlink_per_use_site() {
    let out = hb("[@smith2020] then [@smith2020] again.", vec![smith()]);
    assert!(out.contains("<a id=\"cite-smith2020-1\" href=\"#ref-smith2020\">1</a>"));
    assert!(out.contains("<a id=\"cite-smith2020-2\" href=\"#ref-smith2020\">1</a>"));
    assert!(out.contains("<a href=\"#cite-smith2020-1\" class=\"ref-backref\">\u{21a9}</a>"));
    assert!(out.contains("<a href=\"#cite-smith2020-2\" class=\"ref-backref\">\u{21a9}</a>"));
}

#[test]
fn bib_multi_key_group_anchors_each_key() {
    let out = hb(
        "[@a; @b]",
        vec![
            CslEntry {
                id: "a".to_string(),
                title: Some("Alpha".to_string()),
                ..Default::default()
            },
            CslEntry {
                id: "b".to_string(),
                title: Some("Beta".to_string()),
                ..Default::default()
            },
        ],
    );
    assert!(out.contains("<a id=\"cite-a-1\" href=\"#ref-a\">1</a>"));
    assert!(out.contains("<a id=\"cite-b-1\" href=\"#ref-b\">2</a>"));
}

#[test]
fn bib_unresolved_key_is_verbatim() {
    let out = hb("[@nope]", vec![smith()]);
    assert!(out.contains("[@nope]"));
    assert!(!out.contains("cite-nope"));
    assert!(!out.contains("class=\"references\""));
}

#[test]
fn bib_escapes_csl_entry_text() {
    let out = hb(
        "[@x]",
        vec![CslEntry {
            id: "x".to_string(),
            title: Some("<b>raw</b> & co".to_string()),
            ..Default::default()
        }],
    );
    assert!(out.contains("&lt;b&gt;raw&lt;/b&gt; &amp; co."));
}

#[test]
fn bib_formatter_author_year_title() {
    assert_eq!(
        entry(CslEntry {
            id: "x".to_string(),
            author: Some(vec![CslName {
                family: Some("Smith".to_string()),
                given: Some("John".to_string()),
                literal: None,
            }]),
            issued: Some(CslDate {
                date_parts: Some(vec![vec![2020]]),
                literal: None
            }),
            title: Some("T".to_string()),
        }),
        "Smith, John (2020). T."
    );
}

#[test]
fn bib_formatter_author_only() {
    assert_eq!(
        entry(CslEntry {
            id: "x".to_string(),
            author: Some(vec![CslName {
                family: Some("Doe".to_string()),
                given: None,
                literal: None
            }]),
            ..Default::default()
        }),
        "Doe."
    );
}

#[test]
fn bib_formatter_year_title_no_author() {
    assert_eq!(
        entry(CslEntry {
            id: "x".to_string(),
            issued: Some(CslDate {
                date_parts: Some(vec![vec![1999]]),
                literal: None
            }),
            title: Some("T".to_string()),
            ..Default::default()
        }),
        "(1999). T."
    );
}

#[test]
fn bib_formatter_multiple_authors() {
    assert_eq!(
        entry(CslEntry {
            id: "x".to_string(),
            author: Some(vec![
                CslName {
                    family: Some("A".to_string()),
                    given: Some("X".to_string()),
                    literal: None
                },
                CslName {
                    family: Some("B".to_string()),
                    given: Some("Y".to_string()),
                    literal: None
                },
            ]),
            title: Some("T".to_string()),
            ..Default::default()
        }),
        "A, X; B, Y. T."
    );
}

#[test]
fn bib_formatter_literal_name_and_year() {
    assert_eq!(
        entry(CslEntry {
            id: "x".to_string(),
            author: Some(vec![CslName {
                family: None,
                given: None,
                literal: Some("WHO".to_string())
            }]),
            issued: Some(CslDate {
                date_parts: None,
                literal: Some("n.d.".to_string())
            }),
            title: Some("T".to_string()),
        }),
        "WHO (n.d.). T."
    );
}

#[test]
fn bib_partially_resolved_group_is_fully_verbatim() {
    // A group mixing a resolved and an unresolved key renders verbatim (§6.4):
    // its keys are literal text, not citations, so the defined key is NOT
    // numbered, listed, or a use site - no orphan entry with a dangling
    // back-ref. Matches carve-js / carve-php (§6.5).
    let out = hb(
        "[@a; @missing]",
        vec![CslEntry {
            id: "a".to_string(),
            title: Some("A".to_string()),
            ..Default::default()
        }],
    );
    assert!(out.contains("<p>[@a; @missing]</p>"));
    assert!(!out.contains("id=\"ref-a\""));
    assert!(!out.contains("cite-a-1"));
    assert!(!out.contains("ref-backref"));
    assert!(!out.contains("class=\"references\""));
}

#[test]
fn bib_no_pool_keeps_tier2_behavior() {
    let out = h("[@a].\n\n[@a]: A.");
    assert!(out.contains("<li id=\"ref-a\">A.</li>"));
    assert!(!out.contains("ref-backref"));
    assert!(!out.contains("id=\"cite-a-1\""));
}
