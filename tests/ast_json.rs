use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus")
}

fn corpus_sources() -> Vec<PathBuf> {
    let dir = corpus_dir();
    if !dir.exists() {
        panic!(
            "Spec corpus not found at {}.\n\
             Did you initialize the submodule?\n  git submodule update --init",
            dir.display()
        );
    }
    let mut paths = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension().and_then(|s| s.to_str()) == Some("crv")).then_some(path)
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn parse_with_positions(source: &str) -> carve::Document {
    carve::parse_with_options(source, &carve::Options::new().with_positions(true))
}

fn run(args: &[&str], input: &str) -> (bool, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_carve"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn carve binary");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait carve binary");
    (
        out.status.success(),
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        String::from_utf8(out.stderr).expect("utf8 stderr"),
    )
}

/// Clear the one field a decoded document cannot restore.
///
/// `Paragraph::at_content_column` is parse-internal: it records whether the
/// paragraph's first line sat at its container's content column, it is not on
/// the wire (PART 12 §3 - the reference shape has no such field), and its only
/// reader is the image-figure promotion, which runs during parsing. So a
/// decoded tree carries the default and a parsed one carries what the source
/// said.
///
/// Cleared on BOTH sides rather than compared, and cleared HERE rather than by
/// weakening `PartialEq` on the node: equality is the crate's, used by every
/// other test, and a type whose `eq` quietly ignores a field stops reporting a
/// difference everywhere - including where that field is what a test is about.
fn normalize(mut doc: carve::Document) -> carve::Document {
    /// `from_heading_reference` is a WRITER's concern and is not on the wire:
    /// it records that a reference resolved against a heading rather than a
    /// `[label]: url` line, which the canonical writer needs to reproduce the
    /// authored `[H][]` (PART 11 R1, carve#478). Nothing in PART 12 defines a
    /// field for it and no other engine publishes one, so a decoded document
    /// comes back with the default.
    ///
    /// The consequence is real and shared: a document that has been through the
    /// wire format writes `[H](#H)` where the original writes `[H][]`. carve-php
    /// has the same gap (carve-php#711). Erased here so the round trip measures
    /// what the FORMAT carries, with the gap stated rather than hidden.
    /// THIS WALK MIRRORS `resolve_reference_links_inline`, and the completeness
    /// is the point rather than the coverage. The resolver is what SETS the
    /// flag, so any container it descends into can carry one - and a container
    /// missing here does not hide a difference, it invents one: the parsed side
    /// keeps `true`, the decoded side has the default, and the sweep fails on a
    /// document that round-trips perfectly well.
    ///
    /// It failed exactly that way once. The resolver gained an arm for an inline
    /// note (markup-carve/carve#1203) and this walk did not, so
    /// `315-an-inline-note-s-content-resolves-after-the-note-5` reported a
    /// round-trip mismatch on a field the format does not carry.
    fn inlines(nodes: &mut [carve::InlineNode]) {
        for node in nodes {
            match node {
                carve::InlineNode::Link(l) => {
                    l.from_heading_reference = false;
                    inlines(&mut l.children);
                }
                carve::InlineNode::Emphasis(e) => inlines(&mut e.children),
                carve::InlineNode::Span(sp) => inlines(&mut sp.children),
                carve::InlineNode::Extension(e) => inlines(&mut e.children),
                carve::InlineNode::Footnote(f) => {
                    if let Some(inline) = &mut f.inline {
                        inlines(inline);
                    }
                }
                carve::InlineNode::CriticInsert(c) => inlines(&mut c.children),
                carve::InlineNode::CriticDelete(c) => inlines(&mut c.children),
                carve::InlineNode::CitationGroup(g) => {
                    for item in &mut g.items {
                        if let Some(prefix) = &mut item.prefix {
                            inlines(prefix);
                        }
                        if let Some(locator) = &mut item.locator {
                            inlines(locator);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn blocks(nodes: &mut [carve::BlockNode]) {
        for node in nodes {
            match node {
                carve::BlockNode::Paragraph(p) => {
                    p.at_content_column = false;
                    inlines(&mut p.children);
                }
                carve::BlockNode::Heading(h) => inlines(&mut h.children),
                carve::BlockNode::BlockQuote(b) => blocks(&mut b.children),
                carve::BlockNode::Div(d) => blocks(&mut d.children),
                carve::BlockNode::Admonition(a) => blocks(&mut a.children),
                carve::BlockNode::LineBlock(l) => blocks(&mut l.children),
                carve::BlockNode::Extension(e) => blocks(&mut e.children),
                carve::BlockNode::List(l) => {
                    // `bare_marker` used to be erased here, on the grounds that
                    // the schema forbade it and no other engine published it.
                    // Both halves stopped being true: the schema names
                    // `bareMarker`, this engine publishes and decodes it, and a
                    // bare-dot document round-trips through the wire unchanged
                    // (carve#480). Erasing it now hides a property that holds -
                    // a regression dropping the field would have passed here.
                    for item in &mut l.items {
                        blocks(&mut item.children);
                    }
                }
                carve::BlockNode::DefinitionList(d) => {
                    for item in &mut d.items {
                        for def in &mut item.definitions {
                            blocks(&mut def.children);
                        }
                    }
                }
                carve::BlockNode::Figure(f) => {
                    if let carve::FigureTarget::Paragraph(p) = &mut f.target {
                        p.at_content_column = false;
                    }
                    if let carve::FigureTarget::BlockQuote(q) = &mut f.target {
                        blocks(&mut q.children);
                    }
                }
                _ => {}
            }
        }
    }
    blocks(&mut doc.children);
    for body in doc.footnote_defs.values_mut() {
        blocks(body);
    }
    // `ingest_payload_len` is a READER's own measurement - how many bytes the
    // payload this document was decoded from actually cost - and it is
    // deliberately not on the wire: republishing it would put one reader's
    // measurement where the next reader would read it back as a claim, which is
    // the whole defect it exists to close (carve-rs#811). So a decoded document
    // carries it and a parsed one carries 0, by design, and this comparison is
    // about what the FORMAT carries.
    doc.ingest_payload_len = 0;
    doc
}

fn root_keys(json: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut chars = json.char_indices().peekable();
    assert_eq!(chars.next().map(|(_, c)| c), Some('{'));
    loop {
        match chars.peek().map(|(_, c)| *c) {
            Some('}') => break,
            Some(',') => {
                chars.next();
            }
            Some('"') => {
                let key = read_string(&mut chars);
                assert_eq!(chars.next().map(|(_, c)| c), Some(':'));
                keys.push(key);
                skip_value(&mut chars);
            }
            other => panic!("unexpected root char: {other:?}"),
        }
    }
    keys
}

fn read_string<I>(chars: &mut std::iter::Peekable<I>) -> String
where
    I: Iterator<Item = (usize, char)>,
{
    assert_eq!(chars.next().map(|(_, c)| c), Some('"'));
    let mut out = String::new();
    let mut escaped = false;
    for (_, ch) in chars.by_ref() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            break;
        } else {
            out.push(ch);
        }
    }
    out
}

fn skip_value<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = (usize, char)>,
{
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while let Some((_, ch)) = chars.peek().copied() {
        if in_string {
            chars.next();
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                chars.next();
            }
            '[' | '{' => {
                depth += 1;
                chars.next();
            }
            ']' | '}' if depth > 0 => {
                depth -= 1;
                chars.next();
            }
            ',' | '}' if depth == 0 => break,
            _ => {
                chars.next();
            }
        }
    }
}

#[test]
fn corpus_round_trip() {
    // On a thread with room: the corpus holds a document nested to the parser's
    // cap (200 containers), and one debug-build frame per level overflows the
    // 2 MiB a test thread gets. The library handles the document - parse,
    // encode, decode, render and format all complete - so this buys the sweep,
    // not a behaviour change (carve-rs#530).
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(corpus_round_trip_inner)
        .expect("thread spawns")
        .join()
        .expect("the sweep finishes");
}

fn corpus_round_trip_inner() {
    let paths = corpus_sources();
    assert!(!paths.is_empty());
    for path in &paths {
        let source =
            fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let doc = parse_with_positions(&source);
        let json = carve::to_json(&doc);
        let decoded =
            carve::from_json(&json).unwrap_or_else(|e| panic!("decode {}: {e}", path.display()));

        // PART 12 §6, three ways, because the tree comparison alone has one
        // honest gap (see `normalize` below) and a comparison with a hole in it
        // is the kind that stops catching things.
        assert_eq!(
            carve::to_json(&decoded),
            json,
            "re-encoding a decoded document changed the bytes for {}",
            path.display()
        );
        let options = carve::Options::new();
        assert_eq!(
            carve::render_html_with_options(&decoded, &options)
                .expect("the tree under test is within the render ceiling"),
            carve::render_html_with_options(&doc, &options)
                .expect("the tree under test is within the render ceiling"),
            "a decoded document rendered differently for {}",
            path.display()
        );
        assert_eq!(
            normalize(decoded),
            normalize(doc),
            "round-trip mismatch for {}",
            path.display()
        );
    }
    eprintln!("ast json corpus round-trip count: {}", paths.len());
}

#[test]
fn root_shape_is_exactly_three_keys() {
    let plain = carve::to_json(&carve::parse("Hello.\n"));
    assert_eq!(root_keys(&plain), ["type", "children", "srcByteLength"]);

    let rich = carve::to_json(&carve::parse("---\na: b\n---\n\nHi.[^x]\n\n[^x]: note\n"));
    assert_eq!(root_keys(&rich), ["type", "children", "srcByteLength"]);
    assert!(!rich.contains("\"frontmatter\":"));
    assert!(!rich.contains("\"footnoteDefs\":"));
}

#[test]
fn frontmatter_is_first_child_and_raw() {
    let source = "---toml\n# keep me\nx = 1\n---\n\nBody\n";
    let json = carve::to_json(&carve::parse(source));
    assert!(json.contains("\"children\":[{\"type\":\"frontmatter\",\"format\":\"toml\",\"content\":\"# keep me\\nx = 1\"},{\"type\":\"paragraph\""));
    let decoded = carve::from_json(&json).expect("decode");
    assert!(decoded.frontmatter.is_empty());
    assert_eq!(
        decoded.frontmatter_raw,
        Some(carve::Frontmatter {
            format: "toml".to_string(),
            content: "# keep me\nx = 1".to_string(),
            pos: None,
        })
    );
}

#[test]
fn footnote_definition_is_labelled_document_child() {
    let source = "> quoted\n> [^inside]: lifted\n\nUse [^inside].\n";
    let json = carve::to_json(&carve::parse(source));
    assert!(json.contains("{\"type\":\"footnote\",\"label\":\"inside\",\"children\""));
    assert!(!json.contains("\"id\":\"inside\",\"children\""));
    let decoded = carve::from_json(&json).expect("decode");
    assert!(decoded.footnote_defs.contains_key("inside"));
}

#[test]
fn block_and_inline_pos_present() {
    let doc = parse_with_positions("Hello *world*.\n");
    let json = carve::to_json(&doc);
    assert!(json.contains("\"type\":\"paragraph\""));
    assert!(json.contains("\"pos\":{\"startLine\":1"));
    assert!(json.contains(
        "{\"type\":\"text\",\"value\":\"world\",\"pos\":{\"startLine\":1,\"endLine\":1,\"startColumn\":8,\"endColumn\":13,\"startOffset\":7,\"endOffset\":12}}"
    ));
    assert!(json.contains(
        "\"type\":\"strong\",\"children\":[{\"type\":\"text\",\"value\":\"world\",\"pos\""
    ));
}

#[test]
fn decode_refuses_the_legacy_footnote_id() {
    // It used to be read as an alias for `label`. PART 12 §11 rules ingest
    // strict, and a second spelling of a field name on the wire is the
    // interchange break §3 exists to prevent (carve-rs#820, spec 743).
    let json = "{\"type\":\"document\",\"children\":[{\"type\":\"footnote\",\"id\":\"old\",\"children\":[{\"type\":\"paragraph\",\"children\":[{\"type\":\"text\",\"value\":\"body\"}]}]}],\"srcByteLength\":0}";
    let error = carve::from_json(json).expect_err("the alias is refused");
    assert!(error.to_string().contains("\"id\""), "{error}");

    // The named spelling, which is what every engine publishes.
    let named = json.replace("\"id\":\"old\"", "\"label\":\"old\"");
    let doc = carve::from_json(&named).expect("decode");
    assert!(doc.footnote_defs.contains_key("old"));
}

#[test]
fn malformed_trees_return_errors() {
    assert!(carve::from_json("not json").is_err());
    assert!(
        carve::from_json("{\"type\":\"paragraph\",\"children\":[],\"srcByteLength\":0}").is_err()
    );
    assert!(carve::from_json(
        "{\"type\":\"document\",\"children\":[{\"type\":\"mystery\"}],\"srcByteLength\":0}"
    )
    .is_err());
    assert!(carve::from_json(
        "{\"type\":\"document\",\"children\":[{\"type\":\"paragraph\"}],\"srcByteLength\":0}"
    )
    .is_err());
    assert!(carve::from_json(
        "{\"type\":\"document\",\"children\":[{\"type\":\"paragraph\",\"children\":[{\"type\":\"link\",\"href\":\"/\",\"children\":[],\"title\":1}]}],\"srcByteLength\":0}"
    )
    .is_err());
}

#[test]
fn cli_json_and_from_json() {
    let source = "# Hi\n\nA *bold* word.\n";
    let (ok, json, stderr) = run(&["--json"], source);
    assert!(ok, "stderr: {stderr}");
    assert!(!carve::from_json(&json)
        .expect("decode json")
        .children
        .is_empty());

    let (ok, html_from_json, stderr) = run(&["--from-json", "--html"], &json);
    assert!(ok, "stderr: {stderr}");
    let (ok, html_direct, stderr) = run(&["--html"], source);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(html_from_json, html_direct);

    let (ok, json_again, stderr) = run(&["--from-json", "--json"], &json);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(json_again, json);

    let (ok, _stdout, stderr) = run(&["--from-json", "--html"], "{ nope");
    assert!(!ok);
    assert!(stderr.contains("cannot decode JSON AST"), "{stderr}");
}

#[test]
fn from_json_is_bounded_by_the_profile_max_length() {
    // The tree carries its own `srcByteLength`, and that number arrives INSIDE
    // the payload - so bounding on it would let a hostile tree claim zero and
    // render anything. The payload is what is measured.
    let mut children = String::new();
    for _ in 0..4000 {
        children.push_str(
            r#"{"type":"paragraph","children":[{"type":"text","value":"xxxxxxxxxxxxxxxxxxxxxxxxx"}]},"#,
        );
    }
    children.pop();
    let hostile = format!(r#"{{"type":"document","children":[{children}],"srcByteLength":0}}"#);
    assert!(
        hostile.len() > 100_000,
        "sample must exceed the comment profile's limit"
    );

    let (ok, _stdout, stderr) = run(&["--from-json", "--html", "--profile", "comment"], &hostile);
    assert!(!ok, "an oversize payload must be refused");
    assert!(stderr.contains("maximum length"), "{stderr}");

    // The mirror: a small tree through the same profile still renders, so the
    // bound above cannot pass by rejecting everything.
    let small = carve::to_json(&carve::parse("a short comment\n"));
    let (ok, stdout, stderr) = run(&["--from-json", "--html", "--profile", "comment"], &small);
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("a short comment"), "{stdout}");
}

/// Run `f` on a worker thread with an ample stack, like the other worst-case
/// depth probes (tests/recursion_and_panics.rs): the encoder, the decoder and
/// the parser all use one native frame per level, and a DEBUG build's frames are
/// large enough that 200 nested containers do not fit a default test stack. A
/// release build fits them in 2 MiB, which the release check below pins.
fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker must return, not abort");
}

fn source_at_parser_cap(shape: &str) -> String {
    let cap = 200usize;
    match shape {
        "div ladder" => {
            let mut out = String::new();
            for i in 0..cap {
                out.push_str(&":".repeat(cap + 2 - i));
                out.push('\n');
            }
            out.push_str("x\n");
            for i in (0..cap).rev() {
                out.push_str(&":".repeat(cap + 2 - i));
                out.push('\n');
            }
            out
        }
        "blockquotes" => format!("{}x\n", "> ".repeat(cap)),
        "nested list" => {
            let mut out = String::new();
            for i in 0..cap {
                out.push_str(&" ".repeat(i * 2));
                out.push_str("- item\n");
            }
            out
        }
        "table under blockquotes" => {
            format!("{}\n| =a | =b |\n| 1 | 2 |\n", "> ".repeat(cap))
        }
        other => panic!("unknown shape {other}"),
    }
}

/// Everything the parser can produce has to survive `to_json` then `from_json`.
/// The ingest guard bounds JSON STRUCTURAL depth, and it used to do so with the
/// parser's AST-level cap as the number. One AST level costs two to six
/// structural levels, so the guard rejected this crate's own output past roughly
/// 99 nested containers: `carve --json | carve --from-json` failed on a document
/// `carve` had just parsed.
#[test]
fn from_json_accepts_everything_the_parser_can_produce() {
    on_big_stack(|| {
        for shape in [
            "div ladder",
            "blockquotes",
            "nested list",
            "table under blockquotes",
        ] {
            let json = carve::to_json(&carve::parse(&source_at_parser_cap(shape)));
            let decoded = carve::from_json(&json)
                .unwrap_or_else(|e| panic!("{shape}: parser output must decode, got {e}"));
            assert_eq!(
                carve::to_json(&decoded),
                json,
                "{shape}: decode must reproduce the encoded AST"
            );
        }
    });
}

/// The comment on MAX_JSON_DEPTH claims a release build decodes the deepest wire
/// form the parser can produce inside an ordinary 2 MiB thread stack. Pin it, so
/// raising the cap again cannot quietly turn a rejection into an abort. Debug
/// frames are far larger, so the check only runs where the claim is made.
#[test]
#[cfg(not(debug_assertions))]
fn a_release_build_decodes_the_deepest_wire_form_on_a_default_stack() {
    let json = carve::to_json(&carve::parse(&source_at_parser_cap("nested list")));
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            carve::from_json(&json).expect("decode on a default-size stack");
        })
        .expect("spawn worker")
        .join()
        .expect("worker must return, not abort");
}

#[test]
fn deeply_nested_json_is_refused_rather_than_overflowing() {
    // The reader is recursive-descent, so nesting depth IS stack depth, and this
    // input is untrusted: 200k nested arrays overflowed the stack and aborted the
    // process (SIGABRT, not an error a caller can catch).
    //
    // The reader's budget is NOT the parser's cap, and assuming it was is
    // carve-rs#389: a node costs two structural levels, so a budget equal to the
    // parser's 200 nodes refused ASTs this crate had just emitted. It is now
    // derived from the parser's cap instead. 5000 is far past either, so what
    // this test pins - deep input is refused rather than overflowing - is
    // unaffected by where exactly the boundary sits.
    let n = 5_000;
    let src = format!(
        "{{\"type\":\"document\",\"srcByteLength\":0,\"children\":{}{}}}",
        "[".repeat(n),
        "]".repeat(n)
    );
    let err = carve::from_json(&src).expect_err("a 5000-deep tree must be refused");
    // Matched on the reason, not on a number: the budget is derived now, so
    // hardcoding it here would break every time the parser's cap moves.
    assert!(err.to_string().contains("nests deeper"), "{err}");

    // The mirror: ordinary nesting still decodes, so the cap cannot pass by
    // refusing everything.
    let ok = carve::to_json(&carve::parse("> - /a *b*/\n"));
    assert!(carve::from_json(&ok).is_ok());
}

#[test]
fn footnote_definitions_follow_the_content() {
    // PART 12 §7: definitions are document children, written after the content
    // and ordered by source position. The stored map is keyed by label, so JSON
    // serialization has to sort a positioned view of it.
    let json = carve::to_json(&carve::parse_with_options(
        "first[^z]\n\nsecond[^a]\n\n[^z]: zed\n\n[^a]: ay\n",
        &carve::Options::new().with_positions(true),
    ));

    // The document's FIRST child is the content, not a definition. Checked on
    // the head of the string rather than by hunting for the last paragraph:
    // every definition body contains one, so "the last paragraph" sits inside a
    // definition and any comparison against it is vacuous.
    assert!(
        json.starts_with(r#"{"type":"document","children":[{"type":"paragraph""#),
        "a definition must not be the document's first child: {}",
        &json[..json.len().min(120)]
    );
    assert!(json.contains(r#""type":"footnote","label":"z""#), "{json}");
    let z_at = json
        .find(r#""type":"footnote","label":"z""#)
        .expect("z definition");
    let a_at = json
        .find(r#""type":"footnote","label":"a""#)
        .expect("a definition");
    assert!(
        z_at < a_at,
        "definitions must serialize in source order, not label order: {json}"
    );

    let doc = carve::from_json(&json).expect("decode");
    assert_eq!(doc.children.len(), 2, "the two paragraphs stay in the tree");
    assert_eq!(doc.footnote_defs.len(), 2, "and both definitions survive");
}

#[test]
fn definition_lists_publish_dt_and_dd_nodes() {
    // PART 12: the wire carries the `<dt>` / `<dd>` sequence, not this engine's
    // grouping. `definition_term` and `definition_description` are in the
    // normative block vocabulary - under the grouped form those two entries
    // named nothing and a profile denying either was a silent no-op - and a
    // plain `{terms, definitions}` object can carry no `pos`.
    //
    // The grouping was also not agreed: on this document this engine produced
    // three entries and carve-js produced one, while both rendered the same
    // `<dl>`.
    let source = ":: Term one\n:: Term two\n:  Def A\n:  Def B\n\n:: Second\n:  Only\n";
    let json = carve::to_json(&carve::parse_with_options(
        source,
        &carve::Options::new().with_positions(true),
    ));

    let types: Vec<&str> = json
        .match_indices("\"type\":\"definition_")
        .map(|(i, _)| {
            let rest = &json[i + "\"type\":\"".len()..];
            &rest[..rest.find('"').expect("a type name ends")]
        })
        .collect();
    assert_eq!(
        types,
        [
            "definition_list",
            "definition_term",
            "definition_term",
            "definition_description",
            "definition_description",
            "definition_term",
            "definition_description",
        ],
        "{json}"
    );
    assert!(
        !json.contains("\"terms\""),
        "the grouping is internal: {json}"
    );
}

#[test]
fn definition_lists_round_trip_through_the_flat_form() {
    // §6. Decoding regroups by the renderer's rule - a run of terms opens an
    // entry, the descriptions after it belong to it - which is the only rule
    // all three engines agree on, since it is the one the `<dl>` shows.
    let source = ":: a\n:: b\n:  x\n:  y\n\n:: c\n:  z\n";
    let doc = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
    let decoded = carve::from_json(&carve::to_json(&doc)).expect("decode");

    assert_eq!(carve::to_json(&decoded), carve::to_json(&doc));

    let options = carve::Options::new();
    assert_eq!(
        carve::render_html_with_options(&decoded, &options)
            .expect("the tree under test is within the render ceiling"),
        carve::render_html_with_options(&doc, &options)
            .expect("the tree under test is within the render ceiling"),
    );
}

#[test]
fn the_older_grouped_payload_still_decodes() {
    // Trees in the previous shape are stored, and this engine wrote them.
    let json = r#"{"type":"document","srcByteLength":0,"children":[{"type":"definition_list",
        "items":[{"terms":[[{"type":"text","value":"T"}]],
        "definitions":[[{"type":"paragraph","children":[{"type":"text","value":"D"}]}]]}]}]}"#;
    let doc = carve::from_json(json).expect("decode the older form");

    assert!(carve::to_json(&doc).contains("\"type\":\"definition_term\""));
}

#[test]
fn key_values_serialize_in_the_author_s_source_order() {
    // The order the map is STORED in is this engine's storage choice - a
    // `BTreeMap`, so alphabetical - and PART 12 §1 says an implementation whose
    // internals differ maps on the way out rather than exporting them. The
    // author's order is what `attrs.order` records ("Source-appearance order of
    // the slots", resources/ast-schema.json), and it is what the HTML renderer
    // has always used (PART 10 §1).
    let json = carve::to_json(&carve::parse("[x]{b=1 a=2}\n"));
    assert!(
        json.contains(r#""keyValues":{"b":"1","a":"2"},"order":["b","a"]"#),
        "{json}"
    );

    // The reverse spelling, so the assertion above cannot pass by the emitted
    // order happening to be a fixed one.
    let json = carve::to_json(&carve::parse("[x]{a=2 b=1}\n"));
    assert!(
        json.contains(r#""keyValues":{"a":"2","b":"1"},"order":["a","b"]"#),
        "{json}"
    );
}

#[test]
fn one_attrs_object_states_one_order() {
    // The defect stated plainly: `keyValues` and `order` disagreed inside the
    // same object, on three corpus documents. This is the shape of all three
    // (markup-carve/carve-rs#966) - `297-the-language-sigil-takes-no-padding`,
    // `301-a-derived-title-yields-to-an-authored-one` and
    // `45-inline-extensions-9` - reduced to one source each.
    for source in [
        "[x]{: fr}\n",
        "[x]{time=\"t\" datetime=\"d\"}\n",
        "[x]{kbd data-key=\"k\" onclick=\"o\"}\n",
    ] {
        let json = carve::to_json(&carve::parse(source));
        let attrs = json
            .split(r#""attrs":{"#)
            .nth(1)
            .unwrap_or_else(|| panic!("no attrs in {json}"));
        let emitted: Vec<&str> = attrs
            .split(r#""keyValues":{"#)
            .nth(1)
            .unwrap_or_else(|| panic!("no keyValues in {json}"))
            .split('}')
            .next()
            .expect("keyValues closes")
            .split(',')
            .map(|slot| slot.split(':').next().expect("a key").trim_matches('"'))
            .collect();
        let recorded: Vec<&str> = attrs
            .split(r#""order":["#)
            .nth(1)
            .unwrap_or_else(|| panic!("no order in {json}"))
            .split(']')
            .next()
            .expect("order closes")
            .split(',')
            .map(|slot| slot.trim_matches('"'))
            .filter(|slot| *slot != "#id" && *slot != ".class")
            .collect();
        assert_eq!(emitted, recorded, "{source:?}: {json}");
    }
}

#[test]
fn the_emitted_order_is_the_one_the_html_renderer_uses() {
    // Not two implementations of one rule. The renderer reads `order` and
    // always has; the serializer now reads the same field, so the two cannot
    // drift into stating different orders for one document again.
    let source = "[x]{zz=1 aa=2}\n";
    let html = carve::render_html(&carve::parse(source)).expect("render");
    assert_eq!(html, r#"<p><span zz="1" aa="2">x</span></p>"#);

    let json = carve::to_json(&carve::parse(source));
    assert!(
        json.contains(r#""keyValues":{"zz":"1","aa":"2"}"#),
        "{json}"
    );
}

#[test]
fn a_key_the_order_does_not_mention_is_still_published() {
    // An `Attrs` built programmatically records no order at all (the schema
    // says so), and dropping its attributes to protect an ordering would lose
    // the document to save the bookkeeping.
    let mut doc = carve::parse("x\n");
    let carve::BlockNode::Paragraph(paragraph) = &mut doc.children[0] else {
        panic!("the fixture is a paragraph");
    };
    let mut attrs = carve::Attrs::default();
    attrs.key_values.insert("zz".to_string(), "1".to_string());
    attrs.key_values.insert("aa".to_string(), "2".to_string());
    paragraph.attrs = Some(attrs);

    let json = carve::to_json(&doc);
    assert!(
        json.contains(r#""keyValues":{"aa":"2","zz":"1"}"#),
        "{json}"
    );
    assert!(!json.contains(r#""order":"#), "{json}");
}

#[test]
fn source_order_survives_a_json_round_trip() {
    // §6. `order` is what carries it, so a tree that went out and came back
    // still serializes the way the author wrote it.
    let source = "[x]{b=1 a=2}\n";
    let doc = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
    let json = carve::to_json(&doc);
    let decoded = carve::from_json(&json).expect("decode");

    assert_eq!(carve::to_json(&decoded), json);
    assert!(json.contains(r#""keyValues":{"b":"1","a":"2"}"#), "{json}");
}
