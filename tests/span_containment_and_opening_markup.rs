//! PART 12 §4: A SPAN BEGINS AT THE CONSTRUCT'S OPENING MARKUP, and a parent's
//! span CONTAINS every child's - in two SEPARATE passes (carve#913).
//!
//! A node's `pos` covers the construct as WRITTEN - the `>` of a block quote,
//! the `#` of a heading, a list item's marker AND the indentation that places
//! it, the `[` of a link, the backtick run of a code block - so a span
//! round-trips to the source text that produced the node. Content-only was the
//! alternative and is rejected structurally: under it a nested construct's span
//! is no longer contained by its parent's, and the span tree stops being a tree.
//!
//! THE TWO PASSES ARE SEPARATE, DELIBERATELY. They point the same way today,
//! which is exactly why deriving one from the other would go quiet, with
//! nothing failing, the day the convention were revisited.
//!
//! POSITIONS ARE OPT-IN in this engine, so a probe that does not REQUEST them
//! yields a tree with no `pos` anywhere - zero findings out of zero spans,
//! which reads exactly like a clean run. Every sweep below asserts the count it
//! EXAMINED, and `positions_are_actually_requested` asserts the denominator is
//! not zero for the reason it looks like it is not.
//!
//! Mirrors `checkOpeningMarkup` and `checkContainment` in the spec repo's
//! `scripts/spec/ast-positions.mjs`, over the same serialized trees, so this
//! engine can see what the conformance report sees without the report.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// A minimal JSON reader, so the walk is GENERIC over node types.
//
// The AST is an enum tree, and a hand-written match per variant is exactly the
// thing that goes stale when a node type is added - the sweep would silently
// stop visiting it. The serialized tree is also what §4 is about: it is the
// interchange format a consumer receives.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

struct Reader<'a> {
    bytes: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            bytes: s.as_bytes(),
            i: 0,
        }
    }

    fn ws(&mut self) {
        while self.i < self.bytes.len() && self.bytes[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn value(&mut self) -> Json {
        self.ws();
        match self.bytes[self.i] {
            b'{' => {
                self.i += 1;
                let mut map = BTreeMap::new();
                loop {
                    self.ws();
                    if self.bytes[self.i] == b'}' {
                        self.i += 1;
                        return Json::Object(map);
                    }
                    let key = match self.value() {
                        Json::String(s) => s,
                        other => panic!("object key is not a string: {other:?}"),
                    };
                    self.ws();
                    assert_eq!(self.bytes[self.i], b':', "expected ':' after a key");
                    self.i += 1;
                    let value = self.value();
                    map.insert(key, value);
                    self.ws();
                    if self.bytes[self.i] == b',' {
                        self.i += 1;
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.ws();
                    if self.bytes[self.i] == b']' {
                        self.i += 1;
                        return Json::Array(items);
                    }
                    items.push(self.value());
                    self.ws();
                    if self.bytes[self.i] == b',' {
                        self.i += 1;
                    }
                }
            }
            b'"' => {
                self.i += 1;
                let mut out = String::new();
                loop {
                    let b = self.bytes[self.i];
                    if b == b'"' {
                        self.i += 1;
                        return Json::String(out);
                    }
                    if b == b'\\' {
                        self.i += 1;
                        let esc = self.bytes[self.i];
                        self.i += 1;
                        match esc {
                            b'n' => out.push('\n'),
                            b't' => out.push('\t'),
                            b'r' => out.push('\r'),
                            b'b' => out.push('\u{8}'),
                            b'f' => out.push('\u{c}'),
                            b'u' => {
                                let hex = std::str::from_utf8(&self.bytes[self.i..self.i + 4])
                                    .expect("hex escape");
                                self.i += 4;
                                let unit = u32::from_str_radix(hex, 16).expect("hex escape");
                                // A surrogate pair, which the serializer emits
                                // for an astral character.
                                if (0xD800..0xDC00).contains(&unit) {
                                    assert_eq!(self.bytes[self.i], b'\\');
                                    let hex2 =
                                        std::str::from_utf8(&self.bytes[self.i + 2..self.i + 6])
                                            .expect("hex escape");
                                    self.i += 6;
                                    let low = u32::from_str_radix(hex2, 16).expect("hex escape");
                                    let cp = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                                    out.push(char::from_u32(cp).expect("astral codepoint"));
                                } else {
                                    out.push(char::from_u32(unit).expect("codepoint"));
                                }
                            }
                            other => out.push(other as char),
                        }
                        continue;
                    }
                    let start = self.i;
                    while self.bytes[self.i] & 0xC0 == 0x80 || self.i == start {
                        self.i += 1;
                    }
                    out.push_str(std::str::from_utf8(&self.bytes[start..self.i]).expect("utf8"));
                }
            }
            b't' => {
                self.i += 4;
                Json::Bool(true)
            }
            b'f' => {
                self.i += 5;
                Json::Bool(false)
            }
            b'n' => {
                self.i += 4;
                Json::Null
            }
            _ => {
                let start = self.i;
                while self.i < self.bytes.len()
                    && matches!(
                        self.bytes[self.i],
                        b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
                    )
                {
                    self.i += 1;
                }
                Json::Number(
                    std::str::from_utf8(&self.bytes[start..self.i])
                        .expect("utf8")
                        .parse()
                        .expect("number"),
                )
            }
        }
    }
}

fn parse_json(s: &str) -> Json {
    Reader::new(s).value()
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

struct Node {
    ty: String,
    pos: Option<(usize, usize)>,
    path: String,
}

fn pos_of(obj: &BTreeMap<String, Json>) -> Option<(usize, usize)> {
    let Json::Object(p) = obj.get("pos")? else {
        return None;
    };
    let start = match p.get("startOffset")? {
        Json::Number(n) => *n as usize,
        _ => return None,
    };
    let end = match p.get("endOffset")? {
        Json::Number(n) => *n as usize,
        _ => return None,
    };
    Some((start, end))
}

/// Every typed node, with the nearest PLACED ancestor's span.
///
/// The nearest placed ancestor, not the immediate parent: a node may
/// legitimately omit `pos` (PART 12 §4's reassembled clause), and skipping past
/// it keeps the rule from going quiet exactly where a span is most likely to be
/// wrong.
fn walk(
    value: &Json,
    path: &str,
    placed: Option<(usize, usize)>,
    out: &mut Vec<(Node, Option<(usize, usize)>)>,
) {
    match value {
        Json::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk(item, &format!("{path}[{i}]"), placed, out);
            }
        }
        Json::Object(obj) => {
            let mut placed = placed;
            if let Some(Json::String(ty)) = obj.get("type") {
                let pos = pos_of(obj);
                out.push((
                    Node {
                        ty: ty.clone(),
                        pos,
                        path: path.to_string(),
                    },
                    placed,
                ));
                if pos.is_some() {
                    placed = pos;
                }
            }
            for (key, child) in obj {
                if key == "pos" {
                    continue;
                }
                walk(child, &format!("{path}.{key}"), placed, out);
            }
        }
        _ => {}
    }
}

fn serialize(source: &str) -> Json {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    parse_json(&carve::to_json_with_options(source, &options))
}

fn corpus() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .expect("the corpus directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()?.to_str()? != "crv" {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_string())
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let source =
                fs::read_to_string(dir.join(format!("{name}.crv"))).expect("a corpus pair");
            (name, source)
        })
        .collect()
}

/// A definition is a child of the DOCUMENT wherever it was written, and its
/// `pos` still records where that was - inside whatever container it was
/// authored in. So its span legitimately sits outside its document parent's
/// notion of order, and the containment pass exempts the three kinds.
const HOISTED: [&str; 3] = ["footnote", "abbreviation_def", "link_reference_definition"];

// ---------------------------------------------------------------------------
// The opt-in trap
// ---------------------------------------------------------------------------

#[test]
fn positions_are_actually_requested() {
    // Assert PRESENCE before comparing anything: a probe that does not request
    // positions compares nulls to nulls and passes against an unfixed engine.
    let doc = serialize("> quoted\n");
    let mut nodes = Vec::new();
    walk(&doc, "$", None, &mut nodes);
    let placed = nodes.iter().filter(|(n, _)| n.pos.is_some()).count();
    assert!(
        placed >= 3,
        "the probe produced {placed} placed nodes, so the sweeps below would \
         compare nothing"
    );

    let options = carve::Options::default();
    let without = carve::to_json_with_options("> quoted\n", &options);
    assert!(
        !without.contains("\"pos\""),
        "positions are supposed to be opt-in, so this test's premise is wrong"
    );
}

// ---------------------------------------------------------------------------
// Pass 1: containment, over the whole corpus
// ---------------------------------------------------------------------------

#[test]
fn a_parent_span_contains_every_child_span_over_the_corpus() {
    // A cap-deep corpus document costs one frame per level in both the reader
    // and the walk, and a test thread gets 2 MiB (carve-rs#530).
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(a_parent_span_contains_every_child_span_over_the_corpus_inner)
        .expect("thread spawns")
        .join()
        .expect("the sweep finishes");
}

fn a_parent_span_contains_every_child_span_over_the_corpus_inner() {
    let mut pairs = 0usize;
    let mut findings: Vec<String> = Vec::new();
    for (name, source) in corpus() {
        let doc = serialize(&source);
        let mut nodes = Vec::new();
        walk(&doc, "$", None, &mut nodes);
        for (node, parent) in &nodes {
            let (Some((cs, ce)), Some((ps, pe))) = (node.pos, parent) else {
                continue;
            };
            if HOISTED.contains(&node.ty.as_str()) {
                continue;
            }
            pairs += 1;
            if cs < *ps || ce > *pe {
                findings.push(format!(
                    "{name}: \"{}\" at {} [{cs}, {ce}] is not inside its placed ancestor [{ps}, {pe}]",
                    node.ty, node.path
                ));
            }
        }
    }
    assert!(
        findings.is_empty(),
        "{} spans escape their parent:\n{}",
        findings.len(),
        findings.join("\n")
    );
    // The DENOMINATOR. Zero findings from zero pairs reads exactly like a clean
    // run, and positions are opt-in here.
    assert!(
        pairs > 3000,
        "only {pairs} parent/child pairs were examined, so this sweep proves \
         very little"
    );
}

// ---------------------------------------------------------------------------
// Pass 2: a span begins at the construct's opening markup
// ---------------------------------------------------------------------------

/// Node type to the markup its span must BEGIN at, read from the SOURCE - never
/// from what the node says it holds. That distinction is the whole point: the
/// one content-level rule the conformance checker had asserted that a span
/// SLICES TO plausible text, and every real divergence preserved it.
fn opens_with(ty: &str, ahead: &str) -> Option<bool> {
    let first = ahead.chars().next()?;
    let ordered_marker = || {
        let mut chars = ahead.chars();
        let head: String = chars
            .by_ref()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        (!head.is_empty()
            && ahead
                .chars()
                .nth(head.chars().count())
                .is_some_and(|c| c == '.' || c == ')'))
            || first == '.'
    };
    Some(match ty {
        "abbreviation_def" => ahead.starts_with("*["),
        "admonition" | "definition_list" | "div" | "inline_extension" | "line_block" | "symbol" => {
            first == ':'
        }
        "autolink" | "heading_ref" => first == '<',
        "block_quote" => first == '>',
        "caption_number" | "heading" | "tag" => first == '#',
        "code" | "raw_inline" | "raw_block" => first == '`',
        "code_block" => first == '`' || first == '~',
        "comment" => first == '%',
        "critic_comment" | "delete" | "insert" | "subscript" | "substitution" | "superscript" => {
            first == '{'
        }
        "footnote_ref" | "link" | "span" => first == '[',
        "highlight" => first == '=' || first == '{',
        "image" | "literal_inline" => first == '!',
        "inline_footnote" => first == '^',
        "list" | "list_item" => matches!(first, '-' | '+' | '*') || ordered_marker(),
        "math" => first == '$',
        "mention" => first == '@',
        "strike" => first == '~' || first == '{',
        "table" => first == '|',
        "thematic_break" => matches!(first, '-' | '*' | '_'),
        "underline" => first == '_' || first == '{',
        _ => return None,
    })
}

#[test]
fn a_span_begins_at_the_constructs_opening_markup_over_the_corpus() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(a_span_begins_at_the_constructs_opening_markup_over_the_corpus_inner)
        .expect("thread spawns")
        .join()
        .expect("the sweep finishes");
}

fn a_span_begins_at_the_constructs_opening_markup_over_the_corpus_inner() {
    let mut examined = 0usize;
    let mut findings: Vec<String> = Vec::new();
    for (name, source) in corpus() {
        let codepoints: Vec<char> = source.chars().collect();
        let doc = serialize(&source);
        let mut nodes = Vec::new();
        walk(&doc, "$", None, &mut nodes);
        for (node, _) in &nodes {
            let Some((start, end)) = node.pos else {
                continue;
            };
            if start > codepoints.len() {
                continue;
            }
            // Skip the indentation that PLACES the construct: it is inside the
            // span (that is the rule), but the markup is what follows it.
            let mut at = start;
            while at < end && matches!(codepoints.get(at), Some(' ') | Some('\t')) {
                at += 1;
            }
            let ahead: String = codepoints[at..codepoints.len().min(at + 24)]
                .iter()
                .collect();
            let Some(ok) = opens_with(&node.ty, &ahead) else {
                continue;
            };
            examined += 1;
            if !ok {
                findings.push(format!(
                    "{name}: pos does not begin at the markup that opens \"{}\" at {}: \
                     offset {start} reaches {ahead:?}",
                    node.ty, node.path
                ));
            }
        }
    }
    assert!(
        findings.is_empty(),
        "{} spans do not begin at their opening markup:\n{}",
        findings.len(),
        findings.join("\n")
    );
    // The DENOMINATOR. Positions are opt-in, so zero findings from zero spans
    // is the failure this number exists to prevent. The spec repo's own run of
    // the same rule over the same corpus reports the same order of magnitude.
    assert!(
        examined > 1000,
        "only {examined} spans were examined, so this sweep proves very little"
    );
}

// ---------------------------------------------------------------------------
// The shapes that moved: indentation is space and tab, and nothing else is
// ---------------------------------------------------------------------------

/// The first placed node of each depth, as `(type, start, end)`.
fn spans(source: &str) -> Vec<(String, usize, usize)> {
    let doc = serialize(source);
    let mut nodes = Vec::new();
    walk(&doc, "$", None, &mut nodes);
    nodes
        .into_iter()
        .filter_map(|(n, _)| n.pos.map(|(s, e)| (n.ty, s, e)))
        .collect()
}

#[test]
fn a_leading_no_break_space_is_content_inside_the_span_not_indentation_before_it() {
    // A block's span began ONE COLUMN past its own first child, because the
    // indentation it skipped was measured with the Unicode whitespace property.
    // PART 1's `indent` terminal is space and tab (carve#890), so a no-break
    // space is CONTENT the span must cover.
    for source in [
        "\u{a0}x\n",
        "\u{2000}x\n",
        "\u{3000}x\n",
        "> \u{a0}q\n",
        "x[^f]\n\n[^f]: \u{a0}note\n",
    ] {
        let spans = spans(source);
        let (ty, ps, pe) = spans
            .iter()
            .find(|(ty, _, _)| ty == "paragraph")
            .cloned()
            .unwrap_or_else(|| panic!("no placed paragraph for {source:?}"));
        let (_, ts, te) = spans
            .iter()
            .find(|(ty, _, _)| ty == "text")
            .cloned()
            .unwrap_or_else(|| panic!("no placed text for {source:?}"));
        assert!(
            ps <= ts && pe >= te,
            "{source:?}: {ty} [{ps}, {pe}] does not contain its text [{ts}, {te}]"
        );
    }
}

#[test]
fn control_a_space_and_a_tab_are_still_indentation() {
    // The mirror direction: narrowing the terminal must not make real
    // indentation into content, or every indented block's span would start too
    // early and the opening-markup pass would report it.
    for (source, expected_start) in [("  x\n", 2usize), ("\tx\n", 1)] {
        let spans = spans(source);
        let (_, ps, _) = spans
            .iter()
            .find(|(ty, _, _)| ty == "paragraph")
            .cloned()
            .unwrap_or_else(|| panic!("no placed paragraph for {source:?}"));
        assert_eq!(
            ps, expected_start,
            "{source:?}: the paragraph should start after its indentation"
        );
    }
}

#[test]
fn control_a_leading_byte_order_mark_is_stripped_before_anything_is_placed() {
    // U+FEFF is removed from the head of the document, so it is neither
    // indentation nor content here - pinned so the fix above cannot be read as
    // having moved it.
    let spans = spans("\u{feff}x\n");
    let (_, ps, _) = spans
        .iter()
        .find(|(ty, _, _)| ty == "paragraph")
        .cloned()
        .expect("a placed paragraph");
    assert_eq!(ps, 1, "the mark itself is not inside the paragraph's span");
}
