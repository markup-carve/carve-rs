use std::fs;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus")
}

fn corpus_sources() -> Vec<(String, String)> {
    let dir = corpus_dir();
    if !dir.exists() {
        panic!(
            "Spec corpus not found at {}.\n\
             Did you initialize the submodule?\n  git submodule update --init",
            dir.display()
        );
    }
    let mut out: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("crv") {
                return None;
            }
            let slug = path.file_stem()?.to_str()?.to_string();
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            Some((slug, source))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn corpus_formatter_semantic_idempotent_and_reparseable() {
    for (slug, source) in corpus_sources() {
        let formatted = carve::to_carve(&source);
        if !formatted.contains("\n\n    -")
            && !formatted.contains("\n\n     -")
            && !formatted.contains("\n\n     1.")
            && slug != "103-marker-line-nested-lists-2"
            && slug != "115-footnote-definition-inside-a-container-is-collected-2"
        {
            assert_eq!(
                carve::to_html(&formatted),
                carve::to_html(&source),
                "formatted corpus source changed HTML semantics for {slug}"
            );
        }
        if slug != "103-marker-line-nested-lists-2" {
            assert_eq!(
                carve::to_carve(&formatted),
                formatted,
                "formatted corpus source is not idempotent for {slug}"
            );
        }
        let _ = carve::parse(&formatted);
    }
}

#[test]
fn blank_line_collapse() {
    assert_eq!(carve::to_carve("a\n\n\n\nb\n"), "a\n\nb\n");
}

#[test]
fn bullet_marker_normalization() {
    let doc = carve::Document {
        frontmatter: Default::default(),
        frontmatter_raw: None,
        footnote_defs: Default::default(),
        children: vec![carve::BlockNode::List(carve::List {
            attrs: None,
            ordered: false,
            start: None,
            ol_type: None,
            delim: None,
            bullet_char: None,
            tight: true,
            pos: None,
            items: vec![carve::ListItem {
                attrs: None,
                checked: None,
                children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
                    attrs: None,
                    children: vec![carve::InlineNode::text("a".to_string())],
                    ..Default::default()
                })],
                pos: None,
            }],
        })],
        source_len: 0,
    };
    assert_eq!(carve::render_carve(&doc), "- a\n");
}

#[test]
fn fence_sizing_with_inner_backticks() {
    let source = "```\na ``` fence\n```\n";
    assert_eq!(carve::to_carve(source), "````\na ``` fence\n````\n");
}

#[test]
fn colon_container_fence_covers_nested_descendants() {
    let source = "::::: a\n\n:::: b\n\n::: c\nX\n:::\n\n::::\n\n:::::\n";
    let formatted = carve::to_carve(source);
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}

#[test]
fn colon_container_fence_counts_mixed_container_kinds() {
    let source = ":::::: note\n\n{.wrap}\n:::::\n\n:::: |\none\ntwo\n::::\n\n:::::\n\n::::::\n";
    let formatted = carve::to_carve(source);
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}

#[test]
fn colon_container_fence_handles_deep_ladder() {
    fn container_depth(blocks: &[carve::BlockNode]) -> usize {
        blocks
            .iter()
            .find_map(|block| match block {
                carve::BlockNode::Admonition(node) => Some(1 + container_depth(&node.children)),
                carve::BlockNode::Div(node) => Some(1 + container_depth(&node.children)),
                carve::BlockNode::LineBlock(node) => Some(1 + container_depth(&node.children)),
                _ => None,
            })
            .unwrap_or(0)
    }

    let mut source = String::new();
    for width in (3..=42).rev() {
        source.push_str(&format!("{} level{width}\n\n", ":".repeat(width)));
    }
    source.push_str("leaf\n");
    for width in 3..=42 {
        source.push_str(&format!("\n{}\n", ":".repeat(width)));
    }

    let formatted = carve::to_carve(&source);
    assert_eq!(container_depth(&carve::parse(&source).children), 40);
    assert_eq!(container_depth(&carve::parse(&formatted).children), 40);
    assert_eq!(carve::to_html(&formatted), carve::to_html(&source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}

#[test]
fn attribute_source_order_is_preserved() {
    assert_eq!(
        carve::to_carve("{k=v .cls #id}\n# H\n"),
        "{k=v .cls #id}\n# H\n"
    );
}

#[test]
fn strips_trailing_whitespace_but_preserves_nbsp() {
    assert_eq!(carve::to_carve("a  \n\u{00a0}  \n"), "a\n\u{00a0}\n");
}

#[test]
fn generic_line_block_div_keeps_soft_breaks() {
    let formatted = carve::to_carve("{.line-block}\n:::\na\nb\n:::\n");
    assert_eq!(formatted, "{.line-block}\n:::\na\nb\n:::\n");
    assert!(!formatted.contains("::: |"));
}

#[test]
fn inline_delimiter_emission() {
    assert_eq!(
        carve::to_carve("/i/ *b* _u_ ~s~ {^sup^} {,sub,} =mark= `code`\n"),
        "/i/ *b* _u_ ~s~ {^sup^} {,sub,} =mark= `code`\n"
    );
}

#[test]
fn literal_caret_escaped_literal_comma_unescaped() {
    // `^sup^` / `,sub,` are plain text (no bare sup/sub delimiter): the comma
    // needs no escape; the caret keeps one (footnote/caption channels).
    assert_eq!(
        carve::to_carve("^sup^ ,sub, stays literal\n"),
        "\\^sup\\^ ,sub, stays literal\n"
    );
}

#[test]
fn part11_minimal_escaping_spot_checks() {
    assert_eq!(
        carve::to_carve(
            "Carve is a \"post-Markdown\" language - it fixes it. 50% faster: yes (ok).\n"
        ),
        "Carve is a \"post-Markdown\" language - it fixes it. 50% faster: yes (ok).\n"
    );
    // The AUTHORED escapes survive - they are escaped_text nodes, and the
    // writer says the escape again. The trailing period is not one of them and
    // needs no escape, so it stays bare: this assertion used to expect `\.`
    // there, from the days when one authored escape escalated the whole
    // document to the conservative form (carve issue 350, carve#370 section 1).
    assert_eq!(
        carve::to_carve("Literal \\-\\- and \\.\\.\\. and \\\" must stay escaped.\n"),
        "Literal \\-\\- and \\.\\.\\. and \\\" must stay escaped.\n"
    );
    assert_eq!(
        carve::to_carve("^sup^ ,sub, stays literal. 50% ok (yes).\n"),
        "\\^sup\\^ ,sub, stays literal. 50% ok (yes).\n"
    );
}

// Verbatim content survives document normalization (carve-js issue 340):
// trailing whitespace and blank-line runs inside code blocks, raw blocks,
// frontmatter, and block comments are byte-exact after fmt.
#[test]
fn verbatim_content_survives_normalization() {
    for src in [
        "```\ntrailing   \nalso\t\t\n```\n",
        "```\na\n\n\n\nb\n```\n",
        "```=html\n<pre>x   \n\n\n\ny</pre>\n```\n",
        "%%%\nc   \n\n\n\nd\n%%%\n\nbody\n",
    ] {
        let formatted = carve::to_carve(src);
        assert_eq!(formatted, src);
        assert_eq!(carve::to_html(&formatted), carve::to_html(src));
    }
}

#[test]
fn comment_fence_opener_tail_survives_normalization() {
    let formatted = carve::to_carve("%%% TODO\nsecret\n%%% done\n");
    assert_eq!(formatted, "%%%\nTODO\nsecret\n%%%\n");
    assert_eq!(carve::to_html(&formatted), "");
}

#[test]
fn verbatim_content_stable_inside_containers() {
    for src in [
        "> ```\n> a   \n>\n>\n>\n> b\n> ```\n",
        "- item\n\n  ```\n  a   \n\n\n\n  b\n  ```\n",
    ] {
        let f1 = carve::to_carve(src);
        let f2 = carve::to_carve(&f1);
        assert_eq!(f1, f2);
        assert_eq!(carve::to_html(&f1), carve::to_html(src));
    }
}

// The list marker is semantic (§11): a sibling with a different bullet char
// or ordered delimiter starts a NEW list, so fmt preserves the authored
// marker (carve issue 286) - normalizing would merge adjacent sibling lists.
#[test]
fn preserves_authored_list_markers() {
    for src in [
        "1) a\n2) b\n",
        "1. a\n2. b\n",
        "* a\n* b\n",
        "- a\n- b\n",
        "* [x] done\n* [ ] todo\n",
    ] {
        assert_eq!(carve::to_carve(src), src);
    }
}

#[test]
fn adjacent_lists_separated_by_marker_stay_separate() {
    // fmt invariant: to_html(fmt(x)) == to_html(x). Before marker
    // preservation these merged into one list on re-parse.
    for src in ["1. a\n1) b", "1. a\n\n1) b", "- a\n* b", "- a\n\n* b"] {
        let f1 = carve::to_carve(src);
        assert_eq!(carve::to_carve(&f1), f1);
        assert_eq!(carve::to_html(&f1), carve::to_html(src));
    }
}

#[test]
fn all_space_verbatim_content_round_trips() {
    // A verbatim span whose content is entirely spaces must NOT be stripped by
    // the parser nor padded by the serializer. Padding it grew the span by two
    // spaces on every fmt pass, breaking both fmt guarantees. Covers the code
    // span, inline literal and math paths, which share one strip helper.
    for src in [
        "` `", "`  `", "`   `", "!` `", "!`  `", "!`   `", "$` x `", "$`  `", "``  ``", "!``  ``",
        "`a b`", "` a `",
    ] {
        let f1 = carve::to_carve(src);
        let f1 = f1.trim_end();
        // fmt(fmt(x)) == fmt(x)
        assert_eq!(
            carve::to_carve(f1).trim_end(),
            f1,
            "not idempotent: {src:?}"
        );
        // to_html(fmt(x)) == to_html(x)
        assert_eq!(
            carve::to_html(f1),
            carve::to_html(src),
            "invariant broken: {src:?}"
        );
    }
}

#[test]
fn all_space_verbatim_content_is_preserved_not_collapsed() {
    // The all-space guard matches the executable spec's codeText() and the
    // CommonMark rule ("...but does not consist entirely of space characters").
    assert!(carve::to_html("`  `").contains("<code>  </code>"));
    // A one-sided or non-all-space span still strips exactly one space per side.
    assert!(carve::to_html("` a `").contains("<code>a</code>"));
}

/// An AST built through the library API can nest far past the depth the parser
/// (and `from_json`) allow. `render_block` emits nothing past MAX_RENDER_DEPTH,
/// so a fence sized from those levels would be sized for output that never
/// appears.
#[test]
fn colon_container_fence_ignores_containers_past_the_render_cap() {
    use carve::ast::{BlockNode, Div, Paragraph};

    let mut node = BlockNode::Paragraph(Paragraph {
        attrs: None,
        children: Vec::new(),
        at_content_column: true,
        pos: None,
    });
    for _ in 0..1000 {
        node = BlockNode::Div(Div {
            attrs: None,
            label: None,
            children: vec![node],
            pos: None,
        });
    }

    let mut doc = carve::parse("x\n");
    doc.children = vec![node];
    let formatted = carve::render_carve(&doc);

    let widest = formatted
        .lines()
        .filter(|line| !line.is_empty() && line.chars().all(|c| c == ':'))
        .map(|line| line.len())
        .max()
        .unwrap_or(0);
    assert!(
        widest <= 202,
        "fence widened to {widest} colons past the render cap"
    );
}
