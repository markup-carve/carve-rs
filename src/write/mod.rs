//! The canonical writer of `writer.md`: a tree, and optionally the sidecar
//! that was returned with it, as Carve source (C2-W-001). Blocks that a blank
//! line separates are written and tested on their own; a group is parsed again
//! and written again until it reads back as its blocks (C2-W-089).
//!
//! This engine places every opener found by a repair round of C2-W-088, with
//! the work limit of C2-W-133; the bytes are those the clauses define (C2-W-010).

mod attrs;
mod blocks;
mod carets;
mod check;
mod controls;
mod extension;
mod facts;
mod inline;
mod kinds;
mod node;
mod pairs;
mod spans;
mod state;
mod table;
mod trim;
mod width_data;

pub(crate) use blocks::include_spelling;
pub(crate) use table::display_width;

use crate::api::{Entry, Range};
use crate::hash::{AddrMap, AddrSet};
use crate::view::{Arr, V};
use check::{Check, Chunk, compare_blocks, escape_at, escape_openers, exhausted, extend_chains};
use node::{N, children, has, id, span, text, ty};
use serde_json::Value;
use state::{GroupCertificate, Writer, lose};
use std::collections::HashSet;

/// The link keys of a document, shared by the nested writes of its source bodies.
pub(crate) type LinkKeys = std::sync::Arc<HashSet<String>>;

/// The keys of the link reference definitions among `nodes`, as the writer reads them.
pub(crate) fn link_keys<'n>(nodes: impl IntoIterator<Item = &'n Value>) -> LinkKeys {
    std::sync::Arc::new(
        nodes
            .into_iter()
            .filter(|n| ty(*n) == "link_reference_definition")
            .filter_map(|n| node::s(n, "key"))
            .map(str::to_string)
            .collect(),
    )
}

/// How one write runs.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// C2-W-068.
    pub align_tables: bool,
    /// The standard extensions the write enables, by name.
    pub extensions: Vec<String>,
    /// The sidecar facts; `None` writes the defaults of C2-W-083 (C2-W-102, C2-W-127).
    pub layout: Option<Vec<facts::Entry>>,
    /// The source of the tree, for C2-W-144; `None` for a supplied tree.
    pub source: Option<String>,
    /// The link keys of the document around a nested write, in place of its definitions.
    pub link_keys: Option<LinkKeys>,
}

impl Settings {
    /// The facts of a sidecar JSON value (C2-L-009).
    #[must_use]
    pub fn with_layout(mut self, layout: Option<&Value>) -> Self {
        self.layout = layout.map(|l| facts::from_layout(V::Json(l)));
        // C2-L-056: a base64 source holds bytes, not the text the positions count.
        self.source = layout
            .filter(|l| l.get("encoding").and_then(Value::as_str) != Some("base64"))
            .and_then(|l| l.get("source"))
            .and_then(Value::as_str)
            .map(str::to_string);
        self
    }
}

/// C2-W-134: the codes that a call returning only source cannot hide.
#[must_use]
pub fn has_loss(entries: &[Entry]) -> bool {
    entries.iter().any(|e| {
        matches!(
            e.code.as_str(),
            "writer-refused" | "structure-unspellable" | "field-unspellable"
        )
    })
}

/// The canonical source of `src`: parsed, its facts read from the source unless
/// `normalize` (C2-W-127), and written. Returns the source and the writer's record.
#[must_use]
pub fn format_tree(doc: &Value, src: Option<&str>, settings: &Settings) -> (String, Vec<Entry>) {
    let pool = typed_arena::Arena::new();
    format_view(V::Json(doc), &pool, src, settings)
}

/// [`format_tree`] of a view; `pool` holds the nodes the writer makes and outlives the tree.
pub(crate) fn format_view<'t>(
    doc: V<'t>,
    pool: &'t Pool,
    src: Option<&str>,
    settings: &Settings,
) -> (String, Vec<Entry>) {
    let mut settings = settings.clone();
    if let Some(src) = src {
        settings.layout = Some(facts::from_source(doc, src));
    }
    write_trimmed(doc, pool, &settings, src.map(str::len), None)
}

/// The nodes the writer makes while it writes a tree (C2-W-089 stand-ins).
pub(crate) type Pool = typed_arena::Arena<Value>;

/// `format` of a body with every default, or `None` where the writer reports a loss.
pub(crate) fn format_plain(body: &str) -> Option<String> {
    let doc = parse_plain(body)?;
    let (out, losses) = format_tree(&doc, Some(body), &Settings::default());
    (!has_loss(&losses)).then_some(out)
}

/// [`format_plain`] in the context of a document with the link keys `keys`:
/// an escape or a shortcut reference in `body` is read as it reads there.
pub(crate) fn format_in_context(body: &str, keys: &LinkKeys) -> Option<String> {
    if keys.is_empty() {
        return format_plain(body);
    }
    let doc = crate::tree_value_in_context(body, keys)?;
    let settings = Settings {
        link_keys: Some(std::sync::Arc::clone(keys)),
        ..Settings::default()
    };
    let (out, losses) = format_tree(&doc, Some(body), &settings);
    (!has_loss(&losses)).then_some(out)
}

pub(crate) fn parse_plain(body: &str) -> Option<Value> {
    crate::api::writer::tree_value(&crate::tree_json(body, &crate::Options::default())).ok()
}

#[cfg(test)]
thread_local! {
    static REPAIR_PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PAIR_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static WRITTEN_BLOCKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PROBE_SORTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static INLINE_BUFFERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static READ_CHARS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod nested_inline_write_test {
    use super::*;

    /// The inline buffers one write of a paragraph of `depth` nested superscripts makes.
    fn buffers(depth: usize) -> usize {
        let src = format!("{}a{}\n", "{^".repeat(depth), "^}".repeat(depth));
        let arena = crate::Arena::for_input(&src);
        let doc =
            crate::parse_resolved_in(&src, &crate::Options::default(), &arena).unwrap_or_default();
        let pool = Pool::new();
        let mut w = Writer::new(&pool, false);
        let before = INLINE_BUFFERS.with(std::cell::Cell::get);
        blocks::write_blocks(&mut w, children(V::Doc(&doc)), "", 0, false);
        assert_eq!(w.lines, [src.trim_end()]);
        INLINE_BUFFERS.with(std::cell::Cell::get) - before
    }

    #[test]
    fn nested_marks_write_into_one_buffer() {
        assert_eq!(buffers(1), buffers(200));
    }
}

#[cfg(test)]
mod prune_candidates_test {
    use super::*;

    fn candidates(src: &str) -> Vec<usize> {
        let arena = crate::Arena::for_input(src);
        let doc =
            crate::parse_resolved_in(src, &crate::Options::default(), &arena).unwrap_or_default();
        prunable_escapes(src, &[], V::Doc(&doc), &mut |_| NoteCall::Open)
    }

    #[test]
    fn a_brace_before_a_bare_pair_opener_is_no_candidate() {
        // carve-rs#4660: `{*` would read as a braced opener and take the strong's mark.
        assert_eq!(candidates("\\{*x* a\n"), [] as [usize; 0]);
        assert_eq!(candidates("\\{/x/ a\n"), [] as [usize; 0]);
        // No pair opens at the mark, so the trial still decides.
        assert_eq!(candidates("\\{*x a\n"), [0]);
    }
}

#[cfg(test)]
mod probe_sort_test {
    use super::PROBE_SORTED;

    /// The escape offsets a probe sorts for nested links (carve-js#5262).
    fn sorted(n: usize) -> usize {
        let src = format!("{}^*[*x](u){}\n", "^*[*[[*".repeat(n), "^]]](u)".repeat(n));
        let before = PROBE_SORTED.with(std::cell::Cell::get);
        let written = crate::api::Carve::new()
            .format_with_diagnostics(src.as_str(), &crate::api::WriteOptions::default());
        assert!(written.is_ok_and(|w| w.diagnostics.is_empty()));
        PROBE_SORTED.with(std::cell::Cell::get) - before
    }

    #[test]
    fn probe_escapes_sort_once_per_node_and_round() {
        let (small, large) = (sorted(300), sorted(1200));
        assert!(
            large <= small * 5,
            "n = 300 sorts {small} offsets, n = 1200 {large}"
        );
    }
}

#[cfg(test)]
mod nested_write_test {
    use super::WRITTEN_BLOCKS;

    /// The top-level blocks every write of a format with `carve.tabs` takes,
    /// nested source-body writes included.
    fn blocks_written(groups: usize) -> usize {
        let unit = |k: usize| {
            format!(
                ":::: tabs\n::: tab \"T0\"\nsee [x{k}][r{k}]\n:::\n\n::: tab \"T1\"\nb\n:::\n::::\n\n[r{k}]: http://x/{k}\n"
            )
        };
        let src: String = (0..groups).map(unit).collect::<Vec<_>>().join("\n");
        let opts = crate::api::WriteOptions {
            extensions: vec!["carve.tabs".into()],
            ..crate::api::WriteOptions::default()
        };
        let before = WRITTEN_BLOCKS.with(std::cell::Cell::get);
        let output = crate::api::Carve::new()
            .format_with_diagnostics(src.as_str(), &opts)
            .map_or_else(|e| e.to_string(), |w| w.output);
        assert_eq!(output, src);
        WRITTEN_BLOCKS.with(std::cell::Cell::get) - before
    }

    #[test]
    fn source_bodies_do_not_copy_every_definition() {
        let (small, large) = (blocks_written(8), blocks_written(32));
        assert!(
            large <= small * 5,
            "8 groups write {small} blocks, 32 groups {large}"
        );
    }
}

#[cfg(test)]
mod group_reads_test {
    use super::*;
    use std::fmt::Write as _;

    /// The characters the read-back parses of `fmt` take on `n` paragraphs of `shape`.
    fn read_chars(shape: &str, n: usize) -> (usize, usize) {
        let mut src = String::new();
        for i in 0..n {
            let _ = write!(src, "{shape} w{i}\n\n");
        }
        let arena = crate::Arena::for_input(&src);
        let doc =
            crate::parse_resolved_in(&src, &crate::Options::default(), &arena).unwrap_or_default();
        let pool = Pool::new();
        let before = READ_CHARS.with(std::cell::Cell::get);
        let (out, losses) = format_view(V::Doc(&doc), &pool, Some(&src), &Settings::default());
        assert!(!out.is_empty() && losses.is_empty());
        (READ_CHARS.with(std::cell::Cell::get) - before, src.len())
    }

    /// carve-rs#4779, carve-rs#4780: a text the rounds of a group wrote before is not parsed again, and the
    /// parses grow with the page.
    #[test]
    fn rounds_read_a_written_text_once() {
        // Each text read once is about 5 and 2.2 times the page; reading the repeats again took 8.4 and 2.9.
        for (shape, tenths) in [(r"x{#a\_--}", 60), (r"{_=\._}", 25)] {
            let (small, small_src) = read_chars(shape, 64);
            let (large, large_src) = read_chars(shape, 256);
            assert!(
                large * 10 <= tenths * large_src,
                "{shape}: {large} characters read for {large_src}"
            );
            assert!(
                large * small_src <= small * large_src * 11 / 10,
                "{shape}: {small} then {large}"
            );
        }
    }
}

#[cfg(test)]
mod first_write_test {
    use super::*;
    use std::fmt::Write as _;

    /// The output of `fmt` on `n` paragraphs of `shape` and the characters its read-back parses take.
    fn format_page(shape: &str, n: usize) -> (String, usize, usize) {
        let mut src = String::new();
        for i in 0..n {
            let _ = write!(src, "{shape} w{i}\n\n");
        }
        let arena = crate::Arena::for_input(&src);
        let doc =
            crate::parse_resolved_in(&src, &crate::Options::default(), &arena).unwrap_or_default();
        let pool = Pool::new();
        let before = READ_CHARS.with(std::cell::Cell::get);
        let (out, losses) = format_view(V::Doc(&doc), &pool, Some(&src), &Settings::default());
        assert!(losses.is_empty());
        (
            out,
            READ_CHARS.with(std::cell::Cell::get) - before,
            src.len(),
        )
    }

    /// carve-rs#4779, carve-rs#4780, carve-rs#4788: the first write places the backslashes of C2-W-125 cases five,
    /// six and nine, so each paragraph is read back once (the rounds took two to five reads).
    #[test]
    fn case_backslashes_need_one_read() {
        for shape in [
            r"x{#a\_--}",
            r"{_\=._}",
            r"See \[foo[https://e.com/?q=\](uri)](https://e.com/x) and \[x [a\]b](u) with *em* text.",
        ] {
            let (out, small, small_src) = format_page(shape, 64);
            assert!(out.starts_with(&format!("{shape} w0\n")), "{shape}: {out}");
            let (_, large, large_src) = format_page(shape, 256);
            assert!(
                large * 10 <= 12 * large_src,
                "{shape}: {large} characters read for {large_src}"
            );
            assert!(
                large * small_src <= small * large_src * 11 / 10,
                "{shape}: {small} then {large}"
            );
        }
    }
}

#[cfg(test)]
mod plain_table_certificate_test {
    use super::*;

    fn format_typed(src: &str) -> String {
        let arena = crate::Arena::for_input(src);
        let doc =
            crate::parse_resolved_in(src, &crate::Options::default(), &arena).unwrap_or_default();
        let pool = Pool::new();
        let (out, losses) = format_view(V::Doc(&doc), &pool, Some(src), &Settings::default());
        assert_eq!(losses.len(), 0);
        out
    }

    #[test]
    fn ordinary_rows_need_no_repair_parse() {
        let before = REPAIR_PARSES.with(std::cell::Cell::get);
        let pairs_before = PAIR_SCANS.with(std::cell::Cell::get);
        let output = format_typed("|alpha|beta|\n|7|x|\n");
        assert_eq!(output, "| alpha | beta |\n| 7 | x |\n");
        assert_eq!(REPAIR_PARSES.with(std::cell::Cell::get) - before, 0);
        assert_eq!(PAIR_SCANS.with(std::cell::Cell::get) - pairs_before, 0);
    }

    #[test]
    fn marked_cells_keep_the_repair_parse() {
        let before = REPAIR_PARSES.with(std::cell::Cell::get);
        let output = format_typed("|*alpha*|beta|\n");
        assert_eq!(output, "| *alpha* | beta |\n");
        assert!(REPAIR_PARSES.with(std::cell::Cell::get) > before);
    }

    #[test]
    fn explicit_empty_text_keeps_the_repair_parse() {
        use crate::ast::{
            CellParagraph, Document, Node, Table, TableBody, TableCell, TableColumn, TableRow, Text,
        };

        let arena = crate::Arena::new();
        let text = Node::Text(arena.alloc(Text::default()));
        let paragraph = Node::CellParagraph(arena.alloc(CellParagraph {
            children: arena.one(text),
            ..CellParagraph::default()
        }));
        let cell = Node::TableCell(arena.alloc(TableCell {
            children: arena.one(paragraph),
            ..TableCell::default()
        }));
        let row = Node::TableRow(arena.alloc(TableRow {
            children: arena.one(cell),
            ..TableRow::default()
        }));
        let body = Node::TableBody(arena.alloc(TableBody {
            children: arena.one(row),
            ..TableBody::default()
        }));
        let table = Node::Table(arena.alloc(Table {
            children: arena.one(body),
            columns: arena.one(Node::TableColumn(arena.alloc(TableColumn::default()))),
            ..Table::default()
        }));
        let doc = Document {
            ast_version: "2.0",
            children: arena.one(table),
            ..Document::default()
        };
        let pool = Pool::new();
        let before = REPAIR_PARSES.with(std::cell::Cell::get);
        let _ = format_view(V::Doc(&doc), &pool, None, &Settings::default());
        assert!(REPAIR_PARSES.with(std::cell::Cell::get) > before);
    }

    #[test]
    fn zero_cell_row_keeps_its_closing_pipe() {
        let mut doc = parse_plain("||\n").unwrap_or_default();
        if let Some(row_children) = doc.pointer_mut("/children/0/children/0/children/0/children") {
            *row_children = serde_json::json!([]);
        }
        let (out, losses) = write_value(&doc, &Settings::default());
        assert_eq!(out, "||\n");
        assert!(losses.iter().any(|e| e.code == "structure-unspellable"));
    }
}

#[cfg(test)]
mod batched_repair_test {
    use super::*;

    #[test]
    fn dense_superscripts_need_only_a_few_rounds() -> Result<(), Box<dyn std::error::Error>> {
        let mut de = serde_json::Deserializer::from_str(include_str!(
            "../../spec-conformance/clause-cases.json"
        ));
        de.disable_recursion_limit();
        let cases: Value = serde::Deserialize::deserialize(&mut de)?;
        let case = cases
            .get("cases")
            .and_then(Value::as_array)
            .and_then(|cases| {
                cases
                    .iter()
                    .find(|c| c.get("id").and_then(Value::as_str) == Some("CC-1566"))
            })
            .ok_or("CC-1566")?;
        let source = case.get("input").and_then(Value::as_str).ok_or("source")?;
        let expected = case
            .get("expected")
            .and_then(|value| value.get("carve"))
            .and_then(Value::as_str)
            .ok_or("output")?;
        let tree = parse_plain(source).ok_or("parsed source")?;
        let before = REPAIR_PARSES.with(std::cell::Cell::get);
        let (output, losses) = format_tree(&tree, Some(source), &Settings::default());
        let rounds = REPAIR_PARSES.with(std::cell::Cell::get) - before;
        assert_eq!(output, expected);
        assert!(losses.is_empty());
        // This paragraph nests to the inline depth cap. Its repair rounds may
        // follow that depth, but no longer one round per escaped byte.
        assert!(rounds <= 200, "{rounds} read-back parses for one paragraph");
        Ok(())
    }

    #[test]
    fn nested_links_take_a_few_repair_parses() -> Result<(), Box<dyn std::error::Error>> {
        // carve#6411: the probe rounds escape by kind (C2-W-088), not one escape per parse.
        for n in [146, 584] {
            let src = format!("{}^*[*x](u){}\n", "^*[*[[*".repeat(n), "^]]](u)".repeat(n));
            let tree = parse_plain(&src).ok_or("parsed source")?;
            let before = REPAIR_PARSES.with(std::cell::Cell::get);
            let (output, losses) = format_tree(&tree, Some(&src), &Settings::default());
            let rounds = REPAIR_PARSES.with(std::cell::Cell::get) - before;
            assert!(losses.is_empty());
            assert!(rounds < 20, "{rounds} read-back parses at n = {n}");
            let again = parse_plain(&output).ok_or("parsed output")?;
            assert_eq!(
                format_tree(&again, Some(&output), &Settings::default()).0,
                output
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod nested_span_import_cost_test {
    use super::READ_CHARS;
    use crate::api::{Carve, ImportFormat, ImportOptions, LossOptions};

    fn read_chars(depth: usize) -> Result<usize, Box<dyn std::error::Error>> {
        let html = format!("<p>{}</p>\n", "<span title=\"a\nb\">[ ".repeat(depth));
        let before = READ_CHARS.with(std::cell::Cell::get);
        let result = Carve::new().import_from(
            &html,
            ImportFormat::Html,
            &ImportOptions::default(),
            &LossOptions::default(),
        )?;
        assert!(!result.source.is_empty());
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|d| d.code == "writer-refused")
        );
        Ok(READ_CHARS.with(std::cell::Cell::get) - before)
    }

    #[test]
    fn one_lone_bracket_per_nested_span_reads_linearly() -> Result<(), Box<dyn std::error::Error>> {
        let (small, large) = (read_chars(25)?, read_chars(100)?);
        let capped = read_chars(200)?;
        assert!(
            large <= small * 5,
            "25 spans read {small} chars, 100 read {large}"
        );
        assert!(
            capped <= small * 8,
            "25 spans read {small} chars, 200 read {capped}"
        );
        Ok(())
    }
}

#[cfg(test)]
mod capped_run_test {
    use super::*;
    use crate::api::{Carve, ImportFormat, ImportOptions, LossOptions};

    /// carve#6474: `n` brackets, a link and a pipe, a line break, `| x |` and `n` brackets more.
    fn brackets(n: usize) -> String {
        let run = "[".repeat(n);
        format!("{run}x [[t](/v)]|\n| x |{run}a\n")
    }

    fn import(
        src: &str,
        format: ImportFormat,
    ) -> Result<(String, usize, usize), Box<dyn std::error::Error>> {
        let before = REPAIR_PARSES.with(std::cell::Cell::get);
        let out = Carve::new().import_from(
            src,
            format,
            &ImportOptions::default(),
            &LossOptions::default(),
        )?;
        let parses = REPAIR_PARSES.with(std::cell::Cell::get) - before;
        Ok((out.source, out.diagnostics.len(), parses))
    }

    #[test]
    fn bracket_runs_past_the_depth_cap_take_a_few_rounds() -> Result<(), Box<dyn std::error::Error>>
    {
        for format in [ImportFormat::Markdown, ImportFormat::Djot] {
            assert_eq!(import(&brackets(8), format)?.0, brackets(8));
            for n in [400, 1600] {
                let (source, losses, parses) = import(&brackets(n), format)?;
                assert_eq!(losses, 0);
                assert!(parses < 40, "{parses} read-back parses at n = {n}");
                // The run keeps its first bracket and those from the depth cap on escaped, as one round each did.
                let first = source.lines().next().unwrap_or("");
                assert!(first.starts_with("\\[["));
                assert!(first.ends_with("x \\[[t](/v)]|"));
            }
            let (source, losses, _) = import(&brackets(4000), format)?;
            assert_eq!(losses, 0);
            assert!(source.contains("[t](/v)]|"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod capped_probe_test {
    use super::*;

    /// carve-rs#4719: a probe reads nested content apart from the constructs around it. Read from the top, the
    /// content past the depth cap would nest again, and each level would be probed over the whole content.
    #[test]
    fn probes_of_content_past_the_cap_read_it_at_its_depth() {
        let u = |t: &str, n: usize| t.repeat(n);
        let source = format!(
            "{}[[[]]](){}{}{}{}%x ^[\n%a \\] b^^]\\]\n",
            u("{^", 24),
            u("{^", 192),
            u("^}", 21),
            u("{^", 378),
            u("^}", 276)
        );
        let tree = parse_plain(&source).unwrap_or_default();
        let before = READ_CHARS.with(std::cell::Cell::get);
        let (output, losses) = format_tree(&tree, Some(&source), &Settings::default());
        let read = READ_CHARS.with(std::cell::Cell::get) - before;
        assert!(losses.is_empty() && !output.is_empty());
        assert!(
            read <= 60 * source.len(),
            "{read} characters read back for {} written",
            source.len()
        );
    }
}

#[cfg(test)]
mod escape_prune_test {
    use super::*;

    /// The characters the read-back parses of one `format` of `unit` repeated `n` times read.
    fn read_chars(unit: &str, written: &str, n: usize) -> usize {
        let source = unit.repeat(n);
        let tree = parse_plain(&source).unwrap_or_default();
        let before = READ_CHARS.with(std::cell::Cell::get);
        let (output, losses) = format_tree(&tree, Some(&source), &Settings::default());
        assert_eq!(output, written.repeat(n));
        assert_eq!(losses.len(), 0);
        READ_CHARS.with(std::cell::Cell::get) - before
    }

    fn assert_linear(unit: &str, written: &str) {
        let (small, large) = (
            read_chars(unit, written, 16),
            read_chars(unit, written, 128),
        );
        assert!(
            large <= small * 12,
            "16 units read {small} characters, 128 units {large}"
        );
    }

    #[test]
    fn list_items_with_kept_escapes_read_linearly() {
        assert_linear("* * * {\\{#i}\n* * * # h\n", "* * * {\\{#i}\n* * * = h\n");
    }

    #[test]
    fn paragraph_lines_with_kept_escapes_read_linearly() {
        assert_linear("w {\\{#i}\n", "w {\\{#i}\n");
    }
}

/// The whole-tree checks the writer makes on a supplied JSON tree, read in one walk: each tree
/// walk of a large import misses the cache throughout.
#[allow(clippy::struct_excessive_bools)] // one flag per check
struct JsonFacts {
    /// [`has_attribute_lines`]
    attribute_lines: bool,
    /// [`has_adjacent_plain_text`]
    adjacent_plain_text: bool,
    /// A text value ends in a space or tab, as in `trim::has_spaced_text`.
    spaced_text: bool,
    /// [`pairs::json_has_mark_text`]
    mark_text: bool,
    /// A table cell spans more than one row or column, as in `spans::json_has_counts`.
    counts: bool,
    /// [`json_estimate`]
    estimate: usize,
}

impl JsonFacts {
    fn of(doc: &Value) -> Self {
        let mut facts = Self {
            attribute_lines: false,
            adjacent_plain_text: false,
            spaced_text: false,
            mark_text: false,
            counts: false,
            estimate: 0,
        };
        facts.estimate = facts.visit(doc);
        facts
    }

    fn visit(&mut self, value: &Value) -> usize {
        match value {
            Value::String(s) => s.len().saturating_mul(2).saturating_add(2),
            Value::Array(items) => {
                if !self.adjacent_plain_text {
                    self.adjacent_plain_text =
                        items.windows(2).any(|pair| pair.iter().all(is_plain_text));
                }
                items
                    .iter()
                    .fold(2, |sum: usize, item| sum.saturating_add(self.visit(item)))
            }
            Value::Object(map) => {
                self.read(map);
                map.values()
                    .fold(2, |sum: usize, item| sum.saturating_add(self.visit(item)))
            }
            _ => 2,
        }
    }

    fn read(&mut self, map: &serde_json::Map<String, Value>) {
        if !self.attribute_lines {
            self.attribute_lines = map
                .get("attrs")
                .and_then(|a| a.get("keyValues"))
                .and_then(Value::as_array)
                .is_some_and(|pairs| {
                    pairs.iter().any(|pair| {
                        pair.get(1)
                            .and_then(Value::as_str)
                            .is_some_and(|v| v.contains('\n'))
                    })
                });
        }
        match map.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = map.get("value").and_then(Value::as_str) {
                    self.spaced_text |= text.ends_with([' ', '\t']);
                    self.mark_text |= text.bytes().any(pairs::is_asked_byte);
                }
            }
            Some("table_cell") => {
                let counted = |field| map.get(field).and_then(Value::as_i64).unwrap_or(1) > 1;
                self.counts |= counted("colspan") || counted("rowspan");
            }
            _ => {}
        }
    }
}

/// C2-W-050: a key value cannot carry a line feed in its one-line brace block.
#[cfg(test)]
fn has_attribute_lines(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            if map
                .get("attrs")
                .and_then(|a| a.get("keyValues"))
                .and_then(Value::as_array)
                .is_some_and(|pairs| {
                    pairs.iter().any(|pair| {
                        pair.get(1)
                            .and_then(Value::as_str)
                            .is_some_and(|v| v.contains('\n'))
                    })
                })
            {
                return true;
            }
            map.values().any(has_attribute_lines)
        }
        Value::Array(items) => items.iter().any(has_attribute_lines),
        _ => false,
    }
}

fn replace_attribute_lines(value: &mut Value, path: &str, losses: &mut Vec<Entry>) {
    match value {
        Value::Object(map) => {
            if let Some(pairs) = map
                .get_mut("attrs")
                .and_then(|a| a.get_mut("keyValues"))
                .and_then(Value::as_array_mut)
            {
                for (index, pair) in pairs.iter_mut().enumerate() {
                    if let Some(Value::String(text)) = pair.get_mut(1)
                        && text.contains('\n')
                    {
                        *text = text.replace('\n', " ");
                        losses.push(loss_entry(
                            "field-unspellable",
                            "Carve source cannot spell a line feed in an attribute value".into(),
                            format!("{path}/attrs/keyValues/{index}/1"),
                            None,
                        ));
                    }
                }
            }
            for (key, child) in map.iter_mut() {
                if key != "attrs" {
                    replace_attribute_lines(
                        child,
                        &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                        losses,
                    );
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                replace_attribute_lines(item, &format!("{path}/{index}"), losses);
            }
        }
        _ => {}
    }
}

/// C2-A-040: a run of unadorned text nodes has one value for writing.
/// Keep one empty node where the whole run is empty, so its loss remains visible.
struct Coalesced {
    kept: std::collections::HashMap<String, Vec<usize>>,
    offsets: std::collections::HashMap<String, usize>,
}

impl Coalesced {
    fn remap(&self, path: &str) -> String {
        let mut original = String::new();
        let mut result = String::new();
        for step in path.split('/').skip(1) {
            let old = self.kept.get(&original).and_then(|indices| {
                step.parse::<usize>()
                    .ok()
                    .and_then(|index| indices.get(index))
            });
            original.push('/');
            if let Some(index) = old {
                original.push_str(&index.to_string());
            } else {
                original.push_str(step);
            }
            result.push('/');
            if let Some(index) = old {
                result.push_str(&index.to_string());
            } else {
                result.push_str(step);
            }
        }
        result
    }

    fn forward(&self, path: &str) -> String {
        let mut original = String::new();
        let mut result = String::new();
        for step in path.split('/').skip(1) {
            let mapped = self.kept.get(&original).and_then(|indices| {
                step.parse::<usize>().ok().map(|index| {
                    indices
                        .partition_point(|kept| *kept <= index)
                        .saturating_sub(1)
                })
            });
            original.push('/');
            original.push_str(step);
            result.push('/');
            if let Some(index) = mapped {
                result.push_str(&index.to_string());
            } else {
                result.push_str(step);
            }
        }
        result
    }
}

fn plain_text_value(node: &Value) -> Option<&str> {
    let map = node.as_object()?;
    (map.len() == 2 && map.get("type")?.as_str()? == "text")
        .then(|| map.get("value").and_then(Value::as_str))
        .flatten()
}

fn coalesce_text_list(
    nodes: &mut Vec<Value>,
    path: &str,
    kept: &mut std::collections::HashMap<String, Vec<usize>>,
    offsets: &mut std::collections::HashMap<String, usize>,
) {
    let original_len = nodes.len();
    let mut retained = Vec::with_capacity(original_len);
    let mut source = std::mem::take(nodes).into_iter().enumerate().peekable();
    while let Some((index, mut node)) = source.next() {
        if let Some(first) = plain_text_value(&node) {
            let mut joined = first.to_string();
            while source
                .peek()
                .and_then(|(_, next)| plain_text_value(next))
                .is_some()
            {
                if let Some((next_index, next)) = source.next()
                    && let Some(piece) = plain_text_value(&next)
                {
                    offsets.insert(format!("{path}/{next_index}"), joined.len());
                    joined.push_str(piece);
                }
            }
            if let Some(value) = node.get_mut("value") {
                *value = Value::String(joined);
            }
        }
        retained.push(index);
        nodes.push(node);
    }
    if retained.len() != original_len || retained.iter().enumerate().any(|(i, old)| i != *old) {
        kept.insert(path.to_string(), retained);
    }
}

fn restore_table_form(
    value: &mut Value,
    original_table: Option<Value>,
    path: &str,
    kept: &mut std::collections::HashMap<String, Vec<usize>>,
    offsets: &mut std::collections::HashMap<String, usize>,
) {
    if let Some(original) = original_table
        && table::writes_delimited(V::Json(&original), None)
            != table::writes_delimited(V::Json(value), None)
    {
        *value = original;
        let prefix = format!("{path}/");
        kept.retain(|at, _| at != path && !at.starts_with(&prefix));
        offsets.retain(|at, _| at != path && !at.starts_with(&prefix));
    }
}

fn coalesce_plain_text(
    value: &mut Value,
    path: &str,
    kept: &mut std::collections::HashMap<String, Vec<usize>>,
    offsets: &mut std::collections::HashMap<String, usize>,
) {
    let original_table = (value.get("type").and_then(Value::as_str) == Some("table")
        && has_adjacent_plain_text(value))
    .then(|| value.clone());
    match value {
        Value::Object(fields) => {
            for (key, child) in fields.iter_mut() {
                coalesce_plain_text(
                    child,
                    &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                    kept,
                    offsets,
                );
            }
            restore_table_form(value, original_table, path, kept, offsets);
        }
        Value::Array(nodes) => {
            for (index, child) in nodes.iter_mut().enumerate() {
                coalesce_plain_text(child, &format!("{path}/{index}"), kept, offsets);
            }
            coalesce_text_list(nodes, path, kept, offsets);
        }
        _ => {}
    }
}

#[cfg(test)]
mod coalesced_path_tests {
    use super::{Coalesced, coalesce_plain_text};
    use serde_json::json;

    #[test]
    fn paths_of_following_and_nested_nodes_survive_text_coalescing() {
        let mut tree = json!({"children": [
            {"type":"text","value":"a"}, {"type":"text","value":"b"},
            {"children": [{"type":"text","value":"c"}, {"type":"text","value":"d"}, {"type":"strong"}]}
        ]});
        let mut kept = std::collections::HashMap::new();
        let mut offsets = std::collections::HashMap::new();
        coalesce_plain_text(&mut tree, "", &mut kept, &mut offsets);
        let paths = Coalesced { kept, offsets };
        assert_eq!(paths.forward("/children/1"), "/children/0");
        assert_eq!(paths.offsets.get("/children/1"), Some(&1));
        assert_eq!(
            paths.forward("/children/2/children/2"),
            "/children/1/children/1"
        );
        assert_eq!(paths.offsets.get("/children/2/children/1"), Some(&1));
        assert_eq!(
            paths.remap("/children/1/children/1/type"),
            "/children/2/children/2/type"
        );
    }
}

/// A text node with no field but its value.
fn is_plain_text(node: &Value) -> bool {
    node.as_object().is_some_and(|m| {
        m.len() == 2
            && m.get("type").and_then(Value::as_str) == Some("text")
            && m.get("value").and_then(Value::as_str).is_some()
    })
}

fn has_adjacent_plain_text(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.values().any(has_adjacent_plain_text),
        Value::Array(nodes) => {
            nodes.windows(2).any(|pair| {
                pair.first().is_some_and(is_plain_text) && pair.get(1).is_some_and(is_plain_text)
            }) || nodes.iter().any(has_adjacent_plain_text)
        }
        _ => false,
    }
}

/// Writes the tree `doc` (its JSON value, read and checked).
#[must_use]
pub fn write_value(doc: &Value, settings: &Settings) -> (String, Vec<Entry>) {
    let pool = Pool::new();
    let facts = JsonFacts::of(doc);
    if !facts.attribute_lines && !facts.adjacent_plain_text {
        return write_trimmed(V::Json(doc), &pool, settings, None, Some(&facts));
    }
    let mut normalized = doc.clone();
    let mut losses = Vec::new();
    let mut kept = std::collections::HashMap::new();
    let mut offsets = std::collections::HashMap::new();
    coalesce_plain_text(&mut normalized, "", &mut kept, &mut offsets);
    let coalesced = Coalesced { kept, offsets };
    replace_attribute_lines(&mut normalized, "", &mut losses);
    let mut moved = settings.clone();
    if let Some(layout) = &mut moved.layout {
        for entry in layout {
            entry.path = coalesced.forward(&entry.path);
            if let Some(wraps) = &mut entry.facts.wraps {
                for wrap in wraps {
                    wrap.offset = wrap
                        .offset
                        .saturating_add(coalesced.offsets.get(&wrap.path).copied().unwrap_or(0));
                    wrap.path = coalesced.forward(&wrap.path);
                }
            }
        }
    }
    let (source, mut written_losses) = write_view(V::Json(&normalized), &pool, &moved);
    for entry in &mut written_losses {
        if let Some(path) = &entry.path {
            entry.path = Some(coalesced.remap(path));
        }
    }
    for entry in &mut losses {
        if let Some(path) = &entry.path {
            entry.path = Some(coalesced.remap(path));
        }
    }
    written_losses.extend(losses);
    (source, written_losses)
}

/// [`write_value`] of a view; `pool` outlives the tree.
pub(crate) fn write_view<'t>(
    doc: V<'t>,
    pool: &'t Pool,
    settings: &Settings,
) -> (String, Vec<Entry>) {
    write_trimmed(doc, pool, settings, None, None)
}

/// [`write_view`] with the length of the source the tree was read from, which sizes the repair budget.
/// `facts` are those of `doc` where it is a JSON value.
fn write_trimmed<'t>(
    doc: V<'t>,
    pool: &'t Pool,
    settings: &Settings,
    source_len: Option<usize>,
    facts: Option<&JsonFacts>,
) -> (String, Vec<Entry>) {
    // A parse gives no such spaces, so only a supplied JSON tree is trimmed.
    let recorded = |at: &str| {
        settings
            .layout
            .iter()
            .flatten()
            .find(|entry| entry.path == at)
            .and_then(|entry| entry.facts.source_format.as_deref())
    };
    let delimited = |table: &Value, at: &str| table::writes_delimited(V::Json(table), recorded(at));
    if let V::Json(value) = doc
        && let Some(trimmed) = trim::trim_with(value, &delimited, facts.map(|f| f.spaced_text))
    {
        let pool = Pool::new();
        let moved;
        let settings = if let Some(layout) = &settings.layout {
            moved = Settings {
                layout: Some(trimmed.forward_layout(layout)),
                ..settings.clone()
            };
            &moved
        } else {
            settings
        };
        let (source, mut entries) =
            write_sized(V::Json(&trimmed.doc), &pool, settings, source_len, None);
        for entry in &mut entries {
            if let Some(path) = &entry.path {
                entry.path = Some(trimmed.remap(path));
            }
        }
        return (source, entries);
    }
    write_sized(doc, pool, settings, source_len, facts)
}

/// [`write_trimmed`] after the trim.
fn write_sized<'t>(
    doc: V<'t>,
    pool: &'t Pool,
    settings: &Settings,
    source_len: Option<usize>,
    facts: Option<&JsonFacts>,
) -> (String, Vec<Entry>) {
    let estimate = || facts.map_or_else(|| written_estimate(doc), |f| f.estimate);
    let budget = repair_budget(source_len.unwrap_or_else(estimate));
    let Some(marks) = spans::add(doc, facts.map(|f| f.counts)) else {
        return write_tree(doc, pool, settings, budget, facts);
    };
    let (source, mut entries) = write_tree(V::Json(&marks.doc), pool, settings, budget, None);
    for entry in &mut entries {
        if let Some(path) = &entry.path {
            entry.path = Some(marks.remap(path));
        }
    }
    for at in &marks.unspelled {
        let path = format!("{at}/rowspan");
        if !entries
            .iter()
            .any(|e| e.path.as_deref() == Some(path.as_str()))
        {
            let message =
                "Carve source cannot spell field rowspan of a table_cell node here".to_string();
            entries.push(loss_entry("field-unspellable", message, path, None));
        }
    }
    (source, entries)
}

fn write_tree<'t>(
    doc: V<'t>,
    pool: &'t Pool,
    settings: &Settings,
    budget: usize,
    facts: Option<&JsonFacts>,
) -> (String, Vec<Entry>) {
    let mut w = Writer::new(pool, settings.align_tables);
    w.work_limit = budget;
    w.no_mark_text = match facts {
        Some(f) => !f.mark_text,
        None => matches!(doc, V::Json(value) if !pairs::json_has_mark_text(value)),
    };
    w.extensions.clone_from(&settings.extensions);
    if settings.extensions.iter().any(|e| e == "carve.tabs") {
        w.matchers.push(state::Matcher {
            form: "div".into(),
            name: "tabs".into(),
        });
    }
    w.link_keys = match &settings.link_keys {
        Some(keys) => std::sync::Arc::clone(keys),
        None => std::sync::Arc::new(
            children(doc)
                .iter()
                .filter(|n| ty(n) == "link_reference_definition")
                .filter_map(|n| node::s(n, "key"))
                .map(str::to_string)
                .collect(),
        ),
    };
    #[cfg(test)]
    WRITTEN_BLOCKS.with(|count| count.set(count.get() + children(doc).len()));
    if let Some(entries) = &settings.layout {
        facts::read(&mut w, doc, entries);
    }
    read_abbreviations(&mut w, doc);
    let written = write_document(&mut w, doc);
    let source = controls::keep_above(&w, written, settings.source.as_deref());
    (source, located(doc, &w))
}

/// C2-B-085: the first definition of a term wins.
fn read_abbreviations(w: &mut Writer<'_>, doc: V<'_>) {
    let mut seen = HashSet::new();
    for c in children(doc) {
        if ty(c) != "abbreviation_def" {
            continue;
        }
        let key = node::s(c, "key").unwrap_or("").to_string();
        if seen.insert(key.clone()) {
            w.abbreviations
                .push((key, node::s(c, "expansion").unwrap_or("").to_string()));
        }
    }
    w.abbreviations.sort_by(|a, b| a.0.cmp(&b.0));
}

/// C2-W-158: the record in document order of its paths. Entries at one place keep the order they were found in.
fn located(doc: V<'_>, w: &Writer<'_>) -> Vec<Entry> {
    let wanted: AddrSet<usize> = w.sp.losses.iter().filter_map(|l| l.node).collect();
    let found = node::places(doc, &wanted);
    let mut nodes: AddrMap<usize, V<'_>> = AddrMap::default();
    if !wanted.is_empty() {
        let mut all = vec![doc];
        node::descendants(doc, &mut all);
        for n in all {
            if wanted.contains(&id(n)) {
                nodes.insert(id(n), n);
            }
        }
    }
    let mut keys = Vec::new();
    let mut entries: Vec<Entry> =
        w.sp.losses
            .iter()
            .map(|l| {
                let place = l.node.and_then(|n| found.get(&n));
                keys.push(place.map(|(_, rank)| (*rank, usize::from(l.step.is_some()))));
                let path = place.map(|(p, _)| p.clone()).unwrap_or_default()
                    + l.step.as_deref().unwrap_or("");
                let range = place
                    .and(l.node)
                    .and_then(|n| nodes.get(&n).copied())
                    .and_then(span)
                    .map(|(start_offset, end_offset)| Range {
                        start_offset,
                        end_offset,
                    });
                loss_entry(l.code, l.message.clone(), path, range)
            })
            .collect();
    root_metadata_losses(doc, &mut entries);
    // The fields of the document stand before its blocks.
    keys.resize(entries.len(), Some((0, 1)));
    if entries.len() < 2 {
        return entries;
    }
    if keys.iter().all(Option::is_some) {
        let mut keyed: Vec<_> = keys.into_iter().zip(entries).collect();
        keyed.sort_by_key(|(key, _)| *key);
        return keyed.into_iter().map(|(_, e)| e).collect();
    }
    // A node the walk did not find: its path alone places the entry.
    let mut keyed: Vec<_> = entries
        .into_iter()
        .map(|e| (node::path_key(doc, e.path.as_deref().unwrap_or("")), e))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.into_iter().map(|(_, e)| e).collect()
}

fn loss_entry(code: &str, message: String, path: String, range: Option<Range>) -> Entry {
    Entry {
        code: code.to_string(),
        severity: "warning".into(),
        channel: "render-loss".into(),
        message,
        path: Some(path),
        range,
        file: None,
        foreign_type: None,
    }
}

fn root_metadata_losses(doc: V<'_>, out: &mut Vec<Entry>) {
    let metadata = doc.get("metadata");
    for name in ["headers", "footers", "abstract", "keywords"] {
        if metadata.and_then(|m| m.get(name)).is_some() {
            out.push(loss_entry(
                "field-unspellable",
                format!("Carve source cannot spell root metadata.{name}"),
                format!("/metadata/{name}"),
                None,
            ));
        }
    }
    for name in ["lang", "dir"] {
        if doc.get(name).is_some() {
            out.push(loss_entry(
                "field-unspellable",
                format!("Carve source cannot spell document {name}"),
                format!("/{name}"),
                None,
            ));
        }
    }
    if metadata.and_then(|m| m.get("fields")).is_some()
        && !children(doc).iter().any(|n| ty(n) == "frontmatter")
    {
        out.push(loss_entry(
            "field-unspellable",
            "Carve source cannot spell root metadata.fields".into(),
            "/metadata/fields".into(),
            None,
        ));
    }
}

fn last_block<'t>(nodes: Arr<'t>, from: usize, to: usize, list: Option<N<'t>>) -> Option<N<'t>> {
    nodes
        .slice(from, to)
        .and_then(|g| g.iter().rev().find(|n| ty(n) != "comment"))
        .or(list)
}

fn write_document<'t>(w: &mut Writer<'t>, doc: N<'t>) -> String {
    let nodes = children(doc);
    w.leading_frontmatter = nodes
        .iter()
        .next()
        .filter(|n| ty(*n) == "frontmatter" && !has(*n, "attrs"))
        .map(id);
    w.top_blocks = nodes.iter().map(id).collect();
    (w.below_list, w.indented) = blocks::below_list(w, nodes);
    let mut out: Vec<String> = Vec::new();
    let mut before: Option<N<'t>> = None;
    let mut list: Option<N<'t>> = None;
    let mut at = 0;
    while at < nodes.len() {
        let from = at;
        at = repair_group(w, doc, from, out.is_empty());
        // C2-W-146: test the written text with the five pair rules and `unattached-caption`, and reparse each escape round.
        for _ in 0..16 {
            if w.group_certificate == GroupCertificate::PlainRows {
                break;
            }
            #[cfg(test)]
            PAIR_SCANS.with(|count| count.set(count.get() + 1));
            let asked = pairs::asked(w, nodes.slice(from, at).unwrap_or_else(Arr::empty));
            if asked.is_empty() {
                break;
            }
            let saved_lines = w.lines.clone();
            // Only the group's entries: a copy of the whole map made a document of many groups quadratic.
            let saved_escapes = group_escapes(w, nodes.slice(from, at).unwrap_or_else(Arr::empty));
            let saved_losses = w.sp.losses.len();
            pairs::apply(w, &asked);
            let next = repair_group(w, doc, from, out.is_empty());
            if w.sp.losses.len() > saved_losses {
                let group = nodes.slice(from, at.max(next)).unwrap_or_else(Arr::empty);
                for n in &group_ids(w, group) {
                    w.sp.escapes.remove(n);
                }
                w.sp.escapes.extend(saved_escapes);
                w.lines = saved_lines;
                w.sp.losses.truncate(saved_losses);
                break;
            }
            at = next;
        }
        if w.lines.is_empty() {
            continue;
        }
        if out.is_empty() && w.blank_first {
            out.push(String::new());
        }
        let blanks = match (before, nodes.get(from)) {
            (Some(b), Some(n)) => blocks::blank_lines_between(w, b, list, n, false),
            _ => 0,
        };
        out.extend(std::iter::repeat_n(String::new(), blanks));
        w.written_chars += blanks + w.lines.iter().map(|l| l.len() + 1).sum::<usize>();
        out.append(&mut w.lines);
        before = at.checked_sub(1).and_then(|i| nodes.get(i));
        list = last_block(nodes, from, at, list);
    }
    if out.is_empty() {
        "\n".into()
    } else {
        out.join("\n") + "\n"
    }
}

/// Whether `node` is written into one group with the block `before` above it.
fn joins_group(w: &mut Writer<'_>, before: V<'_>, node: V<'_>) -> bool {
    if ty(node) == "paragraph" {
        // C2-W-154: the paragraph below the container reads back only in its context, and one without a
        // spelling is reported alone.
        return w
            .below_list
            .get(&id(node))
            .map_or(ty(before) == "definition_list", |b| {
                *b != state::Below::Unspellable
            });
    }
    ty(node) == "comment" && blocks::blank_lines_between(w, before, None, node, false) == 0
}

/// `prior` is the last block of the group before `at` that is no comment, and `following` the block behind the
/// comment lines after `at`.
fn is_bridged<'a>(
    w: &Writer<'_>,
    nodes: Arr<'a>,
    prior: Option<V<'a>>,
    at: usize,
    following: Option<V<'a>>,
) -> bool {
    let Some(prior) = prior else {
        return false;
    };
    if !matches!(ty(prior), "list" | "definition_list" | "footnote") {
        return false;
    }
    let node = nodes.get(at);
    let following = if node.is_some_and(|n| ty(n) == "comment") {
        following
    } else {
        node
    };
    !blocks::following_attributes(w, following).is_empty()
}

fn group_end(w: &mut Writer<'_>, nodes: Arr<'_>, from: usize) -> usize {
    let mut end = from + 1;
    // Kept as the group grows, so a run of comments is walked once rather than once per comment.
    let mut prior = nodes.get(from).filter(|n| ty(*n) != "comment");
    let mut after = 0;
    while let (Some(before), Some(node)) = (nodes.get(end - 1), nodes.get(end)) {
        if after <= end {
            after = blocks::comment_lines_end(nodes, end);
        }
        let bridged = is_bridged(w, nodes, prior, end, nodes.get(after));
        let hoisted = ty(node) == "comment" && bridged;
        let belongs = ty(before) == "comment" && bridged;
        if !hoisted && !belongs && !joins_group(w, before, node) {
            break;
        }
        if ty(node) != "comment" {
            prior = Some(node);
        }
        end += 1;
    }
    end
}

fn reset_lines(w: &mut Writer<'_>) {
    // The glue depends on the form of the node before, so the next write decides it again (carve#6420).
    for n in w.glued.drain() {
        if w.forms.get(&n) == Some(&1) {
            w.forms.remove(&n);
        }
    }
    w.lines.clear();
    w.lead = None;
    w.lead_skip = 0;
    w.bare_dot_head = None;
    w.prewritten = None;
    w.clear_starts();
    w.raw_refs.clear();
}

fn group_source(w: &Writer<'_>, first: bool) -> String {
    let mut src = String::with_capacity(w.lines.iter().map(|l| l.len() + 1).sum::<usize>() + 1);
    if !first {
        src.push('\n');
    }
    for line in &w.lines {
        src.push_str(line);
        src.push('\n');
    }
    src
}

fn repair_group<'t>(w: &mut Writer<'t>, doc: N<'t>, from: usize, first: bool) -> usize {
    w.group_certificate = GroupCertificate::None;
    let end = group_end(w, children(doc), from);
    // A loss reported while finding the boundary names a block behind the group (C2-B-137).
    let reported = w.sp.losses.len();
    let group = children(doc).slice(from, end).unwrap_or_else(Arr::empty);
    let saved = SavedGroup::new(w, group);
    w.kind_failed = false;
    let end = repair_rounds(w, doc, from, first, reported);
    if (w.sp.losses.len() == reported && !w.kind_failed) || !w.kind_escapes {
        return end;
    }
    w.kind_failed = false;
    saved.restore(w, reported, group);
    let before = SavedGroup::new(w, group);
    // C2-W-005: where that loses a node, the closer repairs give their kind too, the smallest departure that keeps it.
    w.closer_kinds = true;
    w.group_certificate = GroupCertificate::None;
    let end = repair_rounds(w, doc, from, first, reported);
    w.closer_kinds = false;
    if w.sp.losses.len() == reported && !w.kind_failed {
        return end;
    }
    w.kind_failed = false;
    // C2-W-088: a group whose kind rounds leave a node unread is written by
    // rounds of the reported escapes only.
    before.restore(w, reported, group);
    let unrepaired = SavedGroup::new(w, group);
    w.kind_escapes = false;
    w.group_certificate = GroupCertificate::None;
    let end = repair_rounds(w, doc, from, first, reported);
    w.kind_escapes = true;
    if w.sp.losses.len() == reported {
        return end;
    }
    // C2-W-125: where every form loses a node, a trial that gives the tree only with a backslash behind it is kept.
    let mut last = unrepaired;
    for kinds in [true, false] {
        last.restore(w, reported, group);
        last = SavedGroup::new(w, group);
        w.whole_trials = true;
        w.kind_escapes = kinds;
        w.kind_failed = false;
        w.group_certificate = GroupCertificate::None;
        let end = repair_rounds(w, doc, from, first, reported);
        w.whole_trials = false;
        w.kind_escapes = true;
        if w.sp.losses.len() == reported && !w.kind_failed {
            return end;
        }
        w.kind_failed = false;
    }
    last.restore(w, reported, group);
    w.kind_escapes = false;
    w.group_certificate = GroupCertificate::None;
    let end = repair_rounds(w, doc, from, first, reported);
    w.kind_escapes = true;
    end
}

/// The ids of the nodes of `group` and of their stand-ins.
fn group_ids(w: &Writer<'_>, group: Arr<'_>) -> Vec<usize> {
    let mut ids = Vec::new();
    for node in group {
        node_ids(node, &mut ids);
    }
    let stand_ins: Vec<usize> = ids
        .iter()
        .filter_map(|n| w.stand_ins.get(n))
        .flat_map(|roles| roles.values().map(|n| id(*n)))
        .collect();
    ids.extend(stand_ins);
    ids
}

fn group_escapes(w: &Writer<'_>, group: Arr<'_>) -> Vec<(usize, Vec<usize>)> {
    if w.sp.escapes.is_empty() {
        return Vec::new();
    }
    group_ids(w, group)
        .iter()
        .filter_map(|n| w.sp.escapes.get(n).map(|e| (*n, e.clone())))
        .collect()
}

/// The repair state of a group before its rounds, for the C2-W-088 fallback:
/// only the entries of the group's nodes and their stand-ins, so a document
/// of many groups stays linear.
struct SavedGroup<'t> {
    lines: Vec<String>,
    /// `None` where no entry or stand-in existed to save: `restore` reads the group itself.
    ids: Option<Vec<usize>>,
    escapes: Vec<(usize, Vec<usize>)>,
    attempts: Vec<(usize, state::Attempt<'t>)>,
    lost: Vec<usize>,
    lost_keys: Vec<(usize, Vec<String>)>,
    forms: Vec<(usize, u32)>,
    round_forms: Vec<usize>,
    glued: Vec<usize>,
    blank_first: bool,
}

fn node_ids(v: V<'_>, out: &mut Vec<usize>) {
    if let V::Json(value) = v {
        json_node_ids(value, out);
    } else if let V::Pos(_) | V::Attrs(_) | V::Marker(_) | V::Task(_) = v {
        // Only scalars and arrays of strings below.
        out.push(id(v));
    } else if let Some(items) = v.as_array() {
        for item in items {
            node_ids(item, out);
        }
    } else if v.is_object() {
        out.push(id(v));
        v.each(&mut |_, value| node_ids(value, out));
    }
}

/// [`node_ids`] of a JSON value, walked without the view.
fn json_node_ids(value: &Value, out: &mut Vec<usize>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| json_node_ids(item, out)),
        Value::Object(map) => {
            out.push(id(V::Json(value)));
            map.values().for_each(|item| json_node_ids(item, out));
        }
        _ => {}
    }
}

impl<'t> SavedGroup<'t> {
    fn new(w: &Writer<'t>, group: Arr<'_>) -> Self {
        let nothing_saved = w.stand_ins.is_empty()
            && w.sp.escapes.is_empty()
            && w.sp.attempts.is_empty()
            && w.sp.lost.is_empty()
            && w.sp.lost_keys.is_empty()
            && w.forms.is_empty()
            && w.round_forms.is_empty()
            && w.glued.is_empty();
        let mut ids = Vec::new();
        if !nothing_saved {
            for node in group {
                node_ids(node, &mut ids);
            }
        }
        let stand_ins: Vec<usize> = ids
            .iter()
            .filter_map(|n| w.stand_ins.get(n))
            .flat_map(|roles| roles.values().map(|n| id(*n)))
            .collect();
        ids.extend(stand_ins);
        let take = |n: &usize| *n;
        // An empty map holds no entry of the group: skip its lookups.
        let some = |empty: bool| if empty { &[][..] } else { ids.as_slice() };
        Self {
            lines: w.lines.clone(),
            escapes: some(w.sp.escapes.is_empty())
                .iter()
                .filter_map(|n| w.sp.escapes.get(n).map(|e| (take(n), e.clone())))
                .collect(),
            attempts: some(w.sp.attempts.is_empty())
                .iter()
                .filter_map(|n| w.sp.attempts.get(n).map(|a| (take(n), a.clone())))
                .collect(),
            lost: some(w.sp.lost.is_empty())
                .iter()
                .copied()
                .filter(|n| w.sp.lost.contains(n))
                .collect(),
            lost_keys: some(w.sp.lost_keys.is_empty())
                .iter()
                .filter_map(|n| w.sp.lost_keys.get(n).map(|k| (take(n), k.clone())))
                .collect(),
            forms: some(w.forms.is_empty())
                .iter()
                .filter_map(|n| w.forms.get(n).map(|f| (take(n), *f)))
                .collect(),
            round_forms: some(w.round_forms.is_empty())
                .iter()
                .copied()
                .filter(|n| w.round_forms.contains(n))
                .collect(),
            glued: some(w.glued.is_empty())
                .iter()
                .copied()
                .filter(|n| w.glued.contains(n))
                .collect(),
            blank_first: w.blank_first,
            ids: (!nothing_saved).then_some(ids),
        }
    }

    fn restore(self, w: &mut Writer<'t>, losses: usize, group: Arr<'_>) {
        // Stand-ins made during the rounds belong to the group too.
        let mut fresh = Vec::new();
        for node in group {
            node_ids(node, &mut fresh);
        }
        let mut ids = self.ids.unwrap_or_else(|| fresh.clone());
        let stand_ins: Vec<usize> = fresh
            .iter()
            .filter_map(|n| w.stand_ins.get(n))
            .flat_map(|roles| roles.values().map(|n| id(*n)))
            .collect();
        ids.extend(stand_ins);
        for n in &ids {
            w.sp.escapes.remove(n);
            w.sp.attempts.remove(n);
            w.sp.lost.remove(n);
            w.sp.lost_keys.remove(n);
            w.forms.remove(n);
            w.round_forms.remove(n);
            w.glued.remove(n);
        }
        w.sp.escapes.extend(self.escapes);
        w.sp.attempts.extend(self.attempts);
        w.sp.lost.extend(self.lost);
        w.sp.lost_keys.extend(self.lost_keys);
        w.forms.extend(self.forms);
        w.round_forms.extend(self.round_forms);
        w.glued.extend(self.glued);
        w.lines = self.lines;
        w.sp.losses.truncate(losses);
        w.blank_first = self.blank_first;
    }
}

/// C2-W-133: one repair budget for the whole document, `len` the characters of the document as written.
fn repair_budget(len: usize) -> usize {
    WORK_FLOOR.max(len.saturating_mul(WORK_FACTOR))
}

/// The length of a supplied tree as written, over-estimated: every string twice, a few characters per value.
fn written_estimate(v: V<'_>) -> usize {
    if let V::Json(value) = v {
        return json_estimate(value);
    }
    if let Some(s) = v.as_str() {
        return s.len().saturating_mul(2).saturating_add(2);
    }
    let mut sum: usize = 2;
    if let Some(items) = v.as_array() {
        for item in items {
            sum = sum.saturating_add(written_estimate(item));
        }
    } else if v.is_object() {
        v.each(&mut |_, value| sum = sum.saturating_add(written_estimate(value)));
    }
    sum
}

/// [`written_estimate`] of a JSON value, walked without the view.
fn json_estimate(value: &Value) -> usize {
    match value {
        Value::String(s) => s.len().saturating_mul(2).saturating_add(2),
        Value::Array(items) => items.iter().fold(2, |sum: usize, item| {
            sum.saturating_add(json_estimate(item))
        }),
        Value::Object(map) => map.values().fold(2, |sum: usize, item| {
            sum.saturating_add(json_estimate(item))
        }),
        _ => 2,
    }
}

const MAX_REPAIR_CHARS: usize = 20_000_000;
const MIN_REPAIR_ROUNDS: usize = 32;
const WORK_FACTOR: usize = 1000;
const WORK_FLOOR: usize = 24_000_000;
const MAX_ROUNDS: usize = 8192;

struct History {
    ids: AddrMap<usize, usize>,
    recent: std::collections::VecDeque<RepairKey>,
    seen: HashSet<RepairKey>,
    rounds: usize,
    remaining: i64,
}

/// C2-W-125: once the rounds settle, the stale braced forms are taken out together with the escapes of the
/// group, and the rounds run again from there. Where that loses a node, the settled form stands.
fn repair_rounds<'t>(
    w: &mut Writer<'t>,
    doc: N<'t>,
    from: usize,
    first: bool,
    reported: usize,
) -> usize {
    let end = repair_rounds_once(w, doc, from, first, reported);
    if w.kind_failed || w.sp.losses.len() != reported {
        return end;
    }
    let group = children(doc).slice(from, end).unwrap_or_else(Arr::empty);
    let stale = stale_forms(w, group);
    if stale.is_empty() {
        return end;
    }
    let saved = SavedGroup::new(w, group);
    let certificate = w.group_certificate;
    for n in &stale {
        w.forms.remove(n);
        w.round_forms.remove(n);
        w.glued.remove(n);
    }
    // With no saved ids both maps were empty.
    for n in saved.ids.iter().flatten() {
        w.sp.escapes.remove(n);
        w.sp.attempts.remove(n);
    }
    let again = repair_rounds_once(w, doc, from, first, reported);
    if w.kind_failed || w.sp.losses.len() != reported || again != end {
        w.kind_failed = false;
        w.group_certificate = certificate;
        saved.restore(w, reported, group);
        return end;
    }
    again
}

fn repair_rounds_once<'t>(
    w: &mut Writer<'t>,
    doc: N<'t>,
    from: usize,
    first: bool,
    reported: usize,
) -> usize {
    w.link_bracket_fallback = false;
    w.saw_link = false;
    let mut history = History {
        ids: AddrMap::default(),
        recent: std::collections::VecDeque::new(),
        seen: HashSet::new(),
        rounds: 0,
        remaining: i64::try_from(MAX_REPAIR_CHARS).unwrap_or(i64::MAX),
    };
    let mut first_round = None;
    w.capped_runs.clear();
    // The rounds of a group mostly read again what a round before them tried; their parses are kept across
    // rounds up to a bound, and a round past it keeps its own.
    let kept = typed_arena::Arena::new();
    let kept_reads = Reads::default();
    loop {
        let done = if kept_reads.chars.get() <= MAX_KEPT_READ_CHARS {
            repair_round(
                w,
                doc,
                (from, first, reported),
                (&mut history, &mut first_round),
                &kept,
                &kept_reads,
            )
        } else {
            let chunks = typed_arena::Arena::new();
            let reads = Reads::default();
            repair_round(
                w,
                doc,
                (from, first, reported),
                (&mut history, &mut first_round),
                &chunks,
                &reads,
            )
        };
        if let Some(end) = done {
            w.seed_cases = false;
            return end;
        }
    }
}

/// One round of [`repair_rounds_once`]: the end of the group where the rounds stop, `None` for another round.
fn repair_round<'t, 's>(
    w: &mut Writer<'t>,
    doc: N<'t>,
    (from, first, reported): (usize, bool, usize),
    (history, first_round): (&mut History, &mut Option<FirstKey<'t>>),
    chunks: &'s typed_arena::Arena<Chunk>,
    reads: &'s Reads<'s>,
) -> Option<usize> {
    let nodes = children(doc);
    let end = group_end(w, nodes, from);
    let group = nodes.slice(from, end).unwrap_or_else(Arr::empty);
    if history.rounds == 0 {
        seed_first_write(w, group);
        if *w
            .caret_links
            .get_or_insert_with(|| check::holds_caret_link(doc))
        {
            check::seed_text_note_closers(w, group);
        }
    }
    check::seed_lone_brackets(w, group);
    reset_lines(w);
    blocks::write_blocks(w, group, "", 0, false);
    if w.lines.is_empty() {
        return Some(end);
    }
    if table::certifies_plain_rows(w, group) {
        w.group_certificate = GroupCertificate::PlainRows;
        return Some(end);
    }
    if repeated_repair_in(w, group, history, Some(first_round)) {
        return Some(end);
    }
    let f = first && !w.blank_first;
    let mut round = Round {
        chunks,
        reads,
        last: None,
        source: None,
        equal: false,
    };
    w.new_forms.clear();
    let more = settle(w, doc, group, f, history.rounds == 1, &mut round);
    // C2-W-125: the forms this round asked for, in its probes too, as against those its write placed.
    w.round_forms.extend(w.new_forms.drain(..));
    if more || exhausted(w) {
        return None;
    }
    if w.kind_failed {
        return Some(end);
    }
    trim_capped_runs(w, group, f, &mut round);
    if w.sp.losses.len() == reported && !round.equal {
        report_unread(w, group, round.last);
    }
    prune_group_escapes(w, group, first && !w.blank_first, &round);
    Some(end)
}

/// On the first round of a pass, the backslashes of C2-W-125 that the rounds would reach in a group of one
/// paragraph; the later passes, which run after a loss, decide by rounds alone.
fn seed_first_write<'t>(w: &mut Writer<'t>, group: Arr<'t>) {
    w.seed_cases = w.kind_escapes
        && !w.closer_kinds
        && !w.whole_trials
        && group.len() == 1
        && check::seed_group(w, group);
}

/// C2-I-118: the rounds would place the escapes behind a capped opener one per round and stop at the first
/// round that reads back. Those placed at once are kept up to the shortest run, in their order, with which the
/// group still reads back.
fn trim_capped_runs<'t, 's>(
    w: &mut Writer<'t>,
    group: Arr<'t>,
    first: bool,
    round: &mut Round<'s>,
) {
    let runs = std::mem::take(&mut w.capped_runs);
    if runs.is_empty() || !round.equal {
        return;
    }
    // A change of form clears the escapes of the group: keep only those it still holds.
    let placed: Vec<(N<'t>, usize)> = runs
        .into_iter()
        .filter(|&(n, at)| w.escapes_of(n).binary_search(&at).is_ok())
        .collect();
    if placed.is_empty() {
        return;
    }
    let reads_back = |w: &mut Writer<'t>, keep: usize, restore: bool| -> Option<(&'s str, V<'s>)> {
        let dropped = placed.get(keep..).unwrap_or(&[]);
        for &(n, at) in dropped {
            if let Some(list) = w.sp.escapes.get_mut(&id(n))
                && let Ok(p) = list.binary_search(&at)
            {
                list.remove(p);
            }
        }
        reset_lines(w);
        blocks::write_blocks(w, group, "", 0, false);
        let (source, parsed) = round.parse(w, group_source(w, first));
        let actual = children(parsed);
        let equal = group.len() == actual.len()
            && group
                .iter()
                .zip(actual)
                .all(|(expected, read_back)| node::trees_equal(expected, read_back));
        if restore {
            for &(n, at) in dropped {
                w.add_escape(n, at);
            }
        }
        equal.then_some((source, parsed))
    };
    let mut reads = |w: &mut Writer<'t>, keep: usize| reads_back(w, keep, true).is_some();
    let high = shortest_reading(w, placed.len(), &mut reads);
    if let Some((source, parsed)) = reads_back(w, high, false) {
        round.last = Some(parsed);
        round.source = Some(source);
        round.equal = true;
    } else {
        round.equal = false;
    }
}

/// The shortest count of `len` with which `reads` holds, `len` holding. The rounds stop a few escapes short of
/// the end of the construct at most, so the search starts there and widens.
fn shortest_reading<'t>(
    w: &mut Writer<'t>,
    len: usize,
    reads: &mut dyn FnMut(&mut Writer<'t>, usize) -> bool,
) -> usize {
    let (mut low, mut high) = (0, len);
    let mut step = 1;
    while low < high && !exhausted(w) {
        let keep = low.max(high.saturating_sub(step));
        if !reads(w, keep) {
            low = keep + 1;
            break;
        }
        high = keep;
        step *= 2;
    }
    while low < high && !exhausted(w) {
        let middle = usize::midpoint(low, high);
        if reads(w, middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    high
}

fn refuse_repair(w: &mut Writer<'_>, node: V<'_>) {
    w.sp.losses.push(state::Loss {
        code: "writer-refused",
        message: "the writer could not complete its repair for this block".into(),
        node: Some(id(node)),
        step: None,
    });
}

fn local_id(ids: &mut AddrMap<usize, usize>, n: V<'_>) -> usize {
    let next = ids.len();
    *ids.entry(id(n)).or_insert(next)
}

/// C2-W-133: the repair state of a group, compared exactly: the written lines and
/// a flat record of the per-node state with every optional part length-prefixed.
#[derive(Clone, PartialEq, Eq)]
struct RepairKey {
    lines: Vec<String>,
    state: Vec<usize>,
}

/// Hashes the lengths only: the history holds 16 keys, so a collision costs one
/// exact comparison, while hashing every byte cost more than the comparisons.
impl std::hash::Hash for RepairKey {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        h.write_usize(self.state.len());
        h.write_usize(self.lines.iter().map(String::len).sum());
    }
}

fn push_opt(state: &mut Vec<usize>, v: Option<usize>) {
    match v {
        Some(v) => state.extend([1, v]),
        None => state.push(0),
    }
}

fn repair_key(w: &Writer<'_>, ids: &mut AddrMap<usize, usize>, group: Arr<'_>) -> RepairKey {
    let mut state = vec![usize::from(w.blank_first), w.sp.losses.len()];
    let mut pending: Vec<V<'_>> = group.iter().collect();
    while let Some(n) = pending.pop() {
        state.push(local_id(ids, n));
        let escapes = if ty(n) == "text" {
            w.sp.escapes.get(&id(n))
        } else {
            None
        };
        push_opt(&mut state, escapes.map(Vec::len));
        state.extend(escapes.into_iter().flatten());
        push_opt(
            &mut state,
            w.forms
                .get(&id(n))
                .map(|&f| usize::try_from(f).unwrap_or(usize::MAX)),
        );
        let attempt = w.sp.attempts.get(&id(n));
        push_opt(&mut state, attempt.map(|a| a.step));
        push_opt(&mut state, attempt.map(|a| a.escapes.len()));
        for (t, at) in attempt.map_or(&[][..], |a| a.escapes.as_slice()) {
            state.extend([local_id(ids, *t), *at]);
        }
        state.push(usize::from(w.is_lost(n)));
        node::for_each_slot(n, false, &mut |kids, _, _| pending.extend(kids.iter()));
    }
    RepairKey {
        lines: w.lines.clone(),
        state,
    }
}

/// The key of a first round that ran before any repair state existed: its state
/// is the local ids alone, so the walk waits for a second round that compares it.
struct FirstKey<'g> {
    group: Arr<'g>,
    blank_first: bool,
    losses: usize,
    lines: Vec<String>,
}

fn first_key(ids: &mut AddrMap<usize, usize>, first: FirstKey<'_>) -> RepairKey {
    let mut state = vec![usize::from(first.blank_first), first.losses];
    let mut pending: Vec<V<'_>> = first.group.iter().collect();
    while let Some(n) = pending.pop() {
        state.push(local_id(ids, n));
        // No escapes, form or attempt, and not lost (see `repair_key`).
        state.extend([0; 5]);
        node::for_each_slot(n, false, &mut |kids, _, _| pending.extend(kids.iter()));
    }
    RepairKey {
        lines: first.lines,
        state,
    }
}

fn remember(h: &mut History, key: RepairKey) {
    h.recent.push_back(key.clone());
    h.seen.insert(key);
    if h.recent.len() > 16 {
        if let Some(old) = h.recent.pop_front() {
            h.seen.remove(&old);
        }
    }
}

#[cfg(test)]
fn repeated_repair(w: &mut Writer<'_>, group: Arr<'_>, h: &mut History) -> bool {
    repeated_repair_in(w, group, h, None)
}

/// [`repeated_repair`]; with `first`, a first round without repair state defers its key there.
fn repeated_repair_in<'g>(
    w: &mut Writer<'_>,
    group: Arr<'g>,
    h: &mut History,
    first: Option<&mut Option<FirstKey<'g>>>,
) -> bool {
    let chars: usize = w.lines.iter().map(|l| l.len() + 1).sum();
    if h.rounds == 0 {
        // C2-W-133 sizes the budget by the document as written; the source only bounds it from below.
        w.work_limit = w.work_limit.max(repair_budget(w.written_chars + chars));
        h.remaining = h
            .remaining
            .max(i64::try_from(chars * MIN_REPAIR_ROUNDS).unwrap_or(i64::MAX));
    }
    h.rounds += 1;
    h.remaining -= i64::try_from(chars).unwrap_or(i64::MAX);
    let refuse = h.rounds > MAX_ROUNDS || h.remaining < 0 || exhausted(w) || {
        let deferred = first.and_then(|slot| {
            if let Some(earlier) = slot.take() {
                let key = first_key(&mut h.ids, earlier);
                remember(h, key);
            }
            let blank = h.rounds == 1
                && w.sp.escapes.is_empty()
                && w.forms.is_empty()
                && w.sp.attempts.is_empty()
                && w.sp.lost.is_empty();
            blank.then(|| {
                *slot = Some(FirstKey {
                    group,
                    blank_first: w.blank_first,
                    losses: w.sp.losses.len(),
                    lines: w.lines.clone(),
                });
            })
        });
        if deferred.is_some() {
            false
        } else {
            let key = repair_key(w, &mut h.ids, group);
            if h.seen.contains(&key) {
                true
            } else {
                remember(h, key);
                false
            }
        }
    };
    if refuse {
        if let Some(host) = group.first() {
            refuse_repair(w, host);
        }
    }
    refuse
}

/// The parses of the rounds of one group by their text: a write the rounds try again reads as before.
#[derive(Default)]
struct Reads<'k> {
    /// Searched in order while short; the map holds them all once it is not.
    list: std::cell::RefCell<Vec<(&'k str, V<'k>)>>,
    parsed: std::cell::RefCell<std::collections::HashMap<&'k str, V<'k>>>,
    /// The characters of the chunks held by the arena these parses live in.
    chars: std::cell::Cell<usize>,
}

const LISTED_READS: usize = 8;

impl<'k> Reads<'k> {
    fn get(&self, text: &str) -> Option<(&'k str, V<'k>)> {
        let list = self.list.borrow();
        if list.len() <= LISTED_READS {
            return list.iter().find(|(source, _)| *source == text).copied();
        }
        let (&source, &parsed) = self.parsed.borrow().get_key_value(text)?;
        Some((source, parsed))
    }

    fn insert(&self, source: &'k str, parsed: V<'k>) {
        let mut list = self.list.borrow_mut();
        list.push((source, parsed));
        if list.len() > LISTED_READS {
            let mut map = self.parsed.borrow_mut();
            if map.is_empty() {
                map.extend(list.iter().copied());
            } else {
                map.insert(source, parsed);
            }
        }
    }
}

/// The characters of parses the rounds of one group keep to read again; the rounds past it keep theirs for
/// one round only.
const MAX_KEPT_READ_CHARS: usize = 1 << 18;

/// The parses of one repair round: the last one is read by the loss report and the prune.
struct Round<'s> {
    chunks: &'s typed_arena::Arena<Chunk>,
    reads: &'s Reads<'s>,
    last: Option<V<'s>>,
    /// The source `last` was parsed from.
    source: Option<&'s str>,
    /// The last read-back parse was already proved equal to the group.
    equal: bool,
}

impl<'s> Round<'s> {
    fn known(&self, w: &mut Writer<'_>, text: &str) -> Option<(&'s str, V<'s>)> {
        let (source, parsed) = self.reads.get(text)?;
        count_read(w, source);
        Some((source, parsed))
    }

    fn keeps(&self, text: &str) -> bool {
        self.reads.chars.get().saturating_add(text.len()) <= MAX_KEPT_READ_CHARS
    }

    fn alloc(&self, text: String) -> &'s Chunk {
        self.reads
            .chars
            .set(self.reads.chars.get().saturating_add(text.len()));
        self.chunks.alloc(Chunk::new(text))
    }

    /// The parse of `text`, counted toward the repair limit whether or not it was read before.
    fn parse(&self, w: &mut Writer<'_>, text: String) -> (&'s str, V<'s>) {
        if let Some(read) = self.known(w, &text) {
            return read;
        }
        let chunk = self.alloc(text);
        let parsed = chunk.parse(w);
        self.reads.insert(chunk.text(), parsed);
        (chunk.text(), parsed)
    }

    /// `then` of the parse of `text`; past the kept characters the parse is dropped behind it.
    fn read<R>(
        &self,
        w: &mut Writer<'_>,
        text: String,
        then: impl FnOnce(&mut Writer<'_>, &str, V<'_>) -> R,
    ) -> R {
        if let Some((source, parsed)) = self.known(w, &text) {
            return then(w, source, parsed);
        }
        if self.keeps(&text) {
            let (source, parsed) = self.parse(w, text);
            return then(w, source, parsed);
        }
        let chunk = Chunk::new(text);
        let parsed = chunk.parse(w);
        then(w, chunk.text(), parsed)
    }
}

/// A parse taken again, counted as the parse was.
fn count_read(w: &mut Writer<'_>, text: &str) {
    w.parses += 1;
    w.work_chars = w.work_chars.saturating_add(text.len());
}

fn compare_group<'t>(
    w: &mut Writer<'t>,
    doc: N<'t>,
    group: Arr<'t>,
    first: bool,
    round: &mut Round<'_>,
) -> Check<'t> {
    #[cfg(test)]
    REPAIR_PARSES.with(|count| count.set(count.get() + 1));
    let (source, parsed) = round.parse(w, group_source(w, first));
    round.last = Some(parsed);
    round.source = Some(source);
    let mut c = Check {
        source: Some(source.to_string()),
        ..Check::default()
    };
    let actual = children(parsed);
    round.equal = group.len() == actual.len()
        && group
            .iter()
            .zip(actual)
            .all(|(expected, read_back)| node::trees_equal(expected, read_back));
    if !round.equal {
        check::count_open_above(w, group);
        compare_blocks(w, &mut c, group, actual, doc);
    }
    escape_openers(w, &mut c, parsed, if first { 1 } else { 2 });
    c
}

/// A repair belongs to a link when the changed node is the link or one of its descendants.
fn link_repair_requested(group: Arr<'_>, c: &Check<'_>) -> bool {
    fn visit(node: N<'_>, inside: bool, asked: &AddrSet<usize>) -> bool {
        let inside = inside || node::ty(node) == "link";
        (inside && asked.contains(&node::id(node)))
            || node::children(node).iter().any(|n| visit(n, inside, asked))
    }
    let asked: AddrSet<usize> = c
        .escapes
        .iter()
        .map(|(n, _)| node::id(*n))
        .chain(c.losses.iter().map(|(n, _)| node::id(*n)))
        .chain(c.forms.iter().map(|n| node::id(*n)))
        .collect();
    !asked.is_empty() && group.iter().any(|n| visit(n, false, &asked))
}

/// A bare link-label group is already the C2-W-088 result if it reads back.
/// Only a changed parse asks for the bounded unmatched-bracket fallback; the rounds after a first round that
/// did not read back write the lone brackets of link text with their backslash (C2-W-125 case five).
fn compare_with_link_fallback<'t>(
    w: &mut Writer<'t>,
    doc: N<'t>,
    group: Arr<'t>,
    first: bool,
    initial: bool,
    round: &mut Round<'_>,
) -> Check<'t> {
    // The fallback compares this round again, so the respell steps of the first comparison do not count.
    if initial && w.saw_link && !w.link_bracket_fallback {
        w.attempts_journal = Some(Vec::new());
    }
    let c = compare_group(w, doc, group, first, round);
    let journal = w.attempts_journal.take();
    if !initial || round.equal || !w.saw_link || w.link_bracket_fallback {
        return c;
    }
    w.link_bracket_fallback = true;
    if check::reports_construct(&c, group) && !link_repair_requested(group, &c) {
        return c;
    }
    for (n, prior) in journal.into_iter().flatten().rev() {
        match prior {
            Some(attempt) => w.sp.attempts.insert(n, attempt),
            None => w.sp.attempts.remove(&n),
        };
    }
    reset_lines(w);
    blocks::write_blocks(w, group, "", 0, false);
    compare_group(w, doc, group, first, round)
}

/// One round of the test of C2-W-089. False when the group reads back as its blocks.
fn settle<'t>(
    w: &mut Writer<'t>,
    doc: N<'t>,
    group: Arr<'t>,
    first: bool,
    initial: bool,
    round: &mut Round<'_>,
) -> bool {
    let mut c = compare_with_link_fallback(w, doc, group, first, initial, round);
    check::drop_early_trials(&mut c);
    if drop_trials(w, &mut c, group, first, round) && c.changes() == 0 {
        return true;
    }
    if w.kind_escapes {
        // C2-W-088: a node the kind rounds leave unread sends the group to the
        // rounds of the reported openers only.
        if !c.losses.is_empty() && c.changes() == 0 && !w.sp.escapes.is_empty() {
            w.kind_failed = true;
            return false;
        }
        escape_kinds(w, &mut c, group);
        escape_group_partners(w, &mut c, group, round.source);
    }
    extend_chains(w, &mut c);
    let capped = check::escape_capped_run(w, &mut c);
    w.capped_runs.extend(capped);
    if c.changes() == 0 && !c.losses.is_empty() && first && !w.blank_first {
        // C2-W-005: what only line 1 reads differently is written below one blank line.
        w.blank_first = compare_group(w, doc, group, false, round).losses.is_empty();
        if w.blank_first {
            return true;
        }
    }
    if c.changes() == 0 && !c.losses.is_empty() && braces_unpaired(w, group) {
        return true;
    }
    oust(w, &mut c, group, first);
    apply_changes(w, &c)
}

/// C2-W-125: the escapes a brace of case one repairs come out where the group reads back without them.
fn oust<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, group: Arr<'t>, first: bool) {
    if c.ousted.is_empty() {
        return;
    }
    let mut escapes = w.sp.escapes.clone();
    for &(n, at) in &c.escapes {
        let list = escapes.entry(id(n)).or_default();
        if let Err(place) = list.binary_search(&at) {
            list.insert(place, at);
        }
    }
    for &(n, at) in &c.ousted {
        if let Some(list) = escapes.get_mut(&id(n)) {
            list.retain(|&x| x != at);
        }
    }
    let placed = std::mem::take(&mut w.sp.escapes);
    let lines = w.lines.clone();
    let lead = w.lead.clone();
    let lead_skip = w.lead_skip;
    let starts = (w.starts.clone(), w.start_index.clone());
    let chunk = group_chunk(w, group, first, escapes);
    let kids = children(chunk.parse(w));
    let reads_back =
        kids.len() == group.len() && kids.iter().zip(group).all(|(n, e)| node::trees_equal(n, e));
    w.sp.escapes = placed;
    w.lines = lines;
    w.lead = lead;
    w.lead_skip = lead_skip;
    w.starts = starts.0;
    w.start_index = starts.1;
    if !reads_back {
        return;
    }
    let ousted = std::mem::take(&mut c.ousted);
    for (n, at) in ousted {
        if let Some(p) = c
            .escapes
            .iter()
            .position(|&(m, a)| id(m) == id(n) && a == at)
        {
            c.escapes.remove(p);
        } else if w.escapes_of(n).contains(&at) {
            c.retracted.push((n, at));
        }
    }
}

/// The group written with `escapes`, for a parse.
fn group_chunk<'t>(
    w: &mut Writer<'t>,
    group: Arr<'t>,
    first: bool,
    escapes: AddrMap<usize, Vec<usize>>,
) -> Chunk {
    w.sp.escapes = escapes;
    written_chunk(w, group, first)
}

/// The group written with the escapes `w` holds, for a parse.
fn written_chunk<'t>(w: &mut Writer<'t>, group: Arr<'t>, first: bool) -> Chunk {
    w.lines.clear();
    w.lead = None;
    w.lead_skip = 0;
    w.clear_starts();
    blocks::write_blocks(w, group, "", 0, false);
    Chunk::new(group_source(w, first))
}

/// C2-W-088, C2-W-091: whether the test asks for the escape `at` of `node`.
/// The round's last read-back parse is reused where the text without the escape is the same.
fn is_asked<'t>(
    w: &mut Writer<'t>,
    group: Arr<'t>,
    first: bool,
    (node, at): (V<'_>, usize),
    round: &Round<'_>,
) -> bool {
    let lines = w.lines.clone();
    let lead = w.lead.clone();
    let lead_skip = w.lead_skip;
    let starts = (w.starts.clone(), w.start_index.clone());
    let mut order = AddrMap::default();
    text_order(group, &mut order);
    let own_rank = rank(&order, node);
    let asked = if own_rank < 0 {
        asked_outside(w, group, first, (node, at), &order, round)
    } else {
        asked_in_group(w, group, first, (node, at), (&order, own_rank), round)
    } || (w.whole_trials && gives_tree(w, group, first, (node, at)));
    w.lines = lines;
    w.lead = lead;
    w.lead_skip = lead_skip;
    w.starts = starts.0;
    w.start_index = starts.1;
    asked
}

/// C2-W-125: once every other form lost a node, a trial that gives the tree with every placed escape is taken,
/// though it changes nothing with the text to its right bare, as the caret of `^[a\^[b][[b]\]]` that the closer
/// behind it needs.
fn gives_tree<'t>(
    w: &mut Writer<'t>,
    group: Arr<'t>,
    first: bool,
    (node, at): (V<'_>, usize),
) -> bool {
    let added = !w.escapes_of(node).contains(&at);
    w.add_escape(node, at);
    let chunk = written_chunk(w, group, first);
    let parsed = children(chunk.parse(w));
    if added && let Some(list) = w.sp.escapes.get_mut(&id(node)) {
        list.retain(|&x| x != at);
        if list.is_empty() {
            w.sp.escapes.remove(&id(node));
        }
    }
    parsed.len() == group.len()
        && group
            .iter()
            .zip(parsed)
            .all(|(expected, block)| writer_equal(expected, block))
}

/// [`is_asked`] for a node of the group: the escapes of the text behind it are set aside in place, so the
/// work is that of the group and not of every escape the document holds.
fn asked_in_group<'t>(
    w: &mut Writer<'t>,
    group: Arr<'t>,
    first: bool,
    (node, at): (V<'_>, usize),
    (order, own_rank): (&AddrMap<usize, usize>, i64),
    round: &Round<'_>,
) -> bool {
    let mut behind = Vec::new();
    for (text_id, &r) in order {
        if i64::try_from(r).unwrap_or(i64::MAX) > own_rank
            && let Some(offsets) = w.sp.escapes.remove(text_id)
        {
            behind.push((*text_id, offsets));
        }
    }
    let own = w.sp.escapes.get(&id(node)).cloned();
    if let Some(offsets) = &own {
        let before = offsets.iter().copied().filter(|&o| o < at).collect();
        w.sp.escapes.insert(id(node), before);
    }
    let without = written_chunk(w, group, first);
    let list = w.sp.escapes.entry(id(node)).or_default();
    list.push(at);
    list.sort_unstable();
    let with = written_chunk(w, group, first);
    let asked = asked_with(w, round, with.into_text(), without.into_text());
    match own {
        Some(offsets) => w.sp.escapes.insert(id(node), offsets),
        None => w.sp.escapes.remove(&id(node)),
    };
    w.sp.escapes.extend(behind);
    asked
}

/// [`is_asked`] for a node outside the order of the group: only its own escapes before `at` stay.
fn asked_outside<'t>(
    w: &mut Writer<'t>,
    group: Arr<'t>,
    first: bool,
    (node, at): (V<'_>, usize),
    order: &AddrMap<usize, usize>,
    round: &Round<'_>,
) -> bool {
    let own_rank = -1;
    let placed = w.sp.escapes.clone();
    let mut left: AddrMap<usize, Vec<usize>> = AddrMap::default();
    for (text_id, offsets) in &placed {
        let r = order
            .get(text_id)
            .map_or(-1, |&r| i64::try_from(r).unwrap_or(i64::MAX));
        if r < own_rank {
            left.insert(*text_id, offsets.clone());
        } else if *text_id == id(node) {
            left.insert(
                *text_id,
                offsets.iter().copied().filter(|&o| o < at).collect(),
            );
        }
    }
    let mut escaped = left.clone();
    let list = escaped.entry(id(node)).or_default();
    list.push(at);
    list.sort_unstable();
    let without = group_chunk(w, group, first, left);
    let with = group_chunk(w, group, first, escaped);
    let asked = asked_with(w, round, with.into_text(), without.into_text());
    w.sp.escapes = placed;
    asked
}

/// Whether the group written `with` an escape reads otherwise than `without` it.
fn asked_with(w: &mut Writer<'_>, round: &Round<'_>, with: String, without: String) -> bool {
    round.read(w, with, |w, _, with| match round.source.zip(round.last) {
        // The last read-back, read again uncounted.
        Some((text, parsed)) if text == without => !node::trees_equal(with, parsed),
        _ => round.read(w, without, |_, _, without| {
            !node::trees_equal(with, without)
        }),
    })
}

/// Drops the escapes of trials that were already asked; true when any was dropped.
fn drop_asked_trials<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    group: Arr<'t>,
    first: bool,
    round: &Round<'_>,
) -> bool {
    let mut dropped = false;
    // A lone ineffective trial takes no escape; several trials can work together.
    let trials = if c.trials.len() == 1 {
        c.trials.clone()
    } else {
        Vec::new()
    };
    for (n, at) in trials {
        if is_asked(w, group, first, (n, at), round) {
            continue;
        }
        if let Some(p) = c
            .escapes
            .iter()
            .position(|(e, a)| id(e) == id(n) && *a == at)
        {
            c.escapes.remove(p);
        }
        dropped = true;
    }
    dropped
}

/// The trials of a round that take no escape: a lone one the test does not ask for, and one a sibling covers.
fn drop_trials<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    group: Arr<'t>,
    first: bool,
    round: &Round<'_>,
) -> bool {
    let asked = drop_asked_trials(w, c, group, first, round);
    drop_idle_trials(w, c, group, first, round) || asked
}

const MAX_RETESTED_TRIALS: usize = 8;

/// C2-W-125: a round can place trials for several nodes of one stretch, where the first one already forms the
/// others (the brace before the tag of `x{#a\_--}` forms the dash behind it). A trial that changes nothing beside
/// the other escapes of the round is no backslash of a case and is taken out. Only where the parse reads no node
/// the tree lacks, as a strong of the tree beside the stretch (carve#6687): a construct it reports gives its kind
/// (C2-W-088), and the rounds after it decide those trials.
fn drop_idle_trials<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    group: Arr<'t>,
    first: bool,
    round: &Round<'_>,
) -> bool {
    if !(2..=MAX_RETESTED_TRIALS).contains(&c.trials.len())
        || !round
            .last
            .is_some_and(|read| reads_no_other_node(read, group))
    {
        return false;
    }
    // The round's escapes are placed in the document's own list and taken out again; a copy of that list
    // per trial would cost the size of the document.
    let mut added = Vec::new();
    for &(n, at) in &c.escapes {
        if !w.escapes_of(n).contains(&at) {
            w.add_escape(n, at);
            added.push((id(n), at));
        }
    }
    let mut removed = Vec::new();
    for &(n, at) in &c.retracted {
        if remove_escape(w, id(n), at) {
            removed.push((n, at));
        }
    }
    let lines = w.lines.clone();
    let lead = w.lead.clone();
    let lead_skip = w.lead_skip;
    let starts = (w.starts.clone(), w.start_index.clone());
    let mut dropped = false;
    // The later trial goes first, so the earlier one, which the rounds of C2-W-088 reach first, is kept.
    // A kept trial leaves the escapes as they were and a dropped one leaves them without it: either way the
    // next trial's text with its escape is already read.
    let mut with: Option<(&str, V<'_>)> = None;
    for (n, at) in c.trials.clone().into_iter().rev() {
        let key = (id(n), at);
        if !added.contains(&key) {
            continue;
        }
        let written = match with {
            Some(read) => Err(read),
            None => Ok(written_chunk(w, group, first)),
        };
        remove_escape(w, key.0, at);
        let without = written_chunk(w, group, first);
        let without = round.parse(w, without.into_text());
        let read = match written {
            Ok(chunk) => round.parse(w, chunk.into_text()),
            Err(read) => {
                count_read(w, read.0);
                read
            }
        };
        if !node::trees_equal(without.1, read.1) {
            w.add_escape(n, at);
            with = Some(read);
            continue;
        }
        with = Some(without);
        added.retain(|&k| k != key);
        c.escapes.retain(|&(m, a)| (id(m), a) != key);
        c.trials.retain(|&(m, a)| (id(m), a) != key);
        if let Some(marks) = c.marks.get_mut(&key.0) {
            marks.remove(&at);
        }
        dropped = true;
    }
    for (n, at) in added {
        remove_escape(w, n, at);
    }
    for (n, at) in removed {
        w.add_escape(n, at);
    }
    w.lines = lines;
    w.lead = lead;
    w.lead_skip = lead_skip;
    w.starts = starts.0;
    w.start_index = starts.1;
    dropped
}

fn remove_escape(w: &mut Writer<'_>, node: usize, at: usize) -> bool {
    let Some(list) = w.sp.escapes.get_mut(&node) else {
        return false;
    };
    let Ok(place) = list.binary_search(&at) else {
        return false;
    };
    list.remove(place);
    if list.is_empty() {
        w.sp.escapes.remove(&node);
    }
    true
}

/// Whether each leaf block of `parsed` holds, beside text and breaks, only inline nodes that equal nodes of the same
/// leaf of the tree `group`, in their order: a node read elsewhere, as a span on another bracket, is a construct the
/// tree lacks.
fn reads_no_other_node(parsed: V<'_>, group: Arr<'_>) -> bool {
    let mut read = Vec::new();
    inline_leaves(children(parsed), &mut read);
    let mut tree = Vec::new();
    inline_leaves(group, &mut tree);
    read.len() == tree.len()
        && read.iter().zip(&tree).all(|(&leaf, &held)| {
            let held = children(held);
            let mut at = 0;
            children(leaf)
                .iter()
                .filter(|&n| ty(n) != "text" && !node::is_break(Some(n)))
                .all(|n| {
                    while held.get(at).is_some_and(|h| !node::trees_equal(h, n)) {
                        at += 1;
                    }
                    at += 1;
                    at <= held.len()
                })
        })
}

fn inline_leaves<'p>(nodes: Arr<'p>, out: &mut Vec<V<'p>>) {
    for n in nodes {
        if node::is_inline_parent(n) {
            out.push(n);
        } else {
            inline_leaves(children(n), out);
        }
    }
}

/// The text nodes below `nodes` in the order they are written: a title before the children.
fn text_order(nodes: Arr<'_>, order: &mut AddrMap<usize, usize>) {
    for n in nodes {
        if ty(n) == "text" {
            let next = order.len();
            order.insert(id(n), next);
        }
        node::for_each_slot(n, true, &mut |kids, _, _| text_order(kids, order));
    }
}

fn rank(order: &AddrMap<usize, usize>, n: V<'_>) -> i64 {
    order
        .get(&id(n))
        .map_or(-1, |&r| i64::try_from(r).unwrap_or(i64::MAX))
}

/// C2-W-125 case five for the group, with its bracket partners kept across its rounds.
fn escape_group_partners<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    group: Arr<'t>,
    source: Option<&str>,
) {
    // Every bracketed node writes a `]`: without one there is no partner to escape.
    if !source.is_some_and(|source| source.contains(']')) {
        return;
    }
    let key = (group.first().map_or(0, id), group.len());
    let partners = match w.partners.take() {
        Some((k, partners)) if k == key => partners,
        _ => check::partner_closers(&group.iter().collect::<Vec<_>>()),
    };
    check::escape_partner_closers(w, c, &partners);
    w.partners = Some((key, partners));
}

/// C2-W-088: each kind a round reports in a block escapes every opener of
/// that kind in the block, so the rounds are bounded by the kinds.
fn escape_kinds<'t>(w: &Writer<'t>, c: &mut Check<'t>, group: Arr<'t>) {
    let trials: HashSet<(usize, usize)> = c.trials.iter().map(|(n, at)| (id(n), *at)).collect();
    // CC-1568: a backslash of a C2-W-125 closer repair gives no kind.
    let reported: Vec<(N<'t>, usize)> = c
        .escapes
        .iter()
        .filter(|(n, at)| {
            let key = (id(n), *at);
            (!trials.contains(&key) || text(*n).as_bytes().get(*at) == Some(&b'`'))
                && !c.own_escapes.contains(&key)
                && (w.closer_kinds || !c.closers.contains(&key))
        })
        .copied()
        .collect();
    if reported.is_empty() {
        return;
    }
    let blocks = kinds::KindBlocks::new(group);
    // A comment closer may be reported while the same round repairs its
    // opener. Once the opening brace is escaped, the closer is ordinary text;
    // it does not report an independent `#` or `%` kind (C2-W-088).
    let comment_openers = reported_comment_openers(&blocks, &reported);
    let mut done = HashSet::new();
    for (node, at) in reported {
        let Some(block) = blocks.block_of(node) else {
            continue;
        };
        if reported_comment_closer(&comment_openers, block, node, at) {
            c.escapes
                .retain(|&(n, offset)| id(n) != id(node) || offset != at);
            continue;
        }
        let line = c.line_escapes.contains(&(id(node), at));
        let Some(key) = blocks.key(node, at, line) else {
            continue;
        };
        if !done.insert((block, key.clone())) {
            continue;
        }
        for (n, offset) in blocks.openers(block, &key) {
            escape_at(w, c, n, offset);
        }
    }
}

fn reported_comment_openers<'t>(
    blocks: &kinds::KindBlocks<'t>,
    reported: &[(N<'t>, usize)],
) -> HashSet<(usize, u8)> {
    reported
        .iter()
        .filter_map(|(node, at)| {
            let mark = match blocks.key(*node, *at, false)?.as_str() {
                "{#" => b'#',
                "{%" => b'%',
                _ => return None,
            };
            Some((blocks.block_of(*node)?, mark))
        })
        .collect()
}

fn reported_comment_closer(
    openers: &HashSet<(usize, u8)>,
    block: usize,
    node: N<'_>,
    at: usize,
) -> bool {
    let value = text(node).as_bytes();
    value.get(at).is_some_and(|&mark| {
        matches!(mark, b'#' | b'%')
            && value.get(at + 1) == Some(&b'}')
            && openers.contains(&(block, mark))
    })
}

#[cfg(test)]
mod comment_kind_test {
    use super::*;

    #[test]
    fn comment_opener_split_across_text_nodes_keeps_one_kind() {
        for mark in ["#", "%"] {
            let values = [
                serde_json::json!({"type": "text", "value": "{"}),
                serde_json::json!({"type": "text", "value": mark}),
                serde_json::json!({"type": "soft_break"}),
                serde_json::json!({"type": "text", "value": format!("{mark}}}")}),
            ];
            let nodes: Vec<_> = values.iter().map(V::Json).collect();
            let blocks = kinds::KindBlocks::content(&nodes);
            let [opener_value, _, _, closer_value] = &values;
            let opener = V::Json(opener_value);
            let closer = V::Json(closer_value);
            let reported = [(opener, 0), (closer, 0)];
            let openers = reported_comment_openers(&blocks, &reported);
            assert!(reported_comment_closer(&openers, 0, closer, 0));
        }
    }
}

const LINE_LEAVES: &[&str] = &[
    "code",
    "literal_inline",
    "math",
    "raw_inline",
    "comment",
    "critic_comment",
];

/// Whether a line of `value` after the first starts as a block opener can.
fn has_opener_line(value: &str) -> bool {
    for line in value.split('\n').skip(1) {
        let t = line.trim_start_matches([' ', '\t']);
        let Some(&first) = t.as_bytes().first() else {
            continue;
        };
        if !first.is_ascii_alphanumeric() {
            return true;
        }
        let end = t.bytes().take_while(u8::is_ascii_alphanumeric).count();
        if matches!(t.as_bytes().get(end), Some(b'.' | b')')) {
            return true;
        }
    }
    false
}

/// C2-W-016: a lost node holds a leaf whose later line would open a block.
fn refuse_leaf_lines(w: &mut Writer<'_>, n: V<'_>, depth: usize) {
    if depth > 200 || matches!(ty(n), "paragraph" | "list_item") {
        return;
    }
    if LINE_LEAVES.contains(&ty(n)) {
        if has_opener_line(text(n)) {
            lose(w, n, Some("value"));
        }
        return;
    }
    for c in children(n) {
        refuse_leaf_lines(w, c, depth + 1);
    }
}

/// C2-W-069, C2-W-117: the comparison of a group stops at a loss, so the bare pairs behind it that cannot
/// pair are braced here; true when one was.
fn braces_unpaired<'t>(w: &mut Writer<'t>, group: Arr<'t>) -> bool {
    let forms = w.forms.len();
    reset_lines(w);
    w.strict_pairs = true;
    blocks::write_blocks(w, group, "", 0, false);
    w.strict_pairs = false;
    if w.forms.len() == forms {
        return false;
    }
    // C2-W-125: a new form takes out the backslashes placed for the old one, as `apply_changes` does.
    w.sp.escapes.clear();
    w.sp.attempts.clear();
    true
}

fn apply_changes(w: &mut Writer<'_>, c: &Check<'_>) -> bool {
    if c.forms.iter().any(|n| !w.forms.contains_key(&id(n))) {
        w.sp.escapes.clear();
        w.sp.attempts.clear();
        w.new_forms.extend(
            c.forms
                .iter()
                .map(|n| id(*n))
                .filter(|n| !w.forms.contains_key(n)),
        );
        for n in &c.forms {
            w.add_form(id(n));
        }
        return true;
    }
    for (n, at) in &c.retracted {
        if let Some(list) = w.sp.escapes.get_mut(&id(n)) {
            if let Some(p) = list.iter().position(|x| x == at) {
                list.remove(p);
            }
        }
    }
    for (n, at) in &c.escapes {
        w.add_escape(n, *at);
    }
    for n in &c.forms {
        w.add_form(id(n));
    }
    if c.changes() > 0 {
        return true;
    }
    for (n, _) in &c.losses {
        refuse_leaf_lines(w, *n, 0);
    }
    for (n, field) in &c.losses {
        lose(w, n, field.as_deref());
    }
    false
}

/// C2-W-125: the braced forms the rounds gave pairs behind a braced pair of their inline run while an
/// earlier pair still read on, where the bare form pairs by itself (C2-W-069).
fn stale_forms<'t>(w: &Writer<'t>, group: Arr<'t>) -> Vec<usize> {
    let mut stale = Vec::new();
    // Only a form of `round_forms` is stale.
    if w.round_forms.is_empty() {
        return stale;
    }
    let mut pending: Vec<N<'t>> = group.iter().collect();
    while let Some(node) = pending.pop() {
        // A title and a caption are inline runs of their own outside `children`.
        if let Some(title) = node::title(node) {
            stale_in_run(w, title, &mut stale);
        }
        pending.extend(node::caption(node));
        if node::is_inline_parent(node) {
            stale_in_run(w, children(node), &mut stale);
        } else {
            pending.extend(children(node).iter());
        }
    }
    stale
}

fn stale_in_run<'t>(w: &Writer<'t>, run: Arr<'t>, stale: &mut Vec<usize>) {
    let mut behind = false;
    let mut stack: Vec<(Arr<'t>, usize)> = vec![(run, 0)];
    while let Some((nodes, at)) = stack.pop() {
        let Some(n) = nodes.get(at) else {
            continue;
        };
        stack.push((nodes, at + 1));
        if matches!(ty(n), "emphasis" | "strong") && w.form(n) > 0 {
            if behind && w.round_forms.contains(&id(n)) && pairs_bare(nodes, at) {
                stale.push(id(n));
            } else {
                behind = true;
            }
        } else if matches!(ty(n), "emphasis" | "strong")
            && inline::braced_by_content(n, if ty(n) == "strong" { '*' } else { '/' })
        {
            behind = true;
        }
        stack.push((children(n), 0));
    }
}

/// C2-W-069: whether the bare emphasis or strong at `at` of `nodes` opens and closes at its neighbors
/// and holds no mark of its own.
fn pairs_bare(nodes: Arr<'_>, at: usize) -> bool {
    let Some(n) = nodes.get(at) else {
        return false;
    };
    let mark = match ty(n) {
        "emphasis" => '/',
        "strong" => '*',
        _ => return false,
    };
    let space = |c: Option<char>| c.is_none_or(crate::chars::is_flank_space);
    let word = |c: Option<char>| c.is_some_and(crate::chars::is_word);
    let text_at = |i: Option<usize>| i.and_then(|i| nodes.get(i)).filter(|p| ty(*p) == "text");
    !holds_mark(n, mark)
        && !space(edge_char(n, false))
        && !space(edge_char(n, true))
        && !word(text_at(at.checked_sub(1)).and_then(|p| text(p).chars().last()))
        && !word(text_at(Some(at + 1)).and_then(|p| text(p).chars().next()))
}

/// The first or last character of the text that starts or ends `n`, if a text does.
fn edge_char(mut n: V<'_>, last: bool) -> Option<char> {
    loop {
        if ty(n) == "text" {
            return if last {
                text(n).chars().last()
            } else {
                text(n).chars().next()
            };
        }
        let c = children(n);
        n = if last { c.last()? } else { c.first()? };
    }
}

fn holds_mark(n: V<'_>, mark: char) -> bool {
    let mut pending: Vec<V<'_>> = children(n).iter().collect();
    while let Some(c) = pending.pop() {
        if ty(c) == "text" && text(c).contains(mark) {
            return true;
        }
        pending.extend(children(c).iter());
    }
    false
}

/// C2-W-002: whether `actual` is `expected` for the readback check (C2-A-193).
fn writer_equal(expected: V<'_>, actual: V<'_>) -> bool {
    node::trees_equal(expected, actual)
        || extension::source_core_equivalent(expected, Some(actual))
        || node::trees_equal(&node::without_standard_data(expected), actual)
        || extension::with_core_inlines(expected).is_some_and(|e| node::trees_equal(&e, actual))
}

/// C2-W-011: a block that the rounds let pass and that reads back as another tree is reported.
/// `parsed` is the last read-back parse of the group.
fn report_unread(w: &mut Writer<'_>, group: Arr<'_>, parsed: Option<V<'_>>) {
    let parsed: Arr<'_> = parsed.map_or_else(Arr::empty, children);
    let unread = (0..group.len().max(parsed.len())).find(|&at| {
        !matches!((parsed.get(at), group.get(at)), (Some(block), Some(e)) if writer_equal(e, block))
    });
    if let Some(n) = unread.and_then(|at| group.get(at.min(group.len().saturating_sub(1)))) {
        lose(w, n, None);
    }
}

fn star_after_break(nodes: Arr<'_>, byte: usize, parent: Option<V<'_>>) -> bool {
    // Siblings lie in source order, so the search starts at the first that ends after `byte`: a scan from
    // the front made each prune trial linear in the length of the paragraph.
    let before = |i| {
        nodes
            .get(i)
            .is_some_and(|n| span(n).is_none_or(|(_, end)| end <= byte))
    };
    let (mut low, mut high) = (0, nodes.len());
    while low < high {
        let mid = low + (high - low) / 2;
        if before(mid) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    for index in low..nodes.len() {
        let Some(n) = nodes.get(index) else {
            continue;
        };
        let Some((start, end)) = span(n) else {
            continue;
        };
        if byte < start {
            break;
        }
        if byte >= end {
            continue;
        }
        if ty(n) == "text" {
            return parent.is_some_and(|p| matches!(ty(p), "emphasis" | "strong"))
                && index
                    .checked_sub(1)
                    .and_then(|i| nodes.get(i))
                    .is_some_and(|b| ty(b) == "soft_break");
        }
        if star_after_break(children(n), byte, Some(n)) {
            return true;
        }
    }
    false
}

/// The offset of the next byte `b` of a source at or after `from`.
type NextByte<'a> = dyn FnMut(u8, usize) -> Option<usize> + 'a;

/// A [`NextByte`] for offsets asked in increasing order, each search starting where the last ended.
fn next_byte_after(source: &str) -> impl FnMut(u8, usize) -> Option<usize> + '_ {
    let mut last: [Option<(usize, Option<usize>)>; 3] = [None; 3];
    move |b, from| {
        let slot = match b {
            b'}' => 0,
            b']' => 1,
            _ => 2,
        };
        if let Some(&Some((asked, found))) = last.get(slot)
            && asked <= from
            && found.is_none_or(|f| f >= from)
        {
            return found;
        }
        let found = memchr_from(source, b, from);
        if let Some(cached) = last.get_mut(slot) {
            *cached = Some((from, found));
        }
        found
    }
}

fn memchr_from(source: &str, b: u8, from: usize) -> Option<usize> {
    source
        .as_bytes()
        .get(from..)?
        .iter()
        .position(|&c| c == b)
        .map(|i| i + from)
}

/// `parsed` is the read-back parse of `source`, asked for only before a star.
fn prunable_opener<'p>(
    source: &str,
    at: usize,
    parsed: &mut dyn FnMut() -> Option<V<'p>>,
    find: &mut NextByte<'_>,
    others: &mut Option<usize>,
) -> bool {
    let b = source.as_bytes();
    let next = b.get(at + 1).copied();
    // A mark behind a braced inline closer may no longer open in the complete
    // group. An attribute span ends in `}` too, but its code-kind repair still
    // owns the escape (C2-W-088). The marker before `}` distinguishes the two.
    if matches!(next, Some(b'`' | b'*'))
        && at > 1
        && b.get(at - 1) == Some(&b'}')
        && b.get(at - 2)
            .is_some_and(|mark| b"*/_~=+-^,".contains(mark))
    {
        return true;
    }
    if next == Some(b'*') {
        return parsed().is_some_and(|p| star_after_break(children(p), at, None));
    }
    if next == Some(b'[') {
        let rest = source.get(at + 2..).unwrap_or("");
        if rest.starts_with("^]") {
            return true;
        }
        if !rest.starts_with("----") {
            return false;
        }
        let close = find(b']', at + 2);
        let line_end = find(b'\n', at + 2);
        return at.checked_sub(1).and_then(|p| b.get(p)) != Some(&b'^')
            || close.is_none()
            || matches!((line_end, close), (Some(l), Some(c)) if c > l);
    }
    if next != Some(b'{') {
        return false;
    }
    match b.get(at + 2) {
        Some(b'/' | b'*') => true,
        Some(b'#') => prunable_tag_brace(source, at, parsed, find, others),
        _ => false,
    }
}

const LONGEST_TAG_BRACE: usize = 256;

/// C2-W-090, C2-W-099: a `\-` inside the braces that escapes a hyphen run is a kind of a later round that keeps
/// the brace escape of an earlier one (C2-W-091). Otherwise it is the case-six backslash behind the name, which
/// gives the tree alone, so the brace escape of case one goes where no other construct takes a backslash in text
/// (C2-W-125, carve#6687): `x{#a\-w}`, `x{#a\--w}`, but `x\{#a\---}` and `x\{#a\-w} \*s*`.
fn prunable_tag_brace<'p>(
    source: &str,
    at: usize,
    parsed: &mut dyn FnMut() -> Option<V<'p>>,
    find: &mut NextByte<'_>,
    others: &mut Option<usize>,
) -> bool {
    let Some(close) = find(b'}', at + 3) else {
        return false;
    };
    let Some(dash) = source
        .get(at + 3..close)
        .and_then(|inner| inner.find("\\-"))
        .map(|d| d + at + 3)
    else {
        return true;
    };
    close - at <= LONGEST_TAG_BRACE
        && !in_hyphen_run(source, dash)
        && others
            .or_else(|| {
                *others = parsed().map(|p| text_backslashes(source, p));
                *others
            })
            .is_some_and(|n| n == braced_name_escapes(source, at, close) + 1)
}

fn in_hyphen_run(source: &str, at: usize) -> bool {
    let b = source.as_bytes();
    at.checked_sub(1).and_then(|p| b.get(p)) == Some(&b'-')
        || (b.get(at + 2) == Some(&b'-') && b.get(at + 3) == Some(&b'-'))
}

/// The case-six escapes behind a name between the brace at `at` and its closer.
fn braced_name_escapes(source: &str, at: usize, close: usize) -> usize {
    let mut count = 0;
    let mut slash = memchr_from(source, b'\\', at + 2);
    while let Some(p) = slash.filter(|&p| p < close) {
        if behind_name(source, p) {
            count += 1;
        }
        slash = memchr_from(source, b'\\', p + 2);
    }
    count
}

/// C2-W-125 case six: the backslash at `at` stands directly behind a tag or mention name, not in a hyphen run.
fn behind_name(source: &str, at: usize) -> bool {
    match source.as_bytes().get(at + 1) {
        Some(b'_' | b'.') => {}
        Some(b'-') if !in_hyphen_run(source, at) => {}
        _ => return false,
    }
    let head = source.get(..at).unwrap_or("");
    let name: usize = head
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .map(char::len_utf8)
        .sum();
    name > 0 && matches!(head.as_bytes().get(at - name - 1), Some(b'#' | b'@'))
}

/// The backslashes of `source` that escape a construct in text: a code span, a destination or an attribute value
/// holds its backslashes as they are.
fn text_backslashes(source: &str, parsed: V<'_>) -> usize {
    let mut nodes = Vec::new();
    node::descendants(parsed, &mut nodes);
    let mut ranges: Vec<(usize, usize)> = nodes
        .into_iter()
        .filter(|n| ty(n) == "text")
        .filter_map(span)
        .collect();
    ranges.sort_unstable();
    let b = source.as_bytes();
    let mut count = 0;
    let mut range = 0;
    let mut slash = memchr_from(source, b'\\', 0);
    while let Some(p) = slash {
        while ranges.get(range).is_some_and(|r| r.1 <= p) {
            range += 1;
        }
        let in_text = ranges.get(range).is_some_and(|r| r.0 <= p);
        if in_text
            && b.get(p + 1)
                .is_some_and(|&n| n != b'\\' && n.is_ascii_punctuation())
        {
            count += 1;
        }
        slash = memchr_from(source, b'\\', p + 2);
    }
    count
}

/// The spans of `source` that hold a verbatim reference, sorted and merged for [`in_raw`]. Each reference
/// keeps its own leftmost non-overlapping occurrences, as `match_indices` gives them, from one pass.
fn raw_spans(source: &str, raws: &[String]) -> Vec<(usize, usize)> {
    let patterns: Vec<&str> = raws
        .iter()
        .map(String::as_str)
        .filter(|r| !r.is_empty())
        .collect();
    // Building the automaton costs more than a paragraph's whole prune, so the common cases skip it.
    match patterns.as_slice() {
        [] => return Vec::new(),
        [only] => {
            return source
                .match_indices(only)
                .map(|(at, m)| (at, at + m.len()))
                .collect();
        }
        _ => {}
    }
    let Ok(searcher) = aho_corasick::AhoCorasick::new(&patterns) else {
        return Vec::new();
    };
    let mut next = vec![0; patterns.len()];
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for m in searcher.find_overlapping_iter(source) {
        let Some(free) = next.get_mut(m.pattern().as_usize()) else {
            continue;
        };
        if m.start() >= *free {
            *free = m.end();
            spans.push((m.start(), m.end()));
        }
    }
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(spans.len());
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// Whether the byte `at` lies in one of the [`raw_spans`].
fn in_raw(spans: &[(usize, usize)], at: usize) -> bool {
    let i = spans.partition_point(|&(start, _)| start <= at);
    i.checked_sub(1)
        .and_then(|i| spans.get(i))
        .is_some_and(|&(_, end)| at < end)
}

/// C2-I-089, C2-W-091: the backslash of a `\[^]` inside the brackets of a `^[` of its line stays where the
/// first round, the parse of the text without its escapes, opens a node on that bracket: the procedure
/// placed it for a reported bracket of a note that reads as text (CC-1716).
/// What the note rules of the prune say about one backslash; `Open` leaves it to [`prunable_opener`].
#[derive(Clone, Copy, PartialEq)]
enum NoteCall {
    Prune,
    Keep,
    Open,
}

/// Its caches hold for one source; a caller that changes the source starts a new one.
#[derive(Default)]
struct NoteEscapes {
    /// The line start `inside` was computed for.
    line: Option<usize>,
    inside: HashSet<usize>,
    bare: Option<HashSet<usize>>,
}

impl NoteEscapes {
    fn behind_opener(&mut self, source: &str, at: usize) -> bool {
        let line = source
            .get(..at)
            .and_then(|r| r.rfind('\n'))
            .map_or(0, |n| n + 1);
        if self.line != Some(line) {
            self.inside = note_spans(source, line);
            self.line = Some(line);
        }
        self.inside.contains(&at)
    }

    /// C2-W-125 case five: behind a text `^[` whose caret pairs are ordinary brackets, link or span text
    /// can start with a bare caret, so the backslash before it is a prune candidate.
    fn caret(&mut self, source: &str, at: usize) -> bool {
        let b = source.as_bytes();
        b.get(at + 1) == Some(&b'^')
            && at
                .checked_sub(1)
                .is_some_and(|p| b.get(p) == Some(&b'[') && !is_note_bracket(b, p))
            && self.behind_opener(source, at)
    }

    fn call(&mut self, w: &Writer<'_>, source: &str, at: usize) -> NoteCall {
        if self.caret(source, at) {
            NoteCall::Prune
        } else if self.keeps(w, source, at) {
            NoteCall::Keep
        } else {
            NoteCall::Open
        }
    }

    fn keeps(&mut self, w: &Writer<'_>, source: &str, at: usize) -> bool {
        if !source.get(at + 1..).is_some_and(|r| r.starts_with("[^]"))
            || !self.behind_opener(source, at)
        {
            return false;
        }
        self.bare
            .get_or_insert_with(|| bare_bracket_starts(w, source))
            .contains(&(at + 1))
    }
}

/// The offsets of the line at `line` that stand inside the brackets of a `^[`, its backslashes read as text.
fn note_spans(source: &str, line: usize) -> HashSet<usize> {
    let b = source.as_bytes();
    let mut inside = HashSet::new();
    let mut open = Vec::new();
    let mut notes = 0usize;
    for (at, &ch) in b.iter().enumerate().skip(line) {
        if ch == b'\n' {
            break;
        }
        if notes > 0 {
            inside.insert(at);
        }
        if ch == b'[' {
            let note = is_note_bracket(b, at);
            open.push(note);
            notes += usize::from(note);
        } else if ch == b']'
            && at.checked_sub(1).and_then(|p| b.get(p)) != Some(&b'\\')
            && open.pop() == Some(true)
        {
            notes -= 1;
        }
    }
    inside
}

/// Whether the bracket at `at` follows a caret, with or without a backslash between them.
fn is_note_bracket(b: &[u8], at: usize) -> bool {
    let before = |k: usize| at.checked_sub(k).and_then(|p| b.get(p)).copied();
    before(1) == Some(b'^') || (before(1) == Some(b'\\') && before(2) == Some(b'^'))
}

/// The offsets in `source` of each `[` that opens an inline node of the parse of `source` without its escapes.
fn bare_bracket_starts(w: &Writer<'_>, source: &str) -> HashSet<usize> {
    let b = source.as_bytes();
    let mut bare = String::with_capacity(source.len());
    // Each byte of `bare` and the offset of `source` it came from.
    let mut origin = Vec::with_capacity(source.len());
    let mut from = 0;
    let mut at = 0;
    while at < b.len() {
        if b.get(at) == Some(&b'\\') && b.get(at + 1).is_some_and(u8::is_ascii_punctuation) {
            bare.push_str(source.get(from..at).unwrap_or(""));
            origin.extend(from..at);
            from = at + 1;
            at += 2;
            continue;
        }
        at += 1;
    }
    bare.push_str(source.get(from..).unwrap_or(""));
    origin.extend(from..b.len());
    let chunk = Chunk::new(bare);
    let parsed = chunk.read(w, Some(&w.link_keys));
    let mut nodes = Vec::new();
    node::descendants(parsed, &mut nodes);
    let bb = chunk.text().as_bytes();
    nodes
        .into_iter()
        .filter(|&n| ty(n) != "text")
        .filter_map(|n| span(n).map(|(start, _)| start))
        .filter(|&start| bb.get(start) == Some(&b'['))
        .filter_map(|start| origin.get(start).copied())
        .collect()
}

/// C2-W-088, C2-W-089: test candidate escapes within the group that wrote them.
/// `round.last` is the last read-back parse of the group, the parse of its source as written.
fn prune_group_escapes(w: &mut Writer<'_>, group: Arr<'_>, first: bool, round: &Round<'_>) {
    let mut source = group_source(w, first);
    // Both prunes ask only whether a backslash lies in a verbatim reference, which only a reference holding a
    // backslash can answer yes; searching the source for every other one was quadratic in the references.
    let mut raws: Vec<String> = std::mem::take(&mut w.raw_refs)
        .into_iter()
        .filter(|r| r.contains('\\'))
        .collect();
    raws.sort_unstable();
    raws.dedup();
    let windowed = prune_in_windows(w, group, &source, &raws, round, true)
        .or_else(|| prune_in_windows(w, group, &source, &raws, round, false));
    if let Some(pruned) = windowed {
        source = pruned;
    } else {
        prune_whole_group(w, group, &mut source, &raws, round);
    }
    let body = source
        .get(usize::from(!first)..source.len().saturating_sub(1))
        .unwrap_or("");
    w.lines = body.split('\n').map(str::to_string).collect();
}

/// A leaf block of a parse with its span, or with `lines` the run of its children between two soft breaks.
#[derive(Clone, Copy)]
struct Leaf<'p> {
    start: usize,
    end: usize,
    node: V<'p>,
    lines: Option<(usize, usize)>,
}

/// The leaf blocks below `nodes` in source order; an inline parent is not entered. With `split` a paragraph
/// gives one leaf per run of its children between soft breaks.
fn leaf_blocks<'p>(nodes: Arr<'p>, split: bool, out: &mut Vec<Leaf<'p>>) {
    for n in nodes {
        if !node::is_inline_parent(n) {
            node::for_each_slot(n, true, &mut |kids, _, _| leaf_blocks(kids, split, out));
            continue;
        }
        let Some((start, end)) = span(n) else {
            continue;
        };
        if split && ty(n) == "paragraph" && line_runs(n, out) {
            continue;
        }
        out.push(Leaf {
            start,
            end,
            node: n,
            lines: None,
        });
    }
}

/// The runs of the children of `paragraph` between soft breaks, as leaves; false, adding none, where a
/// child has no span.
fn line_runs<'p>(paragraph: V<'p>, out: &mut Vec<Leaf<'p>>) -> bool {
    let kids = children(paragraph);
    let mut runs = Vec::new();
    let mut from = 0;
    for at in 0..=kids.len() {
        if at < kids.len() && kids.get(at).is_some_and(|k| ty(k) != "soft_break") {
            continue;
        }
        let first = kids.get(from).and_then(span);
        let last = at.checked_sub(1).and_then(|l| kids.get(l)).and_then(span);
        match (first, last) {
            (Some((start, _)), Some((_, end))) if from < at => runs.push(Leaf {
                start,
                end,
                node: paragraph,
                lines: Some((from, at)),
            }),
            _ => return false,
        }
        from = at + 1;
    }
    out.extend(runs);
    true
}

/// Lines `start..end` of a source holding the leaf blocks `leaves` and the escapes `escapes` in them.
struct Window<'p> {
    start: usize,
    end: usize,
    leaves: Vec<Leaf<'p>>,
    escapes: Vec<usize>,
}

/// [`prune_whole_group`] with each trial parsing only the lines of the leaf blocks that hold the escape
/// (C2-W-010). A window is used only where its parse gives its leaf blocks as the group reads them, and the
/// pruned group must read back as a whole; `None` leaves every decision to the whole-group trials.
fn prune_in_windows(
    w: &mut Writer<'_>,
    group: Arr<'_>,
    source: &str,
    raws: &[String],
    round: &Round<'_>,
    split: bool,
) -> Option<String> {
    if !round.equal || round.source != Some(source) {
        return None;
    }
    let parsed = round.last?;
    let mut notes = NoteEscapes::default();
    let escapes = prunable_escapes(source, raws, parsed, &mut |p| notes.call(w, source, p));
    if escapes.is_empty() {
        return Some(source.to_string());
    }
    // Most escapes placed by one repair kind are all redundant after the
    // complete group settles. Test that common case in one readback, then
    // fall back to the per-window trials when only some may be removed.
    if let Some(pruned) = try_prune_all(w, group, source, &escapes, round) {
        return Some(pruned);
    }
    let mut leaves = Vec::new();
    leaf_blocks(children(parsed), split, &mut leaves);
    let mut accepted = Vec::new();
    for window in windows_of(source, &escapes, &leaves)? {
        accepted.extend(prune_window(w, source, &window, round)?);
    }
    if accepted.is_empty() {
        return Some(source.to_string());
    }
    round.read(w, without_bytes(source, &accepted)?, |_, text, parsed| {
        let kids = children(parsed);
        let same = kids.len() == group.len()
            && kids.iter().zip(group).all(|(n, e)| node::trees_equal(n, e));
        same.then(|| text.to_string())
    })
}

fn try_prune_all(
    w: &mut Writer<'_>,
    group: Arr<'_>,
    source: &str,
    escapes: &[usize],
    round: &Round<'_>,
) -> Option<String> {
    if escapes.len() < 2 {
        return None;
    }
    round.read(w, without_bytes(source, escapes)?, |_, text, parsed| {
        let kids = children(parsed);
        (kids.len() == group.len() && kids.iter().zip(group).all(|(n, e)| node::trees_equal(n, e)))
            .then(|| text.to_string())
    })
}

/// The backslashes of `source` that the prune tries, in source order; `parsed` is its parse.
fn prunable_escapes(
    source: &str,
    raws: &[String],
    parsed: V<'_>,
    notes: &mut dyn FnMut(usize) -> NoteCall,
) -> Vec<usize> {
    let raw_spans = raw_spans(source, raws);
    let mut opens: Option<AddrSet<usize>> = None;
    let mut current = || Some(parsed);
    let mut next = next_byte_after(source);
    // The count of text backslashes, asked for the parse of `source` once.
    let mut others = None;
    source
        .match_indices('\\')
        .map(|(p, _)| p)
        .filter(|&p| {
            !(source
                .get(p + 1..p + 3)
                .is_some_and(|m| m == "{*" || m == "{/")
                && opens
                    .get_or_insert_with(|| bare_pair_openers(source, parsed))
                    .contains(&(p + 2)))
                && match notes(p) {
                    NoteCall::Prune => true,
                    NoteCall::Keep => false,
                    NoteCall::Open => {
                        prunable_opener(source, p, &mut current, &mut next, &mut others)
                    }
                }
                && !in_raw(&raw_spans, p)
        })
        .collect()
}

/// The offsets of the bare marks that open an emphasis or strong of `parsed`, the parse of `source`. Without
/// its backslash a `{` before such a mark reads with it as a braced opener, so the pair would not form.
fn bare_pair_openers(source: &str, parsed: V<'_>) -> AddrSet<usize> {
    let mut nodes = Vec::new();
    node::descendants(parsed, &mut nodes);
    nodes
        .into_iter()
        .filter_map(|n| {
            let mark = match ty(n) {
                "strong" => b'*',
                "emphasis" => b'/',
                _ => return None,
            };
            let (start, _) = span(n)?;
            (source.as_bytes().get(start) == Some(&mark)).then_some(start)
        })
        .collect()
}

/// The windows of `escapes`, each the lines of the leaf that holds an escape, merged where they share a line;
/// `None` where an escape lies in no leaf.
fn windows_of<'p>(source: &str, escapes: &[usize], leaves: &[Leaf<'p>]) -> Option<Vec<Window<'p>>> {
    let mut windows: Vec<Window<'p>> = Vec::new();
    let mut next_leaf = leaves.iter().peekable();
    for &p in escapes {
        while next_leaf.peek().is_some_and(|l| l.end <= p) {
            next_leaf.next();
        }
        let &&leaf = next_leaf.peek().filter(|l| l.start <= p)?;
        let line_start = source
            .get(..leaf.start)
            .and_then(|s| s.rfind('\n'))
            .map_or(0, |i| i + 1);
        let tail = leaf.end.saturating_sub(1);
        let line_end = source
            .get(tail..)
            .and_then(|s| s.find('\n'))
            .map_or(source.len(), |i| tail + i + 1);
        match windows.last_mut() {
            Some(last) if line_start < last.end => {
                last.end = last.end.max(line_end);
                if last.leaves.last().is_none_or(|l| l.start != leaf.start) {
                    last.leaves.push(leaf);
                }
                last.escapes.push(p);
            }
            _ => windows.push(Window {
                start: line_start,
                end: line_end,
                leaves: vec![leaf],
                escapes: vec![p],
            }),
        }
    }
    Some(windows)
}

/// `source` without the bytes at the increasing offsets `at`.
fn without_bytes(source: &str, at: &[usize]) -> Option<String> {
    let mut out = String::with_capacity(source.len());
    let mut from = 0;
    for &p in at {
        out.push_str(source.get(from..p)?);
        from = p + 1;
    }
    out.push_str(source.get(from..)?);
    Some(out)
}

/// The escapes of `window` that its text reads the same without, tried in source order as the whole-group
/// trials try them; `None` where the window alone does not read its leaf blocks as the group does.
fn prune_window(
    w: &mut Writer<'_>,
    source: &str,
    window: &Window<'_>,
    round: &Round<'_>,
) -> Option<Vec<usize>> {
    // A window below line 1 keeps one line above, as a group below another does (C2-W-005).
    let lead = usize::from(window.start > 0);
    let local = |at: usize| at - window.start + lead;
    let mut text = String::with_capacity(window.end - window.start + lead);
    if lead == 1 {
        text.push('\n');
    }
    text.push_str(source.get(window.start..window.end)?);
    let (base, read) = round.parse(w, text);
    let mut found = Vec::new();
    leaf_blocks(children(read), false, &mut found);
    let faithful = window.leaves.iter().all(|leaf| {
        found.iter().any(|f| {
            f.start == local(leaf.start)
                && f.end == local(leaf.end)
                && match leaf.lines {
                    None => node::trees_equal(f.node, leaf.node),
                    Some((from, to)) => {
                        ty(f.node) == "paragraph"
                            && children(leaf.node)
                                .slice(from, to)
                                .is_some_and(|run| node::lists_equal(children(f.node), run))
                    }
                }
        })
    });
    if !faithful || exhausted(w) {
        return None;
    }
    let mut text = base.to_string();
    let mut accepted = Vec::new();
    for &p in &window.escapes {
        let at = local(p) - accepted.len();
        let candidate = format!("{}{}", text.get(..at)?, text.get(at + 1..)?);
        let kept = round.read(w, candidate, |_, candidate, parsed| {
            blocks_equal(children(parsed), children(read)).then(|| candidate.to_string())
        });
        if let Some(candidate) = kept {
            text = candidate;
            accepted.push(p);
        }
        if exhausted(w) {
            return None;
        }
    }
    Some(accepted)
}

fn blocks_equal(read: Arr<'_>, blocks: Arr<'_>) -> bool {
    read.len() == blocks.len()
        && read
            .iter()
            .zip(blocks)
            .all(|(n, e)| node::trees_equal(n, e))
}

/// Each prunable escape tried by a parse of the whole group without it.
fn prune_whole_group(
    w: &mut Writer<'_>,
    group: Arr<'_>,
    source: &mut String,
    raws: &[String],
    round: &Round<'_>,
) {
    let mut at = source.find('\\');
    // The parse of `source` as it stands, read again (uncounted) after a prune only where a star asks.
    let mut parsed = round.last;
    let mut parsed_source: Option<String> = None;
    let mut notes = NoteEscapes::default();
    let mut spans = raw_spans(source, raws);
    let mut others = None;
    while let Some(p) = at {
        if exhausted(w) {
            if let Some(host) = group.first() {
                refuse_repair(w, host);
            }
            break;
        }
        let mut current = || {
            if parsed.is_none() {
                let chunk = round.alloc(parsed_source.take()?);
                parsed = Some(chunk.read(w, Some(&w.link_keys)));
            }
            parsed
        };
        let mut next = |b, from| memchr_from(source, b, from);
        let call = notes.call(w, source, p);
        if (call == NoteCall::Prune
            || (call == NoteCall::Open
                && prunable_opener(source, p, &mut current, &mut next, &mut others)))
            && !in_raw(&spans, p)
        {
            let candidate = format!(
                "{}{}",
                source.get(..p).unwrap_or(""),
                source.get(p + 1..).unwrap_or("")
            );
            let chunk = Chunk::new(candidate);
            let kids = children(chunk.parse(w));
            if kids.len() == group.len()
                && kids.iter().zip(group).all(|(n, e)| node::trees_equal(n, e))
            {
                *source = chunk.text().to_string();
                spans = raw_spans(source, raws);
                notes = NoteEscapes::default();
                parsed = None;
                others = None;
                parsed_source = Some(source.clone());
                at = source.get(p..).and_then(|r| r.find('\\')).map(|q| q + p);
                continue;
            }
        }
        at = source
            .get(p + 1..)
            .and_then(|r| r.find('\\'))
            .map(|q| q + p + 1);
    }
}

#[cfg(test)]
mod repair_limit_test {
    use super::*;

    #[test]
    fn exhausted_repair_budget_reports_writer_refused() {
        let pool = typed_arena::Arena::new();
        let mut writer = Writer::new(&pool, false);
        writer.lines.push("x".into());
        let group = [
            serde_json::json!({"type": "paragraph", "children": [{"type": "text", "value": "x"}]}),
        ];
        let mut history = History {
            ids: AddrMap::default(),
            recent: std::collections::VecDeque::new(),
            seen: HashSet::new(),
            rounds: 1,
            remaining: 0,
        };

        assert!(repeated_repair(
            &mut writer,
            Arr::Json(&group),
            &mut history
        ));
        assert_eq!(
            writer.sp.losses.first().map(|loss| loss.code),
            Some("writer-refused")
        );
    }

    #[test]
    fn budget_grows_with_the_written_length() {
        let pool = typed_arena::Arena::new();
        let mut writer = Writer::new(&pool, false);
        writer.work_limit = repair_budget(10);
        writer.written_chars = 40_000;
        writer.lines.push("xyz".into());
        let group = [
            serde_json::json!({"type": "paragraph", "children": [{"type": "text", "value": "xyz"}]}),
        ];
        let mut history = History {
            ids: AddrMap::default(),
            recent: std::collections::VecDeque::new(),
            seen: HashSet::new(),
            rounds: 0,
            remaining: i64::try_from(MAX_REPAIR_CHARS).unwrap_or(i64::MAX),
        };

        assert!(!repeated_repair(
            &mut writer,
            Arr::Json(&group),
            &mut history
        ));
        assert_eq!(writer.work_limit, 40_004 * WORK_FACTOR);
    }
}

#[cfg(test)]
mod json_facts_tests {
    use super::{
        JsonFacts, has_adjacent_plain_text, has_attribute_lines, json_estimate, pairs, spans, trim,
    };
    use serde_json::{Value, json};

    fn trees() -> Vec<Value> {
        let text = include_str!("../../spec-conformance/clause-cases.json");
        let mut de = serde_json::Deserializer::from_str(text);
        de.disable_recursion_limit();
        let cases: Value = serde::Deserialize::deserialize(&mut de).unwrap_or_default();
        let mut out = vec![
            json!({"type": "document", "children": [{"type": "paragraph", "attrs":
                {"keyValues": [["k", "a\nb"]]}, "children": [{"type": "text", "value": "x *y "}]}]}),
            json!({"type": "table_cell", "colspan": 2, "children": []}),
            json!({"type": "paragraph", "children": [{"type": "text", "value": "a"},
                {"type": "text", "value": "b"}]}),
            json!({"type": "table_cell", "rowspan": "3", "data": {"type": "text", "value": "\t"}}),
        ];
        let opts = crate::api::Options::default();
        for case in cases
            .get("cases")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(input) = case.get("input").and_then(Value::as_str)
                && let Ok(tree) = crate::api::parse(input, &opts)
                && let Ok(value) = tree.value()
            {
                out.push(value);
            }
            if let Some(html) = case.pointer("/expected/html").and_then(Value::as_str)
                && let Ok(imported) = crate::import::import_html(html, false, true)
            {
                out.push(imported.tree);
            }
        }
        out
    }

    #[test]
    fn one_walk_reads_what_each_check_reads() {
        let trees = trees();
        assert!(trees.len() > 1000, "{}", trees.len());
        for tree in &trees {
            let facts = JsonFacts::of(tree);
            assert_eq!(facts.attribute_lines, has_attribute_lines(tree), "{tree}");
            assert_eq!(
                facts.adjacent_plain_text,
                has_adjacent_plain_text(tree),
                "{tree}"
            );
            assert_eq!(facts.spaced_text, trim::has_spaced_text(tree), "{tree}");
            assert_eq!(facts.mark_text, pairs::json_has_mark_text(tree), "{tree}");
            assert_eq!(facts.counts, spans::json_has_counts(tree), "{tree}");
            assert_eq!(facts.estimate, json_estimate(tree), "{tree}");
        }
    }
}

#[cfg(test)]
mod break_after_break_test {
    use super::{Settings, parse_plain, write_value};
    use serde_json::{Value, json};

    fn inlines(count: usize, first: &str) -> Value {
        let mut kids = vec![json!({"type":"text","value":"Note"})];
        kids.extend((0..count).map(|k| json!({"type": if k == 0 { first } else { "soft_break" }})));
        kids.push(json!({"type":"text","value":"A note."}));
        Value::Array(kids)
    }

    fn losses(doc: &Value) -> Vec<String> {
        write_value(doc, &Settings::default())
            .1
            .iter()
            .map(|e| format!("{} {}", e.code, e.path.as_deref().unwrap_or("")))
            .collect()
    }

    #[test]
    fn note_tree_reports_each_lost_soft_break() {
        let doc = json!({"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":inlines(3, "soft_break")}]});
        let (out, entries) = write_value(&doc, &Settings::default());
        assert_eq!(out, "Note\n\n\nA note.\n");
        let got: Vec<_> = entries
            .iter()
            .map(|e| {
                (
                    e.code.as_str(),
                    e.channel.as_str(),
                    e.severity.as_str(),
                    e.path.as_deref(),
                    e.message.as_str(),
                )
            })
            .collect();
        let message = "Carve source cannot spell a soft_break node here";
        assert_eq!(
            got,
            [
                (
                    "structure-unspellable",
                    "render-loss",
                    "warning",
                    Some("/children/0/children/2"),
                    message
                ),
                (
                    "structure-unspellable",
                    "render-loss",
                    "warning",
                    Some("/children/0/children/3"),
                    message
                ),
            ]
        );
    }

    #[test]
    fn one_entry_per_soft_break_behind_a_break() {
        for (src, at) in [
            ("Note\n", "/children/0"),
            ("- Note\n", "/children/0/children/0/children/0"),
            ("> Note\n", "/children/0/children/0"),
        ] {
            for first in ["soft_break", "hard_break"] {
                for count in 1..=6 {
                    let mut doc = parse_plain(src).unwrap_or_default();
                    if let Some(kids) = doc.pointer_mut(&format!("{at}/children")) {
                        *kids = inlines(count, first);
                    }
                    let expected: Vec<_> = (1..count)
                        .map(|k| format!("structure-unspellable {at}/children/{}", k + 1))
                        .collect();
                    assert_eq!(losses(&doc), expected, "{src:?} {first} {count}");
                }
            }
        }
    }

    #[test]
    fn inside_emphasis_the_loss_is_the_soft_break() {
        let mut doc = parse_plain("Note\n").unwrap_or_default();
        if let Some(kids) = doc.pointer_mut("/children/0/children") {
            *kids = json!([{"type":"text","value":"A "}, {"type":"emphasis","children":inlines(2, "soft_break")}]);
        }
        let (out, _) = write_value(&doc, &Settings::default());
        assert_eq!(out, "A /Note\n\nA note./\n");
        assert_eq!(
            losses(&doc),
            ["structure-unspellable /children/0/children/1/children/2"]
        );
    }
}
