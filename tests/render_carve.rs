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
    // On a thread with room: the corpus holds a document nested to the parser's
    // cap (200 containers), and one debug-build frame per level overflows the
    // 2 MiB a test thread gets. The library handles the document - parse,
    // encode, decode, render and format all complete - so this buys the sweep,
    // not a behaviour change (carve-rs#530).
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(corpus_formatter_semantic_idempotent_and_reparseable_inner)
        .expect("thread spawns")
        .join()
        .expect("the sweep finishes");
}

/// The canonical published tree of a document, as a comparable string.
///
/// PART 11 §1's comparison, and the three things it forgives are the three that
/// are not facts about the document:
///
///  - `pos` and `srcByteLength`, which describe WHERE the source said it.
///  - escaping. §1 and §2 contradict each other otherwise: §2 writes an escape
///    if and only if omitting it would change the re-parse, so a document whose
///    canonical form needs one necessarily gains an `escaped_text` node its
///    source did not have. An escape that changes the DOCUMENT still fails,
///    because it changes the text value the run carries.
///  - run segmentation. Where an escape lands splits a text run in two, and how
///    a run is split is not a fact about the document either.
///
/// Nothing else is forgiven: a node appearing or vanishing, a construct
/// becoming a different construct, an attribute or a text value moving all
/// fail. That is the point - §2a's family is exactly the family that renders
/// alike and parses differently.
///
/// `attrs` IS NOT DESCENDED INTO. It holds named slots rather than nodes, and
/// an author controls its keys: a `keyValues` entry can be spelled `type`,
/// `pos` or `srcByteLength`, so walking it would rename or delete an ATTRIBUTE.
/// Attributes are content and compare verbatim. carve-php's
/// `CarveFmtCorpusTest::canonical()` states the same rule for the same reason.
/// The node a block's own content spells its wrapper away for under PART 11
/// SS1c, or `None` where the block keeps its wrapper.
///
/// STATED OVER WHAT THE SHAPE SPELLS, never over the node type - SS1c says so
/// in as many words, and the two shapes it names share no vocabulary: an
/// `image` is INLINE and a `comment` is BLOCK, so a rule written over types
/// would have had to name them one at a time and would not reach the next
/// one. A block whose content spells anything else - a second node beside
/// it, a text run, a no-break space - keeps its wrapper, and a writer that
/// dropped one of those still fails the assertion below.
///
/// THIS IS A NARROWING TO THE CONTRACT, NOT A SKIP. No document can satisfy
/// the unqualified form: corpus 411 ships `.fmt` sidecars recording the
/// bytes the writer produces for the indented spelling, because the writer
/// is right to decline it (SS1c: "the ceiling is uniform and not
/// positional"). Do not restore the unqualified comparison, and do not
/// reach for an allowlist instead - an entry would silence the comparison
/// for a whole document, where this states the one difference SS1c licenses
/// and keeps every other difference failing.
fn section_1c_content(
    block: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    if block.get("type")?.as_str()? != "paragraph" {
        return None;
    }
    // ATTRIBUTES ARE NOT CONTENT, and a wrapper carrying them cannot be
    // lost without losing them too - so a paragraph with an attribute block
    // keeps its wrapper here, and a writer that dropped one still fails.
    if block.get("attrs").is_some_and(|attrs| !attrs.is_null()) {
        return None;
    }
    let [only] = block.get("children")?.as_array()?.as_slice() else {
        return None;
    };
    matches!(only.get("type")?.as_str()?, "image" | "comment").then(|| only.clone())
}

/// THE NARROWING IS NARROW, pinned shape by shape rather than inferred from a
/// green sweep.
///
/// The sweep above cannot show this. Collapsing a wrapper in `canonical_tree`
/// is applied to BOTH sides, so widening it can only ever HIDE a difference,
/// never create one - a mutation that adds a node type to the match leaves the
/// sweep green whatever it adds, because both trees collapse the same way. So
/// the sweep passing says nothing about how wide the rule is, and the near
/// misses have to be asserted directly (markup-carve/carve-rs#1353).
///
/// Each `None` below is a shape PART 11 section 1c does not reach: "a block
/// whose content spells anything ELSE -- a second node beside it, a text run, a
/// NO-BREAK SPACE (section 7) -- keeps its wrapper and no ceiling is reached."
#[test]
fn the_section_1c_ceiling_reaches_two_spellings_and_no_others() {
    let block = |json: &str| -> Option<serde_json::Value> {
        let value: serde_json::Value = serde_json::from_str(json).expect("the fixture is JSON");
        section_1c_content(value.as_object().expect("the fixture is an object"))
    };
    let image = r#"{"type":"image","src":"u","alt":"a"}"#;

    // The two shapes the clause names, and they share no vocabulary: `image` is
    // INLINE and `comment` is BLOCK.
    assert_eq!(
        block(&format!(r#"{{"type":"paragraph","children":[{image}]}}"#)),
        Some(serde_json::from_str(image).unwrap())
    );
    assert!(block(r#"{"type":"paragraph","children":[{"type":"comment","value":"c"}]}"#).is_some());

    // A SECOND NODE BESIDE IT.
    assert_eq!(
        block(&format!(
            r#"{{"type":"paragraph","children":[{image},{{"type":"text","value":"x"}}]}}"#
        )),
        None
    );
    // A TEXT RUN, which is the widening that leaves the sweep green.
    assert_eq!(
        block(r#"{"type":"paragraph","children":[{"type":"text","value":"x"}]}"#),
        None
    );
    // NO CHILDREN AT ALL.
    assert_eq!(block(r#"{"type":"paragraph","children":[]}"#), None);
    // ATTRIBUTES ARE NOT CONTENT, so the wrapper carrying them is not lost -
    // dropping it would drop them, and that has to keep failing.
    assert_eq!(
        block(&format!(
            r#"{{"type":"paragraph","attrs":{{"classes":["k"]}},"children":[{image}]}}"#
        )),
        None
    );
    // NOT A PARAGRAPH. A quote holding one image keeps its wrapper: the clause
    // is about a spelling read back as a block opener of the CONTENT's kind,
    // and `> ![a](u)` reads back as the quote.
    assert_eq!(
        block(&format!(r#"{{"type":"block_quote","children":[{image}]}}"#)),
        None
    );
}

fn canonical_tree(source: &str, dissolve_section_1c_wrappers: bool) -> String {
    fn canonical(
        value: &serde_json::Value,
        dissolve_section_1c_wrappers: bool,
    ) -> serde_json::Value {
        match value {
            serde_json::Value::Array(items) => {
                let mut out: Vec<serde_json::Value> = Vec::with_capacity(items.len());
                for item in items {
                    let item = canonical(item, dissolve_section_1c_wrappers);
                    if let Some(merged) = out.last_mut().and_then(|last| merge_text(last, &item)) {
                        *out.last_mut().expect("checked above") = merged;
                        continue;
                    }
                    out.push(item);
                }
                serde_json::Value::Array(out)
            }
            serde_json::Value::Object(fields) => {
                // serde_json's Map is a BTreeMap here (the crate is taken
                // without `preserve_order`), so key order is already the sorted
                // order §1's comparison asks for.
                let mut out = serde_json::Map::new();
                for (key, child) in fields {
                    if key == "pos" || key == "srcByteLength" {
                        continue;
                    }
                    if key == "type" && child.as_str() == Some("escaped_text") {
                        out.insert(key.clone(), serde_json::Value::String("text".to_string()));
                        continue;
                    }
                    if key == "attrs" {
                        out.insert(key.clone(), child.clone());
                        continue;
                    }
                    out.insert(key.clone(), canonical(child, dissolve_section_1c_wrappers));
                }
                // PART 11 SS1c, applied to BOTH sides rather than allowlisted
                // on one. The clause is NORMATIVE and says the wrapper is LOST:
                // where a block's whole content is a single node whose own
                // spelling at the block's own column reads back as a block
                // opener of that node's kind, the writer emits that spelling
                // and `parse(fmt(x)) == parse(x)` is UNATTAINABLE. So the two
                // trees have one spelling between them, and comparing them
                // as though they had two asks for a fixed point no conforming
                // writer has (markup-carve/carve#1658, markup-carve/carve#1672).
                if dissolve_section_1c_wrappers {
                    if let Some(content) = section_1c_content(&out) {
                        return content;
                    }
                }
                serde_json::Value::Object(out)
            }
            other => other.clone(),
        }
    }

    /// Two adjacent bare text runs joined into one, or `None` when they are not
    /// two adjacent bare text runs. A run carrying anything beyond `type` and
    /// `value` is left alone: whatever else it holds would be silently dropped
    /// by the join.
    fn merge_text(last: &serde_json::Value, next: &serde_json::Value) -> Option<serde_json::Value> {
        let (last, next) = (last.as_object()?, next.as_object()?);
        for run in [&last, &next] {
            if run.len() != 2 || !run.contains_key("type") || !run.contains_key("value") {
                return None;
            }
            if run.get("type")?.as_str()? != "text" {
                return None;
            }
        }
        let joined = format!(
            "{}{}",
            last.get("value")?.as_str()?,
            next.get("value")?.as_str()?
        );
        let mut out = serde_json::Map::new();
        out.insert(
            "type".to_string(),
            serde_json::Value::String("text".to_string()),
        );
        out.insert("value".to_string(), serde_json::Value::String(joined));
        Some(serde_json::Value::Object(out))
    }

    let json = carve::to_json(&carve::parse(source));
    // The corpus holds a document nested to the parser's cap (200 containers),
    // which is past serde_json's default 128-level recursion limit - so the
    // reader that has to accept this crate's own output cannot be the default
    // one. The sweep already runs on a 32 MiB stack for the same document.
    let mut reader = serde_json::Deserializer::from_str(&json);
    reader.disable_recursion_limit();
    // Through the stream reader rather than `Value::deserialize`, which would
    // need `serde` itself in scope - serde_json is the only JSON dev-dependency
    // this crate carries, and one document is no reason to add another.
    let encoded = reader
        .into_iter::<serde_json::Value>()
        .next()
        .expect("the encoder emits one document")
        .expect("this crate's own AST encoder emits JSON");
    canonical(&encoded, dissolve_section_1c_wrappers).to_string()
}

fn corpus_formatter_semantic_idempotent_and_reparseable_inner() {
    for (slug, source) in corpus_sources() {
        let formatted = carve::to_carve(&source);
        // UNCONDITIONALLY, over every document. This assertion used to be
        // skipped whenever the formatted output CONTAINED one of three literal
        // indent patterns, plus two named slugs. All five were dead - the
        // patterns matched 0 of the 1370 documents and neither slug named one -
        // and the content-shaped three were the worse half: they are matched
        // against output nobody has seen yet, so the first document whose
        // formatted form happened to hold an indented bullet after a blank line
        // would have dropped out of the sweep with nothing reporting it
        // (markup-carve/carve-rs#1278).
        assert_eq!(
            carve::to_html(&formatted),
            carve::to_html(&source),
            "formatted corpus source changed HTML semantics for {slug}"
        );
        assert_eq!(
            carve::to_carve(&formatted),
            formatted,
            "formatted corpus source is not idempotent for {slug}"
        );
        // PART 11 §1'S OWN INVARIANT, WHICH NEITHER OF THE ABOVE CAN SEE.
        // §1a says it outright: `to_html(fmt(x)) == to_html(x)` is "strictly
        // weaker" and "a writer satisfying only the HTML form still fails this
        // section". Two spellings that render alike are still two spellings.
        //
        // It is asserted LAST because it is the strongest of the three, and
        // over the same 1370 documents with no allowlist - carve-php's
        // `CarveFmtCorpusTest::testTheFormattedDocumentParsesToTheSameTree`
        // manages without one. A tree-changing document must publish a `.fmt`
        // sidecar below, so the corpus itself declares the exemption without
        // an engine-local allowlist that could silently outlive the fixture.
        //
        // NARROWED TO PART 11 SS1c, which is normative and names the one place
        // this equality cannot hold: a wrapper its own content spells away is
        // LOST, and `parse(fmt(x)) == parse(x)` is unattainable for such a
        // document. The strict comparison discovers those documents first;
        // `canonical_tree(..., true)` then states that ceiling on both sides
        // rather than exempting a document from the sweep. See
        // `section_1c_content` above for why it is a narrowing and not a skip.
        //
        // COMPARING THE PARSE, NOT THE RENDER. An HTML comparison in this spot
        // would be a check that cannot fail: it is the assertion three lines
        // up. Every one of the eight documents this caught rendered
        // byte-identical HTML, which is why every gate this crate ran was green
        // on all eight (markup-carve/carve-rs#1277).
        let source_tree = canonical_tree(&source, false);
        let formatted_tree = canonical_tree(&formatted, false);
        if source_tree != formatted_tree {
            // A tree-changing canonical form is never inferred locally. The
            // corpus must publish it, or this sweep is what reports the missing
            // fixture that the byte-only `.fmt` sweep cannot discover.
            let sidecar = corpus_dir().join(format!("{slug}.fmt"));
            let expected = fs::read_to_string(&sidecar).unwrap_or_else(|error| {
                panic!(
                    "parse(fmt(x)) != parse(x) for {slug}, but the corpus has no readable {}: {error}",
                    sidecar.display()
                )
            });

            // The fixture is an assertion, not an allowlist. Reusing the
            // shape-bounded §1c predicate above permits only the one wrapper
            // dissolution the contract names; every other difference remains
            // visible. `canonical_tree` recursively walks every object and
            // array-valued slot, including list items and table rows/cells.
            assert_eq!(
                canonical_tree(&source, true),
                canonical_tree(&formatted, true),
                "the tree difference for {slug} is more than a PART 11 §1c wrapper loss"
            );
            assert_eq!(
                formatted_tree,
                canonical_tree(&expected, false),
                "fmt(source) does not parse to the pinned canonical tree for {slug}"
            );
        }
    }
}

#[test]
fn blank_line_collapse() {
    assert_eq!(carve::to_carve("a\n\n\n\nb\n"), "a\n\nb\n");
}

#[test]
fn empty_footnote_definition_uses_the_empty_sentinel() {
    let source = "See[^f]\n\n[^f]: {empty}\n";
    assert_eq!(carve::to_carve(source), source);
    assert_eq!(
        carve::to_html(&carve::to_carve(source)),
        carve::to_html(source)
    );
}

#[test]
fn bullet_marker_normalization() {
    let doc = carve::Document {
        frontmatter: Default::default(),
        frontmatter_raw: None,
        footnote_defs: Default::default(),
        footnote_def_pos: Default::default(),
        children: vec![carve::BlockNode::List(carve::List {
            attrs: None,
            ordered: false,
            start: None,
            ol_type: None,
            bare_marker: false,
            delim: None,
            bullet_char: None,
            tight: true,
            pos: None,
            items: vec![carve::ListItem {
                attrs: None,
                checked: None,
                task_state: None,
                children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
                    attrs: None,
                    children: vec![carve::InlineNode::text("a".to_string())],
                    ..Default::default()
                })],
                pos: None,
            }],
        })],
        source_len: 0,
        ingest_payload_len: 0,
    };
    assert_eq!(
        carve::render_carve(&doc).expect("the tree under test is within the render ceiling"),
        "- a\n"
    );
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
fn a_literal_caret_and_comma_both_stay_unescaped() {
    // `^sup^` / `,sub,` are plain text: superscript and subscript are
    // braced-only, so neither delimiter opens anything.
    //
    // This test used to expect `\^sup\^`, on the grounds that the caret "keeps
    // one (footnote/caption channels)". Neither channel is open here: an inline
    // footnote is `^[`, and a caption marker is `^` plus a SPACE at the start of
    // a block line. carve-js and carve-php both write this bare, and PART 11 §4
    // asks for the minimal form when dropping the escape changes nothing - it
    // changes nothing, in all three engines (carve-rs#555).
    assert_eq!(
        carve::to_carve("^sup^ ,sub, stays literal\n"),
        "^sup^ ,sub, stays literal\n"
    );
}

#[test]
fn a_caret_after_an_unresolved_image_needs_no_escape() {
    // `[nope]` resolves to nothing, so the image is not an image and the
    // caption line promotes nothing - the bare caret changes no parse, and
    // PART 11 §4 wants it bare. The escape here was forced unconditionally by
    // carve-rs#558 (mine); the three-step choice in `render_carve` tells this
    // case from the one below by ASKING THE PARSER rather than by position
    // (carve-rs#559).
    assert_eq!(
        carve::to_carve("![a][nope]\n^ cap\n"),
        "![a][nope]\n^ cap\n"
    );
}

#[test]
fn a_caption_marker_in_literal_text_keeps_its_escape() {
    // The one shape where a line-initial caret IS dangerous: `^` + space at the
    // start of a block line is a caption marker, so the escape is load-bearing
    // and stays whatever the mode.
    assert_eq!(
        carve::to_carve("![Apollo](a.jpg)\n\\^ Figure 1: moon\n"),
        "![Apollo](a.jpg)\n\\^ Figure 1: moon\n"
    );
    // Inside a table cell the same characters cannot open a caption - a cell's
    // content is not a block line - so nothing is forced there.
    assert_eq!(
        carve::to_carve("| Value |\n| ^2^ |\n"),
        "| Value |\n| ^2^ |\n"
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
        "^sup^ ,sub, stays literal. 50% ok (yes).\n"
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

/// A container inside a blockquote, a list item or a definition body writes its
/// fence lines with that host's prefix or indent, so they cannot close an
/// ancestor fence. Widening for them would only make the source noisier.
#[test]
fn colon_container_fence_ignores_containers_behind_a_prefix() {
    for source in [
        "::: outer\n\n- item\n\n  ::: inner\n  x\n  :::\n\n:::\n",
        "::: outer\n\n> ::: inner\n> x\n> :::\n\n:::\n",
        "::: outer\n\n:: term\n:  ::: inner\n   x\n   :::\n\n:::\n",
    ] {
        let formatted = carve::to_carve(source);
        assert_eq!(carve::to_html(&formatted), carve::to_html(source));
        assert!(
            formatted.starts_with("::: outer"),
            "fence widened needlessly: {formatted}"
        );
    }
}

/// An AST built through the library API can nest far past the depth the parser
/// (and `from_json`) allow. `render_block` emits nothing past MAX_RENDER_DEPTH,
/// so a fence sized from those levels would be sized for output that never
/// appears.
#[test]
fn colon_container_fence_ignores_containers_past_the_render_cap() {
    on_big_stack(|| {
        use carve::ast::{BlockNode, Div, Paragraph};

        let mut node = BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: Vec::new(),
            at_content_column: true,
            block_image: false,
            pos: None,
        });
        // Just under the ceiling: past it the writer refuses outright, and the
        // property under test here is the FENCE WIDTH a bounded writer emits,
        // which needs output to inspect.
        for _ in 0..carve::MAX_RENDER_DEPTH - 1 {
            node = BlockNode::Div(Div {
                attrs: None,
                label: None,
                children: vec![node],
                pos: None,
            });
        }

        let mut doc = carve::parse("x\n");
        doc.children = vec![node];
        let formatted = carve::render_carve(&doc).expect("a tree below the ceiling renders");

        let widest = formatted
            .lines()
            .filter(|line| !line.is_empty() && line.chars().all(|c| c == ':'))
            .map(|line| line.len())
            .max()
            .unwrap_or(0);
        // Derived, not pinned: the outermost fence is `:::` and each level inward
        // adds a colon, so the widest a bounded writer can emit is fixed by the cap
        // itself. Writing the number out made this test track the old cap rather
        // than the rule (issue 517).
        let bound = 3 + carve::MAX_RENDER_DEPTH - 1;
        assert!(
            widest <= bound,
            "fence widened to {widest} colons past the render cap"
        );
    });
}

/// A document nested at exactly the parser's cap parses fine, and every target
/// used to lose its innermost block: the writer's own bound was the parser's
/// number, and the plain / markdown / ansi renderers used half of it, so the
/// same document rendered with content in HTML and without it everywhere else
/// (issue 517).
/// Run on a generous stack, the way the other worst-case-depth probes in this
/// crate do (`recursion_and_panics.rs`). A test thread gets 2 MiB by default,
/// and a debug build's un-inlined frames put these depths over it - the render
/// walk, and the recursive Drop of the tree afterwards. Frame size also varies
/// by toolchain: raising the render bound pushed the fence-width probe over on
/// Rust 1.75 while stable still passed. The property under test is which
/// content survives, not the per-frame size, so the depth probes get room.
fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker must return, not abort");
}

#[test]
fn every_target_keeps_the_innermost_content_at_the_parser_cap() {
    on_big_stack(|| {
        // 200 is the parser's MAX_NESTING_DEPTH; it is not public, and the point of
        // this test is that no renderer bound may sit at or below it.
        let src = "::: note\n".repeat(200) + "body\n";
        let doc = carve::parse(&src);

        for (target, out) in [
            (
                "html",
                carve::render_html(&doc).expect("the tree under test is within the render ceiling"),
            ),
            (
                "markdown",
                carve::render_markdown(&doc)
                    .expect("the tree under test is within the render ceiling"),
            ),
            (
                "plain",
                carve::render_plain_text(&doc)
                    .expect("the tree under test is within the render ceiling"),
            ),
            (
                "ansi",
                carve::render_ansi(&doc).expect("the tree under test is within the render ceiling"),
            ),
            (
                "carve",
                carve::render_carve(&doc)
                    .expect("the tree under test is within the render ceiling"),
            ),
        ] {
            assert!(
                out.contains("body"),
                "{target} dropped the innermost content"
            );
        }

        // PART 11: the canonical writer preserves meaning.
        let written =
            carve::render_carve(&doc).expect("the tree under test is within the render ceiling");
        assert_eq!(
            carve::render_html(&carve::parse(&written))
                .expect("the tree under test is within the render ceiling"),
            carve::render_html(&doc).expect("the tree under test is within the render ceiling")
        );
    });
}

/// Raising a bound must not retire it. An AST that did not come from the parser
/// can nest without limit, so the guard still has to truncate rather than
/// overflow the stack, and truncate at the same point regardless of how much
/// deeper the input goes.
#[test]
fn the_render_cap_still_bounds_a_hand_built_ast() {
    on_big_stack(|| {
        use carve::ast::{BlockNode, Div, Paragraph, Text};

        let build = |depth: usize| {
            let mut node = BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: vec![carve::ast::InlineNode::Text(Text {
                    value: "body".to_string(),
                    pos: None,
                })],
                at_content_column: true,
                block_image: false,
                pos: None,
            });
            for _ in 0..depth {
                node = BlockNode::Div(Div {
                    attrs: None,
                    label: None,
                    children: vec![node],
                    pos: None,
                });
            }
            let mut doc = carve::parse("x\n");
            doc.children = vec![node];
            doc
        };

        let under = carve::render_carve(&build(carve::MAX_RENDER_DEPTH - 2))
            .expect("a tree below the ceiling renders");
        assert!(under.contains("body"), "truncated below the cap");

        // Past the cap the writer REFUSES rather than returning a document with
        // its body deleted (PART 9 §25, carve-rs#511 item 5). The depths are
        // derived from the constant so this tracks the rule, not a number; the
        // larger one stays modest because a deeply nested AST overflows the
        // stack on its recursive Drop long before any renderer sees it.
        for depth in [carve::MAX_RENDER_DEPTH + 1, carve::MAX_RENDER_DEPTH + 200] {
            let err = carve::render_carve(&build(depth))
                .expect_err("past the ceiling the canonical writer refuses");
            let carve::RenderCarveError::Depth(err) = err else {
                panic!("the ceiling must return a depth refusal");
            };
            assert_eq!(err.renderer(), "carve");
            assert_eq!(err.limit(), carve::MAX_RENDER_DEPTH);
        }
    });
}

#[test]
fn a_literal_marker_escapes_only_the_character_that_opens_it() {
    // PART 11 §4: escape what would change the parse, and nothing else. A `:`
    // opens a definition marker or a colon fence only at the START of a line,
    // so the first colon of a run carries the escape and the rest cannot open
    // anything. This engine escaped the whole class - `\:\:\:` for a literal
    // `:::` - which is what carve-rs#566 is about; carve-js and carve-php both
    // write one escape.
    //
    // THREE OF THESE EXPECTATIONS CARRIED AN EXTRA ESCAPE UNTIL PART 11 §2b,
    // and every one of them was in a DIFFERENT BLOCK from the one that needed
    // an escape. §4's vote used to take the conservative form for the whole
    // document, so `A box.` came back `A box\.`, `:  def` came back `\:  def`
    // and `Read [intro][x].` came back `Read [intro][x]\.` - none of which
    // changes what those blocks re-parse to. §2b bounds the fallback to the
    // unit that fails, so each of those three is written bare now while the
    // marker that does open something keeps its escape. That is the same
    // reading this test already applied WITHIN a run - one escape, not the
    // whole class - applied across blocks as well.
    //
    // The escapes that remain are the load-bearing ones, and they are what this
    // test is for: they must not move. carve-js and carve-php write these three
    // documents identically (markup-carve/carve-js#1307,
    // markup-carve/carve-php#1560).
    assert_eq!(
        carve::to_carve(" :::\n A box.\n :::\n"),
        "\\:::\nA box.\n\\:::\n"
    );
    assert_eq!(
        carve::to_carve("- one\n :: term\n :  def\n"),
        "- one\n  \\:: term\n  :  def\n"
    );
    // A colon that is not at a line start opens nothing, escaped or not - here
    // the `[` escape is what keeps the definition from forming, and it is the
    // ONLY one: PART 11 §2 decides per opener occurrence, a definition opens on
    // its `[`, and the closing bracket and the slash after it open nothing on
    // their own (markup-carve/carve#1533).
    assert_eq!(
        carve::to_carve(" Read [intro][x].\n\n [x]: /intro \"T\"\n"),
        "Read [intro][x].\n\n\\[x]: /intro \"T\"\n"
    );
}

// ---------------------------------------------------------------------------
// I1: `carve fmt` preserves include directives
// ---------------------------------------------------------------------------

/// The core has no directive node - `{{ path }}` is plain TEXT (I1) - so the
/// serializer used to escape it like any other punctuation-bearing text and
/// write `\{\{ path \}\}`.
///
/// The formatter's existing invariant, `to_html(fmt(x)) == to_html(x)`, cannot
/// see that: the escaped form renders as the very same literal text, so the
/// suite stayed green while every include in a formatted document was
/// destroyed. Nothing looks broken until a resolver runs and the chapters have
/// silently vanished.
///
/// These tests therefore assert the INTENT instead: EXPANDING the formatted
/// document must give the same result - same HTML and same dependency set - as
/// expanding the original.
mod include_directives_survive_formatting {
    use carve::{expand_includes, parse, render_html, IncludeOptions, IncludeResolved};

    const FILES: &[(&str, &str)] = &[
        ("chapter.crv", "# Chapter\n\nBody."),
        ("my chapter.crv", "Spaced body."),
        (
            "book.crv",
            "# A\n\nskip\n\n{#intro}\n# Intro\n\nIntro body.",
        ),
        ("lines.crv", "skip\nOne\nTwo\nskip"),
        ("shift.crv", "# Shifted"),
        ("inline.crv", "inline body"),
        // Two addressable sections, for directives that must BOTH survive in
        // one run while selecting different parts of the same file.
        (
            "two.crv",
            "{#intro}\n# Intro\n\nIntro body.\n\n{#outro}\n# Outro\n\nOutro body.",
        ),
        ("x.crv", "XXX"),
        ("y.crv", "YYY"),
        ("z.crv", "ZZZ"),
    ];

    struct Expansion {
        html: String,
        dependencies: Vec<(String, bool)>,
        rules: Vec<String>,
    }

    fn expand(source: &str) -> Expansion {
        let resolver = |path: &str, _ctx: &carve::IncludeContext<'_>| {
            FILES
                .iter()
                .find(|(name, _)| *name == path)
                .map(|(_, body)| IncludeResolved::from(*body))
        };
        let opts = IncludeOptions::new().with_resolver(&resolver);
        let result = expand_includes(parse(source), source, &opts);
        Expansion {
            html: render_html(&result.doc).unwrap(),
            dependencies: result
                .dependencies
                .iter()
                .map(|d| (d.id.clone(), d.resolved))
                .collect(),
            rules: result.warnings.iter().map(|w| w.rule.clone()).collect(),
        }
    }

    /// The real assertion: formatting must not change what the document
    /// INCLUDES, which the round-trip invariant could never express.
    fn assert_survives(source: &str) {
        let formatted = carve::to_carve(source);
        let before = expand(source);
        let after = expand(&formatted);
        assert_eq!(
            after.html, before.html,
            "formatting changed the expanded output of {source:?} (formatted: {formatted:?})"
        );
        assert_eq!(
            after.dependencies, before.dependencies,
            "formatting changed the dependency set of {source:?} (formatted: {formatted:?})"
        );
        // A directive that is preserved but whose DIAGNOSTICS are lost is
        // still a regression: the warning is the only thing that tells the
        // author their typo is a typo.
        assert_eq!(
            after.rules, before.rules,
            "formatting changed the include warnings of {source:?} (formatted: {formatted:?})"
        );
        assert_eq!(
            carve::to_carve(&formatted),
            formatted,
            "formatting {source:?} is not idempotent"
        );
    }

    #[test]
    fn bare_path_is_preserved() {
        assert_eq!(
            carve::to_carve("{{ chapter.crv }}\n"),
            "{{ chapter.crv }}\n"
        );
        assert_survives("{{ chapter.crv }}\n");
    }

    #[test]
    fn quoted_path_with_spaces_is_preserved() {
        // Also pins that the path is NOT run through smart typography, which
        // would curl the quotes into a path naming a different file.
        assert_eq!(
            carve::to_carve("{{ \"my chapter.crv\" }}\n"),
            "{{ \"my chapter.crv\" }}\n"
        );
        assert_survives("{{ \"my chapter.crv\" }}\n");
    }

    #[test]
    fn section_selection_is_preserved() {
        // `#intro` arrives as a Tag node, so recognition has to reassemble the
        // run exactly as expansion does (I9a).
        assert_eq!(
            carve::to_carve("{{ book.crv #intro }}\n"),
            "{{ book.crv #intro }}\n"
        );
        assert_survives("{{ book.crv #intro }}\n");
    }

    #[test]
    fn line_range_option_is_preserved() {
        assert_eq!(
            carve::to_carve("{{ lines.crv @lines:2-3 }}\n"),
            "{{ lines.crv @lines:2-3 }}\n"
        );
        assert_survives("{{ lines.crv @lines:2-3 }}\n");
    }

    #[test]
    fn shift_options_are_preserved() {
        // `@shift:2` arrives as a Mention node.
        assert_eq!(
            carve::to_carve("{{ shift.crv @shift:2 }}\n"),
            "{{ shift.crv @shift:2 }}\n"
        );
        assert_survives("{{ shift.crv @shift:2 }}\n");
        assert_eq!(
            carve::to_carve("{{ shift.crv @shift:auto }}\n"),
            "{{ shift.crv @shift:auto }}\n"
        );
        assert_survives("# Top\n\n{{ shift.crv @shift:auto }}\n");
    }

    #[test]
    fn inline_directive_within_a_sentence_is_preserved() {
        assert_eq!(
            carve::to_carve("See {{ inline.crv }} now.\n"),
            "See {{ inline.crv }} now.\n"
        );
        assert_survives("See {{ inline.crv }} now.\n");
    }

    #[test]
    fn directives_in_code_are_left_exactly_as_the_core_produces_them() {
        // I9: code is verbatim and the directive is inert there, so the
        // serializer must not treat it specially - the code paths already emit
        // their content raw, and that must not regress into unescaping.
        assert_eq!(
            carve::to_carve("`{{ chapter.crv }}`\n"),
            "`{{ chapter.crv }}`\n"
        );
        assert_eq!(
            carve::to_carve("```txt\n{{ chapter.crv }}\n```\n"),
            "```txt\n{{ chapter.crv }}\n```\n"
        );
        // And they stay inert after formatting.
        assert_survives("`{{ chapter.crv }}`\n");
        assert_survives("```txt\n{{ chapter.crv }}\n```\n");
    }

    #[test]
    fn runs_that_are_not_shape_well_formed_are_left_to_the_ordinary_writer() {
        // None of these is a directive: no closing `}}`, no path token, or a
        // section and options with nothing to include. So none is PRESERVED -
        // each takes the ordinary escaping path.
        //
        // That path is MINIMAL now: the writer re-parses what it wrote and
        // escapes only where the round trip needs it, so a brace run that reads
        // back as the same text is emitted bare. What this case is really
        // about is that the run is not treated as a directive, which the round
        // trip below is the evidence for - the escaping itself proves nothing
        // either way.
        assert_eq!(carve::to_carve("{{ oops\n"), "{{ oops\n");
        assert_eq!(carve::to_carve("{{ }}\n"), "{{ }}\n");
        assert_eq!(carve::to_carve("{{ #intro }}\n"), "{{ #intro }}\n");
        assert_eq!(carve::to_carve("{{ @lines:2-4 }}\n"), "{{ @lines:2-4 }}\n");
        // The quoted form can spell an empty or whitespace-only path where the
        // bare form cannot; neither is a directive.
        //
        // This case used to prove they took the ordinary-text path by their
        // quotes coming back CURLED. That proof is gone: the Carve writer emits
        // smart punctuation as the author's own run now, so nothing is curled
        // on this target whichever path it took. The round trip below is the
        // evidence instead.
        assert_eq!(carve::to_carve("{{ \"\" }}\n"), "{{ \"\" }}\n");
        assert_eq!(carve::to_carve("{{ \"   \" }}\n"), "{{ \"   \" }}\n");
        assert_survives("{{ oops\n");
        assert_survives("{{ }}\n");
        assert_survives("{{ #intro }}\n");
        assert_survives("{{ @lines:2-4 }}\n");
        assert_survives("{{ \"\" }}\n");
        assert_survives("{{ \"   \" }}\n");
        // The empty token between two real directives is escaped without
        // taking its neighbors: both of those still expand.
        assert_survives("a {{ x.crv }} b {{ \"\" }} c {{ y.crv }} d\n");
    }

    /// Preservation is scoped to SHAPE, not validity (spec I1).
    ///
    /// `@bogus:1` is an invalid option, but the run is unmistakably a
    /// directive: it opens, closes, and names a path. Escaping it would freeze
    /// a one-character typo into permanent literal text AND silently drop the
    /// `include-unknown-option` warning that names the mistake - turning a
    /// fixable error into prose that merely looks like an error. Option
    /// validity is an expansion-time DIAGNOSTIC, never a preservation gate.
    #[test]
    fn a_shaped_directive_with_an_invalid_option_is_preserved_with_its_warning() {
        assert_eq!(
            carve::to_carve("{{ chapter.crv @bogus:1 }}\n"),
            "{{ chapter.crv @bogus:1 }}\n"
        );
        assert_eq!(
            expand("{{ chapter.crv @bogus:1 }}\n").rules,
            ["include-unknown-option"]
        );
        assert_survives("{{ chapter.crv @bogus:1 }}\n");
    }

    /// Same rule for the other selector: a `#section` the target does not have
    /// is a diagnostic (`include-section`), so the directive round-trips and
    /// the warning still fires on the formatted document.
    #[test]
    fn a_shaped_directive_with_a_missing_section_is_preserved_with_its_warning() {
        assert_eq!(
            carve::to_carve("{{ book.crv #nope }}\n"),
            "{{ book.crv #nope }}\n"
        );
        assert_eq!(expand("{{ book.crv #nope }}\n").rules, ["include-section"]);
        assert_survives("{{ book.crv #nope }}\n");
    }

    // -----------------------------------------------------------------------
    // More than ONE directive per run
    // -----------------------------------------------------------------------

    /// Every fixture above uses a SINGLE directive, which is exactly how this
    /// bug class stays invisible: an implementation that preserves the first
    /// directive and escapes the rest of the run passes all of them. carve-php
    /// had precisely that defect - it escaped the remainder wholesale instead
    /// of rescanning it - so `a {{ x }} b {{ y }} c` silently lost `{{ y }}`
    /// with two FULLY VALID directives.
    ///
    /// rs rescans (`split_run_directives` advances a cursor past each match and
    /// keeps looking), so it is correct here. These pin that property rather
    /// than trusting the loop to stay that way. Verified to be load-bearing:
    /// with the rescan disabled all four tests below fail while every
    /// single-directive test above still passes.
    #[test]
    fn every_directive_in_a_run_is_preserved_not_just_the_first() {
        for source in [
            // Two, three, four - prose between each.
            "a {{ x.crv }} b {{ y.crv }} c\n",
            "a {{ x.crv }} b {{ y.crv }} c {{ z.crv }} d\n",
            "{{ x.crv }} a {{ y.crv }} b {{ z.crv }} c {{ chapter.crv }}\n",
            // No prose between them at all.
            "{{ x.crv }}{{ y.crv }}\n",
            "{{ x.crv }} {{ y.crv }}\n",
            // First and last position in the run, nothing outside them.
            "{{ x.crv }} middle prose {{ y.crv }}\n",
        ] {
            assert_eq!(carve::to_carve(source), source, "not preserved: {source:?}");
            assert_survives(source);
        }
    }

    /// The highest-risk shape: `#section` parses to a Tag and `@option` to a
    /// Mention, so these runs are Text+Tag+Mention+Text sequences rather than
    /// one text node. A per-text-node scan finds none of them, and a scan that
    /// stops after the first match keeps only the leading one.
    #[test]
    fn multiple_sectioned_and_optioned_directives_all_survive_one_run() {
        for source in [
            "a {{ two.crv #intro }} b {{ two.crv #outro }} c\n",
            "a {{ lines.crv @lines:2-3 }} b {{ shift.crv @shift:2 }} c\n",
            "a {{ two.crv #intro }} b {{ shift.crv @shift:auto }} c {{ lines.crv @lines:1-2 }} d\n",
            "{{ two.crv #intro @shift:1 }} x {{ two.crv #outro @shift:2 }}\n",
        ] {
            assert_eq!(carve::to_carve(source), source, "not preserved: {source:?}");
            assert_survives(source);
        }
    }

    /// A run that mixes preservable and non-preservable directives must handle
    /// each on its own merits: the invalid one is escaped as ordinary text and
    /// the valid ones AROUND it - including the ones AFTER it - still survive.
    /// An implementation that bailed out of the run at the first non-match
    /// would silently drop everything downstream.
    #[test]
    fn an_unpreservable_directive_does_not_take_the_rest_of_the_run_with_it() {
        // Empty path in the middle: escaped, both neighbors intact.
        assert_eq!(
            carve::to_carve("a {{ x.crv }} b {{ }} c {{ y.crv }} d\n"),
            "a {{ x.crv }} b {{ }} c {{ y.crv }} d\n"
        );
        assert_survives("a {{ x.crv }} b {{ }} c {{ y.crv }} d\n");

        // Unclosed opener trailing a valid directive: only the opener escapes.
        assert_eq!(
            carve::to_carve("a {{ x.crv }} b {{ oops\n"),
            "a {{ x.crv }} b {{ oops\n"
        );
        assert_survives("a {{ x.crv }} b {{ oops\n");

        // A merely mis-OPTIONED directive is shape-well-formed, so all three
        // are preserved and the middle one keeps its warning.
        let mixed = "a {{ x.crv }} b {{ z.crv @bogus:1 }} c {{ y.crv }} d\n";
        assert_eq!(carve::to_carve(mixed), mixed);
        assert_eq!(expand(mixed).rules, ["include-unknown-option"]);
        assert_survives(mixed);
    }

    /// Directives spread across blocks, and across block vs inline position,
    /// are independent of each other.
    #[test]
    fn directives_survive_across_separate_blocks_and_mixed_positions() {
        for source in [
            "{{ x.crv }}\n\n{{ y.crv }}\n\n{{ z.crv }}\n",
            "{{ x.crv }}\n\nprose {{ y.crv }} prose\n",
            "prose {{ x.crv }} prose\n\n{{ y.crv }}\n",
            "- a {{ x.crv }} b {{ y.crv }} c\n",
            "> a {{ x.crv }} b {{ y.crv }} c\n",
            "# a {{ x.crv }} b {{ y.crv }}\n",
        ] {
            assert_eq!(carve::to_carve(source), source, "not preserved: {source:?}");
            assert_survives(source);
        }
    }

    #[test]
    fn preserved_directives_are_idempotent_under_repeated_formatting() {
        for source in [
            "{{ chapter.crv }}\n",
            "{{ \"my chapter.crv\" }}\n",
            "{{ book.crv #intro }}\n",
            "{{ lines.crv @lines:2-3 }}\n",
            "{{ shift.crv @shift:auto }}\n",
            "See {{ inline.crv }} now.\n",
            // Multi-directive runs must be idempotent too: a formatter that
            // re-escaped on the second pass would corrupt an already-clean file.
            "a {{ x.crv }} b {{ y.crv }} c {{ z.crv }} d\n",
            "a {{ two.crv #intro }} b {{ shift.crv @shift:2 }} c\n",
            "a {{ x.crv }} b {{ }} c {{ y.crv }} d\n",
        ] {
            let once = carve::to_carve(source);
            assert_eq!(carve::to_carve(&once), once, "not idempotent: {source:?}");
        }
    }
}
