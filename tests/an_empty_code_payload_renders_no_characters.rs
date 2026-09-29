//! Empty code fences contain no payload, regardless of how they end.

use carve::{parse, to_carve, to_html, to_markdown, BlockNode};

fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

fn payload(source: &str) -> String {
    for block in &parse(source).children {
        if let BlockNode::CodeBlock(code) = block {
            return code.content.clone();
        }
    }
    panic!("no code block in {source:?}");
}

/// Corpus 524, both rows. The pair is the point: one row alone would pass for a
/// reader that simply moved the newline off both spellings.
#[test]
fn the_corpus_rows() {
    assert_eq!(html("```\n```\n"), "<pre><code></code></pre>");
    assert_eq!(html("```\n\n```\n"), "<pre><code>\n</code></pre>");
}

/// A payload of blank lines keeps every one of them, and the count is what
/// `content` has to carry.
#[test]
fn a_blank_payload_keeps_its_lines() {
    assert_eq!(payload("```\n```\n"), "");
    assert_eq!(payload("```\n\n```\n"), "\n");
    assert_eq!(payload("```\n\n\n```\n"), "\n\n");
    assert_eq!(html("```\n\n\n```\n"), "<pre><code>\n\n</code></pre>");
}

/// Nonempty payloads include the final line ending.
#[test]
fn a_payload_with_content_keeps_its_line_endings() {
    assert_eq!(payload("```\nx\n```\n"), "x\n");
    assert_eq!(html("```\nx\n```\n"), "<pre><code>x\n</code></pre>");
    assert_eq!(payload("```\nx\n\n```\n"), "x\n\n");
    assert_eq!(html("```\nx\n\n```\n"), "<pre><code>x\n\n</code></pre>");
    assert_eq!(html("```\n\nx\n```\n"), "<pre><code>\nx\n</code></pre>");
}

/// An info string changes nothing about the payload.
#[test]
fn an_info_string_changes_nothing() {
    assert_eq!(
        html("```rs\n```\n"),
        "<pre><code class=\"language-rs\"></code></pre>"
    );
    assert_eq!(html("~~~\n~~~\n"), "<pre><code></code></pre>");
}

/// Every host, since the payload rule is the fence's and not the container's.
#[test]
fn every_host_reads_the_same_way() {
    assert!(html("> ```\n> ```\n").contains("<pre><code></code></pre>"));
    assert!(html("- ```\n  ```\n").contains("<pre><code></code></pre>"));
    assert!(html("::: note\n```\n```\n:::\n").contains("<pre><code></code></pre>"));
}

/// Container termination and a closer both leave a zero-line payload empty.
#[test]
fn an_unterminated_empty_fence_has_no_payload() {
    for source in [
        "- ```\nx\n```\n",
        "> ```\nx\n```\n",
        "> > ```\nc\n",
        "> > ```\n> > ```\ny\n",
    ] {
        assert!(html(source).contains("<pre><code></code></pre>"));
    }
}

/// Both spellings round trip through the Carve writer, which could only spell one
/// of them before the distinction existed.
#[test]
fn both_spellings_round_trip() {
    for source in [
        "```\n```\n",
        "```\n\n```\n",
        "```\n\n\n```\n",
        "```\nx\n```\n",
        "```\nx\n\n```\n",
        "```\n\nx\n```\n",
        "```rs\n```\n",
        "- a\n\n  ```\n  ```\n\n- s\n",
        "> ```\n> ```\n",
    ] {
        assert_eq!(to_carve(source), source, "{source:?}");
        assert_eq!(to_html(&to_carve(source)), to_html(source), "{source:?}");
    }
}

/// The Markdown target writes the payload's ending the same way. Writing one
/// unconditionally gave an unterminated empty fence two blank lines where the
/// document has one.
#[test]
fn the_markdown_target_writes_one_ending() {
    assert_eq!(to_markdown("```\n```\n"), "```\n```\n");
    assert_eq!(to_markdown("```\n\n```\n"), "```\n\n```\n");
    assert_eq!(to_markdown("```\nx\n```\n"), "```\nx\n```\n");
    assert_eq!(to_markdown("> ```\n> ```\n"), "> ```\n> ```\n");
}
