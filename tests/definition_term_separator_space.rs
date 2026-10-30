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
fn a_definition_body_fence_after_a_blank_keeps_the_earlier_paragraph() {
    for fence in ["```", "~~~"] {
        let source = format!("a\n\n:: t\n: {fence}\n  body\n  {fence}\n");
        let output = html(&source);
        assert!(
            output.starts_with("<p>a</p>\n<dl>\n"),
            "{source:?}: {output}"
        );
        assert!(output.contains("<dt>t</dt>"), "{source:?}: {output}");
        assert!(
            output.contains("<pre><code>body\n</code></pre>"),
            "{source:?}: {output}"
        );
    }
}

#[test]
fn a_list_fence_after_a_term_starts_its_own_list() {
    let source = "a\n\n:: t\n- ```\n  body\n  ```\n";
    let output = html(source);
    assert!(output.contains("<dt>t</dt>\n</dl>\n<ul>"), "{output}");
    assert!(
        output.contains("<pre><code>body\n</code></pre>"),
        "{output}"
    );
}

#[test]
fn a_fence_outside_a_quoted_term_is_not_its_definition_body() {
    let source = "\n> :: t\n: ```\nx\n```\n";
    let output = html(source);
    assert!(output.contains("</blockquote>\n<p>: <code>"), "{output}");
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
        assert_eq!(doc.children.len(), 2, "{source:?}: {doc:?}");
        assert_eq!(
            html(&source).contains("<pre>"),
            !fence.contains('='),
            "{source:?}"
        );
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
            matches!(
                doc.children.as_slice(),
                [BlockNode::Paragraph(_), BlockNode::CodeBlock(_)]
            ),
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
        assert!(expected.contains("<pre>"), "{source:?}: {expected}");
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
    let source = "p\n~~~\n\n:: t\n```\n[r]: /u\n[^a]: note\n";
    assert_eq!(
        html(source),
        "<p>p</p>\n<pre><code>\n:: t\n```\n[r]: /u\n[^a]: note\n</code></pre>"
    );
    for source in [
        "- a\n\n  ~~~\n\n:: t\n```\n[r]: /u\n[^a]: note\n",
        ":: a\n:  ```\n   code\n   ```\n\n:: t\n```\n[r]: /u\n[^a]: note\n",
        "- ```\n  code\n  ```\n\n:: t\n```\n[r]: /u\n[^a]: note\n",
        ": ```\n\n:: t\n```\n[r]: /u\n[^a]: note\n",
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
            !doc.children.iter().any(
                |node| matches!(node, BlockNode::LinkReferenceDefinition(def) if def.label == "r")
            ),
            "{source:?}: {doc:?}"
        );
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
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
    let source = "- > :: t\n   > ```\n  > [r]: /u\n";
    assert_eq!(
        html(source),
        "<ul>\n  <li>\n    <blockquote>\n      <dl>\n        <dt>t</dt>\n      </dl>\n      <pre><code>[r]: /u\n</code></pre>\n    </blockquote>\n  </li>\n</ul>"
    );
    let source = "- head\n   > :: t\n  > ```\n  > [r]: /u\n";
    let output = html(source);
    assert!(
        output.contains("<pre><code>[r]: /u\n</code></pre>"),
        "{source:?}: {output}"
    );
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
#[ignore = "rule A ledger: carve-rs#3524"]
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

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn indented_description_quotes_stay_literal_term_content() {
    for source in [
        ":: t\n  :  > [^a]: note\n",
        "> :: t\n>   :  > [^a]: note\n",
        "- :: t\n    :  > [^a]: note\n",
    ] {
        let doc = carve::parse(source);
        assert!(doc.footnote_defs.is_empty(), "{source:?}: {doc:?}");
        assert!(
            html(source).contains("[^a]: note"),
            "{source:?}: {}",
            html(source)
        );
    }
    let source = ": > :: t\n> ```\n> [^a]: note\n";
    let doc = carve::parse(source);
    assert!(doc.footnote_defs.is_empty(), "{doc:?}");
    assert!(html(source).contains("<pre><code>[^a]: note\n</code></pre>"));
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn indented_term_text_does_not_own_a_fence_past_a_paragraph_break() {
    for indent in [" ", "  ", "    "] {
        let source =
            format!("{indent}:: t\n{indent}```\n\n{indent}- a\n{indent}  [^a]: note\n\nsee [^a]\n");
        let output = html(&source);
        assert!(
            output.contains("role=\"doc-noteref\""),
            "{source:?}: {output}"
        );
        assert!(!output.contains("<pre>"), "{source:?}: {output}");
    }
}

#[test]
fn folded_list_marker_terms_do_not_own_a_later_fence() {
    for (above, prefix, owns_fence) in [
        ("p\n", "", true),
        ("> p\n", "> ", true),
        ("- p\n", "  ", true),
    ] {
        let source = format!("{above}{prefix}- :: t\n{prefix}  ```\n{prefix}\n{prefix}  - a\n{prefix}    [^a]: note\n\nsee [^a]\n");
        let output = html(&source);
        assert_eq!(
            output.contains("role=\"doc-noteref\""),
            !owns_fence,
            "{source:?}: {output}"
        );
        assert_eq!(output.contains("<pre>"), owns_fence, "{source:?}: {output}");
    }
}

#[test]
fn repeated_admitted_list_terms_keep_their_final_fence_payload() {
    for count in [100, 1000] {
        let source = "- :: t\n".repeat(count) + "  ```\n  [^a]: note\n\nsee [^a]\n";
        let doc = carve::parse(&source);
        assert!(
            doc.footnote_defs.is_empty(),
            "{count}: {:?}",
            doc.footnote_defs
        );
        assert!(html(&source).contains("<pre><code>[^a]: note\n"), "{count}");
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn collected_definitions_allow_following_list_terms_to_own_fences() {
    for prefix in ["", "> "] {
        for above in [
            "[r]: /u\n",
            "[^a]: note\n",
            "[^a]: note\n  body\n",
            "[r]: /u\n- head\n",
        ] {
            let above = above
                .lines()
                .map(|line| format!("{prefix}{line}\n"))
                .collect::<String>();
            let source = format!("{above}{prefix}- :: t\n{prefix}  ```\n{prefix}  [x]: /v\n");
            let output = html(&source);
            assert!(
                output.contains("<pre><code>[x]: /v\n"),
                "{source:?}: {output}"
            );
            assert!(!output.contains('\u{e005}'), "{source:?}: {output}");
        }
    }
    let source = "p\n  [r]: /u\n- :: t\n  ```\n\n  - a\n    [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(output.contains("role=\"doc-noteref\""), "{output}");
    assert!(!output.contains("<pre>"), "{output}");
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn description_bodies_preserve_their_admitted_list_term_host() {
    for prefix in ["", "> "] {
        let above = format!("{prefix}- :: a\n{prefix}  : desc a\n").repeat(100);
        for continuation in [
            "{prefix}    [x]: /v\n",
            "{prefix}  ```\n{prefix}  [x]: /v\n",
        ] {
            let continuation = continuation.replace("{prefix}", prefix);
            let source = format!("{above}{prefix}- :: t\n{continuation}");
            let output = html(&source);
            assert!(output.contains("[x]: /v"), "{source:?}: {output}");
            assert!(!output.contains('\u{e005}'), "{source:?}: {output}");
        }
        let source = format!("{above}{prefix}\n{prefix}p\n{prefix}- :: t\n{prefix}  ```\n{prefix}\n{prefix}  - a\n{prefix}    [^a]: note\n\nsee [^a]\n");
        let output = html(&source);
        assert!(output.contains("role=\"doc-noteref\""), "{output}");
        assert!(!output.contains("<pre>"), "{output}");
        let source = format!("{above}{prefix}\n{prefix}- :: t\n{prefix}  ```\n{prefix}  [x]: /v\n");
        assert!(html(&source).contains("<pre><code>[x]: /v\n"));
    }
}

#[test]
fn lazy_description_continuations_keep_the_admitted_list_host() {
    for prefix in ["", "> "] {
        let above = format!("{prefix}- :: a\n{prefix}  : desc a\n{prefix}p\n").repeat(100);
        let source =
            format!("{above}{prefix}- :: t\n{prefix}  ```\n{prefix}  [^a]: note\n\nsee [^a]\n");
        assert!(carve::parse(&source).footnote_defs.is_empty(), "{source:?}");
        assert!(html(&source).contains("<pre><code>[^a]: note\n"));
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn root_block_boundaries_release_and_readmit_list_term_hosts() {
    let above = "- :: a\n  : desc\n# heading\n".repeat(100);
    let source = above + "- :: t\n    [x]: /v\n";
    assert!(html(&source).contains("[x]: /v"));
    let source = "- :: a\n{.k}\np\n- :: t\n  ```\n\n  - a\n    [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(output.contains("role=\"doc-noteref\""), "{output}");
    assert!(!output.contains("<pre>"), "{output}");
}

#[test]
fn nonparagraph_item_blocks_release_a_following_root_paragraph() {
    for prefix in ["", "> "] {
        for body in ["# h\n", "```\ncode\n```\n", "|x|\n"] {
            let body = body
                .lines()
                .map(|line| format!("{prefix}  {line}\n"))
                .collect::<String>();
            let source = format!("{prefix}- :: a\n{body}{prefix}p\n{prefix}- :: t\n{prefix}  ```\n{prefix}\n{prefix}  - a\n{prefix}    [^a]: note\n\nsee [^a]\n");
            let output = html(&source);
            assert!(
                !output.contains("role=\"doc-noteref\""),
                "{source:?}: {output}"
            );
        }
    }
}

#[test]
fn successive_term_fences_do_not_repeat_the_list_prefix_probe() {
    for count in [600, 1000] {
        for note in [false, true] {
            let prefix = "- x\n".repeat(count);
            let payload = |label: &str| {
                if note {
                    format!("[^{label}]: note")
                } else {
                    format!("[{label}]: /u")
                }
            };
            let source = format!(
                "{prefix}- :: a\n  ````\n  {}\n- y\n- :: b\n  ```\n  {}\n- y\n- :: c\n  ~~~\n  {}\n",
                payload("r1"),
                payload("r2"),
                payload("r3")
            );
            let output = html(&source);
            for label in ["r1", "r2", "r3"] {
                assert!(
                    output.contains(&format!("<pre><code>{}\n</code></pre>", payload(label))),
                    "{count}/{note}: {output}"
                );
            }
            assert!(carve::parse(&source).footnote_defs.is_empty());
        }
    }
}

#[test]
fn folded_generic_fence_does_not_admit_a_following_marker_term() {
    let source = "p\n- ```\n  x\n  ```\n- :: t\n  ~~~\n\n  - a\n    [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(!output.contains("role=\"doc-noteref\""), "{output}");
    assert!(output.contains("<pre>"), "{output}");
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn a_folded_marker_does_not_admit_an_indented_bare_term_fence() {
    for prefix in ["", "> "] {
        for first in ["- x", "- :: a"] {
            let source = format!("{prefix}p\n{prefix}{first}\n{prefix}  :: t\n{prefix}  ```\n{prefix}\n{prefix}  - y\n{prefix}    [^a]: note\n\nsee [^a]\n");
            let output = html(&source);
            assert!(
                output.contains("role=\"doc-noteref\""),
                "{source:?}: {output}"
            );
            assert!(!output.contains("<pre>"), "{source:?}: {output}");
        }
    }
}

#[test]
fn blank_code_payload_keeps_the_following_term_fence_opaque() {
    for prefix in ["", "> "] {
        for payload in ["[r]: /u", "[^r]: note"] {
            let source = format!("{prefix}- :: a\n{prefix}  ```\n{prefix}\n{prefix}  payload\n{prefix}- :: b\n{prefix}  ~~~\n{prefix}  {payload}\n");
            let output = html(&source);
            assert!(
                output.contains(&format!("<pre><code>{payload}\n</code></pre>")),
                "{source:?}: {output}"
            );
            assert!(carve::parse(&source).footnote_defs.is_empty());
        }
    }
}

#[test]
fn nested_term_fences_keep_the_proved_outer_item_through_paragraphs() {
    for prefix in ["", "> "] {
        let source = format!("{prefix}- x\n{prefix}  - :: a\n{prefix}    ```\n{prefix}    c\n{prefix}    ```\n{prefix}  p\n{prefix}  - :: t\n{prefix}    ```\n{prefix}\n{prefix}    - a\n{prefix}      [^a]: note\n\nsee [^a]\n");
        let output = html(&source);
        assert!(
            !output.contains("role=\"doc-noteref\""),
            "{source:?}: {output}"
        );
        assert!(carve::parse(&source).footnote_defs.is_empty());
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn a_marker_between_item_columns_does_not_reuse_a_proved_term_host() {
    for prefix in ["", "> "] {
        let source = format!("{prefix}- :: a\n{prefix}  ```\n{prefix}  c\n{prefix}  ```\n{prefix}- para\n{prefix} - :: t\n{prefix}   ```\n{prefix}\n{prefix}   - y\n{prefix}     [^a]: note\n\nsee [^a]\n");
        let output = html(&source);
        assert!(
            output.contains("role=\"doc-noteref\""),
            "{source:?}: {output}"
        );
    }
}

#[test]
fn a_bare_blank_releases_a_proved_quote_term_host() {
    let source = "> - :: a\n>   ```\n>   c\n>   ```\n\n>   p\n> - :: t\n>   ```\n>\n>   - y\n>     [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(!output.contains("role=\"doc-noteref\""), "{output}");
}

#[test]
fn a_task_marker_term_keeps_its_folded_definition_shaped_text() {
    let source = "- [ ] :: t\n       [r]: /u\n";
    assert_eq!(html(source), "<ul class=\"task-list\">\n  <li><input type=\"checkbox\" disabled> \n    <dl>\n      <dt>t</dt>\n    </dl>\n  </li>\n</ul>");
}

#[test]
fn an_outer_list_marker_does_not_prove_a_host_inside_its_quote() {
    for lead in ["", "- > p\n  > - x\n"] {
        let source = format!("{lead}- > :: t\n  > ```\n  > c\n  > ```\n  > p\n  > - :: u\n  >   ```\n  >\n  >   - y\n  >     [^a]: note\n\nsee [^a]\n");
        let output = html(&source);
        assert!(
            !output.contains("role=\"doc-noteref\""),
            "{source:?}: {output}"
        );
    }
}

#[test]
fn an_unowned_fence_scan_releases_a_host_after_a_dedent() {
    let source = "- :: a\n  ```\n  c\n  ```\n\n  ~~~\np\n~~~\n- :: t\n  ```\n\n  - y\n    [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(!output.contains("role=\"doc-noteref\""), "{output}");
}

#[test]
fn a_bare_blank_during_an_unowned_fence_scan_releases_its_quote_host() {
    let source = "> - :: a\n>   ```\n>   c\n>   ```\n>\n>   ~~~\n\n>   p\n>   ~~~\n> - :: t\n>   ```\n>\n>   - y\n>     [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(!output.contains("role=\"doc-noteref\""), "{output}");
}

#[test]
fn a_different_quote_at_the_same_depth_releases_a_proved_term_host() {
    let source = "- > - :: a\n  >   ```\n  >   c\n  >   ```\n>   p\n> - :: t\n>   ```\n>\n>   - y\n>     [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(!output.contains("role=\"doc-noteref\""), "{output}");
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn a_band_marker_inside_a_nested_item_does_not_reuse_its_outer_host() {
    let source = "- x\n  - :: a\n    ```\n    c\n    ```\n  - para\n   - :: t\n     ```\n\n     - y\n       [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(output.contains("role=\"doc-noteref\""), "{output}");
}

#[test]
fn a_prepass_only_list_column_cannot_prove_an_outer_item_host() {
    let source = "p\n- x\n\n  - :: a\n    ```\n    c\n    ```\n  q\n  - :: t\n    ```\n\n    - y\n      [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(!output.contains("role=\"doc-noteref\""), "{output}");
}

#[test]
fn sibling_marker_width_changes_update_the_proved_content_column() {
    let source = "- :: a\n  ```\n  c\n  ```\n1. :: t\n   ~~~\n   c\n   ~~~\n  p\n- :: u\n  ```\n\n  - y\n    [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(!output.contains("role=\"doc-noteref\""), "{output}");
}

#[test]
fn an_expired_outer_item_cannot_prove_a_later_nested_term_host() {
    for first_block in ["  # h\n", "  ```\n  c\n  ```\n"] {
        let source = format!("- x\n{first_block}p\n\n  - :: a\n    ```\n    c\n    ```\n  q\n  - :: t\n    ```\n\n    - y\n      [^a]: note\n\nsee [^a]\n");
        let output = html(&source);
        assert!(
            !output.contains("role=\"doc-noteref\""),
            "{source:?}: {output}"
        );
    }
}

#[test]
fn a_marker_left_of_a_proved_item_releases_its_term_host() {
    let source = "  - :: a\n    ```\n    c\n    ```\n1. q\n  - :: t\n    ```\n\n    - y\n      [^a]: note\n\nsee [^a]\n";
    let output = html(source);
    assert!(!output.contains("role=\"doc-noteref\""), "{output}");
}

#[test]
fn repeated_root_terms_keep_reference_payload_after_the_probe_budget_boundary() {
    for (count, prefix) in [
        (25, ""),
        (200, ""),
        (200, "```\nx\n```\n\n"),
        (200, "::: box\nx\n:::\n\n"),
        (200, "::: box\n::: inner\nx\n:::\n:::\n\n"),
    ] {
        let source = prefix.to_string()
            + &(0..count)
                .map(|index| format!("# h\n- :: t\n  ``` x\n  [r{index}]: /u\n"))
                .collect::<String>();
        let output = html(&source);
        assert_eq!(
            output.matches("<pre><code class=\"language-x\">").count(),
            count
        );
        for index in 0..count {
            assert!(
                output.contains(&format!("[r{index}]: /u")),
                "missing {index}"
            );
        }
        assert!(!output.contains('\u{e005}'));
    }
}

#[test]
fn repeated_root_terms_keep_note_payload_after_the_probe_budget_boundary() {
    for count in [25, 200] {
        let source = (0..count)
            .map(|index| format!("# h\n- :: t\n  ``` x\n  [^n{index}]: note\n"))
            .collect::<String>();
        let doc = carve::parse(&source);
        assert!(doc.footnote_defs.is_empty());
        let output = carve::render_html(&doc).unwrap();
        assert_eq!(
            output.matches("<pre><code class=\"language-x\">").count(),
            count
        );
        for index in 0..count {
            assert!(
                output.contains(&format!("[^n{index}]: note")),
                "missing {index}"
            );
        }
    }
}

#[test]
fn a_folded_marker_fence_with_a_flush_closer_cannot_hide_later_definitions() {
    for quoted in [false, true] {
        for term_first in [false, true] {
            for definition in ["[r]: /u", "[^a]: note"] {
                let body = format!("p\n- ```\n  x\n```\n\n{definition}\n\nsee [^a]\n\n");
                let source = if term_first {
                    format!(":: t\n\n{body}")
                } else {
                    format!("{body}:: t\n")
                };
                let source = if quoted {
                    source.lines().map(|line| format!("> {line}\n")).collect()
                } else {
                    source
                };
                let doc = carve::parse(&source);
                let output = carve::render_html(&doc).unwrap();
                assert!(!output.contains(definition), "{source:?}: {output}");
                if definition.starts_with("[^") {
                    assert_eq!(doc.footnote_defs.len(), 1, "{source:?}: {output}");
                    assert!(output.contains("role=\"doc-noteref\""), "{output}");
                }
            }
        }
    }
}
