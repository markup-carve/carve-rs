//! The SEPARATOR after `::` is the space, and whitespace past it is not
//! content (carve#513, carve#530, carve-rs#511 item 3).
//!
//! `::  u` rendered `<dt> u</dt>` here and `<dt>u</dt>` in carve-js and
//! carve-php. The rule is a property of the separator, not of any one marker:
//! this engine already stripped the extra column for `#`, `>` and `-`, and the
//! definition term was the one marker that kept it. That is the same shape
//! carve#513 and carve#530 rejected twice.

use carve::ast::{BlockNode, InlineNode};

fn html(source: &str) -> String {
    carve::render_html(&carve::parse(source))
        .expect("the tree under test is within the render ceiling")
}

fn term_text(source: &str) -> String {
    let doc = carve::parse(source);
    let BlockNode::DefinitionList(list) = doc.children.first().expect("a block") else {
        panic!("expected a definition list, got {:?}", doc.children.first());
    };
    let term = list
        .items
        .first()
        .expect("an item")
        .terms
        .first()
        .expect("a term");
    term.children
        .iter()
        .map(|node| match node {
            InlineNode::Text(t) => t.value.clone(),
            other => panic!("expected text, got {other:?}"),
        })
        .collect()
}

#[test]
fn extra_columns_after_the_separator_are_not_term_content() {
    assert_eq!(term_text("::  u\n:  d\n"), "u");
    assert_eq!(term_text("::    spaced\n:  d\n"), "spaced");
}

#[test]
fn the_single_separator_space_is_unchanged() {
    assert_eq!(term_text(":: u\n:  d\n"), "u");
}

#[test]
fn a_term_keeps_its_interior_and_loses_its_trailing_run() {
    // Only the run against the marker is separator; spaces the author wrote
    // inside the term are content, and the trailing run was already dropped.
    assert_eq!(term_text("::   a  b   \n:  d\n"), "a  b");
}

#[test]
fn every_term_in_an_item_strips_its_own_padding() {
    // An item can hold several terms, and each `::` line carries its own
    // separator. Trimming only the first left the second `<dt>` padded - found
    // in review, not by the first three tests, which all used one term.
    let doc = carve::parse(":: a\n::  b\n:  d\n");
    let BlockNode::DefinitionList(list) = doc.children.first().expect("a block") else {
        panic!("expected a definition list");
    };
    let terms = &list.items.first().expect("an item").terms;
    assert_eq!(terms.len(), 2, "{terms:?}");
    let text = |term: &carve::ast::DefinitionTerm| -> String {
        term.children
            .iter()
            .map(|node| match node {
                InlineNode::Text(t) => t.value.clone(),
                other => panic!("expected text, got {other:?}"),
            })
            .collect()
    };
    assert_eq!(text(&terms[0]), "a");
    assert_eq!(text(&terms[1]), "b");
}

#[test]
fn the_rendered_term_matches_the_other_two_engines() {
    assert!(
        html("::  u\n:  d\n").contains("<dt>u</dt>"),
        "got {}",
        html("::  u\n:  d\n")
    );
}

#[test]
fn the_terms_own_text_is_positioned_past_the_separator() {
    // The column has to move with the trim, or the term's text selects the
    // separator. `stripped_col` matches the slice as a SUFFIX of the raw line,
    // so passing the trimmed slice is what carries the column along.
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options("::  u\n:  d\n", &options);
    let BlockNode::DefinitionList(list) = doc.children.first().expect("a block") else {
        panic!("expected a definition list");
    };
    let term = list
        .items
        .first()
        .expect("an item")
        .terms
        .first()
        .expect("a term");
    let InlineNode::Text(text) = term.children.first().expect("term text") else {
        panic!("expected text");
    };
    let pos = text.pos.as_ref().expect("a position");
    // `::  u` - the `u` sits in column 5, which is where carve-js puts it too.
    assert_eq!(pos.start_column, 5, "{pos:?}");
    assert_eq!(pos.start_offset, 4, "{pos:?}");
}

#[test]
fn an_unclosed_code_opener_at_the_container_column_ends_a_term() {
    for fence in ["```", "~~~", "``` rust", "```=html"] {
        for ending in ["", "\n"] {
            let source = format!(":: t\n{fence}{ending}");
            let doc = carve::parse(&source);
            assert_eq!(doc.children.len(), 2, "{source:?}: {doc:?}");
            let BlockNode::DefinitionList(list) = &doc.children[0] else {
                panic!("{source:?}: expected definition list");
            };
            assert_eq!(list.items[0].terms[0].children.len(), 1, "{source:?}");
            assert!(
                matches!(&list.items[0].terms[0].children[0], InlineNode::Text(t) if t.value == "t")
            );
            assert!(
                matches!(
                    &doc.children[1],
                    BlockNode::CodeBlock(_) | BlockNode::RawBlock(_)
                ),
                "{source:?}: {doc:?}"
            );
        }
    }
}

#[test]
fn a_code_opener_past_the_container_column_stays_term_text() {
    for fence in ["```", "~~~", "``` rust", "```=html"] {
        let source = format!(":: t\n  {fence}\n");
        let doc = carve::parse(&source);
        assert_eq!(doc.children.len(), 1, "{source:?}: {doc:?}");
        assert!(!html(&source).contains("<pre>"), "{source:?}");
    }
}

#[test]
fn an_unclosed_fence_ends_a_term_inside_a_quote_or_list() {
    for source in ["> :: t\n> ```", "- :: t\n  ```", "> - :: t\n>   ```"] {
        let output = html(source);
        assert!(output.contains("<dt>t</dt>"), "{source:?}: {output}");
        assert!(output.contains("<pre><code>"), "{source:?}: {output}");
    }
}

#[test]
fn an_unclosed_fence_keeps_folding_into_an_ordinary_paragraph() {
    for source in ["t\n```", "t\n~~~"] {
        let doc = carve::parse(source);
        assert!(
            matches!(doc.children.as_slice(), [BlockNode::Paragraph(_)]),
            "{source:?}: {doc:?}"
        );
    }
}

#[test]
fn definitions_inside_an_unclosed_fence_after_a_term_remain_payload() {
    for source in [
        ":: t\n```\n[r]: /u\n[^a]: note\n",
        ":: t\nmore\n```\n[r]: /u\n[^a]: note\n",
        ":: a\n:: b\n```\n[r]: /u\n[^a]: note\n",
        "> :: t\n> ```\n> [r]: /u\n> [^a]: note\n",
        "- :: t\n  ```\n  [r]: /u\n  [^a]: note\n",
        "> - :: t\n>   ```\n>   [r]: /u\n>   [^a]: note\n",
        "> > :: t\n> > ```\n> > [r]: /u\n> > [^a]: note\n",
        "- > :: t\n  > ```\n  > [r]: /u\n  > [^a]: note\n",
        "- - > :: t\n    > ```\n    > [r]: /u\n    > [^a]: note\n",
        "> - - > :: t\n>     > ```\n>     > [r]: /u\n>     > [^a]: note\n",
        ":: outer\n:  > :: t\n   > ```\n   > [r]: /u\n   > [^a]: note\n",
        "> - > :: t\n>   > ```\n>   > [r]: /u\n>   > [^a]: note\n",
    ] {
        let output = html(source);
        assert!(
            output.contains("<pre><code>[r]: /u\n[^a]: note\n</code></pre>"),
            "{source:?}: {output}"
        );
        assert!(!output.contains("footnotes"), "{source:?}: {output}");
    }
}

#[test]
fn inline_term_continuations_do_not_expose_a_later_fences_definitions() {
    for source in [
        ":: t\n![a](b.png)\n```\n[r]: /u\n[^a]: note\n",
        ":: t\n[@r]: /u\n```\n[r]: /u\n[^a]: note\n",
        ":: t\n[r]:\n```\n[r]: /u\n[^a]: note\n",
        "> :: t\nmore\n> ```\n> [r]: /u\n> [^a]: note\n",
    ] {
        let output = html(source);
        assert!(
            output.contains("<pre><code>[r]: /u\n[^a]: note\n</code></pre>"),
            "{source:?}: {output}"
        );
        assert!(!output.contains("footnotes"), "{source:?}: {output}");
    }
}

#[test]
fn fences_past_a_hosted_terms_column_preserve_the_writer_roundtrip() {
    for source in [":: t\n  ~~~\n", "- :: t\n    ~~~\n", "> :: t\n>   ~~~\n"] {
        let expected = html(source);
        assert!(!expected.contains("<pre>"), "{source:?}: {expected}");
        let canonical = carve::to_carve(source);
        assert_eq!(html(&canonical), expected, "{source:?}: {canonical:?}");
        assert_eq!(carve::to_carve(&canonical), canonical, "{source:?}");
    }
}

#[test]
fn the_term_span_ends_before_the_unclosed_fence() {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(":: t\n```\n[r]: /u", &options);
    let BlockNode::DefinitionList(list) = &doc.children[0] else {
        panic!("expected definition list")
    };
    let term = list.items[0].terms[0].pos.as_ref().expect("term span");
    assert_eq!(term.end_line, 1);
    assert_eq!(term.end_offset, 4);
    let BlockNode::CodeBlock(code) = &doc.children[1] else {
        panic!("expected code block")
    };
    let pos = code.pos.as_ref().expect("code span");
    assert_eq!(pos.start_line, 2);
    assert_eq!(pos.start_column, 1);
    assert_eq!(pos.start_offset, 5);
}

#[test]
fn a_closed_hosted_term_fence_releases_following_definitions() {
    for source in [
        ":: t\n```\ncode\n```\n\nuse [^a]\n\n[^a]: note\n",
        "- :: t\n  ```\n  code\n  ```\n\n  use [^a]\n\n  [^a]: note\n",
        "> :: t\n> ```\n> code\n> ```\n> use [^a]\n\n[^a]: note\n",
        "- :: t\n  ```\n  code\n  ```\n  use [^a]\n  [^a]: note\n",
        "> - :: t\n>   ```\n>   code\n>   ```\n>   use [^a]\n>   [^a]: note\n",
        "> > :: t\n> > ```\n> > code\n> > ```\n\nuse [^a]\n\n[^a]: note\n",
        "> > - :: t\n> >   ```\n> >   code\n> >   ```\n\nuse [^a]\n\n[^a]: note\n",
        "- > :: t\n  > ```\n  > code\n  > ```\n\nuse [^a]\n\n[^a]: note\n",
    ] {
        let doc = carve::parse(source);
        assert!(doc.footnote_defs.contains_key("a"), "{source:?}: {doc:?}");
        let output = html(source);
        assert!(
            output.contains("<pre><code>code\n</code></pre>"),
            "{source:?}: {output}"
        );
        assert!(output.contains("doc-noteref"), "{source:?}: {output}");
    }
}

#[test]
fn payload_markers_cannot_close_a_term_owned_fence() {
    for (source, payload) in [
        (
            "> :: t\n> ```\n> > ```\n> [r]: /u\n> ```\n\nuse [^a]\n\n[^a]: note\n",
            "&gt; ```\n[r]: /u\n",
        ),
        (
            "- :: t\n  ```\n  - ```\n  [r]: /u\n  ```\n\nuse [^a]\n\n[^a]: note\n",
            "- ```\n[r]: /u\n",
        ),
    ] {
        let doc = carve::parse(source);
        assert!(doc.footnote_defs.contains_key("a"), "{source:?}: {doc:?}");
        let output = html(source);
        assert!(
            output.contains(&format!("<pre><code>{payload}</code></pre>")),
            "{source:?}: {output}"
        );
    }
}

#[test]
fn an_unclosed_term_fence_releases_definitions_after_its_host_ends() {
    for prefix in ["> ", "> > ", "- > "] {
        let source = if prefix == "- > " {
            "- > :: t\n  > ```\n  > code\n\nuse [^a]\n\n[^a]: note\n[r]: /u\n".to_string()
        } else {
            format!("{prefix}:: t\n{prefix}```\n{prefix}code\n\nuse [^a]\n\n[^a]: note\n[r]: /u\n")
        };
        let doc = carve::parse(&source);
        assert!(doc.footnote_defs.contains_key("a"), "{source:?}: {doc:?}");
        assert!(
            doc.children.iter().any(
                |node| matches!(node, BlockNode::LinkReferenceDefinition(def) if def.label == "r")
            ),
            "{source:?}: {doc:?}"
        );
        assert!(html(&source).contains("doc-noteref"), "{source:?}");
    }
    let source = "> - :: t\n>   ```\n>   code\n> [x]: /v\n";
    assert!(
        html(source).contains("<pre><code>code\n</code></pre>"),
        "{source:?}: {}",
        html(source)
    );
}

#[test]
fn a_shallower_fence_after_a_term_host_starts_its_own_payload() {
    let source = "> > :: t\n> > ```\n> > code\n> ```\n> [^a]: x\n";
    let doc = carve::parse(source);
    assert!(doc.footnote_defs.is_empty(), "{doc:?}");
    let output = html(source);
    assert_eq!(output.matches("<pre><code>").count(), 2, "{output}");
    assert!(
        output.contains("<pre><code>[^a]: x\n</code></pre>"),
        "{output}"
    );
}

#[test]
fn a_later_closer_does_not_extend_a_term_fence_past_its_host() {
    for source in [
        "> - :: t\n>   ```\n> [r]: /u\n>   ```\n",
        "> > :: t\n> > ```\n[r]: /u\n> > ```\n",
        "> :: t\n> ```\n[r]: /u\n> ```\n",
        "- :: t\n  ```\n[r]: /u\n  ```\n",
    ] {
        let doc = carve::parse(source);
        assert!(
            doc.children.iter().any(
                |node| matches!(node, BlockNode::LinkReferenceDefinition(def) if def.label == "r")
            ),
            "{source:?}: {doc:?}"
        );
        assert!(
            html(source).contains("<pre><code></code></pre>"),
            "{source:?}: {}",
            html(source)
        );
    }
}

#[test]
fn an_earlier_folded_or_host_ended_fence_does_not_hide_a_later_term() {
    for source in [
        "p\n~~~\n\n:: t\n```\n[r]: /u\n[^a]: note\n",
        "- a\n\n  ~~~\n\n:: t\n```\n[r]: /u\n[^a]: note\n",
    ] {
        let output = html(source);
        assert!(output.contains("<dt>t</dt>"), "{source:?}: {output}");
        assert!(
            output.contains("<pre><code>[r]: /u\n[^a]: note\n</code></pre>"),
            "{source:?}: {output}"
        );
    }
}

#[test]
fn intervening_blocks_end_term_ownership_before_a_later_paragraph_fence() {
    for block in ["::: \\\nbody\n:::", "{.k\n .other}"] {
        let source = format!(":: t\n{block}\np\n```\n[r]: /u\n");
        let doc = carve::parse(&source);
        assert!(
            doc.children.iter().any(
                |node| matches!(node, BlockNode::LinkReferenceDefinition(def) if def.label == "r")
            ),
            "{source:?}: {doc:?}"
        );
    }
}

#[test]
fn term_fences_release_definitions_when_the_parent_list_ends() {
    for source in [
        "- > :: t\n  > ```\n  > code\n> [r]: /u\n> [^a]: note\n> use [^a]\n",
        "> - > :: t\n>   > ```\n>   > code\n> > [r]: /u\n> > [^a]: note\n> > use [^a]\n",
        "- head\n  > :: t\n  > ```\n> [r]: /u\n> [^a]: note\n> use [^a]\n",
    ] {
        let doc = carve::parse(source);
        assert!(doc.footnote_defs.contains_key("a"), "{source:?}: {doc:?}");
        let output = html(source);
        assert!(output.contains("doc-noteref"), "{source:?}: {output}");
        assert!(!output.contains("<code>code\n[r]"), "{source:?}: {output}");
    }
}

#[test]
fn term_quote_hosts_allow_indentation_above_the_list_floor() {
    for source in [
        "- > :: t\n   > ```\n  > [r]: /u\n",
        "- head\n   > :: t\n  > ```\n  > [r]: /u\n",
    ] {
        let output = html(source);
        assert!(
            output.contains("<pre><code>[r]: /u\n</code></pre>"),
            "{source:?}: {output}"
        );
    }
}

#[test]
fn a_fence_in_a_new_quote_uses_its_own_host_floor() {
    for prefix in ["- > :: t\n> ", "- head\n  > :: t\n> ", "> - > :: t\n> > "] {
        let quote = if prefix.starts_with(">") {
            "> > "
        } else {
            "> "
        };
        for closer in ["", "```\n"] {
            let source = format!("{prefix}```\n{quote}[r]: /u\n{quote}[^a]: note\n{quote}{closer}");
            let doc = carve::parse(&source);
            assert!(doc.footnote_defs.is_empty(), "{source:?}: {doc:?}");
            let output = html(&source);
            assert!(
                output.contains("<pre><code>[r]: /u\n[^a]: note\n"),
                "{source:?}: {output}"
            );
        }
    }
}

#[test]
fn an_unowned_description_marker_does_not_open_a_term_quote() {
    let source = ":  > :: t\n   > ```\n   > [r]: /u\n   > [^a]: note\n";
    let doc = carve::parse(source);
    assert!(
        matches!(doc.children.as_slice(), [BlockNode::Paragraph(_)]),
        "{doc:?}"
    );
    assert!(doc.footnote_defs.is_empty(), "{doc:?}");
    assert!(!html(source).contains("<pre>"));
}
