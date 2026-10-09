use carve::ast::{BlockNode, InlineNode};
use carve::{from_json, parse, render_carve, to_carve, to_html, to_json, RenderCarveError};

#[test]
fn a_crossref_target_keeps_its_literal_backslashes() {
    for target in [r"p\an", r"ls\-greet", r"a\\b", "a\\", "日本語"] {
        let source = format!("See </#{target}>.\n");
        let once = to_carve(&source);
        assert_eq!(once, source);
        assert_eq!(to_carve(&once), once);
        assert_eq!(to_html(&once), to_html(&source));
        let document = parse(&source);
        let BlockNode::Paragraph(paragraph) = &document.children[0] else {
            panic!("paragraph")
        };
        let InlineNode::CrossRef(reference) = &paragraph.children[1] else {
            panic!("crossref")
        };
        assert_eq!(reference.target, target);
    }
}

#[test]
fn an_ingested_target_without_a_spelling_is_refused() {
    for target in ["", "a>b", "a b", "a\tb", "a\nb", "a\rb", "a\x0cb"] {
        let value = serde_json::json!({
            "type":"document", "srcByteLength":0,
            "children":[{"type":"paragraph", "children":[{"type":"heading_ref", "target":target}]}]
        });
        let document = from_json(&value.to_string()).expect("decode");
        let error = render_carve(&document).expect_err("refuse the target");
        let RenderCarveError::SourceUnspellable(error) = error else {
            panic!("source refusal")
        };
        assert_eq!(error.node_type(), "heading_ref");
    }
}

#[test]
fn literal_targets_in_table_and_link_hosts_read_back() {
    for source in ["| </#a\\> |\n", "| </#a\\|b> |\n", "[x </#a\\]>](u)\n"] {
        let once = to_carve(source);
        assert_eq!(once, source);
        let target = source
            .split_once("</#")
            .unwrap()
            .1
            .split_once('>')
            .unwrap()
            .0;
        let tree = to_json(&parse(source));
        assert!(tree.contains("\"type\":\"heading_ref\""));
        assert!(tree.contains(&format!(
            "\"target\":{}",
            serde_json::to_string(target).unwrap()
        )));
        assert_eq!(to_carve(&once), once);
        assert_eq!(to_html(&once), to_html(source));
    }
}

#[test]
fn resolved_legacy_links_use_literal_targets() {
    for target in [r"p\an", "a b"] {
        let mut document = parse("[label](u)\n");
        let BlockNode::Paragraph(paragraph) = &mut document.children[0] else {
            panic!("paragraph")
        };
        let InlineNode::Link(link) = &mut paragraph.children[0] else {
            panic!("link")
        };
        link.from_crossref = true;
        link.href = format!("#{target}");
        if target.contains(' ') {
            assert!(matches!(
                render_carve(&document),
                Err(RenderCarveError::SourceUnspellable(_))
            ));
        } else {
            assert_eq!(render_carve(&document).unwrap(), format!("</#{target}>\n"));
        }
    }
}

#[test]
fn ingest_replaces_nul_before_target_spelling() {
    let value = serde_json::json!({"type":"document", "srcByteLength":0,
        "children":[{"type":"paragraph", "children":[{"type":"heading_ref", "target":"a\0b"}]}]});
    let document = from_json(&value.to_string()).unwrap();
    assert_eq!(render_carve(&document).unwrap(), "</#a�b>\n");
}
