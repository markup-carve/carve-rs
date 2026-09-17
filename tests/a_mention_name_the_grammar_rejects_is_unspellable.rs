//! A mention or tag whose name `name_word {'.' name_word}` rejects has no
//! spelling, so the writer refuses it (ruling markup-carve/carve-php#2159).

use carve::{from_json, parse, render_carve, BlockNode, Document, InlineNode, RenderCarveError};

fn tree(ty: &str, name: &str) -> Document {
    let field = if ty == "tag" { "name" } else { "user" };
    let json = serde_json::json!({
        "type": "document",
        "srcByteLength": 0,
        "children": [{"type": "paragraph", "children": [
            {"type": "text", "value": "ping "},
            {"type": ty, field: name}
        ]}]
    });
    from_json(&json.to_string()).expect("the tree decodes")
}

fn refusal(doc: &Document) -> Option<&'static str> {
    match render_carve(doc) {
        Err(RenderCarveError::SourceUnspellable(error)) => Some(error.node_type()),
        _ => None,
    }
}

#[test]
fn a_name_the_grammar_rejects_is_refused() {
    let names = [
        "Lea Thompson",
        "o'brien",
        "lea.",
        ".lea",
        "lea..t",
        "Zoë",
        "@lea",
        "a#b",
        "",
    ];
    for ty in ["mention", "tag"] {
        for name in names {
            let doc = tree(ty, name);
            assert_eq!(
                refusal(&doc),
                Some(ty),
                "{ty} {name:?} wrote {:?}",
                render_carve(&doc)
            );
        }
    }
}

#[test]
fn a_spellable_name_is_written_and_reads_back() {
    for (ty, sigil) in [("mention", '@'), ("tag", '#')] {
        for name in ["lea", "john.doe.2", "a_b-c", "v1.0"] {
            let written = render_carve(&tree(ty, name)).expect("the name writes");
            assert_eq!(written, format!("ping {sigil}{name}\n"));
            assert_eq!(render_carve(&parse(&written)).unwrap(), written);
        }
    }
}

fn count_mentions(doc: &Document) -> usize {
    doc.children
        .iter()
        .map(|block| match block {
            BlockNode::Paragraph(paragraph) => paragraph
                .children
                .iter()
                .filter(|inline| matches!(inline, InlineNode::Mention(_) | InlineNode::Tag(_)))
                .count(),
            _ => 0,
        })
        .sum()
}

/// `fmt` never reaches the refusal: every mention or tag the parser builds
/// carries a name the writer accepts. Exhaustive over short runs of the
/// characters that end, split or break a name.
#[test]
fn formatting_parsed_source_never_refuses_a_name() {
    let alphabet = ['@', '#', 'a', '.', ' ', '\'', 'é', '-', '\\'];
    let mut runs = vec![String::new()];
    let mut all = Vec::new();
    for _ in 0..4 {
        runs = runs
            .iter()
            .flat_map(|run| alphabet.iter().map(move |ch| format!("{run}{ch}")))
            .collect();
        all.extend(runs.iter().cloned());
    }
    let mut mentions = 0;
    for run in &all {
        for source in [format!("x {run} y"), format!("{run}b")] {
            let doc = parse(&source);
            mentions += count_mentions(&doc);
            if let Err(RenderCarveError::SourceUnspellable(error)) = render_carve(&doc) {
                assert!(
                    !matches!(error.node_type(), "mention" | "tag"),
                    "fmt refused {source:?}: {error}"
                );
            }
        }
    }
    assert!(mentions > 1000, "only {mentions} mentions were built");
}
