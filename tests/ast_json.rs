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
    fn blocks(nodes: &mut [carve::BlockNode]) {
        for node in nodes {
            match node {
                carve::BlockNode::Paragraph(p) => p.at_content_column = false,
                carve::BlockNode::BlockQuote(b) => blocks(&mut b.children),
                carve::BlockNode::Div(d) => blocks(&mut d.children),
                carve::BlockNode::Admonition(a) => blocks(&mut a.children),
                carve::BlockNode::LineBlock(l) => blocks(&mut l.children),
                carve::BlockNode::Extension(e) => blocks(&mut e.children),
                carve::BlockNode::List(l) => {
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
            carve::render_html_with_options(&decoded, &options),
            carve::render_html_with_options(&doc, &options),
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
fn block_pos_present_inline_pos_absent() {
    let doc = parse_with_positions("Hello *world*.\n");
    let json = carve::to_json(&doc);
    assert!(json.contains("\"type\":\"paragraph\""));
    assert!(json.contains("\"pos\":{\"startLine\":1"));
    assert!(json
        .contains("{\"type\":\"strong\",\"children\":[{\"type\":\"text\",\"value\":\"world\"}]}"));
}

#[test]
fn decode_accepts_legacy_footnote_id() {
    let json = "{\"type\":\"document\",\"children\":[{\"type\":\"footnote\",\"id\":\"old\",\"children\":[{\"type\":\"paragraph\",\"children\":[{\"type\":\"text\",\"value\":\"body\"}]}]}],\"srcByteLength\":0}";
    let doc = carve::from_json(json).expect("decode");
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
    assert!(hostile.len() > 100_000, "sample must exceed the comment profile's limit");

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

#[test]
fn deeply_nested_json_is_refused_rather_than_overflowing() {
    // The reader is recursive-descent, so nesting depth IS stack depth, and this
    // input is untrusted: 200k nested arrays overflowed the stack and aborted the
    // process (SIGABRT, not an error a caller can catch). The markup parser bounds
    // itself the same way and at the same depth, so an AST this deep could not
    // have been produced by parsing anything.
    let n = 5_000;
    let src = format!(
        "{{\"type\":\"document\",\"srcByteLength\":0,\"children\":{}{}}}",
        "[".repeat(n),
        "]".repeat(n)
    );
    let err = carve::from_json(&src).expect_err("a 5000-deep tree must be refused");
    assert!(err.to_string().contains("200"), "{err}");

    // The mirror: ordinary nesting still decodes, so the cap cannot pass by
    // refusing everything.
    let ok = carve::to_json(&carve::parse("> - /a *b*/\n"));
    assert!(carve::from_json(&ok).is_ok());
}
