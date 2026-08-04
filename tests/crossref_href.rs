//! A crossref publishes its resolution beside the authored target.
//!
//! PART 12 section 3a, as carve#614 applies it to `</#id>`: this engine
//! published the authored half only, so a consumer decoding the tree had to
//! rebuild the heading table and re-run the case-insensitive match before it
//! could render a crossref. carve-rs#541.

fn crossref_fields(source: &str) -> Vec<(String, Option<String>)> {
    let doc = carve::parse(source);
    let mut found = Vec::new();
    fn walk(nodes: &[carve::InlineNode], found: &mut Vec<(String, Option<String>)>) {
        for node in nodes {
            match node {
                carve::InlineNode::CrossRef(c) => found.push((c.target.clone(), c.href.clone())),
                carve::InlineNode::Emphasis(e) => walk(&e.children, found),
                carve::InlineNode::Link(l) => walk(&l.children, found),
                carve::InlineNode::Span(s) => walk(&s.children, found),
                _ => {}
            }
        }
    }
    for block in &doc.children {
        if let carve::BlockNode::Paragraph(p) = block {
            walk(&p.children, &mut found);
        }
    }
    found
}

#[test]
fn a_resolved_crossref_carries_the_destination() {
    assert_eq!(
        crossref_fields("# Intro\n\nSee </#Intro>.\n"),
        vec![("Intro".to_string(), Some("#Intro".to_string()))]
    );
}

#[test]
fn the_authored_spelling_is_kept_beside_the_resolved_id() {
    // Ids resolve case-insensitively, so `href` alone cannot say which spelling
    // the author wrote - which is why section 3a keeps both.
    let lower = crossref_fields("# Intro\n\nSee </#intro>.\n");
    let upper = crossref_fields("# Intro\n\nSee </#Intro>.\n");
    assert_eq!(lower[0].1, upper[0].1);
    assert_ne!(lower[0].0, upper[0].0);
    assert_eq!(lower[0].0, "intro");
}

#[test]
fn an_unresolved_crossref_has_no_destination() {
    assert_eq!(
        crossref_fields("See </#Nope>.\n"),
        vec![("Nope".to_string(), None)]
    );
}

#[test]
fn a_crossref_inside_a_container_resolves_too() {
    // The fill walk has to reach every block that can hold inlines; a list item
    // is the shape that catches a walk written for paragraphs only.
    let doc = carve::parse("# Intro\n\n- see </#intro>\n");
    let json = carve::to_json(&doc);
    assert!(json.contains(r##""href":"#Intro""##), "{json}");
}

#[test]
fn the_wire_survives_a_round_trip() {
    let source = "# Intro\n\nSee </#intro>.\n";
    let json = carve::to_json(&carve::parse(source));
    let decoded = carve::from_json(&json).expect("decode");
    assert_eq!(carve::to_json(&decoded), json);
    let rendered =
        carve::render_carve(&decoded).expect("the decoded tree is within the render ceiling");
    assert!(rendered.contains("See </#intro>."), "{rendered}");
}

#[test]
fn html_is_unchanged_by_the_new_field() {
    // The renderers resolve through their own index, which is built by the same
    // function this field is filled from - so the two cannot disagree.
    let html = carve::to_html("# Intro\n\nSee </#intro> and </#Nope>.\n");
    assert!(html.contains(r##"<a href="#Intro">Intro</a>"##), "{html}");
    assert!(html.contains("&lt;/#Nope&gt;"), "{html}");
}
