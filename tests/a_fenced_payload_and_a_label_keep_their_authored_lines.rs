//! Two writer defects that share this file because they share the writer, and one
//! leave-one-out control each so a green run isolates them.
//!
//! carve-rs#2168: a raw payload of two or more blank lines came back as one. The
//! all-blank branch emitted real newlines instead of staging them, so the pass
//! that folds a run of blank lines reached them.
//!
//! carve-rs#2163: `admonition.label` published the trailing comment the HTML
//! renderer drops, and so did the Markdown, plain-text and ANSI targets. Six render
//! sites read `CARVE-P9-041` and four publishers did not, so the cut moved to the
//! node.

use carve::{parse, to_ansi, to_carve, to_html, to_json, to_markdown, to_plain_text, BlockNode};

fn raw_payload(source: &str) -> String {
    for block in &parse(source).children {
        if let BlockNode::RawBlock(raw) = block {
            return raw.content.clone();
        }
    }
    panic!("no raw block in {source:?}");
}

/// The label a container node carries, whichever of the two the opener made: a
/// bare `:::[x]` has no kind word and opens a div.
fn container_label(source: &str) -> String {
    for block in &parse(source).children {
        match block {
            BlockNode::Admonition(node) => {
                return node.label.clone().expect("the admonition carries a label")
            }
            BlockNode::Div(node) => return node.label.clone().expect("the div carries a label"),
            _ => {}
        }
    }
    panic!("no labelled container in {source:?}");
}

// ---------------------------------------------------------------- carve-rs#2168

/// The authored line count survives, which is what the round trip lost. Compared
/// as BYTES: the rendered output of the two agreed while the lines went missing,
/// so an HTML comparison alone cannot see this.
#[test]
fn issue_2168_a_blank_raw_payload_keeps_every_line() {
    for source in [
        "```=html\n```\n",
        "```=html\n\n```\n",
        "```=html\n\n\n```\n",
        "```=html\n\n\n\n```\n",
    ] {
        assert_eq!(to_carve(source), source, "{source:?}");
    }
}

/// And the parse side already told them apart, so the loss was the writer's alone.
#[test]
fn issue_2168_the_payloads_were_never_the_same_block() {
    assert_eq!(raw_payload("```=html\n```\n"), "");
    assert_eq!(raw_payload("```=html\n\n```\n"), "\n");
    assert_eq!(raw_payload("```=html\n\n\n```\n"), "\n\n");
    assert_eq!(raw_payload("```=html\n\n\n\n```\n"), "\n\n\n");
}

/// Every host, and a payload with content, because the writer is one function.
#[test]
fn issue_2168_every_host_keeps_its_lines() {
    for source in [
        "- a\n\n  ```=html\n\n\n  ```\n\n- s\n",
        "> ```=html\n>\n>\n> ```\n",
        "```=html\n<b>x</b>\n\n```\n",
        "```=html\n\n<b>x</b>\n```\n",
    ] {
        assert_eq!(to_carve(source), source, "{source:?}");
        assert_eq!(to_html(&to_carve(source)), to_html(source), "{source:?}");
    }
}

// ---------------------------------------------------------------- carve-rs#2163

/// The node carries the cut label, so no consumer can publish what the author
/// hid. Corpus 518-9 and 518-10 are the two documents.
#[test]
fn issue_2163_the_node_carries_the_cut_label() {
    assert_eq!(container_label("::: note [a\t%% hidden]\nbody\n:::\n"), "a");
    assert_eq!(container_label("::: note [%% hidden]\nbody\n:::\n"), "");
}

/// Every publisher, which is the point: the HTML was already right and four
/// others were not.
#[test]
fn issue_2163_no_publisher_leaks_the_comment() {
    for source in [
        "::: note [a\t%% hidden]\nbody\n:::\n",
        "::: note [%% hidden]\nbody\n:::\n",
        ":::[a  %% hidden]\nbody\n:::\n",
    ] {
        for published in [
            to_html(source),
            to_markdown(source),
            to_plain_text(source),
            to_ansi(source),
            to_json(&parse(source)),
            to_carve(source),
        ] {
            assert!(
                !published.contains("hidden"),
                "{source:?} published the comment: {published}"
            );
        }
    }
}

/// What the rule must NOT eat. A marker with no separator is not a marker, and a
/// construct that is opaque to one keeps its text - the run itself arbitrates,
/// which is the property carve-rs#2158 established and this must not undo.
#[test]
fn issue_2163_an_opaque_construct_keeps_its_text() {
    assert_eq!(
        container_label(":::[a%%b and 50%%]\nbody\n:::\n"),
        "a%%b and 50%%"
    );
    assert_eq!(container_label(":::[`%% x`]\nbody\n:::\n"), "`%% x`");
    assert_eq!(
        container_label(":::[a ` b %% c]\nbody\n:::\n"),
        "a ` b %% c"
    );
}

/// A label with no comment at all is untouched, in every publisher.
#[test]
fn issue_2163_an_ordinary_label_is_untouched() {
    assert_eq!(container_label("::: note [a]\nbody\n:::\n"), "a");
    assert_eq!(
        to_carve("::: note [a]\nbody\n:::\n"),
        "::: note [a]\nbody\n:::\n"
    );
    assert!(to_markdown("::: note [a]\nbody\n:::\n").contains('a'));
}

// -------------------------------------------------------- the two do not overlap

/// The leave-one-out control. Each ticket's assertions fail only for its own
/// change, which is what makes one PR carrying both still isolate them: the raw
/// payload never reaches the label cut, and a label never reaches the payload
/// staging.
#[test]
fn the_two_fixes_touch_different_documents() {
    let payload_doc = "```=html\n\n\n```\n";
    let label_doc = "::: note [a\t%% hidden]\nbody\n:::\n";
    assert!(!payload_doc.contains("%%"));
    assert!(!label_doc.contains("=html"));
    assert_eq!(to_carve(payload_doc), payload_doc);
    assert_eq!(container_label(label_doc), "a");
}
