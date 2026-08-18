//! Carve parser (MVP subset).
//!
//! Block-level reads line by line; inline does a single linear scan
//! over each block's text. No backtracking.

use crate::ast::Pos;
use crate::ast::*;
use crate::extension::{BlockMatch, InlineMatch, MatcherContext, Options};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};

/// The line a collected definition leaves behind, and the marker that says it
/// is ours rather than something the author wrote.
///
/// A definition removed from a container cannot leave a blank line: inside a
/// quoted list item the leftover structural prefix IS blank, and a blank there
/// loosens the list (§17 L1) even though the definition rendered nothing. `%%`
/// is the one construct that is invisible at any column and closes nothing
/// (§24 C3), so the line is replaced with a comment rather than emptied.
///
/// It must not become a comment NODE. An authored `%%` and this placeholder are
/// otherwise the same thing, so the writer serialized a comment the author never
/// typed - carve-rs#602, corpus 194 - and an item holding one is not empty, so
/// the writer's emptied-item branch never fired and it wrote `- %%` where the
/// other two engines write `- +` (markup-carve/carve#620).
///
/// The private-use suffix is what tells them apart. It reaches only the block
/// parser's `%%` arm, which drops the line instead of building a node; nothing
/// downstream sees it, and the line is still non-blank for every collection and
/// tightness decision made before that point. Same device as `VERBATIM_BLANK`
/// in the writer.
const DEFINITION_PLACEHOLDER: &str = "%%\u{E005}";
const DOCUMENT_DEFINITION_PLACEHOLDER: &str = "%%\u{E006}";

fn is_definition_placeholder(line: &str) -> bool {
    matches!(
        trim_ascii(line),
        DEFINITION_PLACEHOLDER | DOCUMENT_DEFINITION_PLACEHOLDER
    )
}

/// Maximum block + inline nesting depth. Pathological input (deeply nested
/// blockquotes, indented lists, bracketed inlines) recurses one stack frame
/// per level; without a cap a ~1000-deep document aborts the process with a
/// stack overflow (uncatchable -- a hard DoS for any embedder). Over the cap
/// the parser degrades gracefully (remaining block content becomes a flat
/// paragraph; inline content stays literal text) instead of recursing further.
///
/// The cap also bounds the depth of the AST the renderers walk recursively, so
/// it bounds the depth of the AST the renderers walk recursively.
///
/// The cap is 200, applied UNIFORMLY to blockquote, list, div, and admonition
/// nesting, matching carve-js (`MAX_NESTING_DEPTH = 200`) and carve-php so the
/// three implementations degrade at the same depth. Deeply nested input
/// degrades gracefully at the cap (remaining block content becomes a flat
/// paragraph; inline content stays literal) instead of recursing further, so
/// the AST depth is bounded by this constant. The recursive-descent parser and
/// the renderers use one native stack frame per level; in a release build 200
/// levels fit comfortably in a default 2 MiB thread stack (a debug build's
/// larger frames need more, which is why the worst-case-depth robustness tests
/// run on a generous worker stack). carve-php's analogous cap relies on PHP
/// growing its VM stack on the heap.
pub(crate) const MAX_NESTING_DEPTH: usize = 200;

/// The format a BARE frontmatter fence is in (PART 1, "a bare `---` defaults to
/// `yaml`").
///
/// Named rather than spelled twice: the parser publishes it as
/// `Frontmatter::format` and the canonical writer emits it on the opening fence
/// (PART 11 section 6b), and a second literal is a second place for the two to
/// drift apart.
pub(crate) const DEFAULT_FRONTMATTER_FORMAT: &str = "yaml";

fn trim_ascii_start(s: &str) -> &str {
    s.trim_start_matches([' ', '\t'])
}

fn trim_ascii_end(s: &str) -> &str {
    s.trim_end_matches([' ', '\t'])
}

fn trim_ascii(s: &str) -> &str {
    trim_ascii_end(trim_ascii_start(s))
}

fn is_blank_line(s: &str) -> bool {
    trim_ascii(s).is_empty()
}

thread_local! {
    // Plain initializer (not `const { … }`) to keep the crate's 1.75 MSRV;
    // the inline-const thread-local form clippy suggests requires Rust 1.79+,
    // so the lint is allowed here rather than followed.
    #[allow(clippy::missing_const_for_thread_local)]
    static NESTING_DEPTH: Cell<usize> = Cell::new(0);
}

/// RAII recursion-depth guard. `enter()` returns `None` when the cap is
/// already reached (the caller must degrade without recursing); otherwise it
/// increments the shared depth and returns a guard that decrements on drop
/// (including during panic unwind, so a normal parse always returns to 0).
struct DepthGuard;

impl DepthGuard {
    fn enter() -> Option<DepthGuard> {
        NESTING_DEPTH.with(|d| {
            if d.get() >= MAX_NESTING_DEPTH {
                None
            } else {
                d.set(d.get() + 1);
                Some(DepthGuard)
            }
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        NESTING_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

thread_local! {
    // GROUPS DO NOT NEST (PART 9 §4c): a bare `::: figure` opener anywhere
    // inside an open group's body - at ANY depth, through divs, quotes and
    // list items - is a generic Tier-2 container, not an inner group. The
    // body parses through several re-entrant helpers on this thread, so the
    // state lives beside the depth counter they already share.
    //
    // Plain initializer for the same MSRV reason as `NESTING_DEPTH` above.
    #[allow(clippy::missing_const_for_thread_local)]
    static IN_FIGURE_GROUP: Cell<bool> = Cell::new(false);
}

thread_local! {
    // WHERE A FLOATING ATTRIBUTE RAN OUT OF BLOCKS (PART 9 §15 A4, ruled in
    // markup-carve/carve#1281). `None` while nobody is collecting, which is
    // every parse but the linter's - a `Some` is installed for the duration of
    // one call and taken back out at the end.
    //
    // A thread-local rather than an out-parameter because the drop happens
    // wherever a container's body finished, deep inside a recursion that returns
    // `Vec<BlockNode>` through a dozen call sites - the same reason the depth
    // counter and the figure-group flag above live here.
    //
    // Plain initializer for the same MSRV reason as `NESTING_DEPTH`.
    #[allow(clippy::missing_const_for_thread_local)]
    static UNATTACHED_BLOCK_ATTRS: RefCell<Option<Vec<Pos>>> = const { RefCell::new(None) };
}

/// Run `f`, collecting the source spans of every block attribute that reached
/// no block (PART 9 §15 A4).
///
/// The two ways to run out of following blocks - the end of the DOCUMENT and
/// the end of the CONTAINER holding the attribute - meet at one site, because a
/// container's body is parsed by its own [`parse_blocks`] call: a set still
/// pending when that call's loop ends is a set with nothing left in scope.
///
/// Positions are only recorded when the caller asked for them ([`Options`]
/// `positions`), since a span is what makes a diagnostic locatable and there is
/// nothing useful to report without one.
pub(crate) fn collecting_unattached_block_attrs<T>(f: impl FnOnce() -> T) -> (T, Vec<Pos>) {
    let previous = UNATTACHED_BLOCK_ATTRS.with(|slot| slot.replace(Some(Vec::new())));
    let out = f();
    let collected = UNATTACHED_BLOCK_ATTRS.with(|slot| slot.replace(previous));
    (out, collected.unwrap_or_default())
}

/// Record one dangling attribute run, if anybody is collecting.
fn note_unattached_block_attrs(pos: Option<Pos>) {
    if probing() {
        return;
    }
    let Some(pos) = pos else { return };
    UNATTACHED_BLOCK_ATTRS.with(|slot| {
        if let Some(spans) = slot.borrow_mut().as_mut() {
            spans.push(pos);
        }
    });
}

thread_local! {
    // Set while a PROBE is parsing a candidate source to answer a question about
    // it - "does this body end in an open paragraph", "does it end in a heading".
    // Those parses are thrown away, and their pending-attribute state is a fact
    // about a FRAGMENT rather than about the document: `:  d` / `   {.k}` /
    // `tail` probes the body `d` / `{.k}` WITHOUT the line the attribute might
    // still reach, so a recorder that counted probes reported an attribute the
    // finished parse had attached.
    //
    // Plain initializer for the same MSRV reason as `NESTING_DEPTH`.
    #[allow(clippy::missing_const_for_thread_local)]
    static PROBING: Cell<bool> = const { Cell::new(false) };
}

/// RAII guard marking a throwaway parse, restoring the previous value on drop
/// (panic unwind included), the discipline [`DepthGuard`] keeps.
struct ProbeGuard {
    previous: bool,
}

impl ProbeGuard {
    fn enter() -> ProbeGuard {
        ProbeGuard {
            previous: PROBING.with(|p| p.replace(true)),
        }
    }
}

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        PROBING.with(|p| p.set(self.previous));
    }
}

fn probing() -> bool {
    PROBING.with(|p| p.get())
}

/// Parse `source` to ANSWER A QUESTION about it, not to publish it.
///
/// What such a parse notices about attributes it could not place is a fact about
/// the fragment, so it is suppressed - see [`PROBING`].
fn probe_blocks(source: &str, options: &Options<'_>) -> Vec<BlockNode> {
    let _probing = ProbeGuard::enter();
    parse_blocks_with_options(source, options)
}

/// [`probe_blocks`] for a fragment that stands at the DOCUMENT level.
///
/// The level is not a detail the caller may leave at its default: an
/// abbreviation definition is only recognised there (see the `at_document_level`
/// arm of `line_starts_block`), so a probe run one level down reads `*[HTML]: x`
/// as an ordinary paragraph and answers every question about the line after it
/// the wrong way.
fn probe_blocks_at_document_level(source: &str, options: &Options<'_>) -> Vec<BlockNode> {
    let _probing = ProbeGuard::enter();
    parse_blocks_with_options_at_level(source, options, true)
}

/// RAII guard that marks the current thread as parsing a figure group's body,
/// restoring the previous state on drop (panic unwind included), the same
/// discipline [`DepthGuard`] keeps for the depth counter.
struct FigureGroupGuard {
    previous: bool,
}

impl FigureGroupGuard {
    fn enter() -> FigureGroupGuard {
        let previous = IN_FIGURE_GROUP.with(Cell::get);
        IN_FIGURE_GROUP.with(|flag| flag.set(true));
        FigureGroupGuard { previous }
    }
}

impl Drop for FigureGroupGuard {
    fn drop(&mut self) {
        let previous = self.previous;
        IN_FIGURE_GROUP.with(|flag| flag.set(previous));
    }
}

/// Whether a container opener line is the BARE `::: figure` form - the fence,
/// its separator and the kind word, NOTHING else (PART 9 §4c,
/// `figure_group_open`). An opener carrying a quoted title or a label matches
/// `admonition_open` instead and stays a generic container.
fn is_bare_figure_open(open: &ContainerOpen) -> bool {
    open.kind.as_deref() == Some("figure") && open.title.is_none() && open.label.is_none()
}

pub fn parse(source: &str) -> Document {
    parse_with_options(source, &Options::default())
}

pub fn parse_with_options(source: &str, options: &Options<'_>) -> Document {
    parse_with_options_mode(source, options, ParseMode::Html)
}

/// The fmt parse WITHOUT positions, for comparing two renders' shapes.
///
/// `escaping_is_redundant` asks whether the minimal and conservative forms parse
/// to the same document. Two renders that differ only in escape bytes have
/// different offsets, so comparing position-bearing trees answers "no" for every
/// document carrying an escapable character, and the writer escalates to
/// conservative escaping - `See [it][ref].` came back `See [it][ref]\.`
/// (carve-rs#682, found the moment positions were enabled for the ordering
/// parse).
pub(crate) fn parse_for_carve_shape(source: &str) -> Document {
    parse_with_options_mode(source, &Options::default(), ParseMode::Carve)
}

pub(crate) fn parse_for_carve(source: &str) -> Document {
    // POSITIONS ON. The writer orders hoisted definitions by source position
    // (§7, PART 11 §6), and this parse is the only view of the document it
    // gets - without `pos` every definition reports usize::MAX and the order
    // falls back to "children, then footnotes by label", which is what put a
    // link definition ahead of the footnote it was written inside
    // (carve-rs#682).
    parse_with_options_mode(
        source,
        &Options::default().with_positions(true),
        ParseMode::Carve,
    )
}

#[derive(Clone, Copy)]
enum ParseMode {
    Html,
    Carve,
}

/// The text every entry point must read, normalized once (matching carve-js /
/// carve-php), allocating only when there is something to change:
///
///  - strip a single leading UTF-8 BOM (U+FEFF) so `\u{feff}# T` is a heading;
///  - collapse CRLF / CR to LF;
///  - replace a NUL (U+0000) with the U+FFFD replacement char so a control byte
///    never reaches output (WHATWG-style).
///
/// This lived inline in `parse_with_options_mode` and was the parser's alone,
/// so `to_carve` scanned the RAW source for the frontmatter block while the
/// parser scanned this copy. On a CRLF or BOM'd document the two disagreed
/// about whether the file HAD frontmatter, and the writer lost the block's
/// format token or dropped the block entirely (carve-rs#732). It is a function
/// so there is one answer to that question rather than one per caller - the
/// same reason carve-rs#725 had to unify the frontmatter OPENER test between
/// these two callers.
/// Join collected lines back into a source string, TERMINATED.
///
/// The parser rebuilds its source several times - a footnote body, the document
/// body after definitions are lifted out, a container's collected lines - and
/// each consumer splits it again with `str::lines()`. `join` alone makes that
/// round trip lossy in exactly one place, a trailing EMPTY line:
///
/// ```text
/// ["a", ""]  ->  join  ->  "a\n"    ->  lines()  ->  ["a"]      the blank is gone
/// ["a", ""]  ->  here  ->  "a\n\n"   ->  lines()  ->  ["a", ""]  preserved
/// ["a"]      ->  here  ->  "a\n"    ->  lines()  ->  ["a"]      unchanged
/// ```
///
/// Every other line survives a plain `join` because the separator before it is
/// still in the string; only the last one has nothing after it to imply it.
/// `lines()` drops one trailing newline, so terminating changes the
/// empty-last-line case and nothing else (markup-carve/carve-rs#908).
fn joined_source(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut joined = lines.join("\n");
    joined.push('\n');
    joined
}

pub(crate) fn normalize_source(source: &str) -> std::borrow::Cow<'_, str> {
    if !(source.starts_with('\u{feff}') || source.contains('\r') || source.contains('\0')) {
        return std::borrow::Cow::Borrowed(source);
    }
    let trimmed = source.strip_prefix('\u{feff}').unwrap_or(source);
    std::borrow::Cow::Owned(
        trimmed
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\0', "\u{fffd}"),
    )
}

fn parse_with_options_mode(source: &str, options: &Options<'_>, mode: ParseMode) -> Document {
    // Kept for the offset table below: normalization rewrites the text the
    // parser sees, and PART 12 §4 positions index the ORIGINAL file.
    let original = source;
    let normalized = normalize_source(source);
    let source = normalized.as_ref();
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, options.positions);
    let body_start_line = source
        [..(body.as_ptr() as usize).saturating_sub(source.as_ptr() as usize)]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    let (body, footnote_defs_src, mut footnote_def_pos, note_link_defs) =
        extract_footnote_defs(body, body_start_line, options.positions, options);
    let (body_source, mut link_defs) = extract_link_defs(&body.source);
    // A definition written inside a footnote body is document-level metadata,
    // and a definition at the top level WINS over one of the same label there -
    // measured on carve-js and carve-php, which both resolve `[t][r]` to the
    // outer target when both exist.
    for (label, def) in note_link_defs {
        link_defs.entry(label).or_insert(def);
    }
    let body = remap_source(body_source, &body);
    let mut footnote_defs: BTreeMap<String, Vec<BlockNode>> = footnote_defs_src
        .into_iter()
        .map(|(label, source)| (label, parse_mapped_source(&source, options)))
        .collect();
    let mut children = parse_mapped_source_at_document_level(&body, options);
    if options.positions {
        // Offsets need the original text, which the parser only ever sees as
        // already-stripped lines, so they are derived here in one pass.
        // BUILT FROM THE ORIGINAL TEXT, not the normalized copy. Normalization
        // strips a leading BOM and collapses CRLF/CR to LF, so a table built
        // from the result is short by one codepoint per removed character and
        // every offset in the document lands before the text it names -
        // `<BOM># T` reported the space where the node said `T` (carve#876).
        //
        // Line NUMBERS are unaffected: the normalization preserves the line
        // count, so entry N still describes line N. Only where each line starts
        // in the file changes, which is exactly what this table holds.
        let line_starts = original_line_start_offsets(original);
        fill_offsets(&mut children, &line_starts);
        for blocks in footnote_defs.values_mut() {
            fill_offsets(blocks, &line_starts);
        }
        include_comment_indentation(&mut children, original, &line_starts);
        for blocks in footnote_defs.values_mut() {
            include_comment_indentation(blocks, original, &line_starts);
        }
        // A definition whose BODY places something already has an extent, and
        // that extent - not this line - is what reaches the wire. Keeping the
        // line beside it would put a fact in the document that its own
        // serialization does not carry, so an ingested copy would differ from
        // the parse (§6). Dropped here rather than never recorded because the
        // bodies are parsed after the scan that finds the definition lines.
        // Place both boundaries; a multi-line definition ends at the following
        // line start rather than on its opener line.
        for pos in footnote_def_pos.values_mut() {
            let Some(start) = line_starts.get(pos.start_line - 1).copied() else {
                continue;
            };
            let Some(end) = line_starts.get(pos.end_line - 1).copied() else {
                continue;
            };
            pos.start_offset = start + pos.start_column - 1;
            pos.end_offset = end + pos.end_column - 1;
        }
        for (label, pos) in &mut footnote_def_pos {
            if let Some(last) = footnote_defs
                .get(label)
                .and_then(|blocks| blocks.iter().rev().find_map(crate::ast_json::block_pos))
            {
                if last.end_offset > pos.end_offset {
                    pos.end_line = last.end_line;
                    pos.end_column = last.end_column;
                    pos.end_offset = last.end_offset;
                }
            }
        }
        widen_over_hosted_definitions(&mut children, &footnote_def_pos);
    }
    let mut doc = Document {
        frontmatter,
        frontmatter_raw,
        footnote_defs,
        footnote_def_pos,
        children,
        source_len: source.len(),
        // Measured here, so nothing about the parse path needs a second number.
        ingest_payload_len: 0,
    };
    let heading_index = heading_index(
        &doc.children,
        &doc.footnote_defs,
        options.lowercase_heading_ids,
    );
    resolve_reference_links(&mut doc, &link_defs, &heading_index);
    append_link_reference_definitions(&mut doc, &link_defs, source, options);
    if matches!(mode, ParseMode::Html) {
        apply_abbreviations(&mut doc);
        number_crossref_captions(&mut doc);
        // PART 12 §5 again: a heading's GENERATED id is a resolution result and
        // is serialized. It is not recomputable from the heading - dedup assigns
        // the next free suffix in DOCUMENT ORDER, so `Notes-2` needs every
        // heading before it.
        //
        // Assigned HERE rather than in `ast_json::to_json`, because §6 asks that
        // `from_json(to_json(parse(x))) == parse(x)`: a field added at encode
        // time comes back on decode and the trees no longer match. carve-js
        // assigns it in its parse for the same reason (carve#750).
        stamp_generated_heading_ids(&mut doc, options.lowercase_heading_ids);
        // PART 12 §5: footnote numbering is a resolution result that IS
        // serialized, "because recomputing them requires reimplementing PART 9R".
        // Caption numbers were assigned here and reached the wire; footnote
        // numbers were assigned only inside the HTML renderer, so `--ast` and
        // every other consumer of the tree saw none (carve-rs#638).
        //
        // The existing pass is reused rather than a second numbering rule
        // written: first-use order, a repeat reusing its number, and a reference
        // to an undefined label left unnumbered. Its endnote list is discarded
        // here - only the numbers it writes onto the refs are wanted.
        // `ref_id` is NOT assigned here. It is an HTML backlink anchor; the
        // schema permits the field and carve-js does not publish it, so writing
        // it would add something to the wire no other engine emits. The flag is a
        // parameter rather than a second walk - this tree has 51 inline variants
        // and a hand-written sweep to undo it would silently miss one.
        let _ = crate::render::collect_footnotes(&mut doc, false);
        // A resolved reference image lands as a one-image paragraph (the
        // syntactic block-image check ran before resolution); promote it to a
        // block image like a standalone direct image, matching carve-php.
        promote_block_images(&mut doc.children, false);
        for blocks in doc.footnote_defs.values_mut() {
            promote_block_images(blocks, false);
        }
    } else {
        // Carve/fmt mode: promote image+caption paragraphs to figures too, so a
        // caption serializes as an unescaped `^ …` line -- portable and
        // round-tripping in every implementation. Without this the caption would
        // stay a paragraph `[Image, SoftBreak, "^ …"]` and the leading `^` would
        // be escaped to `\^`, which only carve-js's lenient parser reads back as
        // a caption (carve-rs / carve-php read it as literal text, losing the
        // figure). Reference-link resolution already ran above.
        promote_block_images(&mut doc.children, true);
        for blocks in doc.footnote_defs.values_mut() {
            promote_block_images(blocks, true);
        }
    }
    for ext in &options.extensions {
        doc = ext.after_parse(doc);
    }
    // Last, so it also covers runs an extension left behind: §1a is about the
    // tree that gets published, whoever produced it.
    coalesce_text_runs(&mut doc);
    // After every extension has had its say, so a heading an extension added is
    // a crossref target like any other.
    fill_crossref_hrefs(&mut doc, options.lowercase_heading_ids);
    doc
}

/// Publish each crossref's resolution BESIDE its authored target
/// (PART 12 section 3a).
///
/// This engine resolves `</#id>` at RENDER time, from an index built per
/// render, which is why the tree carried only what the author wrote. That is
/// half of what section 3a asks for: a consumer decoding the published tree
/// had to rebuild the heading table and re-run the case-insensitive match
/// before it could render a crossref, which is the recomputation section 5
/// exists to prevent.
///
/// The renderers keep using their index rather than this field. Both come from
/// the same builder, so they cannot disagree, and the render path stays able to
/// resolve a tree that arrived without hrefs at all.
fn fill_crossref_hrefs(doc: &mut Document, lowercase_ids: bool) {
    let index = crossref_index_for_document(doc, lowercase_ids);

    fn inlines(nodes: &mut [InlineNode], index: &CrossrefIndex) {
        for node in nodes {
            match node {
                InlineNode::CrossRef(c) => {
                    c.href = index.resolve(&c.target).map(|(id, _)| format!("#{id}"));
                }
                InlineNode::Emphasis(e) => inlines(&mut e.children, index),
                InlineNode::Span(sp) => inlines(&mut sp.children, index),
                InlineNode::Link(l) => inlines(&mut l.children, index),
                InlineNode::Footnote(f) => {
                    if let Some(inline) = &mut f.inline {
                        inlines(inline, index);
                    }
                }
                _ => {}
            }
        }
    }

    fn blocks(nodes: &mut [BlockNode], index: &CrossrefIndex) {
        for node in nodes {
            match node {
                BlockNode::Paragraph(p) => inlines(&mut p.children, index),
                BlockNode::Heading(h) => inlines(&mut h.children, index),
                BlockNode::BlockQuote(b) => {
                    blocks(&mut b.children, index);
                }
                BlockNode::Div(d) => blocks(&mut d.children, index),
                BlockNode::Figure(f) => inlines(&mut f.caption, index),
                BlockNode::FigureGroup(g) => {
                    blocks(&mut g.children, index);
                    if let Some(caption) = &mut g.caption {
                        inlines(caption, index);
                    }
                }
                BlockNode::List(l) => {
                    for item in &mut l.items {
                        blocks(&mut item.children, index);
                    }
                }
                BlockNode::DefinitionList(d) => {
                    for entry in &mut d.items {
                        for term in &mut entry.terms {
                            inlines(&mut term.children, index);
                        }
                        for description in &mut entry.definitions {
                            blocks(&mut description.children, index);
                        }
                    }
                }
                BlockNode::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            inlines(&mut cell.children, index);
                        }
                    }
                    if let Some(caption) = &mut t.caption {
                        inlines(caption, index);
                    }
                }
                BlockNode::LineBlock(l) => blocks(&mut l.children, index),
                _ => {}
            }
        }
    }

    blocks(&mut doc.children, &index);
    let labels: Vec<String> = doc.footnote_defs.keys().cloned().collect();
    for label in labels {
        if let Some(body) = doc.footnote_defs.get_mut(&label) {
            blocks(body, &index);
        }
    }
}

fn remap_source(source: String, original: &MappedSource) -> MappedSource {
    let source_line_count = source.lines().count();
    if source_line_count <= original.line_map.len() {
        return MappedSource {
            source,
            line_map: original.line_map[..source_line_count].to_vec(),
            col_map: original.col_map[..source_line_count.min(original.col_map.len())].to_vec(),
        };
    }
    MappedSource {
        line_map: (1..=source_line_count).map(Some).collect(),
        // Top-level source: nothing has been stripped, so every column in this
        // text is a column in the document.
        col_map: vec![Some(0); source_line_count],
        source,
    }
}

/// The content column of the innermost list item a line-based prepass is inside.
///
/// Both definition prepasses need it for the same reason: a definition on an
/// item's CONTINUATION line carries no marker, so `strip_container_prefixes`
/// leaves the item's indentation in front of the `[` and the line stops looking
/// like a definition. Stripping exactly this many columns - never more - is what
/// separates a definition AT the content column (collected) from one below it
/// (paragraph text that registers nothing, PART 9 §24 C3 as carve#624 states it).
///
/// A line-based approximation, as the note in `extract_link_defs` says: tab-vs-
/// space marker alignment is char-counted, the post-blank `baseIndent + 2` rule
/// is not modeled, and lists inside blockquotes are not fully modeled.
#[derive(Default)]
struct ContentColumns {
    /// One frame per open BLOCKQUOTE level, `frames[0]` being document level.
    /// Each frame holds the list content columns live inside that container.
    ///
    /// A flat stack could not answer this: cases 1 and 2 of carve-rs#593 need an
    /// item's columns DISCARDED when a quote opens under it, cases 3 and 4 need
    /// them KEPT across a quote nested at the item's own column - and the indent
    /// a flat stack compares against is in a different coordinate system
    /// depending on how deep the quote nesting is. Scoping by container answers
    /// "which columns are live" from structure instead of from an indent
    /// measured in whichever coordinate the caller happened to strip to.
    frames: Vec<ColumnFrame>,
    prev_blank: bool,
}

/// One container's live content columns, plus the definition list open inside
/// it.
///
/// A DEFINITION BODY IS A CONTAINER WITH A CONTENT COLUMN, like a list item
/// (PART 9 §24 C3, markup-carve/carve#1350). The columns alone could not say so:
/// a `:  ` line opens a body whose content column is three past the marker, and
/// a definition written there is that body's block - so it registers, and it
/// interrupts. Without the open list recorded, a bare `:  x` line anywhere would
/// claim a column that no `dd` was ever opened at.
#[derive(Default)]
struct ColumnFrame {
    cols: Vec<usize>,
    /// The indent of the innermost OPEN definition list, set by its `:: ` term.
    def_list: Option<usize>,
}

impl ContentColumns {
    fn new() -> Self {
        Self {
            frames: vec![ColumnFrame::default()],
            prev_blank: true,
        }
    }

    /// Feed the next RAW line (quote markers included). `opaque` suppresses
    /// tracking inside a fence or a line block, where `- verse` is content
    /// rather than a marker and a `>` is text rather than a container.
    ///
    /// THE COLUMN IS REACHED BY COMPOSING THE STRIPS, NOT BY WALKING THE
    /// PREFIX (PART 1 S4). A `>` written at an item's content column is that
    /// item's container prefix exactly as a flush-left one is, so the walk
    /// peels ONE level at a time and records each level's markers in that
    /// level's own frame. Counting only the flush-left `>` runs left every
    /// list opened inside an indented quote invisible: `- > - - x` recorded
    /// the outer item and nothing else, so a definition at the inner item's
    /// content column read as lazy text - while the same body with one
    /// container peeled off (`> - - x`) registered (carve-rs#1096).
    fn observe(&mut self, raw_line: &str, opaque: bool) -> usize {
        let bare = without_blockquote_prefixes(raw_line);
        let was_prev_blank = self.prev_blank;
        self.prev_blank = trim_ascii(bare).is_empty();
        if opaque {
            return self.current().cols.last().copied().unwrap_or(0);
        }
        // A blank line does not open or close a container: a quote's blank
        // continuation is commonly written without the `>`, and treating its
        // absence as leaving the quote would drop the columns of the item the
        // next line still belongs to. It opens nothing either, so it is fed to
        // the innermost frame as it stands rather than walked.
        if trim_ascii(bare).is_empty() {
            let level = self.frames.len() - 1;
            self.observe_segment(level, bare, was_prev_blank);
            return self.current().cols.last().copied().unwrap_or(0);
        }
        let mut rest = raw_line;
        let mut level = 0usize;
        loop {
            while let Some(inner) = strip_blockquote_prefix(rest) {
                rest = inner;
                level += 1;
            }
            while self.frames.len() <= level {
                self.frames.push(ColumnFrame::default());
            }
            self.observe_segment(level, rest, was_prev_blank);
            let Some(offset) = self.next_quote_offset(level, rest) else {
                break;
            };
            rest = &rest[offset..];
        }
        self.frames.truncate(level + 1);
        self.current().cols.last().copied().unwrap_or(0)
    }

    /// Where this level hands off to the NEXT container level, if it does: the
    /// byte offset of a `>` that is this level's own content column rather than
    /// text sitting in it.
    ///
    /// Two spellings reach the same column and both are container prefixes.
    /// `- > x` writes the quote straight after the marker, where the item's
    /// content begins. `  > x` writes it at the column that marker established,
    /// which is the spelling every CONTINUATION line uses - and the one a
    /// flush-left walk can never see.
    fn next_quote_offset(&self, level: usize, line: &str) -> Option<usize> {
        let mut inside = line;
        let mut consumed_marker = false;
        while let Some(marker) = detect_list_marker_full(inside) {
            if marker.content.as_ptr() as usize <= inside.as_ptr() as usize {
                break;
            }
            inside = marker.content;
            consumed_marker = true;
        }
        if consumed_marker {
            // The item's content begins exactly here, so a `>` at this position
            // is written AT the content column.
            return inside
                .starts_with('>')
                .then(|| inside.as_ptr() as usize - line.as_ptr() as usize);
        }
        // EXACTLY a live content column, never past one: an indented `>` that
        // reaches no open item is ordinary text, and the block parser reads it
        // that way too (carve-rs#1082).
        let content_col = self.reached_by_at(level, leading_ws(line));
        let bytes = line.as_bytes();
        (content_col > 0
            && bytes.len() > content_col
            && bytes[..content_col].iter().all(|b| *b == b' ')
            && bytes[content_col] == b'>')
            .then_some(content_col)
    }

    /// Record ONE container level's own markers. `line` is the raw line with
    /// every prefix up to and including this level's quote markers removed, so
    /// its columns are measured inside that container.
    fn observe_segment(&mut self, level: usize, line: &str, was_prev_blank: bool) {
        let indent = leading_ws(line);
        let raw_trimmed = trim_ascii(line);
        let starts_block = is_heading_marker_line(raw_trimmed)
            || raw_trimmed.starts_with('>')
            || detect_fence_open(raw_trimmed).is_some()
            || detect_thematic_break(raw_trimmed);
        if let Some(term_indent) = detect_prepass_def_term(line) {
            // A term opens the list (or re-opens it at a new indent). Its own
            // line has no content column: `:: t` holds the TERM, and only the
            // `:  ` line below it opens a body.
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > term_indent) {
                frame.cols.pop();
            }
            frame.def_list = Some(term_indent);
        } else if let Some(body_indent) =
            detect_prepass_def_body(line).filter(|at| self.frames[level].def_list == Some(*at))
        {
            // `:  ` is three columns wide, so the body's content begins three
            // past the marker - the same arithmetic the collector does when it
            // slices the continuation lines it takes.
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > body_indent) {
                frame.cols.pop();
            }
            frame.cols.push(body_indent + DEF_BODY_MARKER_WIDTH);
        } else if let Some((marker_indent, marker_width)) = detect_prepass_list_marker(line) {
            let cols = &mut self.frames[level].cols;
            while cols.last().is_some_and(|col| *col > marker_indent) {
                cols.pop();
            }
            // One line can open SEVERAL items: `- - a` opens an outer item
            // whose content is another item, so BOTH content columns are
            // live under it (2 and 4). Recording only the outer one left a
            // definition at the INNER column looking like text, so it
            // registered nothing here while carve-js and carve-php read it
            // as that item's block (carve#655).
            let mut offset = marker_width;
            cols.push(offset);
            while let Some((nested_indent, nested_width)) =
                detect_prepass_list_marker(&line[offset..])
            {
                if nested_indent != 0 {
                    break;
                }
                offset += nested_width;
                self.frames[level].cols.push(offset);
            }
        } else if !raw_trimmed.is_empty() && (was_prev_blank || starts_block) {
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > indent) {
                frame.cols.pop();
            }
            // A line that dedents to or past the term's own column has left the
            // definition list, so the next `:  ` line down there opens no body
            // until another term does.
            if frame.def_list.is_some_and(|at| indent <= at) {
                frame.def_list = None;
            }
        }
    }

    fn current(&self) -> &ColumnFrame {
        self.frames
            .last()
            .expect("the document frame is never popped")
    }

    /// The content column of the open item a line at `indent` actually reaches:
    /// the deepest one at or below it, or 0 when it reaches none.
    ///
    /// Not always the innermost. Under `- - a` a definition written at column 2
    /// belongs to the outer item and one at column 4 to the inner one; between
    /// them it reaches neither and folds as text (PART 9 §24 C3).
    ///
    /// Only the INNERMOST container's columns are consulted: a column measured
    /// outside the quote a line sits in is not a column that line can reach.
    fn reached_by(&self, indent: usize) -> usize {
        self.reached_by_at(self.frames.len() - 1, indent)
    }

    /// `reached_by`, asked of a NAMED container level rather than the innermost
    /// one. The composed walk needs it: a `>` written at an outer item's content
    /// column is measured against THAT item's columns, and the innermost frame's
    /// columns are in a different coordinate system entirely.
    fn reached_by_at(&self, level: usize, indent: usize) -> usize {
        self.frames
            .get(level)
            .and_then(|frame| {
                frame
                    .cols
                    .iter()
                    .copied()
                    .filter(|col| *col <= indent)
                    .max()
            })
            .unwrap_or(0)
    }
}

/// A definition line with the enclosing item's content column removed, so a
/// continuation line reads as the definition it is. Exactly a column, never
/// between two: past one the line is item paragraph text and defines nothing.
fn at_content_column<'a>(bare: &'a str, structural: &str, content_col: usize) -> &'a str {
    // A BLOCKQUOTE prefix does not disqualify the strip: columns are measured
    // inside the quote, so `> - a` / `>   [r]: /u` is the same shape as its
    // unquoted twin and the definition is the item's block either way
    // (carve#658). A LIST marker in the structural prefix does disqualify it -
    // there the marker already consumed the column, and stripping again would
    // eat the item's own content.
    let quoted_only = !structural.is_empty()
        && structural
            .chars()
            .all(|c| c == '>' || c == ' ' || c == '\t');
    if content_col > 0 && (structural.is_empty() || quoted_only) {
        bare.strip_prefix(&" ".repeat(content_col)).unwrap_or(bare)
    } else {
        bare
    }
}

/// What `extract_footnote_defs` hands back, in order: the document body with
/// the definition lines removed, each definition's own source, where each
/// definition LINE sits, and the link definitions lifted out of the note bodies.
type FootnoteExtraction = (
    MappedSource,
    BTreeMap<String, MappedSource>,
    BTreeMap<String, Pos>,
    BTreeMap<String, LinkDef>,
);

/// Footnote definitions, and the LINK DEFINITIONS written inside their bodies.
///
/// A note body is lifted out of the document here, before `extract_link_defs`
/// runs - so a `[r]: /u` on a body continuation line was never offered to that
/// pass at all. It stayed in the body and rendered as text, and the reference
/// below it never resolved (carve-rs#599). carve-js, carve-php and the
/// executable spec all collect it.
///
/// Collected here rather than by re-running the link pass over the extracted
/// body: the body's `line_map` is built line by line in this loop, and a pass
/// that removes a line from the middle of a finished body would have to rebuild
/// that mapping from nothing.
fn extract_footnote_defs(
    source: &str,
    first_source_line: usize,
    positions: bool,
    options: &Options<'_>,
) -> FootnoteExtraction {
    let lines: Vec<&str> = source.lines().collect();
    // See `probe_budget_for`: spent by `line_folds_into_an_open_paragraph`, and
    // running out only ever collects a definition this guard would have left as
    // text.
    let mut probe_budget = probe_budget_for(source.len());
    let mut body = Vec::new();
    let mut body_line_map = Vec::new();
    let mut defs = BTreeMap::new();
    // Where each definition was written. Lines and columns are final here; the
    // OFFSETS are filled by the caller, which is the only place that holds the
    // original text the offsets are measured in - the same split `fill_offsets`
    // already uses for every other block.
    let mut def_positions: BTreeMap<String, Pos> = BTreeMap::new();
    let mut note_link_defs: BTreeMap<String, LinkDef> = BTreeMap::new();
    // Fence state for the note-body walk below, declared per definition since a
    // fence cannot span two notes.
    let mut in_fence: Option<FenceOpen> = None;
    // A LINE BLOCK's body is inline content, so a definition-shaped line inside
    // one is text. Without this the line was extracted here and never reached
    // the block parser, so it vanished from the output (#491). carve-js keeps it
    // and does NOT register the footnote; this matches that.
    let mut in_line_block: Option<usize> = None;
    // A `%%%` comment fence is opaque, so a literal `::: |` inside one is not
    // an opener. Entering the state there left it open past the comment's own
    // closer - which is not a colon fence - and every later definition in the
    // document was skipped. Tracked only to gate the opener.
    let mut in_comment_fence: Option<OpenCommentFence> = None;
    let comment_fence_closers = comment_fence_close_index(&lines);
    // Built on the first CONTAINER-scoped opener and never for a document that
    // has none, which is every document that only ever writes `%%%` at column 0.
    let mut container_closers: Option<ContainerCommentClosers> = None;
    let mut comment_closers: Option<HashMap<usize, usize>> = None;
    // See ContentColumns: a definition on an item's CONTINUATION line carries no
    // marker, so without this the line kept its indentation, stopped looking
    // like a definition, and was neither collected nor rendered - the author's
    // line disappeared and a reference to it stayed literal (carve-rs#568).
    let mut columns = ContentColumns::new();
    let mut i = 0;
    while i < lines.len() {
        // A footnote definition is collected at the top level AND from inside a
        // blockquote / bullet-list container: `> [^a]: body` and `- [^a]: body`
        // both stash the def and leave the container empty, matching carve-js
        // (which recognizes the def inside the container's sub-lexer). Strip the
        // container prefix first, then test the bare content (corpus 115).
        // The footnote prepass asks per LINE which column a definition reaches
        // (see `reached_by`), so the innermost column alone is not enough here.
        // Called for its effect on the column stack, not for its return: every
        // question below asks `reached_by` about a specific column instead of
        // taking the innermost one (carve-rs#1054).
        columns.observe(
            lines[i],
            in_fence.is_some() || in_line_block.is_some() || in_comment_fence.is_some(),
        );
        // A quote nested in a list item sits AT the item's content column, so
        // the prefix scan needs that column to see it (carve-rs#588).
        let after_term = i > 0 && opens_definition_entry(lines[i - 1]);
        let stripped = strip_container_prefixes_at(lines[i], &columns, after_term);
        let in_container = !stripped.structural.is_empty();
        // A footnote definition is NEVER collected from inside a fenced code
        // block: a `[^x]: ...` line there is literal content. The prepass has
        // only a prefix-stripped line, not the block parser's container-column
        // context, so it recognizes fences only with no residual indentation.
        // This can collect a def from a container-nested fence body, but avoids
        // opening a fence the block parser never opens and swallowing every
        // later definition in the document.
        let fence_line = stripped.bare;
        if let Some(fence_len) = in_line_block {
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            // The RAW line, not the prefix-stripped one: a literal `- :::` or `> :::`
            // is verse text, and the block parser does not close on it. Stripping
            // here ended the block early and lost every definition-shaped line
            // between there and the real closer.
            if exact_colon_fence_len(lines[i]) == Some(fence_len) {
                in_line_block = None;
            }
            i += 1;
            continue;
        }
        if let Some(open) = in_fence {
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            if is_fence_close(fence_line, open) {
                in_fence = None;
            }
            i += 1;
            continue;
        }
        // Only a TOP-LEVEL, unindented opener. Two things this pre-pass cannot
        // do are both fatal if it guesses:
        //
        // `detect_line_block_open` trims, so it says yes to an indented `  ::: |`
        // where the block parser says no - entering there would swallow every
        // later definition in the document.
        //
        // And a line block opened on a marker line (`- ::: |`) is closed at the
        // item's content column (`  :::`), which this line-based pass cannot
        // recognise, so the state would never end.
        //
        // A nested line block therefore still loses definition-shaped lines,
        // exactly as it does today. That is the same limitation the comment on
        // this function already records, and the same answer: the sound fix is
        // collecting definitions during block parsing.
        if let Some(open) = in_comment_fence {
            // ANY column, matching `comment_fence_close_index` and the block
            // parser. A comment fence closes at whatever indent its closer sits
            // at (PART 9 §24 C3, markup-carve/carve#629), so testing the strict
            // form here made this pass disagree with the one that decides: the
            // block parser closed the comment, this scan did not, and every
            // definition after it went unregistered and came back as visible
            // text (#574 regression). Any column AT THE OPENER'S QUOTE DEPTH -
            // see `closes_open_comment_fence`.
            if closes_open_comment_fence(lines[i], open) {
                in_comment_fence = None;
            }
            // A comment's body is OPAQUE: a `[^a]: note` inside one is comment
            // text and defines nothing. The state was tracked here already, but
            // only to gate the line-block opener below, so the scan still walked
            // in and registered the footnote - producing an endnote nobody wrote
            // and a live reference for a later `see [^a]` (#504).
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            i += 1;
            continue;
        } else if let Some((open, scope)) = detect_comment_fence_opener_scoped(lines[i], &columns) {
            // THE CONTAINER BOUNDS THE SPAN, NOT THE DELIMITER. For a column
            // scope the bound ends the container at the first line that dedents
            // below the column the fence REACHES, so measuring from where the
            // `%%%` happens to be written read a legal body line as the end:
            // `- item` / `    %%%` / `  [r]: /u` / `    %%%` put the fence at 4
            // and its body at 2, still inside the item, and the fence was
            // declined (carve-rs#1054). For a quote scope the blank line is the
            // bound - see `CommentFenceScope`.
            if comment_fence_scope_closes(
                scope,
                &lines,
                i,
                open.fence_len,
                &columns,
                &comment_fence_closers,
                &mut container_closers,
            ) {
                in_comment_fence = Some(OpenCommentFence {
                    fence_len: open.fence_len,
                    quote_depth: scope.quote_depth(),
                });
                body.push(lines[i].to_string());
                body_line_map.push(Some(first_source_line + i));
                i += 1;
                continue;
            }
        }
        if in_comment_fence.is_none() && !in_container && !fence_line.starts_with([' ', '\t']) {
            if let Some(fence_len) = detect_line_block_open(fence_line) {
                in_line_block = Some(fence_len);
                body.push(lines[i].to_string());
                body_line_map.push(Some(first_source_line + i));
                i += 1;
                continue;
            }
        }
        if let Some(open) = detect_fence_open(fence_line) {
            in_fence = Some(open);
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            i += 1;
            continue;
        }
        let def_line = at_content_column(
            stripped.bare,
            stripped.structural,
            columns.reached_by(leading_ws(stripped.bare)),
        );
        if let Some((label, first)) = parse_footnote_def_line(def_line).filter(|_| {
            // A LIST MARKER on a line the block parser folds into an open
            // paragraph is lazy paragraph text, and the definition behind it is
            // part of that text (markup-carve/carve-rs#1024). Cutting it out
            // deleted the author's line AND defined a note nobody wrote.
            //
            // The marker test is what SCOPES the guard, and it reads the RAW
            // line. §10 gives a list the property this turns on - it does not
            // interrupt an open paragraph - and a quote does not have it, so
            // `r` then `> [^f]: t` opens a real quote whose definition IS
            // collected, in all three engines. A quoted marker (`> - [^f]: t`)
            // does not match here either, so the same shape inside a quote is
            // still collected; `line_folds_into_an_open_paragraph` answers that
            // one correctly if it is ever asked, but widening what asks it is a
            // separate change with its own controls.
            //
            // Everything past the marker is the block parser's answer, not this
            // pass's guess. See `line_folds_into_an_open_paragraph`.
            !(detect_list_marker_full(lines[i]).is_some()
                && line_folds_into_an_open_paragraph(&body, lines[i], options, &mut probe_budget))
        }) {
            let def_start_line = first_source_line + i;
            // The definition's OWN extent, before the body is collected.
            //
            // PART 12 §4: a span begins at the markup that opens the construct,
            // which for a definition is the `[` of `[^label]:` and not the
            // container prefix that carried the line. `def_line` is that text,
            // so what precedes it on the raw line is exactly the prefix to skip -
            // measured by taking `def_line` off the end rather than by re-deriving
            // a column, so a tab or a quote marker cannot be counted differently
            // here than it was when the line was stripped.
            let raw_def_line = lines[i];
            if positions {
                if let Some(prefix) = raw_def_line.strip_suffix(def_line) {
                    // First definition for a label wins, matching `defs` below.
                    def_positions
                        .entry(label.to_string())
                        .or_insert_with(|| Pos {
                            // `def_start_line` is already the 1-based source
                            // line - `body_start_line` starts the count at 1 -
                            // and it is the same number `def_line_map` records
                            // for the body's first line.
                            start_line: def_start_line,
                            end_line: def_start_line,
                            start_column: prefix.chars().count() + 1,
                            end_column: raw_def_line.chars().count() + 1,
                            start_offset: 0,
                            end_offset: 0,
                        });
                }
            }
            i += 1;
            let mut def_lines = vec![first.to_string()];
            let mut def_line_map = vec![Some(def_start_line)];
            let mut def_col_map = if positions {
                vec![stripped_col(
                    Some(stripped.structural.chars().count() as isize),
                    stripped.bare,
                    first,
                )]
            } else {
                Vec::new()
            };
            // Multi-line continuation is only gathered for a TOP-LEVEL
            // definition. A container-nested def is single-line here: its
            // continuation would carry the container prefix and is left to
            // normal block parsing, which the spec corpus does not pin.
            //
            let body_indent = footnote_body_floor(lines[def_start_line - first_source_line]);
            let mut note_fence: Option<FenceOpen> = None;
            if !in_container {
                while i < lines.len() {
                    let line = lines[i];
                    if parse_footnote_def_line(line).is_some() {
                        break;
                    }
                    if is_blank_line(line) {
                        // A footnote body extends to following lines indented by
                        // >= 2 spaces (grammar PART 9 §16); single blank lines
                        // are allowed between chunks. A `+` continuation marker
                        // also keeps the body open (PART 9 §17).
                        if i + 1 < lines.len()
                            && (indent_columns(lines[i + 1]) >= body_indent
                                || is_plus_marker(lines[i + 1]))
                        {
                            def_lines.push(String::new());
                            def_line_map.push(Some(first_source_line + i));
                            if positions {
                                def_col_map.push(stripped_col(Some(0), lines[i], ""));
                            }
                            i += 1;
                            continue;
                        }
                        break;
                    }
                    // Form B: a lone `+` attaches the following flush-left block
                    // to the note with no indentation (the same continuation
                    // marker lists, block quotes and definition bodies use); the
                    // attached block ends at a blank line, another `+`, or the
                    // next footnote definition.
                    if is_plus_marker(line) {
                        i += 1;
                        let mut attached: Vec<String> = Vec::new();
                        let attached_start = i;
                        let end =
                            attached_block_end(&lines, i, &mut comment_closers, &mut |a, _| {
                                is_blank_line(a)
                                    || is_plus_marker(a)
                                    || parse_footnote_def_line(a).is_some()
                            });
                        while i < end {
                            let a = lines[i];
                            attached.push(a.to_string());
                            i += 1;
                        }
                        if !attached.is_empty() {
                            def_lines.push(String::new());
                            def_line_map.push(None);
                            if positions {
                                def_col_map.push(None);
                            }
                            let attached_len = attached.len();
                            def_lines.extend(attached);
                            def_line_map.extend(
                                (attached_start..attached_start + attached_len)
                                    .map(|line_idx| Some(first_source_line + line_idx)),
                            );
                            if positions {
                                def_col_map.extend((0..attached_len).map(|_| Some(0)));
                            }
                        }
                        continue;
                    }
                    // COLUMNS, not a byte count. §16 asks for >= 2 columns and
                    // §24 C1 gives a tab a column value, so a bare tab reaches
                    // the body's column exactly as two spaces do. Counting
                    // bytes made this half disagree with the dedent below, which
                    // has always used `strip_leading_columns`: a bare tab
                    // counted as one and was refused (carve#796).
                    if indent_columns(line) >= body_indent {
                        // Strip EXACTLY the body's own indent, not all leading
                        // whitespace. A full trim flattened everything the
                        // relative indentation says: a nested list marker
                        // landed at its parent's column and the sublist became
                        // siblings, and a code block's interior indentation -
                        // which is content, not layout - was eaten with it
                        // (#611). Every other container strips its own prefix
                        // and leaves the rest, which is why the same two lines
                        // nest in a quote, a div and a list item.
                        let dedented = strip_leading_columns(line, body_indent);
                        let trimmed: &str = &dedented;
                        // A CODE FENCE inside the body is opaque, so a
                        // definition-shaped line in it is content. Every engine
                        // already agrees about that at the top level - none
                        // registers `[r]: /u` written inside ``` - and the
                        // top-level pass in `extract_link_defs` tracks fences
                        // for exactly this reason. Without the state here the
                        // line was consumed as a definition and vanished from
                        // the code block.
                        if let Some(open) = note_fence {
                            if is_fence_close(trimmed, open) {
                                note_fence = None;
                            }
                        } else if let Some(open) = detect_fence_open(trimmed) {
                            note_fence = Some(open);
                        } else if let Some((label_part, target_part)) = parse_link_def_line(trimmed)
                        {
                            // A LINK DEFINITION inside the body is
                            // document-level metadata like any other, so it is
                            // collected and the line renders nothing - the same
                            // answer §16 already gives a definition inside a
                            // list item or a quote.
                            //
                            // LAST one wins among note bodies, which is what
                            // `extract_link_defs` does for the document; a
                            // top-level definition still beats both, and that
                            // precedence is applied by the caller.
                            if !label_part.starts_with('@') && !target_part.trim().is_empty() {
                                let mut def = parse_link_def_target_with_attrs(target_part.trim());
                                // The DOCUMENT line, so the hoisted node gets a
                                // `pos` like every other definition (§4, §10).
                                // `first_source_line` is 1-BASED - it is built as
                                // a newline count plus one - while `LinkDef.line`
                                // is the 0-based index `extract_link_defs`
                                // records, so the conversion is explicit here
                                // rather than off by one (carve-rs#636).
                                def.line = Some(first_source_line + i - 1);
                                note_link_defs.insert(label_part.to_string(), def);
                                i += 1;
                                continue;
                            }
                        }
                        def_lines.push(trimmed.to_string());
                        def_line_map.push(Some(first_source_line + i));
                        if positions {
                            def_col_map.push(stripped_col(Some(0), line, trimmed));
                        }
                        i += 1;
                        continue;
                    }
                    break;
                }
            }
            // ONLY FOR THE DEFINITION THIS POSITION DESCRIBES. The first
            // definition for a label wins, both here and in `defs` below, so a
            // later duplicate must not move the accepted one's end: this wrote
            // the DUPLICATE's following line onto the FIRST definition's span,
            // and the container that hosted the first one was then reported as
            // running to it (carve-rs#1106). `defs` holds the label exactly
            // when an earlier definition was accepted, which is the test.
            let first_for_label = !defs.contains_key(label);
            if positions
                && !in_container
                && first_for_label
                && i < lines.len()
                && is_blank_line(lines[i])
            {
                if let Some(pos) = def_positions.get_mut(label) {
                    pos.end_line = first_source_line + i;
                    pos.end_column = 1;
                }
            }
            // First definition for a label wins (later duplicates are ignored).
            defs.entry(label.to_string())
                .or_insert_with(|| MappedSource {
                    col_map: def_col_map,
                    source: def_lines.join("\n"),
                    line_map: def_line_map,
                });
            // Leave the container's structural prefix (or a blank line at top
            // level) where the invisible definition was, so the container still
            // renders and the line still acts as a block boundary -- a following
            // paragraph or a lazy blockquote continuation does not absorb across
            // it.
            // A definition matched through the CONTENT-COLUMN strip leaves the
            // container prefix behind, and inside a quoted list item that
            // prefix alone is a BLANK line - which loosens the list (§17 L1).
            // The definition rendered nothing, so it is not the item's second
            // block and must not loosen it (§17 L2), exactly as the
            // marker-consuming case above keeps the item non-empty. `%%` is
            // invisible at any column and closes nothing (§24 C3).
            let mut replacement = stripped.replacement();
            // NOT gated on a container prefix. The hazard is the blank the
            // removal leaves, and a definition at an item's content column with
            // no marker or quote in front of it - `- a` / `  [^f]: x` / `  more`
            // - leaves exactly the same blank. That read as an interior
            // separator and loosened the item, so corpus 228 rendered
            // `<p>a</p>` and `<p>more</p>` where the other two engines render
            // both bare (carve#801, the `list.tight` divergence).
            //
            // Top-level needs the marker too: a plain blank is caption_slot's
            // optional blank line, so replacing a definition with one allowed a
            // caption to attach THROUGH the definition (carve#1028). The marker
            // is invisible but non-blank, preserving the interruption.
            if !replacement.ends_with("%%") && !replacement.ends_with(DEFINITION_PLACEHOLDER) {
                // AND IT HAS TO STAND AT THE DEFINITION'S OWN COLUMN. Inside a
                // container the structural prefix already carries it; at top
                // level that prefix is empty, so an unindented placeholder
                // lands at column 0 and CLOSES the item it exists to keep
                // open - the line after it leaves the list entirely.
                let document_column = replacement.is_empty() && leading_ws(stripped.bare) == 0;
                replacement.push_str(&stripped.bare[..leading_ws(stripped.bare)]);
                replacement.push_str(if document_column {
                    DOCUMENT_DEFINITION_PLACEHOLDER
                } else {
                    DEFINITION_PLACEHOLDER
                });
            }
            replacement.push_str(
                &" ".repeat(
                    raw_def_line
                        .chars()
                        .count()
                        .saturating_sub(replacement.chars().count()),
                ),
            );
            body.push(replacement);
            body_line_map.push(Some(def_start_line));
        } else {
            body.push(lines[i].to_string());
            body_line_map.push(Some(first_source_line + i));
            i += 1;
        }
    }
    (
        MappedSource {
            // The document body's lines are top-level: the footnote-definition
            // extraction removes whole lines, never a prefix, so nothing has
            // been stripped from the front of the ones that remain.
            col_map: if positions {
                vec![Some(0); body.len()]
            } else {
                Vec::new()
            },
            source: joined_source(&body),
            line_map: body_line_map,
        },
        defs,
        def_positions,
        note_link_defs,
    )
}

fn parse_footnote_def_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("[^")?;
    let (label, body) = rest.split_once("]: ")?;
    // The same two validations `parse_link_def_line` below carries, for the
    // same reason: a line that is NOT a definition must stay on the page.
    // Consuming it as one renders nothing, so the line disappears from the
    // document entirely.
    //
    // A `]` inside the label - `[^a]b]: x`, `[^]]: x` - is excluded by the
    // reference-label production at every position, so the line is a paragraph.
    // This is the case a first fix on the link-def side missed (carve-rs#456);
    // this function got neither fix.
    // `footnote_label = {character - ']'}+` - one or more. `[^]: x` has an EMPTY
    // label, so it is not a footnote definition. It is a valid LINK reference
    // definition whose label is `^`, and claiming it here kept the link path
    // from ever seeing it - `[text][^]` then never resolved (carve#552).
    if label.is_empty() || label.contains(']') {
        return None;
    }
    // No content after the separator is not a definition either. `[^a]: ` was
    // consumed with an empty body and vanished; carve-js and carve-php both
    // keep it as `<p>[^a]:</p>`.
    let body = trim_ascii_start(body);
    if body.is_empty() {
        return None;
    }
    Some((label, body))
}

#[derive(Clone)]
struct LinkDef {
    href: String,
    title: Option<String>,
    /// Zero-based index of the line the definition was written on. Kept so PART
    /// 12 §10's node can carry a `pos`, and so the hoisted definitions come out
    /// in SOURCE order rather than the label order of the map (carve-rs#631).
    /// `None` for a definition that did not come from a source line.
    line: Option<usize>,
    /// A TRAILING attribute block on the definition line. PART 9R's symbol
    /// table is `label -> (url, title?, attrs?)`, and R1 transfers these to
    /// every link that resolves the label (carve#604).
    attrs: Option<Attrs>,
}

/// How many BYTES the definition pre-pass may hand to the block parser to
/// answer [`line_folds_into_an_open_paragraph`], for one document.
///
/// The question is answered by parsing, and what has to be parsed is the run of
/// lines back to the last blank one - so a document that is one long blank-free
/// run of definition-shaped marker lines asks it once per line over a run that
/// grows by one each time, which is quadratic. The budget makes the pre-pass
/// linear again by capping the total, and running out is SAFE in the direction
/// that matters: an unanswered question is answered `false`, which collects the
/// definition exactly as the engine did before this guard existed. A document
/// has to be built to reach it - the question is only asked for a line that is
/// both a list marker and a footnote definition behind it.
///
/// BYTES RATHER THAN LINES, because a probe costs what it PARSES and a line is
/// not a unit of that. Counting lines prices one long line and one short one
/// the same, so a run holding a single huge line under many short
/// definition-shaped ones buys as many full re-parses of that line as it has
/// candidates - the amplification the cap exists to bound, priced at zero.
fn probe_budget_for(source_bytes: usize) -> usize {
    source_bytes.saturating_mul(4).saturating_add(4096)
}

/// One level of a probe's LAST-CHILD CHAIN: what kind of node the level ends
/// with, and - carried alongside in [`OpenFrame`] - how many nodes it holds.
///
/// A list's ITEMS and a definition list's are levels of their own rather than
/// blocks, and they have to be, because the count that separates a sibling item
/// from a lazy continuation lives exactly there: `- a` / `  lazy` and the same
/// with `- [^f]: t` under it hold the identical blocks at every OTHER level and
/// differ only in how many items the list has.
#[derive(PartialEq, Eq)]
enum ProbeLevel {
    Blocks(std::mem::Discriminant<BlockNode>),
    ListItems,
    DefinitionItems,
}

/// The chain of still-open levels a parse ends in, from the document level down
/// to the innermost last node.
struct OpenFrame {
    levels: Vec<(ProbeLevel, usize)>,
    /// Whether the innermost node the chain reached is a paragraph. Read from
    /// the chain rather than re-derived, so the two always describe one walk.
    ends_in_paragraph: bool,
}

/// The child level a block holds, for the last-child walk [`open_frame`] makes.
///
/// EXHAUSTIVE ON PURPOSE, with no `_` arm: a block kind added later is a
/// compile error here rather than a silent answer. And the answer a `Leaf` gives
/// is the safe one anyway - a level the walk cannot enter ends the chain on a
/// node that is not a paragraph, so the caller declines to suppress.
enum ProbeChildren<'a> {
    Blocks(&'a [BlockNode]),
    ListItems(&'a [ListItem]),
    DefinitionItems(&'a [DefinitionItem]),
    Leaf,
}

fn probe_children(block: &BlockNode) -> ProbeChildren<'_> {
    match block {
        BlockNode::BlockQuote(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::Div(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::Admonition(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::FigureGroup(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::LineBlock(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::Extension(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::List(b) => ProbeChildren::ListItems(&b.items),
        BlockNode::DefinitionList(b) => ProbeChildren::DefinitionItems(&b.items),
        BlockNode::Heading(_)
        | BlockNode::Paragraph(_)
        | BlockNode::CodeBlock(_)
        | BlockNode::Table(_)
        | BlockNode::Figure(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::LinkReferenceDefinition(_)
        | BlockNode::CitationDefinition(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => ProbeChildren::Leaf,
    }
}

fn open_frame(blocks: &[BlockNode]) -> OpenFrame {
    let paragraph = std::mem::discriminant(&BlockNode::Paragraph(Paragraph::default()));
    let mut frame = OpenFrame {
        levels: Vec::new(),
        ends_in_paragraph: false,
    };
    let mut blocks = blocks;
    loop {
        let Some(last) = blocks.last() else {
            return frame;
        };
        let kind = std::mem::discriminant(last);
        frame.ends_in_paragraph = kind == paragraph;
        frame.levels.push((ProbeLevel::Blocks(kind), blocks.len()));
        match probe_children(last) {
            ProbeChildren::Blocks(children) => blocks = children,
            ProbeChildren::ListItems(items) => {
                frame.ends_in_paragraph = false;
                let Some(item) = items.last() else {
                    return frame;
                };
                frame.levels.push((ProbeLevel::ListItems, items.len()));
                blocks = &item.children;
            }
            ProbeChildren::DefinitionItems(items) => {
                frame.ends_in_paragraph = false;
                let Some(item) = items.last() else {
                    return frame;
                };
                frame
                    .levels
                    .push((ProbeLevel::DefinitionItems, items.len()));
                let Some(def) = item.definitions.last() else {
                    return frame;
                };
                blocks = &def.children;
            }
            ProbeChildren::Leaf => return frame,
        }
    }
}

/// Does the BLOCK PARSER fold `line` into a paragraph that was already open?
///
/// This is the question the footnote pre-pass has to answer before it may cut a
/// definition out of a line, and the pre-pass has no business answering it
/// itself. §10 says a list does not interrupt an open paragraph, so `r` then
/// `. [^f]: t` is one paragraph holding both lines - and a pre-pass that
/// collects the definition out of the second line deletes text the block parser
/// keeps and defines a note nobody wrote (markup-carve/carve-rs#1024).
///
/// THE ANSWER IS ASKED OF THE PARSER, NOT ENUMERATED. The predicate this
/// replaced listed the openers a line can be and called every line it did not
/// recognise ordinary paragraph text - so every opener nobody thought of
/// answered "a paragraph is open", and that answer SUPPRESSES a collection the
/// engine used to make. Four such openers were found and closed; one review
/// pass then found three more of the same class, and a custom `match_block`
/// extension is a fourth that cannot be listed at all, because the pre-pass does
/// not know the extension's syntax. The list is unbounded by construction, so
/// there is no version of it that is finished.
///
/// So the run is handed to the block parser TWICE, once without `line` and once
/// with it, and the two open frames are compared. A line that folds into an open
/// paragraph adds NO node anywhere: every level holds what it held, and the
/// innermost one is still a paragraph. A line that opens anything - a sibling
/// item, a nested list, a quote, a container, a block an extension defines -
/// changes a count somewhere along that chain, and the frames differ. The parser
/// answers for its own extensions for free, which is the case no list could have
/// covered.
///
/// FAILING SAFE IS THE DEFAULT AND NOT A CLAIM. Every early return here is
/// `false`, and `false` declines to suppress: an empty run, an exhausted budget,
/// a chain the walk cannot enter, a probe that ends in anything but a paragraph.
/// The worst an unrecognised shape can do is collect the definition the way the
/// engine did before the guard existed, which is a defect that reports itself.
/// It can never make the pre-pass delete an author's line unasked.
///
/// THE RUN IS BOUNDED BY THE LAST BLANK LINE, which is sound and not an
/// approximation: a blank line closes every paragraph in every container, so a
/// paragraph still open at the end of `body` began after the last blank one.
/// Losing the container frame ABOVE that blank can only cost a suppression -
/// an indented run read at the document level parses its marker lines as list
/// items rather than as lazy text - which is the safe direction again.
///
/// The run is taken from `body`, the document THIS PASS has extracted so far,
/// not from the raw source: a definition already collected is gone from the
/// parser's input, and a probe that still saw it would read the paragraph it is
/// not. Link definitions are the other half of the same point - they are
/// stripped downstream of here, so the probe strips them too, and `[a]: /u` is
/// the line it leaves rather than the paragraph a raw parse would find.
fn line_folds_into_an_open_paragraph(
    body: &[String],
    line: &str,
    options: &Options<'_>,
    budget: &mut usize,
) -> bool {
    let run_start = body
        .iter()
        .rposition(|written| is_blank_line(written))
        .map_or(0, |blank| blank + 1);
    let run = &body[run_start..];
    if run.is_empty() {
        return false;
    }
    // What the two probes will PARSE, which is the run twice over plus the
    // candidate line - so the price is the work rather than the question.
    let cost = run
        .iter()
        .map(|written| written.len().saturating_add(1))
        .fold(0usize, |total, len| total.saturating_add(len))
        .saturating_mul(2)
        .saturating_add(line.len());
    if *budget < cost {
        return false;
    }
    *budget -= cost;

    let before = run.join("\n");
    let mut after = String::with_capacity(before.len() + line.len() + 1);
    after.push_str(&before);
    after.push('\n');
    after.push_str(line);

    let before = open_frame(&probe_blocks_at_document_level(
        &extract_link_defs(&before).0,
        options,
    ));
    let after = open_frame(&probe_blocks_at_document_level(
        &extract_link_defs(&after).0,
        options,
    ));
    after.ends_in_paragraph && before.levels == after.levels
}

fn extract_link_defs(source: &str) -> (String, BTreeMap<String, LinkDef>) {
    let mut body: Vec<String> = Vec::new();
    let mut defs = BTreeMap::new();
    let mut in_fence: Option<FenceOpen> = None;
    // A LINE BLOCK's body is inline content (`line_block_line = {whitespace},
    // inline_content, newline`), so a definition-shaped line inside one is text,
    // not a definition. Without this the line was extracted here and never
    // reached the block parser, so it vanished from the output entirely -
    // carve-js and carve-php both render it (#491).
    //
    // Tracked the same way the code fence above is, and for the same reason: this
    // pre-pass is line-based, so the only thing it can do is refuse to look inside
    // a region whose contents are not blocks.
    let mut in_line_block: Option<usize> = None;
    // A `%%%` comment fence is opaque, so a literal `::: |` inside one is not
    // an opener. Entering the state there left it open past the comment's own
    // closer - which is not a colon fence - and every later definition in the
    // document was skipped. Tracked only to gate the opener.
    let mut in_comment_fence: Option<OpenCommentFence> = None;
    // Track enclosing list item content columns so the strict fence test can be
    // re-based to the item's content column. This remains a line-based
    // approximation: tab-vs-space marker alignment is char-counted, the
    // post-blank baseIndent+2 continuation rule is not modeled, and lists
    // nested inside blockquotes are not fully modeled. Those residual cases can
    // still produce a spurious link, not content loss; the sound fix is
    // collecting definitions during block parsing.
    let mut columns = ContentColumns::new();
    // Collected so an unterminated `%%%` can be told from a real fenced comment
    // before the state is entered - see comment_fence_closes.
    let all_lines: Vec<&str> = source.lines().collect();
    let comment_closers = comment_fence_close_index(&all_lines);
    // See the note in `extract_footnote_defs`: lazy, so a document with no
    // container-scoped comment fence pays nothing for it.
    let mut container_closers: Option<ContainerCommentClosers> = None;
    // Suffix maxima make the "could this opener close later?" rejection O(1).
    // The exact scan below is then needed only when a compatible closer really
    // exists; once found, the fence state makes all intervening opener-shaped
    // lines opaque. Without this index, N unterminated marker lines each scanned
    // the remaining N lines (perf_regressions).
    let mut backtick_closer_max = vec![0usize; all_lines.len() + 1];
    let mut tilde_closer_max = vec![0usize; all_lines.len() + 1];
    for index in (0..all_lines.len()).rev() {
        backtick_closer_max[index] = backtick_closer_max[index + 1];
        tilde_closer_max[index] = tilde_closer_max[index + 1];
        let mut candidate = trim_ascii_start(all_lines[index]);
        while let Some(rest) = strip_blockquote_prefix(candidate) {
            candidate = trim_ascii_start(rest);
        }
        let Some(&fence_char) = candidate.as_bytes().first() else {
            continue;
        };
        if fence_char != b'`' && fence_char != b'~' {
            continue;
        }
        let run = candidate
            .as_bytes()
            .iter()
            .take_while(|byte| **byte == fence_char)
            .count();
        if run < 3
            || !candidate[run..]
                .bytes()
                .all(|byte| byte == b' ' || byte == b'\t')
        {
            continue;
        }
        if fence_char == b'`' {
            backtick_closer_max[index] = backtick_closer_max[index].max(run);
        } else {
            tilde_closer_max[index] = tilde_closer_max[index].max(run);
        }
    }
    for (line_index, line) in all_lines.iter().copied().enumerate() {
        // Verse text is opaque: a line-block body line like `- verse` is not a
        // list marker, and letting it push a content column left the NEXT
        // top-level opener unprotected. A COMMENT body is opaque the same way,
        // and was missing here where `extract_footnote_defs` already had it: a
        // `- hidden` inside a comment seeded a content column that outlived the
        // fence, so `  [r]: /u` after it was stripped to that phantom column and
        // registered, where the block parser sees a top-level line two columns
        // in and reads it as text.
        let content_col = columns.observe(
            line,
            in_fence.is_some() || in_line_block.is_some() || in_comment_fence.is_some(),
        );
        // A quote nested in a list item sits AT the item's content column, so
        // the prefix scan needs that column to see it (carve-rs#588).
        let after_term = line_index > 0 && opens_definition_entry(all_lines[line_index - 1]);
        let stripped = strip_container_prefixes_at(line, &columns, after_term);
        // A marker after an already-open paragraph is lazy paragraph text, not
        // a new container from which this line-oriented pre-pass may collect a
        // definition (`r\n. [f]: t`).
        let marker_is_lazy_text = line_index > 0
            && !is_blank_line(all_lines[line_index - 1])
            && detect_list_marker_full(line).is_some();
        let raw_is_quoted = prepass_line_is_quoted(line);
        if let Some(fence_len) = in_line_block {
            // The line is KEPT whatever it looks like - that is the whole point.
            // A definition-shaped line inside a line block is inline content, so
            // it renders; blanking it here is what made it disappear (#491).
            //
            // It is not REGISTERED either. That was left in deliberately while
            // carve#557 was open - the three engines all registered here, and
            // this change was only about the line surviving. carve#574 answered
            // it: nothing inside verse is claimed, so a definition-shaped line
            // renders and defines nothing.
            body.push(line.to_string());
            // The RAW line - see the note in extract_footnote_defs.
            if exact_colon_fence_len(line) == Some(fence_len) {
                in_line_block = None;
            }
            continue;
        }
        if let Some(open) = in_fence {
            body.push(line.to_string());
            // CLOSER: strip a blockquote prefix only when the fence was opened
            // quoted, and NEVER a list marker. A fence closer is a continuation
            // line of pure indentation, so a literal marker line inside a
            // document-level code sample stays content.
            let close_kept = if open.quoted {
                strip_prepass_blockquote_prefix(line).unwrap_or(line)
            } else {
                line
            };
            let close_indent = leading_ws(close_kept);
            let close_line = if close_indent >= open.content_col {
                &close_kept[open.content_col..]
            } else {
                close_kept
            };
            if is_fence_close(close_line, open) {
                in_fence = None;
            }
            continue;
        }
        // OPENER: strip container prefixes (blockquote AND list marker), then
        // re-base to the current list-item content column. This recognizes a
        // fence on the marker line (`- ````) and on continuation lines.
        let opener_kept;
        let fence_line = if content_col == 0 {
            stripped.bare
        } else {
            opener_kept = strip_container_prefixes_keep_indent(line);
            let kept_indent = leading_ws(&opener_kept);
            if kept_indent >= content_col {
                &opener_kept[content_col..]
            } else {
                opener_kept.as_str()
            }
        };
        // Top-level and unindented only - see the note in extract_footnote_defs
        // for why a nested opener is refused rather than guessed at.
        if let Some(open) = in_comment_fence {
            // ANY column, at the opener's quote depth - see the note in
            // `extract_footnote_defs`.
            if closes_open_comment_fence(line, open) {
                in_comment_fence = None;
            }
            // A comment's body is OPAQUE, so a definition-shaped line inside
            // one is comment text and registers nothing. The state was already
            // tracked here, but only to gate the line-block opener below - the
            // scan still walked into the body and registered from it, so
            // `%%%` / `[^a]: note` / `%%%` produced an endnote nobody wrote and
            // a live reference for `see [^a]` after it (#504).
            //
            // carve-js has never registered from inside a comment and carve-php
            // stopped (carve-php#698); this brings the third engine into line.
            body.push(line.to_string());
            continue;
        } else if let Some((open, scope)) = detect_comment_fence_opener_scoped(line, &columns) {
            // The same scopes as the footnote prepass above: the fence is gated
            // by the column it REACHES, and its span is bounded by the container
            // holding it rather than by the delimiter's own column.
            if comment_fence_scope_closes(
                scope,
                &all_lines,
                line_index,
                open.fence_len,
                &columns,
                &comment_closers,
                &mut container_closers,
            ) {
                in_comment_fence = Some(OpenCommentFence {
                    fence_len: open.fence_len,
                    quote_depth: scope.quote_depth(),
                });
                body.push(line.to_string());
                continue;
            }
        }
        if in_comment_fence.is_none()
            && stripped.structural.is_empty()
            && content_col == 0
            && !fence_line.starts_with([' ', '\t'])
        {
            if let Some(fence_len) = detect_line_block_open(fence_line) {
                in_line_block = Some(fence_len);
                body.push(line.to_string());
                continue;
            }
        }
        if let Some(mut open) = detect_fence_open(fence_line) {
            open.content_col = content_col;
            open.quoted = raw_is_quoted;
            let follows_open_paragraph =
                line_index > 0 && !is_blank_line(all_lines[line_index - 1]);
            let suffix_max = if open.fence_char == b'`' {
                backtick_closer_max[line_index + 1]
            } else {
                tilde_closer_max[line_index + 1]
            };
            let closes_ahead = follows_open_paragraph
                && suffix_max >= open.fence_len
                && all_lines[line_index + 1..].iter().any(|candidate| {
                    let kept = if open.quoted {
                        strip_prepass_blockquote_prefix(candidate).unwrap_or(candidate)
                    } else {
                        candidate
                    };
                    let kept = strip_container_prefixes_keep_indent(kept);
                    let indent = leading_ws(&kept);
                    let candidate = if indent >= open.content_col {
                        &kept[open.content_col..]
                    } else {
                        kept.as_str()
                    };
                    is_fence_close(candidate, open)
                });
            // An unterminated fence cannot interrupt an open paragraph. Do not
            // make the pre-pass opaque in that case: later definitions still
            // need collecting (`:\n```\n[A]: b`).
            if !follows_open_paragraph || closes_ahead {
                in_fence = Some(open);
            }
            body.push(line.to_string());
            continue;
        }
        // A definition on an item's CONTINUATION line carries no marker, so
        // `strip_container_prefixes` (which strips blockquote markers and list
        // MARKERS) leaves the item's indentation in front of the `[` and the
        // line reads as text. carve-js, carve-php and the executable spec all
        // collect it; this engine rendered the line AND failed to resolve the
        // reference (carve-rs#552).
        //
        // Exactly the tracked content column is stripped, never more: a line
        // indented PAST the column is item paragraph text, and all four
        // implementations agree it defines nothing there. Zero columns is the
        // top-level case, where `text` / `  [r]: /u` is likewise text
        // everywhere - so this only ever fires inside a list item.
        let def_line = if marker_is_lazy_text {
            line
        } else {
            at_content_column(
                stripped.bare,
                stripped.structural,
                columns.reached_by(leading_ws(stripped.bare)),
            )
        };
        if let Some((label_part, target_part)) = parse_link_def_line(def_line) {
            // A reference definition needs a non-empty destination (carve-js
            // `RE_LINK_DEF` requires `(\S+)` after the colon). An empty target
            // (`[r]:` + only whitespace) is NOT a definition -- the line stays
            // literal text. (corpus 34-reference-link-9)
            if label_part.starts_with('@') || target_part.trim().is_empty() {
                body.push(line.to_string());
                continue;
            }
            let mut def = parse_link_def_target_with_attrs(target_part.trim());
            // The line the author wrote it on, for PART 12 §10's node: its `pos`
            // and the SOURCE order of the hoisted definitions both come from here.
            def.line = Some(line_index);
            defs.insert(label_part.to_string(), def);
            // Leave a blank line in place of the (invisible) definition so it
            // still acts as a block boundary (matches carve-js, where a
            // definition interrupts a paragraph / ends a lazy blockquote).
            // A definition matched through the CONTENT-COLUMN strip leaves the
            // container prefix behind, and inside a quoted list item that
            // prefix alone is a BLANK line - which loosens the list (§17 L1).
            // The definition rendered nothing, so it is not the item's second
            // block and must not loosen it (§17 L2), exactly as the
            // marker-consuming case above keeps the item non-empty. `%%` is
            // invisible at any column and closes nothing (§24 C3).
            let mut replacement = stripped.replacement();
            // NOT gated on a container prefix. The hazard is the blank the
            // removal leaves, and a definition at an item's content column with
            // no marker or quote in front of it - `- a` / `  [^f]: x` / `  more`
            // - leaves exactly the same blank. That read as an interior
            // separator and loosened the item, so corpus 228 rendered
            // `<p>a</p>` and `<p>more</p>` where the other two engines render
            // both bare (carve#801, the `list.tight` divergence).
            //
            // Top-level needs the marker too: a plain blank is caption_slot's
            // optional blank line, so replacing a definition with one allowed a
            // caption to attach THROUGH the definition (carve#1028).
            if !replacement.ends_with("%%") && !replacement.ends_with(DEFINITION_PLACEHOLDER) {
                // AND IT HAS TO STAND AT THE DEFINITION'S OWN COLUMN. Inside a
                // container the structural prefix already carries it; at top
                // level that prefix is empty, so an unindented placeholder
                // lands at column 0 and CLOSES the item it exists to keep
                // open - the line after it leaves the list entirely.
                let document_column = replacement.is_empty() && leading_ws(stripped.bare) == 0;
                replacement.push_str(&stripped.bare[..leading_ws(stripped.bare)]);
                replacement.push_str(if document_column {
                    DOCUMENT_DEFINITION_PLACEHOLDER
                } else {
                    DEFINITION_PLACEHOLDER
                });
            }
            replacement.push_str(
                &" ".repeat(
                    line.chars()
                        .count()
                        .saturating_sub(replacement.chars().count()),
                ),
            );
            body.push(replacement);
        } else {
            body.push(line.to_string());
        }
    }
    (joined_source(&body), defs)
}

fn prepass_line_is_quoted(line: &str) -> bool {
    strip_prepass_blockquote_prefix(line).is_some()
        || detect_list_marker_full(line)
            .is_some_and(|marker| strip_prepass_blockquote_prefix(marker.content).is_some())
}

fn strip_prepass_blockquote_prefix(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if bytes.get(i) != Some(&b'>') {
        return None;
    }
    i += 1;
    match bytes.get(i) {
        None => {}
        Some(b' ') => i += 1,
        _ => return None,
    }
    Some(&line[i..])
}

/// The quote a line opens: how many markers deep, the COLUMN the first of them
/// starts at, and what is left after them.
///
/// Depth rather than a boolean because stripping the markers collapses
/// `> > %%%` and `> %%%` onto the same run, and they belong to different quotes.
/// `prepass_line_is_quoted` only ever needed to know THAT a line was quoted; the
/// comment-fence scope needs to know how deep, so a closer can be matched
/// against the quote its opener was written in.
///
/// A LIST MARKER comes off first, the way `prepass_line_is_quoted` already reads
/// one and `detect_comment_fence_opener_at_any_column` already walks one: a
/// quote may open on an item's marker line, so `- > %%%` is quoted at depth 1
/// and its body continues at `  > `. Reading only the leading markers left that
/// one spelling of the same fence unrecognized.
///
/// The column is what keeps the two prefixes from being confused for each other.
/// A `> %%%` written back at column 0 is not the closer of a `- > %%%`, because
/// it has left the item; the depth alone cannot tell them apart, and the column
/// is what the dedent half of the bound measures against. For a quote at column
/// 0 nothing can dedent below it, so that half never fires and the blank line is
/// the whole bound, exactly as it is for the plain spelling.
fn prepass_quote_scope(line: &str) -> (usize, usize, &str) {
    let mut rest = line;
    let mut col = 0usize;
    loop {
        let trimmed = trim_ascii_start(rest);
        col = advance_columns(&rest[..rest.len() - trimmed.len()], col);
        rest = trimmed;
        let Some(marker) = detect_list_marker_full(rest) else {
            break;
        };
        let consumed = marker.content.as_ptr() as usize - rest.as_ptr() as usize;
        if consumed == 0 {
            break;
        }
        col = advance_columns(&rest[..consumed], col);
        rest = marker.content;
    }
    let quote_col = col;
    let mut depth = 0;
    while let Some(inner) = strip_prepass_blockquote_prefix(rest) {
        depth += 1;
        rest = inner;
    }
    (depth, quote_col, rest)
}

/// `:  ` - the definition-body marker, three columns wide.
const DEF_BODY_MARKER_WIDTH: usize = 3;

/// The indent of a `:: ` TERM line, which opens a definition list.
fn detect_prepass_def_term(line: &str) -> Option<usize> {
    let indent = leading_ws(line);
    is_definition_list_start(&line[indent.min(line.len())..]).then_some(indent)
}

/// The indent of a `:  ` BODY line, which opens a definition body.
///
/// The separator is the marker's own, so a line that is only the marker opens
/// nothing: `parse_definition_list` reads `:  ` with `strip_prefix`, and an
/// empty remainder is the placeholder form rather than a body with content.
fn detect_prepass_def_body(line: &str) -> Option<usize> {
    let indent = leading_ws(line);
    line[indent.min(line.len())..]
        .strip_prefix(":  ")
        .filter(|body| !is_blank_line(body))
        .map(|_| indent)
}

fn detect_prepass_list_marker(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let marker_indent = i;
    if i >= bytes.len() {
        return None;
    }
    match bytes[i] {
        b'-' | b'*' => i += 1,
        // The bare dot is a decimal ordered marker whose authored width is one
        // column, exactly like a bullet. Leaving it out of the definition
        // prepass made definitions at its content column render as literal
        // item text even though the block parser recognized the list.
        b'.' => i += 1,
        b'0'..=b'9' => {
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if !matches!(bytes.get(i), Some(b'.' | b')')) {
                return None;
            }
            i += 1;
        }
        b'a'..=b'z' | b'A'..=b'Z' => {
            let marker_start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let marker = &line[marker_start..i];
            let ordered = marker.len() == 1
                || marker.bytes().all(|b| {
                    matches!(
                        b,
                        b'i' | b'v'
                            | b'x'
                            | b'l'
                            | b'c'
                            | b'd'
                            | b'm'
                            | b'I'
                            | b'V'
                            | b'X'
                            | b'L'
                            | b'C'
                            | b'D'
                            | b'M'
                    )
                });
            if !ordered || !matches!(bytes.get(i), Some(b'.' | b')')) {
                return None;
            }
            i += 1;
        }
        _ => return None,
    }
    if bytes.get(i) == Some(&b'{') {
        while i < bytes.len() && bytes[i] != b'}' {
            i += 1;
        }
        if bytes.get(i) != Some(&b'}') {
            return None;
        }
        i += 1;
    }
    let spaces_start = i;
    while bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    if i == spaces_start || !bytes.get(i).is_some_and(|b| !b.is_ascii_whitespace()) {
        return None;
    }
    Some((marker_indent, i))
}

struct StrippedContainerLine<'a> {
    structural: &'a str,
    bare: &'a str,
    needs_empty_list_content: bool,
}

impl StrippedContainerLine<'_> {
    fn replacement(&self) -> String {
        let mut replacement = self.structural.to_string();
        if self.needs_empty_list_content {
            replacement.push_str(DEFINITION_PLACEHOLDER);
        }
        replacement
    }
}

fn parse_link_def_line(line: &str) -> Option<(&str, &str)> {
    let (label, target) = line.strip_prefix('[').and_then(|s| s.split_once("]: "))?;
    // The grammar requires at least one character:
    //
    //     reference_label = (character - ']' - '@'), {character - ']'} ;
    //
    // So `[]` is not a label and `[]: u` is a paragraph. Without this the line
    // was consumed as a definition and rendered nothing, so it DISAPPEARED -
    // carve-js and carve-php both keep it as text (carve-rs#451).
    //
    // Validate against the WHOLE production, not just the empty case. The first
    // fix here rejected only `[]`, and left `[]]: u` and `[a]b]: u` consumed -
    // both have a `]` inside the label, which the production excludes at every
    // position, and both vanished the same way (carve-rs#451).
    //
    // A space IS a `character`, so `[ ]` is a legal one-character label, which
    // is what all three engines do.
    let first = label.chars().next()?;
    if first == '@' || label.contains(']') {
        return None;
    }
    // THE PRODUCTION IS ANCHORED AT END OF LINE (PART 7, carve#911):
    //
    //     reference_definition = '[', reference_label, ']', ':', space,
    //                            link_destination, [link_title],
    //                            [space, attributes], newline ;
    //
    // What follows the destination and the optional title makes the production
    // FAIL, and the line is then an ordinary paragraph. This engine read the
    // tail as junk and ignored it, which nothing in the grammar authorized.
    //
    // The anchor lives HERE, in the shape test, rather than at the two places
    // that build the node - so paragraph interruption, the container scan and
    // the footnote-body scan all get the same answer from the same code. That
    // is the sweep carve#922 asks for: while the pattern ended in a
    // swallow-everything tail, a caller could test the RAW line and be right by
    // accident.
    if !link_def_target_is_anchored(target) {
        return None;
    }
    Some((label, target))
}

/// Does everything after `]:` match the production to END OF LINE?
///
/// PART 7 promises that a slot which fails to match "falls back to prose rather
/// than silently dropping metadata". At this line there was no prose to fall
/// back to: the swallowing tail ate whatever a failed slot rejected, so the
/// promised failure mode was unreachable and every narrowing here dropped
/// metadata instead of failing visibly. With the line anchored, both the
/// mixed-run forms at the title slot and at the trailing-attributes slot
/// produce the visible failure.
fn link_def_target_is_anchored(target: &str) -> bool {
    // THE LINE ENDING IS `whitespace` - a space or a tab, the same terminal
    // `blank_line` takes (PART 1, carve#890). So `[a]: /u<SP>` is a definition
    // and `[a]: /u<NBSP>` is not: a no-break space, an en quad, a byte order
    // mark and a form feed are CONTENT under that ruling, and content after the
    // destination is what the anchor rejects. Implementing this as a Unicode
    // whitespace PROPERTY reads all of them as a line ending, and a plain tab
    // fixture cannot see the difference - a tab is inside the property too.
    //
    // THE HEAD OF THE LINE IS DELIBERATELY UNTOUCHED. carve#911 rules what
    // follows the destination; a run BEFORE the destination is a different
    // question, and both call sites already hand `parse_link_def_target` a
    // Unicode-trimmed target. The executable spec agrees with that leniency -
    // `[a]: <U+202F>javascript:alert(1)` is still a definition there, with the
    // destination sanitized rather than the line refused - so tightening it
    // here would decide an unruled question as a side effect of this one, and
    // would silently change what `ansi_destination_denylist` pins about an
    // obfuscated scheme.
    let target = trim_ascii_end(target.trim_start());
    let (rest, _) = split_trailing_attr_block(target);
    let rest = trim_ascii_end(rest);
    // `link_destination` ends at the first whitespace, and that scan IS the
    // Unicode property (`unicode_url_char` is "non-whitespace, non-ASCII"), the
    // same reading `parse_link_def_target` applies below.
    let end = rest
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map_or(rest.len(), |(idx, _)| idx);
    if end == 0 {
        // No destination: `[r]:` with nothing after the colon is not a
        // definition, and never was.
        return false;
    }
    let after = &rest[end..];
    if after.is_empty() {
        return true;
    }
    // `link_title = space, ('"' … '"' | "'" … "'")`, ONE space (carve#912).
    let Some(after_pad) = after.strip_prefix(' ') else {
        return false;
    };
    let mut chars = after_pad.char_indices();
    let quote = match chars.next() {
        Some((_, q @ ('"' | '\''))) => q,
        _ => return false,
    };
    let mut escaped = false;
    for (idx, c) in chars {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == quote {
            // Only the line ending may follow the closing quote.
            return after_pad[idx + c.len_utf8()..]
                .chars()
                .all(|c| c == ' ' || c == '\t');
        }
    }
    // An unterminated title is not a title, and the tail no longer excuses it.
    false
}

/// A line with its blockquote prefixes removed, for CONTENT-COLUMN purposes.
///
/// Columns are measured inside the quote: `> - a` puts the item's content
/// column at 2 of the quoted content, not of the raw line - which carries the
/// `> ` and matches no marker, so the tracker saw no item at all and a
/// definition written at that column registered nothing while the item consumed
/// the line (carve#658).
fn without_blockquote_prefixes(line: &str) -> &str {
    let mut rest = line;
    while let Some(inner) = strip_blockquote_prefix(rest) {
        rest = inner;
    }
    rest
}

/// Does a `:` line here open a definition list's DESCRIPTION?
///
/// Only when a term opened the entry above it. A description line with no term
/// above it is not a description at all - it is paragraph text, and a
/// definition in it defines nothing (corpus
/// `216-a-description-line-needs-a-term-above-it`). Stripping the marker from
/// every `: ` line collects that one too, which is the opposite of what 216
/// pins.
///
/// The previous line decides it: a `::` term opens an entry and a further
/// description continues one.
/// Does the PREVIOUS line open a definition entry, read through its container?
///
/// The prefixes come off first, the way they do for the line being tested one
/// line down. Asking the raw line meant `> :: term` did not read as a term, so
/// the `:  ` marker below it was never stripped and a definition written there
/// was neither collected nor hoisted - PART 12 §10 puts it on the DOCUMENT
/// (markup-carve/carve#840). A div was the one container that worked, because
/// it adds no per-line prefix for this to hide behind.
fn opens_definition_entry(previous: &str) -> bool {
    opens_definition_entry_bare(strip_container_prefixes(previous, false).bare)
}

fn opens_definition_entry_bare(previous: &str) -> bool {
    let t = previous.trim_start_matches([' ', '\t']);
    let rest = match t.strip_prefix("::") {
        Some(after) if !after.starts_with(':') => after,
        Some(_) => return false,
        None => match t.strip_prefix(':') {
            Some(after) => after,
            None => return false,
        },
    };
    rest.starts_with(' ') || rest.starts_with('\t')
}

/// The description marker itself: `:` then whitespace, at the start of a line.
///
/// `::` is the TERM marker and a `:::` fence opener is a fence; both need
/// whitespace after a SINGLE colon and neither has it, so neither matches here.
fn strip_description_prefix(line: &str) -> Option<&str> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let rest = trimmed.strip_prefix(':')?;
    if !(rest.starts_with(' ') || rest.starts_with('\t')) {
        return None;
    }
    let content = rest.trim_start_matches([' ', '\t']);
    if content.is_empty() {
        return None;
    }
    Some(content)
}

fn strip_container_prefixes(mut line: &str, after_term: bool) -> StrippedContainerLine<'_> {
    let original = line;
    let mut needs_empty_list_content = false;
    loop {
        let before = line;
        while let Some(rest) = strip_blockquote_prefix(line) {
            line = rest;
            needs_empty_list_content = false;
        }
        // Every list spelling carries definitions at its content column. The
        // marker dialect changes numbering only; it cannot make an otherwise
        // identical reference definition visible.
        if let Some(marker) = detect_list_marker_full(line) {
            line = marker.content;
            needs_empty_list_content = true;
        }
        // A definition list's DESCRIPTION marker opens entry content exactly as
        // a bullet does, so a definition written on that line is collected from
        // it (carve-rs#668, spec markup-carve/carve#801).
        if after_term {
            if let Some(content) = strip_description_prefix(line) {
                line = content;
                // The entry must survive the line's removal as an EMPTY
                // description, the same way an item does when its only content
                // was a definition - otherwise the marker is left behind as a
                // stray paragraph and the `dd` disappears entirely.
                needs_empty_list_content = true;
            }
        }
        if line.len() == before.len() {
            break;
        }
    }
    // Compute the structural prefix length from the BYTE OFFSET of `line`
    // within `original`, not by length subtraction. `marker_tail` trims trailing
    // ASCII whitespace off the END of the collected content, so `line` can be
    // shorter than its true offset; a length-difference cut would then land
    // inside a leading multibyte content char and panic (`- ́ ` repro). `line`
    // is always a subslice of `original` whose START pointer is preserved by an
    // end-trim, so pointer subtraction yields the correct, char-boundary length.
    let structural_len = line.as_ptr() as usize - original.as_ptr() as usize;
    StrippedContainerLine {
        structural: &original[..structural_len],
        bare: line,
        needs_empty_list_content,
    }
}

/// `strip_container_prefixes`, but a quote marker AT an item's content column
/// counts as a container prefix.
///
/// A block quote nested in a list item is indented to that column (`- a` /
/// `  > [r]: /u`), and the plain stripper only reads a marker at position 0 - so
/// the definition inside such a quote was never collected, while at TOP level
/// the same `> [r]: /u` is collected by every engine (carve-rs#588).
///
/// EXACTLY that column, never arbitrary indentation: a top-level
/// `    > [r]: /u` is indented text, not a quote, and stays uncollected.
fn strip_container_prefixes_at<'a>(
    line: &'a str,
    columns: &ContentColumns,
    after_term: bool,
) -> StrippedContainerLine<'a> {
    // ASKED AFTER THE QUOTE MARKERS, because that is where the column it asks
    // about is measured. `at_content_column` records why: columns are counted
    // INSIDE the quote, so the item opened by `> - a` has content column 2 and
    // not 4. Asking about the raw line answered for the wrong depth - the indent
    // before a `>` is zero on a line that starts with one, so `> - a` /
    // `>   > [r]: /u` found no column, the inner quote was never stripped, and
    // the definition inside it stayed paragraph text where the block parser
    // publishes it (markup-carve/carve-rs#1082).
    //
    // The column is still what decides, which is the whole rule: an indented `>`
    // opens a quote only AT a live item's content column. `> a` / `>   > b` has
    // no item, so no column matches, the `>` is ordinary text, and this pass
    // agrees with the block parser about that too.
    // AND ASKED AGAIN AT EACH DEPTH, because the prefixes alternate. Each hop
    // consumes one indent-then-quote, and the markers between two hops are what
    // the workhorse below already walks - so `- > - > x` /
    // `  >   > [r]: /url` needs the question twice, once per item that holds a
    // quote. Asking once left the inner one unstripped.
    let mut cut = 0usize;
    // WHICH LEVEL the question is asked at, not just how deep the walk has got.
    // Each hop enters one more container, and the column a `>` must sit at is
    // that container's, measured in its own coordinates - so the frame the
    // walk consults advances with it. Asking the innermost frame every time
    // answered for the wrong depth as soon as a nested quote opened a list
    // (carve-rs#1096).
    let mut level = 0usize;
    loop {
        let mut inside = &line[cut..];
        while let Some(rest) = strip_blockquote_prefix(inside) {
            inside = rest;
            level += 1;
        }
        if let Some(marker) = detect_list_marker_full(inside) {
            inside = marker.content;
        }
        let content_col = columns.reached_by_at(level, leading_ws(inside));
        if content_col == 0
            || inside.len() <= content_col
            || !inside.as_bytes()[..content_col].iter().all(|b| *b == b' ')
            || inside.as_bytes()[content_col] != b'>'
        {
            break;
        }
        let next = (inside.as_ptr() as usize - line.as_ptr() as usize) + content_col;
        if next <= cut {
            break;
        }
        cut = next;
    }
    if cut > 0 {
        let inner = strip_container_prefixes(&line[cut..], after_term);
        let structural_len = inner.bare.as_ptr() as usize - line.as_ptr() as usize;
        return StrippedContainerLine {
            structural: &line[..structural_len],
            bare: inner.bare,
            needs_empty_list_content: inner.needs_empty_list_content,
        };
    }
    strip_container_prefixes(line, after_term)
}

fn strip_container_prefixes_keep_indent(mut line: &str) -> String {
    let mut out = String::new();
    loop {
        let before = line;
        while let Some(rest) = strip_blockquote_prefix(line) {
            line = rest;
        }
        if let Some(marker) = detect_list_marker_full(line) {
            let marker_width = marker.content.as_ptr() as usize - line.as_ptr() as usize;
            out.extend(std::iter::repeat(' ').take(marker_width));
            line = marker.content;
        }
        if line.len() == before.len() {
            break;
        }
    }
    out.push_str(line);
    out
}

// Counts every `strip_blockquote_prefix` call on the current thread.
//
// The blockquote prefix is the one operation this parser can be made to
// repeat superlinearly (markup-carve/carve-rs#731), so the regression guard
// counts calls rather than reading a clock: a counted curve is independent of
// machine load, and this repo's own perf tests record that a ratio bound
// "flaked on nearly every run". Test-only, so a release build carries nothing.
#[cfg(test)]
thread_local! {
    pub(crate) static QUOTE_PREFIX_CALLS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

fn strip_blockquote_prefix(line: &str) -> Option<&str> {
    #[cfg(test)]
    QUOTE_PREFIX_CALLS.with(|c| c.set(c.get() + 1));
    let rest = line.strip_prefix('>')?;
    if rest.is_empty() {
        return Some(rest);
    }
    rest.strip_prefix(' ')
}

fn parse_link_def_target(target: &str) -> LinkDef {
    // UNICODE whitespace, not just ASCII. `unicode_url_char` is "any
    // non-whitespace, non-ASCII Unicode character", unqualified, so a narrow
    // no-break space ends the destination exactly as a plain space does.
    // Scanning bytes for ASCII whitespace alone left one inside the href
    // (carve#404).
    let i = target
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map_or(target.len(), |(idx, _)| idx);
    let href = target[..i].to_string();
    // THE TITLE'S PADDING RUN IS SPACES HERE TOO. `reference_definition` reuses
    // `link_title`, which PART 7 spells `space`: the slot sits after the first
    // non-whitespace character of the line, where a tab is not syntax
    // (carve#901, carve-rs#726). This copy was a full Unicode `trim`, so it
    // admitted a tab in either direction and U+00A0 besides.
    //
    // A run holding anything but a space means NO TITLE, not "no definition".
    // The production tolerates trailing junk after the destination - `[r]: /u x`
    // is a definition whose `x` is ignored - so the line stays a definition and
    // only the title is dropped.
    let after_dest = &target[i..];
    let run_len = after_dest
        .find(|c: char| !c.is_whitespace())
        .unwrap_or(after_dest.len());
    //
    // AND IT IS EXACTLY ONE SPACE (carve#912). `reference_definition` reuses
    // `link_title`, whose slot is one `space`; a wider run means NO TITLE by
    // the same reading that makes a tab mean no title. A run with nothing after
    // it is the line ending rather than this slot and answers "no title" too,
    // so the cardinality test is only reached where a title could follow.
    let rest = if run_len == 1 && after_dest.starts_with(' ') {
        after_dest[run_len..].trim_end()
    } else {
        ""
    };
    // A title needs the opening AND a distinct closing quote: a lone `"` (or
    // `'`) satisfies both starts_with and ends_with on the same byte, so guard
    // len >= 2 before `rest[1..len-1]` underflows (begin > end panic).
    let title = if rest.len() >= 2
        && ((rest.starts_with('"') && rest.ends_with('"'))
            || (rest.starts_with('\'') && rest.ends_with('\'')))
    {
        // A backslash-escaped quote (or any escaped ASCII punctuation) inside
        // the title is unescaped, matching inline-link titles and carve-js
        // `unescapeAttrValue` (`[y]: /u "a\"b\"c"` -> title `a"b"c`).
        Some(unescape_title(&rest[1..rest.len() - 1]))
    } else {
        None
    };
    LinkDef {
        href,
        title,
        attrs: None,
        line: None,
    }
}

/// `parse_link_def_target`, with a trailing attribute block split off first
/// (carve#604). The block comes off BEFORE the destination/title scan, so
/// widening the parse cannot change what counts as a definition.
/// Hoist every authored `[label]: /url` definition into the document as a
/// `LinkReferenceDefinition` node (PART 12 §10, NORMATIVE).
///
/// §10 wants a NODE rather than a root field because §4 requires a `pos` on
/// everything but the root and a root field cannot carry one - a definition
/// occupies real bytes, and an editor, a formatter and a language server all need
/// to find them. Without it the canonical writer had nowhere to write a definition
/// back from and INLINED every resolved reference instead, which lost
/// `ref`/`raw_ref` on the reparse and turned one destination into N (carve-rs#631).
///
/// SOURCE ORDER, not the label order of the map: §10 answers "which definition
/// wins" by document order, and the writer has to put the lines back where the
/// author had them.
///
/// A definition lifted out of a FOOTNOTE BODY has no document line to point at -
/// the index it was found at belongs to the lifted body, not the source. It still
/// hoists, with no `pos`: §4 allows omitting a position the parser cannot
/// determine, and says a position pointing somewhere else is worse than none.
/// Dropping the node instead would lose the line from `fmt` entirely.
fn append_link_reference_definitions(
    doc: &mut Document,
    link_defs: &BTreeMap<String, LinkDef>,
    source: &str,
    options: &Options<'_>,
) {
    // Nothing to hoist is the common case, and both scans below are O(source) -
    // so a document with no definitions must not pay for them at all.
    if link_defs.is_empty() {
        return;
    }
    let line_starts = line_start_offsets(source);
    let lines: Vec<&str> = source.split('\n').collect();
    let mut authored: Vec<(Option<usize>, LinkReferenceDefinition)> = Vec::new();
    for (label, def) in link_defs {
        let pos = match (options.positions, def.line) {
            (true, Some(line)) => {
                let text = lines.get(line).copied().unwrap_or("");
                let start = line_starts.get(line).copied().unwrap_or(0);
                Some(Pos {
                    start_line: line + 1,
                    end_line: line + 1,
                    start_column: 1,
                    end_column: text.chars().count() + 1,
                    start_offset: start,
                    end_offset: start + text.chars().count(),
                })
            }
            _ => None,
        };
        authored.push((
            def.line,
            LinkReferenceDefinition {
                label: label.clone(),
                href: def.href.clone(),
                title: def.title.clone(),
                attrs: def.attrs.clone(),
                pos,
            },
        ));
    }
    // A definition with no line sorts last, and keeps the map's label order among
    // its peers - there is nothing better to order it by.
    authored.sort_by_key(|(line, _)| line.unwrap_or(usize::MAX));
    doc.children.extend(
        authored
            .into_iter()
            .map(|(_, node)| BlockNode::LinkReferenceDefinition(node)),
    );
}

fn parse_link_def_target_with_attrs(target: &str) -> LinkDef {
    let (rest, attr_text) = split_trailing_attr_block(target);
    let mut def = parse_link_def_target(rest);
    // `parse_attrs` takes the INNER content, not the braces (see the block
    // attribute-line caller, which strips them the same way).
    def.attrs = attr_text.and_then(|t| parse_attrs(&t[1..t.len() - 1]));
    def
}

/// Split a TRAILING attribute block off a definition's target (carve#604).
///
/// Scanned rather than matched: an attribute value may hold a `}` inside
/// quotes (`{data-x="}"}`), and stopping at the first `}` drops every attribute
/// on the line silently. Only a `}` outside quotes closes the block.
///
/// The block must be preceded by whitespace and end the target, so
/// `[a]: /u{.x}` keeps the braces in the DESTINATION, matching the
/// production's `space, attributes`.
fn split_trailing_attr_block(target: &str) -> (&str, Option<&str>) {
    // Space and tab, not the Unicode property: a no-break space after the block
    // is CONTENT, so a line ending in one is not a definition at all once the
    // production is anchored (carve#890, carve#911).
    let end = trim_ascii_end(target);
    if !end.ends_with('}') {
        return (target, None);
    }
    let mut quote: Option<char> = None;
    let mut open: Option<usize> = None;
    let mut escaped = false;
    let last = end.len() - '}'.len_utf8();
    for (i, c) in end.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(q) => {
                if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '{' => {
                    if open.is_none() {
                        open = Some(i);
                    }
                }
                '}' if i == last => {
                    let Some(start) = open else {
                        return (target, None);
                    };
                    // THE WHOLE SEPARATOR RUN, not the character adjacent to the
                    // `{`. PART 7 names "the reference-definition slot before
                    // its trailing `attributes`" as a padding slot spelled
                    // `space`, and this test read only `chars().next_back()` -
                    // so `[a]: /u<TAB><SP>{.c}` put a space next to the brace
                    // while the run still held a tab, and the block attached
                    // anyway. A last-character test standing in for a run test
                    // is the mirror of the first-character test found in
                    // carve-rs#722 (carve#901, carve-rs#726).
                    let sep_len = end[..start].len()
                        - end[..start].trim_end_matches(char::is_whitespace).len();
                    let sep = &end[start - sep_len..start];
                    //
                    // ONE space, not a run (carve#912): `reference_definition`
                    // spells the slot `[space, attributes]`. A wider run leaves
                    // the braces where they are, exactly as a zero-space run
                    // already leaves them in the destination.
                    if sep.len() != 1 || !sep.starts_with(' ') {
                        return (target, None);
                    }
                    let block = &end[start..];
                    // AN INVALID BLOCK IS NOT `attributes`, SO THE LINE IS NOT A
                    // DEFINITION (markup-carve/carve#933). `[space, attributes]`
                    // names the `attributes` production, and a balanced `{...}`
                    // that production does not accept is not an instance of it -
                    // it is leftover content, and the end-of-line anchor above
                    // disposes of it like any other leftover.
                    //
                    // The scan is what has to say so. It peels the block off
                    // BEFORE anything validates it, so a rejected block had
                    // already been consumed and DISCARDED and the line went on
                    // to define with the author's braces gone from the page -
                    // the exact outcome PART 7 names as the one to avoid. The
                    // remedy is structural: handing the block back as CONTENT is
                    // a third outcome, and where "rejected" and "absent" are the
                    // same value the failure has nowhere to be observed.
                    //
                    // `x {#}` in a paragraph already keeps its braces as text,
                    // because `attributes` rejects that block there too. Two
                    // readings of the same characters one construct apart is
                    // what this removes.
                    if parse_attrs(&block[1..block.len() - 1]).is_none() {
                        return (target, None);
                    }
                    return (end[..start].trim_end(), Some(block));
                }
                _ => {}
            },
        }
    }
    (target, None)
}

type SplitFrontmatter<'a> = (BTreeMap<String, String>, Option<Frontmatter>, &'a str);

/// The key/value view of a frontmatter block, derived from its raw text.
///
/// Shared with the AST decoder rather than duplicated there. The wire form
/// carries the RAW block only (PART 12 §7 - a parsed map cannot be serialized
/// back to the bytes the author wrote), so a decoded document has to rebuild
/// this the same way a parsed one built it. Deriving it with the same function
/// is what makes decode(encode(x)) equal x instead of nearly equal.
pub(crate) fn frontmatter_map(format: &str, content: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    // Only the bare / yaml form is key:value; typed blocks (json/toml) are
    // structured and just stripped.
    if format.is_empty() || format.eq_ignore_ascii_case("yaml") {
        for line in content.lines() {
            if let Some((key, value)) = line.split_once(':') {
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    map
}

/// The span of a frontmatter block, fences included. It always starts at the
/// first character of the document, so only the end has to be worked out - and
/// the block is taken from the raw source before any line is stripped, so every
/// column here is a column in the document.
fn frontmatter_pos(source: &str, block_end: usize) -> Pos {
    let block = &source[..block_end];
    let last_line_start = block.rfind('\n').map_or(0, |at| at + 1);
    Pos {
        start_line: 1,
        start_column: 1,
        start_offset: 0,
        end_line: block.bytes().filter(|b| *b == b'\n').count() + 1,
        // Columns and offsets are counted in CODEPOINTS (PART 12 section 4).
        end_column: block[last_line_start..].chars().count() + 1,
        end_offset: block.chars().count(),
    }
}

/// The format token of a frontmatter opener, given everything after the `---`.
///
/// `Some("")` is a bare fence (`---`), which defaults to yaml; `None` means the
/// line is not a typed opener at all.
///
/// THE PADDING RUN IS SPACES. `frontmatter_open`'s slot before the format token
/// is a PADDING slot - the `---` pair has already decided the block, and the
/// token only names the metadata dialect - but PART 7's MARKER SEPARATORS AND
/// PADDING SLOTS decides the terminal by POSITION rather than by role: the slot
/// sits after the first non-whitespace character of the line, and a tab is
/// syntax only inside a line's leading indentation run. So `---<TAB>yaml` is
/// not a typed opener; it is an ordinary line (carve#901, carve#905).
///
/// The whole run is checked, not its first character. A check on the first
/// character rejects `---<TAB>yaml` and passes `---<SP><TAB>yaml`, and the rule
/// is about the run. This is the shape that survived carve-rs#720 inside
/// `detect_container_open` and was only found in carve-rs#722.
///
/// The terminal is a literal `' '`, not `[' ', '\t']`. Widening it to a set
/// would re-admit the tab being removed, and narrowing to `' '` drops U+00A0
/// with it - which the previous `str::trim` admitted, so `---<NBSP>yaml` opened
/// frontmatter too.
///
/// CARDINALITY IS UNCHANGED. The production spells the slot `[space]`, exactly
/// one, while every engine reads a run; that is a separate question from the
/// terminal and no ticket asks for it here. `resources/carve-core.ohm` makes
/// the same call for `titleSp+`.
///
/// TRAILING whitespace after the token is the line-ending rule rather than this
/// slot, so it stays tolerated - matching the spec oracle. THE SAME IS TRUE
/// WITH NO TOKEN AT ALL: `---<TAB>` is a bare opener whose whole tail is
/// trailing, because a frontmatter delimiter takes no content on its line and
/// there is therefore no slot for the terminal to govern (carve#1295).
pub(crate) fn frontmatter_format_token(after_marker: &str) -> Option<&str> {
    // WHITESPACE IS SPACE OR TAB (PART 7, carve#977), at both of this
    // function's slots.
    //
    // The TRAILING one is what was wrong. `char::is_whitespace` is the Unicode
    // White_Space property and takes a VERTICAL TAB, a FORM FEED and a NO-BREAK
    // SPACE, so `trim_end` cut a trailing vertical tab off the token and a yaml
    // opener carrying one was typed - and its block then swallowed the document
    // down to the next bare three-dash line.
    //
    // NAMED SURVIVOR: reverting the LEADING scan alone changes no output, and
    // no test can be written that it does. The next statement already rejects
    // any character in that run that is not a space, so a wider scan cannot
    // leak - it only shifts which of the two statements says no. It is narrowed
    // anyway so the function reads its own production rather than depending on
    // a downstream check, which is what the padding-slot history in this
    // function (carve-rs#720, carve-rs#722) is a record of.
    let token_start = after_marker
        .find(|c: char| !matches!(c, ' ' | '\t'))
        .unwrap_or(after_marker.len());
    let kind = trim_ascii_end(&after_marker[token_start..]);
    // NOTHING AFTER THE MARKER: the run is the LINE ENDING, not this slot.
    //
    // POSITION DECIDES (carve#1295). A tab BEFORE content is a separator and
    // the terminal is `space` alone; a tab with nothing after it is TRAILING,
    // and PART 2's NO TRAILING WHITESPACE drops it - its run is `whitespace`,
    // `' ' | '\t'`. A frontmatter delimiter takes no content on its line, so
    // `---<TAB>` lands on the trailing side and opens the block.
    //
    // It was reaching the space-only test below, which refused it - while the
    // same line still read as a THEMATIC BREAK. One trailing tab disqualified
    // one construct and not the other, on the same line.
    //
    // The test order is what carries this: the emptiness question is asked
    // BEFORE the terminal question, because the terminal only governs a slot
    // and there is no slot on a content-less line.
    if kind.is_empty() {
        return Some(kind);
    }
    if after_marker[..token_start].chars().any(|c| c != ' ') {
        return None;
    }
    // AND EXACTLY ONE SPACE (carve#912). `frontmatter_open = "---", [space],
    // [frontmatter_format]` spells the slot as one; a wider run makes the line
    // no typed opener, and since it is not a thematic break either it is
    // ordinary paragraph text that the metadata lines fold into.
    if token_start > 1 {
        return None;
    }
    if !kind.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(kind)
}

/// Whether `source` opens a frontmatter block, by the parser's own test.
///
/// The canonical writer needs exactly one question answered: would a leading
/// `---` in the bytes it is about to emit be read back as a frontmatter opener
/// instead of as the thematic break it wrote? It asks `split_frontmatter`
/// rather than re-deriving the answer, because a second reader of the same
/// question is only safe while it agrees - the seam carve-rs#725 had to unify
/// once already and carve-rs#732 lost a whole document to.
///
/// `normalize_source` runs first for the same reason: the parser tests the
/// normalized text, so a reader that tested the raw text would disagree about
/// a CRLF or BOM'd document - the exact fall-through carve-rs#732 lost a
/// frontmatter block to.
///
/// NAMED SURVIVOR: no fixture can tell that call apart from a no-op today.
/// The only caller passes the CANONICAL WRITER'S OUTPUT, which is built from a
/// tree that was itself parsed out of normalized text, so it can hold no
/// carriage return, no NUL and no leading byte order mark. The call is kept so
/// the helper answers for any text rather than only for the one caller's, which
/// is the same reason `raw_frontmatter` was made to share the opener test
/// instead of keeping a second copy of it (carve-rs#725).
pub(crate) fn opens_frontmatter(source: &str) -> bool {
    let normalized = normalize_source(source);
    split_frontmatter(normalized.as_ref(), false).1.is_some()
}

fn split_frontmatter(source: &str, positions: bool) -> SplitFrontmatter<'_> {
    // Opening fence: `---` optionally followed by a type token (`---yaml`,
    // `---json`, `---toml`, ...; canonical has no space). Closer is a bare `---`.
    if !source.starts_with("---") {
        return (BTreeMap::new(), None, source);
    }
    let Some(first_nl) = source.find('\n') else {
        return (BTreeMap::new(), None, source);
    };
    let Some(kind) = frontmatter_format_token(&source[3..first_nl]) else {
        return (BTreeMap::new(), None, source);
    };
    let rest = &source[first_nl + 1..];
    // The closer is a line that is exactly `---`. It may be the FIRST line of
    // `rest` (an empty frontmatter, `---\n---`) or follow a newline.
    let (content_len, after) = if rest == "---" {
        (0, rest.len())
    } else if let Some(r) = rest.strip_prefix("---\n") {
        (0, rest.len() - r.len())
    } else if let Some(close) = rest.find("\n---\n") {
        (close, close + 5)
    } else if let Some(close) = rest.strip_suffix("\n---").map(|s| s.len()) {
        (close, rest.len())
    } else {
        return (BTreeMap::new(), None, source);
    };
    let frontmatter_src = &rest[..content_len];
    let body = &rest[after..];
    let frontmatter = frontmatter_map(kind, frontmatter_src);
    let raw = Frontmatter {
        // A bare fence is yaml, which is what the reference publishes.
        format: if kind.is_empty() {
            DEFAULT_FRONTMATTER_FORMAT.to_string()
        } else {
            kind.to_string()
        },
        content: frontmatter_src.trim_end_matches('\n').to_string(),
        pos: positions.then(|| {
            // `after` runs past the closing fence's newline when it has one;
            // the span stops at the fence, not at the blank after it.
            let block_end = first_nl + 1 + after;
            frontmatter_pos(source, source[..block_end].trim_end_matches('\n').len())
        }),
    };
    (frontmatter, Some(raw), body)
}

pub(crate) fn parse_blocks_with_options(source: &str, options: &Options<'_>) -> Vec<BlockNode> {
    parse_blocks_with_options_at_level(source, options, false)
}

fn parse_blocks_with_options_at_level(
    source: &str,
    options: &Options<'_>,
    at_document_level: bool,
) -> Vec<BlockNode> {
    let mut lines: Vec<&str> = source.lines().collect();
    // `lines()` already drops a single trailing newline; nothing more to do.
    let _ = &mut lines;

    // The line map serves two features now: the source-line render option, and
    // PART 12 positions. Either one asking for it is enough.
    let want_lines = options.source_lines || options.positions;
    let line_map: Vec<Option<usize>> = if want_lines {
        (1..=lines.len()).map(Some).collect()
    } else {
        Vec::new()
    };
    // Nothing has been stripped from a top-level line, so every column here is
    // a column in the document.
    let col_map: Vec<Option<isize>> = if options.positions {
        vec![Some(0); lines.len()]
    } else {
        Vec::new()
    };
    let mut cursor = LineCursor::new_with_cols(
        &lines,
        want_lines.then_some(line_map.as_slice()),
        options.positions.then_some(col_map.as_slice()),
    );
    cursor.at_document_level = at_document_level;
    parse_blocks(&mut cursor, options)
}

struct LineCursor<'a> {
    lines: &'a [&'a str],
    line_map: Option<&'a [Option<usize>]>,
    /// Columns already stripped from the front of each line by an enclosing
    /// container, so a nested strip accumulates rather than resetting. `None`
    /// for a line whose stripped width is not known - a block starting there
    /// gets no position rather than a wrong one.
    col_map: Option<&'a [Option<isize>]>,
    pos: usize,
    at_document_level: bool,
    comment_closer_last_index: Option<HashMap<usize, usize>>,
    code_closer_last_index: Option<HashMap<u8, Vec<usize>>>,
}

impl<'a> LineCursor<'a> {
    fn new_with_cols(
        lines: &'a [&'a str],
        line_map: Option<&'a [Option<usize>]>,
        col_map: Option<&'a [Option<isize>]>,
    ) -> Self {
        LineCursor {
            lines,
            line_map,
            col_map,
            pos: 0,
            at_document_level: false,
            comment_closer_last_index: None,
            code_closer_last_index: None,
        }
    }

    fn peek(&self) -> Option<&'a str> {
        self.lines.get(self.pos).copied()
    }
    fn consume(&mut self) -> Option<&'a str> {
        let line = self.peek();
        if line.is_some() {
            self.pos += 1;
        }
        line
    }
    fn eof(&self) -> bool {
        self.pos >= self.lines.len()
    }
    fn source_line(&self, pos: usize) -> Option<usize> {
        self.line_map
            .and_then(|map| map.get(pos).copied().flatten())
    }

    /// Columns stripped from the front of the line at `pos`, when known.
    fn source_col(&self, pos: usize) -> Option<isize> {
        self.col_map.and_then(|map| map.get(pos).copied().flatten())
    }

    /// Is there a comment-fence closer of exactly `fence_len` at or after `start`?
    ///
    /// A closer must match the opener width EXACTLY, so ANY later line carrying a
    /// fence of that width IS a valid closer: the question is exactly "last index
    /// for this width >= start". One pass builds the width -> last index map and
    /// every lookup after that is O(1).
    ///
    /// There used to be a per-width negative cache in front of this map. It could
    /// never change an outcome: the map already answers in O(1), and its own hit
    /// condition (a second opener of the same width after a proven-no-closer
    /// point) is unreachable, because a second line of the same width IS a closer
    /// for the first.
    /// Whether any line after `start` could close a code fence of this char and
    /// width. Deliberately OVER-approximate: it ignores indentation, which the
    /// real closer test does not. A `false` is therefore final and a `true` only
    /// means "worth scanning", so the caller keeps its exact scan behind this.
    ///
    /// Without it the exact scan runs from every opener to the end of the
    /// document, which is quadratic in the number of unterminated openers - the
    /// same shape `comment_closer_last_index` was added to remove for `%%%`.
    /// Code fences never got the equivalent.
    fn has_code_closer_after(&mut self, start: usize, fence_char: u8, fence_len: usize) -> bool {
        if self.code_closer_last_index.is_none() {
            self.code_closer_last_index = Some(build_code_closer_last_index(self.lines));
        }
        self.code_closer_last_index
            .as_ref()
            .is_some_and(|index| code_closer_exists_after(index, start, fence_char, fence_len))
    }

    fn has_comment_closer_after(&mut self, start: usize, fence_len: usize) -> bool {
        if self.comment_closer_last_index.is_none() {
            self.comment_closer_last_index = Some(build_comment_closer_last_index(self.lines));
        }
        self.comment_closer_last_index
            .as_ref()
            .and_then(|last_index| last_index.get(&fence_len).copied())
            .is_some_and(|last| last >= start)
    }
}

#[derive(Default)]
struct LineBuffer {
    lines: Vec<String>,
    line_map: Vec<Option<usize>>,
    /// Codepoints the container took from the front of each line, parallel to
    /// `lines`. Kept in lockstep by `push_at`: a shifted entry would hand a
    /// nested block a WRONG column, which is worse than the `None` an absent
    /// entry produces.
    col_map: Vec<Option<isize>>,
    /// Whether the LAST line pushed was a synthetic blank rather than one the
    /// author wrote. `into_source` needs the difference: a real trailing blank
    /// is content and must survive the round trip, a synthetic one is
    /// scaffolding and must not (markup-carve/carve-rs#908).
    last_is_synthetic: bool,
}

impl LineBuffer {
    fn push(&mut self, line: String, source_line: Option<usize>) {
        self.push_at(line, source_line, None)
    }

    /// Like `push`, recording how many codepoints were stripped from the front
    /// of the line by the enclosing container.
    fn push_at(&mut self, line: String, source_line: Option<usize>, stripped: Option<isize>) {
        self.last_is_synthetic = false;
        self.lines.push(line);
        if source_line.is_some() || !self.line_map.is_empty() {
            self.line_map.push(source_line);
        }
        self.col_map.push(stripped);
    }

    fn push_synthetic_blank(&mut self) {
        self.push(String::new(), None);
        self.last_is_synthetic = true;
    }

    fn into_source(self) -> MappedSource {
        // TERMINATE only when the buffer ends in a blank the AUTHOR wrote.
        //
        // `join` loses a trailing empty line on the round trip back through
        // `str::lines()`, so a fence that runs to a container's closer came out
        // a line short. Terminating unconditionally fixed that and broke the
        // other half: `push_synthetic_blank` inserts blanks so an attached
        // block parses on its own, and preserving one of those re-parses as a
        // real blank line that `fmt` then writes out
        // (tests/comment_body_is_relative_to_its_fence).
        //
        // The two are told apart by who pushed the line, which is the only
        // place the difference is known (markup-carve/carve-rs#908).
        let ends_in_authored_blank =
            !self.last_is_synthetic && self.lines.last().is_some_and(|line| line.is_empty());
        let mut source = self.lines.join("\n");
        if ends_in_authored_blank {
            source.push('\n');
        }
        MappedSource {
            col_map: self.col_map,
            source,
            line_map: self.line_map,
        }
    }
}

struct MappedSource {
    source: String,
    line_map: Vec<Option<usize>>,
    /// Bytes stripped from the FRONT of each line - a blockquote marker, a list
    /// indent, a container prefix. Without it a column in `source` cannot be
    /// mapped back to a column in the document, which is why nested blocks
    /// could not carry a position (spec PART 12 section 4).
    col_map: Vec<Option<isize>>,
}

impl MappedSource {
    /// Like `new_line`, recording how many bytes were stripped from the front.
    fn new_line_at(line: String, source_line: Option<usize>, stripped: Option<isize>) -> Self {
        MappedSource {
            source: line,
            line_map: source_line.into_iter().map(Some).collect(),
            col_map: vec![stripped],
        }
    }

    /// Append a line, recording how many codepoints were stripped from its
    /// front by the enclosing container.
    fn push_newline_at(
        &mut self,
        line: String,
        source_line: Option<usize>,
        stripped: Option<isize>,
    ) {
        if !self.source.is_empty() {
            self.source.push('\n');
        }
        self.source.push_str(&line);
        if source_line.is_some() || !self.line_map.is_empty() {
            self.line_map.push(source_line);
        }
        self.col_map.push(stripped);
    }

    fn append(&mut self, other: MappedSource) {
        if other.source.is_empty() {
            return;
        }
        if !self.source.is_empty() {
            self.source.push('\n');
        }
        self.source.push_str(&other.source);
        self.line_map.extend(other.line_map);
        self.col_map.extend(other.col_map);
    }
}

/// Codepoints a container took from the front of a line, when that is knowable.
///
/// SIGNED, because a container can hand the parser a line that is LONGER at the
/// front than the source line was. When a tab straddles the column a container
/// strips to, `strip_leading_columns` re-inserts the overshoot as spaces - two
/// characters where one was consumed - and the constant that maps a column in
/// the result back to a column in the document is then NEGATIVE. An unsigned
/// map cannot hold it, which is the whole reason a tab-indented footnote
/// continuation published no positions while the two-space spelling published
/// all five (carve-rs#736).
///
/// Three shapes have a knowable width, and all three are affine:
///
///   A PREFIX REMOVAL. The line the parser sees is a SUFFIX of the source line,
///   and the difference between them is what the container took.
///
///   A TRIM AT BOTH ENDS. The line the parser sees is the source line with its
///   leading indentation AND its trailing ASCII whitespace removed. Dropping
///   characters off the END moves nothing in front of them, so the constant is
///   the leading width alone - the trailing trim is invisible to it.
///
///   Without this a paragraph line ending in a space or a tab placed NOTHING:
///   `abc<SP>` is not a suffix of itself trimmed, so every inline anchored on
///   that line lost its position and `abc` published no `pos` while the same
///   document without the trailing space published one (PART 12 §4's test is
///   whether a true span EXISTS, and here it plainly does - `abc` is the
///   source at offset 0). Fifteen of this engine's position findings were that
///   one line, across a paragraph, a list item, a block quote and a line
///   block.
///
///   A RESIDUAL-AWARE DEDENT. The line the parser sees is a suffix of the
///   source line carrying a SYNTHETIC prefix of spaces, and what the container
///   consumed in front of that suffix was itself nothing but indentation. The
///   constant is then what it consumed minus what it synthesized, which may be
///   negative.
///
/// Anything else - a tab expansion in the middle of a line, a synthesized
/// replacement, a line block's indent sentinel - has no such correspondence and
/// yields `None`, so blocks starting there carry no position rather than a
/// wrong one. The guard is that the two lines differ ONLY in leading
/// whitespace: if what either side put in front of the shared tail contains a
/// single non-indent character, there is no constant and this returns `None`.
///
/// THAT LAST GUARD IS UNREACHABLE TODAY, and is recorded as such rather than
/// presented as a fix (markup-carve/carve#755: a check that cannot fail is
/// recorded, not deleted and not counted as proof). Deleting it changes no
/// output over the 830-document spec corpus and over 8018 generated
/// tab-and-space indentation shapes, and no test in this suite fails. The
/// reason is that the second branch is only reached when the parser's line is
/// NOT a suffix of the source line, and the only producer that rewrites a line
/// that way is `strip_leading_columns`, which consumes nothing but whitespace.
/// It is here so a future producer that does consume content cannot silently
/// publish an affine map that is not one.
fn stripped_col(outer: Option<isize>, original: &str, stripped: &str) -> Option<isize> {
    let outer = outer?;
    if original.ends_with(stripped) {
        return Some(outer + original.chars().count() as isize - stripped.chars().count() as isize);
    }
    // THE SAME SUFFIX TEST, against the line with its trailing whitespace
    // dropped. It stays anchored at the end - of the trimmed body - so it names
    // exactly one offset, and the prefix it implies is measured rather than
    // guessed. That is what keeps it as safe as the rule above it: a trailing
    // trim is invisible to a constant that describes the front of the line.
    let body = trim_ascii_end(original);
    if body.ends_with(stripped) {
        return Some(outer + body.chars().count() as isize - stripped.chars().count() as isize);
    }
    let synthetic = stripped.chars().take_while(|c| *c == ' ').count();
    let tail = &stripped[synthetic..];
    if synthetic == 0 || !original.ends_with(tail) {
        return None;
    }
    let consumed = original.chars().count().checked_sub(tail.chars().count())?;
    if !original
        .chars()
        .take(consumed)
        .all(|c| c == ' ' || c == '\t')
    {
        return None;
    }
    Some(outer + consumed as isize - synthetic as isize)
}

/// WHERE THE SIGNED QUANTITY STOPS: a 1-based document column.
///
/// `stripped` is the constant the column map carries for a line and `within` is
/// the codepoint offset inside the line the parser saw, so their sum is the
/// 0-based document column. Every `Pos` column in this file is built here, so
/// there is exactly one place the two units meet and exactly one place the
/// clamp lives.
///
/// The clamp is UNREACHABLE for a well-formed map, and is a guard rather than a
/// fix: a negative constant only ever arises from a synthetic space prefix, and
/// nothing is placed at a column inside that prefix - it is indentation, and
/// `within` has already skipped it. Saturating here means a future producer
/// that does get it wrong publishes column 1 rather than wrapping to
/// `usize::MAX` and reporting a span past the end of the document.
fn document_column(stripped: isize, within: usize) -> usize {
    (stripped + within as isize).max(0) as usize + 1
}

/// Seed a list item's body with its marker line, carrying the column the marker
/// itself occupied so a block opened on that line lands where the author wrote
/// it rather than at column 1.
fn item_marker_source(cur: &LineCursor<'_>, content: &str, at: usize) -> MappedSource {
    let stripped = cur
        .lines
        .get(at)
        .and_then(|line| stripped_col(cur.source_col(at), line, content));
    MappedSource::new_line_at(content.to_string(), cur.source_line(at), stripped)
}

/// Span of a list item's lead paragraph. It starts where the marker's CONTENT
/// starts - the paragraph is the text, not the bullet - and ends at the last
/// line folded into it by lazy continuation.
fn item_paragraph_span(
    cur: &LineCursor<'_>,
    start_at: usize,
    end_at: usize,
    content: &str,
    options: &Options<'_>,
) -> Option<Pos> {
    if !options.positions {
        return None;
    }
    let start_line = cur.source_line(start_at)?;
    let line = cur.lines.get(start_at)?;
    let start_stripped = stripped_col(cur.source_col(start_at), line, content)?;
    let end_line = cur.source_line(end_at).unwrap_or(start_line);
    let end_width = cur
        .lines
        .get(end_at)
        .map(|l| l.chars().count())
        .unwrap_or(0);
    Some(Pos {
        start_line,
        end_line,
        start_column: document_column(start_stripped, 0),
        end_column: document_column(cur.source_col(end_at).unwrap_or(0), end_width),
        start_offset: 0,
        end_offset: 0,
    })
}

fn inline_anchor_for_line(
    cur: &LineCursor<'_>,
    pos: usize,
    inline_line: &str,
) -> Option<(usize, isize)> {
    Some((
        cur.source_line(pos)?,
        stripped_col(cur.source_col(pos), cur.lines.get(pos)?, inline_line)?,
    ))
}

fn parse_inline_lines_with_anchor(
    text: &str,
    options: &Options<'_>,
    lines: Vec<Option<(usize, isize)>>,
) -> Vec<InlineNode> {
    parse_inline_with_anchor(text, options, InlineAnchor::lines(&lines))
}

/// The same, for a text whose segments are NOT separated by newlines.
///
/// `breaks` are the byte offsets at which the next entry of `lines` takes over.
/// A table cell rebuilt across a `+` continuation is the only text shaped this
/// way: its fragments come from different source lines and are joined by a
/// manufactured space, so nothing in the text itself marks where one ends.
fn parse_inline_segments_with_anchor(
    text: &str,
    options: &Options<'_>,
    lines: Vec<Option<(usize, isize)>>,
    breaks: Vec<usize>,
) -> Vec<InlineNode> {
    parse_inline_with_anchor(
        text,
        options,
        InlineAnchor {
            lines: &lines,
            breaks: &breaks,
        },
    )
}

/// Build a span for the lines `[start, end)` of `cur`, in the ORIGINAL source.
///
/// Returns `None` when the source line or the stripped column width is unknown
/// for the first line - a position that cannot be trusted is worse than no
/// position, because a consumer cannot tell the difference.
///
/// Offsets are left at zero here and filled by `fill_offsets` once the whole
/// document is parsed: an offset needs the original text, which the parser sees
/// only as already-stripped lines.
fn span_of(cur: &LineCursor<'_>, start: usize, end: usize, options: &Options<'_>) -> Option<Pos> {
    if !options.positions {
        return None;
    }
    let start_line = cur.source_line(start)?;
    let stripped = cur.source_col(start)?;
    let last = end.saturating_sub(1).max(start);
    let end_line = cur.source_line(last).unwrap_or(start_line);
    // The parser sees the line with its container prefix removed, so the
    // document column is what the container took plus the indent that remains.
    //
    // INDENTATION IS SPACE AND TAB, and nothing else is. `trim_start` here was
    // the Unicode whitespace property, so a line opening with a no-break space
    // or an en quad had that character counted as INDENT - the block's span
    // began one column past its own first child, which PART 12 §4's containment
    // rule forbids (carve#913, and the finding that pass reports). Those
    // characters are CONTENT under PART 1's `indent` terminal (carve#890), so
    // they belong inside the span, not in front of it.
    let indent = cur
        .lines
        .get(start)
        .map(|l| l.chars().count() - trim_ascii_start(l).chars().count())
        .unwrap_or(0);
    let width = cur.lines.get(last).map(|l| l.chars().count()).unwrap_or(0);
    // The LAST line may have had a different amount taken off it than the
    // first: a lazily continued paragraph starts inside a blockquote or list
    // item and ends flush left, so reusing the opening line's count runs the
    // end column past the end of the document.
    let end_stripped = if last == start {
        stripped
    } else {
        cur.source_col(last)?
    };
    Some(Pos {
        start_line,
        end_line,
        start_column: document_column(stripped, indent),
        end_column: document_column(end_stripped, width),
        start_offset: 0,
        end_offset: 0,
    })
}

/// Fill the offset fields from the original source, in CODEPOINTS (PART 12
/// section 4). Runs once per document: the line table is one pass, and the
/// conversion is the identity for any document without an astral character.
/// A block's own span, to write through.
///
/// EXHAUSTIVE on purpose. A `_ => None` arm here is why an `abbreviation_def`
/// shipped with a correct line and column and offsets of `0..0` - present, and
/// selecting nothing. That is the fourth node family to fail exactly that way,
/// after figure captions, footnote definition bodies and definition terms. A
/// new variant is now a compile error rather than a silent 0..0.
fn block_pos_mut(block: &mut BlockNode) -> Option<&mut Pos> {
    match block {
        BlockNode::LinkReferenceDefinition(d) => d.pos.as_mut(),
        BlockNode::Heading(h) => h.pos.as_mut(),
        BlockNode::Paragraph(p) => p.pos.as_mut(),
        BlockNode::ThematicBreak(t) => t.pos.as_mut(),
        BlockNode::CodeBlock(c) => c.pos.as_mut(),
        BlockNode::RawBlock(r) => r.pos.as_mut(),
        BlockNode::Comment(c) => c.pos.as_mut(),
        BlockNode::Div(d) => d.pos.as_mut(),
        BlockNode::Admonition(a) => a.pos.as_mut(),
        BlockNode::BlockQuote(b) => b.pos.as_mut(),
        BlockNode::List(l) => l.pos.as_mut(),
        BlockNode::Table(t) => t.pos.as_mut(),
        BlockNode::LineBlock(l) => l.pos.as_mut(),
        BlockNode::Figure(f) => f.pos.as_mut(),
        BlockNode::FigureGroup(g) => g.pos.as_mut(),
        BlockNode::BlockImage(i) => i.pos.as_mut(),
        BlockNode::DefinitionList(d) => d.pos.as_mut(),
        BlockNode::AbbreviationDef(a) => a.pos.as_mut(),
        // The Citations extension builds this one in `after_parse`, which runs
        // after `fill_offsets`, and derives its `pos` from inline positions
        // that pass has already converted - so there is nothing there to
        // convert, and an arm is still required rather than a `_`.
        BlockNode::CitationDefinition(d) => d.pos.as_mut(),
        BlockNode::Extension(e) => e.pos.as_mut(),
    }
}

fn fill_offsets(blocks: &mut [BlockNode], line_starts: &[usize]) {
    for block in blocks {
        if let Some(pos) = block_pos_mut(block) {
            apply_offsets(pos, line_starts);
        }
        // Recurse into the containers that hold blocks and inline content.
        match block {
            BlockNode::Heading(h) => apply_inline_offsets(&mut h.children, line_starts),
            BlockNode::Paragraph(p) => {
                apply_inline_offsets(&mut p.children, line_starts);
                let first = p.children.iter().find_map(|node| {
                    (!matches!(node, InlineNode::SoftBreak(_) | InlineNode::HardBreak(_)))
                        .then(|| owned_inline_pos(node))
                        .flatten()
                });
                let last = p.children.iter().rev().find_map(owned_inline_pos);
                if let (Some(pos), Some(first), Some(last)) = (&mut p.pos, first, last) {
                    pos.start_line = first.start_line;
                    pos.start_column = first.start_column;
                    pos.start_offset = first.start_offset;
                    pos.end_line = last.end_line;
                    pos.end_column = last.end_column;
                    pos.end_offset = last.end_offset;
                }
            }
            BlockNode::BlockQuote(b) => {
                fill_offsets(&mut b.children, line_starts);
            }
            BlockNode::Div(d) => fill_offsets(&mut d.children, line_starts),
            BlockNode::Admonition(a) => {
                if let Some(title) = &mut a.title {
                    apply_inline_offsets(title, line_starts);
                }
                fill_offsets(&mut a.children, line_starts);
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    // The ITEM's own span too, not only the blocks inside it:
                    // a span whose offsets are never filled stays 0..0, which
                    // reads as present and selects nothing.
                    if let Some(pos) = item.pos.as_mut() {
                        apply_offsets(pos, line_starts);
                    }
                    fill_offsets(&mut item.children, line_starts);
                    if let (Some(pos), Some(last)) = (
                        item.pos.as_mut(),
                        item.children
                            .iter()
                            .rev()
                            .find_map(crate::ast_json::block_pos)
                            .copied(),
                    ) {
                        pos.end_line = last.end_line;
                        pos.end_column = last.end_column;
                        pos.end_offset = last.end_offset;
                    }
                }
            }
            BlockNode::Table(t) => apply_table_offsets(t, line_starts),
            BlockNode::LineBlock(l) => fill_offsets(&mut l.children, line_starts),
            BlockNode::DefinitionList(d) => {
                for item in &mut d.items {
                    for term in &mut item.terms {
                        // The TERM's own span, not only its inline content. This
                        // walk reached the children and skipped the node, so a
                        // `<dt>` carried line and column with offsets of 0..0 -
                        // present, and selecting nothing. Same for `<dd>` below.
                        if let Some(pos) = term.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        apply_inline_offsets(&mut term.children, line_starts);
                    }
                    for def in &mut item.definitions {
                        if let Some(pos) = def.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        fill_offsets(&mut def.children, line_starts);
                    }
                }
            }
            BlockNode::Figure(f) => {
                // The CAPTION first. It was the one part of a figure this walk
                // never reached, so a caption's inline spans kept line and
                // column but offsets of 0..0 - which reads as present and
                // selects nothing, the exact shape section 4 forbids.
                apply_inline_offsets(&mut f.caption, line_starts);
                match &mut *f.target {
                    FigureTarget::BlockQuote(q) => {
                        // The quote's OWN span, which this arm skipped while
                        // filling everything inside it: it kept line and column
                        // and offsets of 0..0, so the quote selected nothing and
                        // every block within it fell outside its own parent
                        // (carve#565). Same defect the code-block arm below
                        // records, one target along.
                        if let Some(pos) = q.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        fill_offsets(&mut q.children, line_starts);
                    }
                    FigureTarget::Paragraph(p) => {
                        if let Some(pos) = p.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        apply_inline_offsets(&mut p.children, line_starts);
                    }
                    FigureTarget::Table(t) => apply_table_offsets(t, line_starts),
                    FigureTarget::Image(i) => {
                        if let Some(pos) = i.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                    }
                    // A code block was reached by the catch-all below and so
                    // kept offsets of 0..0 - present, and selecting nothing.
                    // The arms are exhaustive now: a target added later fails to
                    // compile rather than silently reporting an empty span.
                    FigureTarget::CodeBlock(c) => {
                        if let Some(pos) = c.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                    }
                }
            }
            BlockNode::FigureGroup(g) => {
                if let Some(caption) = &mut g.caption {
                    apply_inline_offsets(caption, line_starts);
                }
                fill_offsets(&mut g.children, line_starts);
            }
            _ => {}
        }
    }
}

fn include_comment_indentation(blocks: &mut [BlockNode], source: &str, line_starts: &[usize]) {
    let source_lines: Vec<&str> = source.lines().collect();
    fn walk(blocks: &mut [BlockNode], lines: &[&str], starts: &[usize]) {
        for block in blocks {
            if let BlockNode::Comment(comment) = block {
                if let Some(pos) = &mut comment.pos {
                    let raw = lines
                        .get(pos.start_line.saturating_sub(1))
                        .copied()
                        .unwrap_or("");
                    if raw.trim_start_matches([' ', '\t']).starts_with('%') && leading_ws(raw) == 1
                    {
                        pos.start_column = 1;
                        pos.start_offset = starts
                            .get(pos.start_line.saturating_sub(1))
                            .copied()
                            .unwrap_or(pos.start_offset);
                    }
                }
            }
            match block {
                BlockNode::BlockQuote(n) => walk(&mut n.children, lines, starts),
                BlockNode::Div(n) => walk(&mut n.children, lines, starts),
                BlockNode::Admonition(n) => walk(&mut n.children, lines, starts),
                BlockNode::FigureGroup(n) => walk(&mut n.children, lines, starts),
                BlockNode::List(n) => {
                    for item in &mut n.items {
                        walk(&mut item.children, lines, starts);
                    }
                }
                BlockNode::LineBlock(n) => walk(&mut n.children, lines, starts),
                BlockNode::DefinitionList(n) => {
                    for item in &mut n.items {
                        for def in &mut item.definitions {
                            walk(&mut def.children, lines, starts);
                        }
                    }
                }
                BlockNode::Figure(n) => {
                    if let FigureTarget::BlockQuote(q) = &mut *n.target {
                        walk(&mut q.children, lines, starts);
                    }
                }
                BlockNode::Extension(n) => walk(&mut n.children, lines, starts),
                _ => {}
            }
        }
    }
    walk(blocks, &source_lines, line_starts);
}

/// Widen a container over the DEFINITION it hosted.
///
/// A footnote definition is lifted out of the body before the block parser
/// runs, and only ONE invisible placeholder is left behind, on the line the
/// definition opened - padded, but not always to the same width the author
/// wrote, and never covering the body's continuation lines. So a container
/// that hosted a definition can end short of the source it consumed:
/// `- a` / `  [^f]: t` / `    more` reported the list as ending on line 2, and
/// `- > [^f]: t` ended the item at the placeholder rather than at the end of
/// the line. carve-js and carve-php reach the definition's end in both, and
/// PART 12 section 4 is markup-inclusive (markup-carve/carve#913): a
/// container's extent has to cover the source it consumed.
///
/// Only a block whose span ENDS ON the definition's own line is widened. That
/// placeholder is the last line such a block consumed, so it is precisely the
/// block that consumed the definition; a block that ended earlier never
/// reached it, and a block that ended later already covers it. This is why the
/// list widens in the first example while its item, which ends on line 1, does
/// not - the same split carve-js reports.
///
/// Runs after `fill_offsets`, because a definition's own end is only known
/// once its body has been placed. Nothing re-derives a span from its children
/// afterwards, so the widening is not undone.
fn widen_over_hosted_definitions(blocks: &mut [BlockNode], def_pos: &BTreeMap<String, Pos>) {
    // Keyed by the line the definition OPENS on, which is the line its
    // placeholder stands on. Two definitions cannot open on one line.
    //
    // A SINGLE-LINE definition is kept too. It looks like it could never
    // widen anything - the definition ends where its own line ends, and so
    // does the container - but the placeholder is not always as wide as the
    // line it replaced, and then the container ends inside its own last line.
    // Filtering these out left `- > [^f]: t` reporting the item and the quote
    // as ending at column 8 of an 11-column line.
    let ends: HashMap<usize, Pos> = def_pos.values().map(|pos| (pos.start_line, *pos)).collect();
    if ends.is_empty() {
        return;
    }
    fn widen(pos: &mut Pos, ends: &HashMap<usize, Pos>) {
        let Some(def) = ends.get(&pos.end_line) else {
            return;
        };
        if def.end_offset <= pos.end_offset {
            return;
        }
        pos.end_line = def.end_line;
        pos.end_column = def.end_column;
        pos.end_offset = def.end_offset;
    }
    fn walk(blocks: &mut [BlockNode], ends: &HashMap<usize, Pos>) {
        for block in blocks {
            if let Some(pos) = block_pos_mut(block) {
                widen(pos, ends);
            }
            match block {
                BlockNode::BlockQuote(n) => walk(&mut n.children, ends),
                BlockNode::Div(n) => walk(&mut n.children, ends),
                BlockNode::Admonition(n) => walk(&mut n.children, ends),
                BlockNode::FigureGroup(n) => walk(&mut n.children, ends),
                BlockNode::List(n) => {
                    for item in &mut n.items {
                        if let Some(pos) = item.pos.as_mut() {
                            widen(pos, ends);
                        }
                        walk(&mut item.children, ends);
                    }
                }
                BlockNode::LineBlock(n) => walk(&mut n.children, ends),
                BlockNode::DefinitionList(n) => {
                    for item in &mut n.items {
                        for def in &mut item.definitions {
                            if let Some(pos) = def.pos.as_mut() {
                                widen(pos, ends);
                            }
                            walk(&mut def.children, ends);
                        }
                    }
                }
                BlockNode::Figure(n) => {
                    if let FigureTarget::BlockQuote(q) = &mut *n.target {
                        walk(&mut q.children, ends);
                    }
                }
                BlockNode::Extension(n) => walk(&mut n.children, ends),
                _ => {}
            }
        }
    }
    walk(blocks, &ends);
}

/// Turn the line/column pair already on a span into codepoint offsets.
fn apply_offsets(pos: &mut Pos, line_starts: &[usize]) {
    if let Some(start) = line_starts.get(pos.start_line.saturating_sub(1)) {
        pos.start_offset = start + pos.start_column.saturating_sub(1);
    }
    if let Some(end) = line_starts.get(pos.end_line.saturating_sub(1)) {
        pos.end_offset = end + pos.end_column.saturating_sub(1);
    }
    if line_starts.first() == Some(&1) {
        if pos.start_line == 1 {
            pos.start_column += 1;
        }
        if pos.end_line == 1 {
            pos.end_column += 1;
        }
    }
}

/// Fill offsets for a table: the rows' and cells' OWN spans, and the inline
/// content of the caption and of every cell.
///
/// Both halves in one place, because they were added a week apart and the
/// second nearly dropped the first: a span whose offsets are never filled stays
/// 0..0, which reads as present and selects nothing.
fn apply_table_offsets(table: &mut Table, line_starts: &[usize]) {
    if let Some(caption) = &mut table.caption {
        apply_inline_offsets(caption, line_starts);
    }
    for row in &mut table.rows {
        if let Some(pos) = row.pos.as_mut() {
            apply_offsets(pos, line_starts);
        }
        for cell in &mut row.cells {
            if let Some(pos) = cell.pos.as_mut() {
                apply_offsets(pos, line_starts);
            }
            apply_inline_offsets(&mut cell.children, line_starts);
        }
    }
}

fn apply_inline_offsets(nodes: &mut [InlineNode], line_starts: &[usize]) {
    for node in nodes {
        if let Some(pos) = inline_pos_mut(node) {
            apply_offsets(pos, line_starts);
        }
        match node {
            InlineNode::Emphasis(e) => apply_inline_offsets(&mut e.children, line_starts),
            InlineNode::Link(l) => apply_inline_offsets(&mut l.children, line_starts),
            InlineNode::Span(s) => apply_inline_offsets(&mut s.children, line_starts),
            InlineNode::Extension(e) => apply_inline_offsets(&mut e.children, line_starts),
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        apply_inline_offsets(prefix, line_starts);
                    }
                    if let Some(locator) = &mut item.locator {
                        apply_inline_offsets(locator, line_starts);
                    }
                    if let Some(suffix) = &mut item.suffix {
                        apply_inline_offsets(suffix, line_starts);
                    }
                }
            }
            InlineNode::Footnote(f) => {
                if let Some(inline) = &mut f.inline {
                    apply_inline_offsets(inline, line_starts);
                }
            }
            InlineNode::CriticInsert(c) => apply_inline_offsets(&mut c.children, line_starts),
            InlineNode::CriticDelete(c) => apply_inline_offsets(&mut c.children, line_starts),
            _ => {}
        }
    }
}

fn inline_pos_mut(node: &mut InlineNode) -> Option<&mut Pos> {
    match node {
        InlineNode::Text(n) => n.pos.as_mut(),
        InlineNode::EscapedText(n) => n.pos.as_mut(),
        InlineNode::SmartPunctuation(n) => n.pos.as_mut(),
        InlineNode::Emphasis(n) => n.pos.as_mut(),
        InlineNode::Code(n) => n.pos.as_mut(),
        InlineNode::Link(n) => n.pos.as_mut(),
        InlineNode::Image(n) => n.pos.as_mut(),
        InlineNode::Span(n) => n.pos.as_mut(),
        InlineNode::Math(n) => n.pos.as_mut(),
        InlineNode::RawInline(n) => n.pos.as_mut(),
        InlineNode::LiteralInline(n) => n.pos.as_mut(),
        InlineNode::Symbol(n) => n.pos.as_mut(),
        InlineNode::AutoLink(n) => n.pos.as_mut(),
        InlineNode::CrossRef(n) => n.pos.as_mut(),
        InlineNode::CaptionNumber(n) => n.pos.as_mut(),
        InlineNode::Mention(n) => n.pos.as_mut(),
        InlineNode::Tag(n) => n.pos.as_mut(),
        InlineNode::CitationGroup(n) => n.pos.as_mut(),
        InlineNode::Extension(n) => n.pos.as_mut(),
        InlineNode::Abbreviation(n) => n.pos.as_mut(),
        InlineNode::Footnote(n) => n.pos.as_mut(),
        InlineNode::SoftBreak(n) | InlineNode::HardBreak(n) => n.pos.as_mut(),
        InlineNode::CriticInsert(n) => n.pos.as_mut(),
        InlineNode::CriticDelete(n) => n.pos.as_mut(),
        InlineNode::CriticSubstitute(n) => n.pos.as_mut(),
        InlineNode::Comment(n) => n.pos.as_mut(),
        InlineNode::CriticComment(n) => n.pos.as_mut(),
    }
}

fn owned_inline_pos(node: &InlineNode) -> Option<Pos> {
    let mut cloned = node.clone();
    inline_pos_mut(&mut cloned).copied()
}

/// Codepoint offset of the start of each line.
/// Line-start offsets in the ORIGINAL text, in codepoints.
///
/// Splits on every `newline` the grammar admits - '\n', '\r\n' and a lone
/// '\r' - so the entry count matches the normalized line count, and skips a
/// leading BOM so line 0 starts at the first real character rather than at the
/// mark (carve#876).
pub(crate) fn original_line_start_offsets(source: &str) -> Vec<usize> {
    let mut chars = source.chars().peekable();
    let mut starts = Vec::new();
    let mut count = 0usize;
    if chars.peek() == Some(&'\u{feff}') {
        chars.next();
        count += 1;
    }
    starts.push(count);
    while let Some(ch) = chars.next() {
        count += 1;
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
                count += 1;
            }
            starts.push(count);
        } else if ch == '\n' {
            starts.push(count);
        }
    }

    starts
}

fn line_start_offsets(source: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    let mut count = 0usize;
    for ch in source.chars() {
        count += 1;
        if ch == '\n' {
            starts.push(count);
        }
    }
    starts
}

fn parse_mapped_source(source: &MappedSource, options: &Options<'_>) -> Vec<BlockNode> {
    parse_mapped_source_at_level(source, options, false)
}

fn parse_mapped_source_at_document_level(
    source: &MappedSource,
    options: &Options<'_>,
) -> Vec<BlockNode> {
    parse_mapped_source_at_level(source, options, true)
}

fn parse_mapped_source_at_level(
    source: &MappedSource,
    options: &Options<'_>,
    at_document_level: bool,
) -> Vec<BlockNode> {
    if !options.source_lines && !options.positions {
        if let Some(blocks) = parse_eof_closed_colon_ladder(source, options) {
            return blocks;
        }
        return parse_blocks_with_options_at_level(&source.source, options, at_document_level);
    }
    let lines: Vec<&str> = source.source.lines().collect();
    // The mapped source carries the widths its container already stripped, so a
    // nested block's column is measured against the document, not against the
    // rewritten text the parser sees.
    let mut cursor = LineCursor::new_with_cols(
        &lines,
        Some(&source.line_map),
        options.positions.then_some(source.col_map.as_slice()),
    );
    cursor.at_document_level = at_document_level;
    parse_blocks(&mut cursor, options)
}

fn parse_eof_closed_colon_ladder(
    source: &MappedSource,
    options: &Options<'_>,
) -> Option<Vec<BlockNode>> {
    let lines: Vec<&str> = source.source.lines().collect();
    if lines.is_empty()
        || lines
            .iter()
            .any(|line| exact_colon_fence_len(line).is_some())
    {
        return None;
    }

    let mut opens = Vec::new();
    for line in &lines {
        if line.starts_with([' ', '\t']) {
            break;
        }
        let Some(open) = detect_container_open(line) else {
            break;
        };
        opens.push(open);
    }
    if opens.is_empty() {
        return None;
    }

    let current_depth = NESTING_DEPTH.with(Cell::get);
    let available = MAX_NESTING_DEPTH.saturating_sub(current_depth);
    let take = opens.len().min(available);

    // PART 9 §4c along this fast path too: the outermost bare `::: figure`
    // opener not already inside a group is a composite figure; every bare
    // opener under it stays a generic container (groups do not nest). Walked
    // outer-in, seeded from the thread state, so a ladder inside an open
    // group's body demotes exactly as the ordinary path would.
    let mut in_group = IN_FIGURE_GROUP.with(Cell::get);
    let mut opens_a_group: Vec<bool> = Vec::with_capacity(take);
    for open in opens.iter().take(take) {
        let bare = is_bare_figure_open(open) && !in_group;
        opens_a_group.push(bare);
        in_group = in_group || bare;
    }

    let tail = &lines[take..];
    let mut children = if tail.iter().all(|line| is_blank_line(line)) {
        Vec::new()
    } else {
        let _guard = in_group.then(FigureGroupGuard::enter);
        if opens.len() > available {
            // PART 9 §25: past the cap an opener "becomes literal paragraph
            // text" - it degrades, it does not vanish. This used to locate the
            // first non-opener line and keep only the body from there, so every
            // opener past the cap was discarded with no text, no marker and no
            // diagnostic: the output for 205 openers and for 8000 was
            // byte-identical, which made the amount dropped invisible
            // (carve-rs#418). carve-php keeps them, and now so does this.
            // §25: the flattened run and the text after it are ONE paragraph
            // "ending at the first blank line like any other". Joining the whole
            // tail put a literal blank line inside a paragraph - which nothing
            // else in the language can hold - and swallowed the block after it
            // (carve-rs#530).
            // No maps: this path is only reached with positions OFF (its
            // guard is `!options.source_lines && !options.positions`), so
            // there is nothing to place and nothing to lose.
            flattened_paragraphs(tail, None, options)
        } else {
            let tail_source = tail.join("\n");
            parse_blocks_with_options(&tail_source, options)
        }
    };

    for (open, opens_group) in opens.into_iter().take(take).zip(opens_a_group).rev() {
        if opens_group {
            // An EOF-closed group never wrote its closer, so it has no line
            // for a caption to hang on (§4c).
            children = vec![BlockNode::FigureGroup(FigureGroup {
                attrs: open.attrs,
                children,
                caption: None,
                pos: None,
            })];
            continue;
        }
        children = vec![if let Some(kind) = open.kind {
            BlockNode::Admonition(Admonition {
                attrs: open.attrs,
                kind,
                title: open
                    .title
                    .map(|title| parse_inline_with_options(&title, options)),
                label: open.label,
                children,
                pos: None,
            })
        } else {
            BlockNode::Div(Div {
                attrs: open.attrs,
                label: open.label,
                children,
                pos: None,
            })
        }];
    }

    Some(children)
}

/// Inline-parse flattened over-cap text with the depth budget handed back.
///
/// The block and inline parsers share `NESTING_DEPTH`, so AT the cap the inline
/// pass refuses too and returns the run verbatim - escapes included, so a
/// canonical `\\:\\: x` published its backslashes where every other engine
/// publishes `:: x`, and `fmt` stopped round-tripping the corpus document that
/// reaches the cap. Flattening is the LAST step at that depth: nothing recurses
/// after it, so the inline pass can have the budget back (carve-rs#530).
fn parse_flattened_inline(text: &str, options: &Options<'_>) -> Vec<InlineNode> {
    let saved = NESTING_DEPTH.with(Cell::get);
    NESTING_DEPTH.with(|d| d.set(0));
    let out = parse_inline_with_options(text, options);
    NESTING_DEPTH.with(|d| d.set(saved));
    out
}

/// `parse_flattened_inline` with the per-line anchors that place what it
/// returns. Same budget handling, for the same reason.
///
/// A flattened run is not REASSEMBLED in PART 12 §4's sense: its lines are
/// contiguous and verbatim, so every node in it has an honest span. Only the
/// anchors were missing (carve-rs#716).
fn parse_flattened_inline_with_anchors(
    text: &str,
    options: &Options<'_>,
    anchors: Vec<Option<(usize, isize)>>,
) -> Vec<InlineNode> {
    let saved = NESTING_DEPTH.with(Cell::get);
    NESTING_DEPTH.with(|d| d.set(0));
    let out = parse_inline_lines_with_anchor(text, options, anchors);
    NESTING_DEPTH.with(|d| d.set(saved));
    out
}

/// The source line and stripped-column maps for a run of lines, parallel to the
/// lines themselves. `None` where the caller has none to give.
///
/// This is what a `LineCursor` carries per line; a flattened over-cap run has
/// already been lifted out of one, so its two maps travel together instead.
type LineMaps<'a> = Option<(&'a [Option<usize>], &'a [Option<isize>])>;

/// Span of the flattened lines `[start, end)`, in the ORIGINAL source.
///
/// The line-column pair per body line is what a `LineCursor` would carry; here
/// the run has already been lifted out of one, so the maps are passed directly.
/// The arithmetic is `span_of`'s, deliberately: a flattened paragraph is an
/// ordinary paragraph that happened to be built past the cap, and a second
/// spelling of the same span is how the two drift apart.
fn flattened_span(lines: &[&str], maps: LineMaps<'_>, start: usize, end: usize) -> Option<Pos> {
    let (line_map, col_map) = maps?;
    let last = end.saturating_sub(1).max(start);
    let start_line = *line_map.get(start)?.as_ref()?;
    let end_line = line_map.get(last).copied().flatten().unwrap_or(start_line);
    let stripped = *col_map.get(start)?.as_ref()?;
    let end_stripped = *col_map.get(last)?.as_ref()?;
    let first = lines.get(start)?;
    // Space and tab, the same reason as in `span_of`: a leading no-break space
    // is content the span must cover, not indentation in front of it.
    //
    // UNREACHABLE TODAY, and recorded as such rather than presented as a fix.
    // A flattened run begins at the line that exceeded the nesting cap, and
    // that line is a marker line by construction - `:::`, `>`, a bullet - so
    // this indent is zero in every shape reachable from the parser. Mutating it
    // back to the Unicode property changes no output, which was measured over a
    // colon ladder, a quote ladder, an indented list ladder and an indented
    // body line before writing this. It matches `span_of` because the two are
    // one measurement written twice, and a copy that disagrees with its twin is
    // how the pair diverges the day a caller does reach it
    // (markup-carve/carve#755: a check that cannot fail is recorded, not
    // deleted and not counted as proof).
    let indent = first.chars().count() - trim_ascii_start(first).chars().count();
    let width = lines.get(last).map(|l| l.chars().count()).unwrap_or(0);
    Some(Pos {
        start_line,
        end_line,
        start_column: document_column(stripped, indent),
        end_column: document_column(end_stripped, width),
        start_offset: 0,
        end_offset: 0,
    })
}

/// Split flattened over-cap lines into paragraphs at every blank line (§25).
///
/// `maps` are the source line and stripped-column maps, parallel to `lines`.
/// `None` means the caller has none to give - the EOF-closed ladder builds its
/// tail from a plain slice - and the paragraphs then carry no span, which §4
/// permits only because there is genuinely nothing to report. Every caller that
/// HAS the maps passes them: a run past the cap is contiguous verbatim source,
/// so it has an honest span and omitting it is a gap, not an exemption
/// (carve-rs#716).
fn flattened_paragraphs(
    lines: &[&str],
    maps: LineMaps<'_>,
    options: &Options<'_>,
) -> Vec<BlockNode> {
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < lines.len() {
        while idx < lines.len() && is_blank_line(lines[idx]) {
            idx += 1;
        }
        if idx >= lines.len() {
            break;
        }
        let start = idx;
        while idx < lines.len() && !is_blank_line(lines[idx]) {
            idx += 1;
        }
        let text = lines[start..idx].join("\n");
        // The lines are joined VERBATIM here, unlike `parse_paragraph` which
        // strips each line's indentation first, so a line's anchor is the
        // column its container took and nothing more.
        let children = match (options.positions, maps) {
            (true, Some((line_map, col_map))) => {
                let anchors = (start..idx)
                    .map(|i| {
                        Some((
                            line_map.get(i).copied().flatten()?,
                            col_map.get(i).copied().flatten()?,
                        ))
                    })
                    .collect();
                parse_flattened_inline_with_anchors(&text, options, anchors)
            }
            _ => parse_flattened_inline(&text, options),
        };
        out.push(BlockNode::Paragraph(Paragraph {
            attrs: None,
            children,
            // The `positions` gate is REDUNDANT today and kept deliberately.
            // `parse_mapped_source` withholds the column map from the cursor
            // unless positions were asked for, so every entry reaching here is
            // `None` in that case and `flattened_span` returns `None` on its
            // own. Removing the gate is therefore a GREEN mutation - no test
            // can catch it, because the invariant it guards is established two
            // layers up. It stays because the opt-in contract is worth stating
            // where the field is set, rather than inferred from a distant call
            // (markup-carve/carve#755: a check that cannot fail is recorded as
            // such, not deleted and not presented as proof).
            pos: options
                .positions
                .then(|| flattened_span(lines, maps, start, idx))
                .flatten(),
            ..Default::default()
        }));
    }
    out
}

fn parse_capped_colon_body(inner: LineBuffer, options: &Options<'_>) -> Vec<BlockNode> {
    let source = inner.into_source();
    if NESTING_DEPTH.with(|d| d.get() < MAX_NESTING_DEPTH) {
        return parse_mapped_source(&source, options);
    }

    let lines: Vec<&str> = source.source.lines().collect();
    if lines.iter().all(|line| is_blank_line(line)) {
        return Vec::new();
    }

    flattened_paragraphs(&lines, Some((&source.line_map, &source.col_map)), options)
}

/// For each fence char, `vec[len]` is one past the index of the LAST line that
/// is closer-shaped with a run of at least `len` - suffix-maxed so a single
/// lookup answers "at least this wide", which is what a code fence closer needs
/// (unlike a comment fence, which matches its width exactly).
fn build_code_closer_last_index(lines: &[&str]) -> HashMap<u8, Vec<usize>> {
    let mut per_char: HashMap<u8, Vec<usize>> = HashMap::new();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let bytes = trimmed.as_bytes();
        let Some(&fence_char) = bytes.first() else {
            continue;
        };
        if fence_char != b'`' && fence_char != b'~' {
            continue;
        }
        let run = bytes.iter().take_while(|&&b| b == fence_char).count();
        // The same tail test `is_fence_close` makes, and it has to be: `false`
        // from this index is FINAL, so a tail this prefilter rejects can never
        // be re-examined by the exact scan. A trailing tab is dropped
        // whitespace at a closer (carve#1295), so admitting it here is what
        // lets the scan below ever see the line.
        if !bytes[run..].iter().all(|&b| b == b' ' || b == b'\t') {
            continue;
        }
        let by_len = per_char.entry(fence_char).or_default();
        if by_len.len() <= run {
            by_len.resize(run + 1, 0);
        }
        by_len[run] = by_len[run].max(idx + 1);
    }
    for by_len in per_char.values_mut() {
        for len in (0..by_len.len().saturating_sub(1)).rev() {
            by_len[len] = by_len[len].max(by_len[len + 1]);
        }
    }
    per_char
}

/// Query the index built by `build_code_closer_last_index`. `false` is final;
/// `true` only means an exact scan is worth running.
fn code_closer_exists_after(
    index: &HashMap<u8, Vec<usize>>,
    start: usize,
    fence_char: u8,
    fence_len: usize,
) -> bool {
    index
        .get(&fence_char)
        .and_then(|by_len| by_len.get(fence_len).copied())
        .is_some_and(|last| last > start + 1)
}

fn build_comment_closer_last_index(lines: &[&str]) -> HashMap<usize, usize> {
    let mut last_index = HashMap::new();
    for (idx, line) in lines.iter().enumerate() {
        // Any column: the consumption sites read an indented fence, so the
        // index that tells them whether a closer exists has to see one too.
        if let Some(open) = detect_comment_fence_line_any_column(line) {
            last_index.insert(open.fence_len, idx);
        }
    }
    last_index
}

/// What `take_comment_block` found on the cursor.
enum CommentBlock {
    /// Not a comment line; the cursor has not moved.
    NotAComment,
    /// A comment line was consumed. `None` is the placeholder a collected
    /// definition leaves behind, which produces no node. BOXED because a
    /// `BlockNode` dwarfs the empty variant beside it.
    Consumed(Option<Box<BlockNode>>),
}

/// Take a `%%` line comment or a `%%%` comment fence off the cursor.
///
/// SHARED BY BOTH BLOCK PARSERS on purpose. `parse_blocks` had this and
/// `parse_block` did not, so the same `%% c` was a block comment at top level
/// and a paragraph wrapping an inline comment when a `+` continuation marker
/// attached it - one construct, two shapes, decided by which entry point ran
/// (carve-rs#678). The rendered HTML agrees either way, since a comment renders
/// nothing, which is why only the tree showed it.
fn take_comment_block(cur: &mut LineCursor, options: &Options<'_>) -> CommentBlock {
    let Some(line) = cur.peek() else {
        return CommentBlock::NotAComment;
    };
    if let Some(open) = detect_comment_fence_line_any_column(line) {
        // No matching closer: degrade to the ordinary `%%` line comment path
        // below instead of swallowing to EOF.
        if cur.has_comment_closer_after(cur.pos + 1, open.fence_len) {
            let span_start = cur.pos;
            // A body line's indentation is measured FROM ITS FENCE, not from
            // column 0. Stored absolute, a fence that sits at a column - which
            // it may, since #585 keeps a below-content-column span's own
            // columns - hands the writer a body already carrying the fence's
            // indent, and the writer adds the fence column on top: corpus 186
            // came back with `x` one column deeper than carve-js and carve-php
            // write it (carve-rs#601, markup-carve/carve#653).
            //
            // The strip is capped at each line's own indent, so a body line
            // shallower than its fence keeps what it has rather than eating
            // into its text.
            let fence_indent = indent_columns(line);
            let mut content = Vec::new();
            if !open.tail.is_empty() {
                content.push(open.tail);
            }
            cur.consume();
            while let Some(line) = cur.peek() {
                cur.consume();
                if is_comment_fence_close_any_column(line, open.fence_len) {
                    break;
                }
                content.push(slice_columns(
                    line,
                    fence_indent.min(indent_columns(line)),
                    false,
                ));
            }

            let mut pos = span_of(cur, span_start, cur.pos, options);
            if let Some(pos) = &mut pos {
                pos.start_column = pos.start_column.saturating_sub(leading_ws(line));
            }
            return CommentBlock::Consumed(Some(Box::new(BlockNode::Comment(Comment {
                block: true,
                delimited: false,
                content: content.join("\n"),
                pos,
            }))));
        }
    }
    if trim_ascii_start(line).starts_with("%%") {
        // The line a collected definition left behind is consumed here and
        // produces nothing. It did its work earlier - it was non-blank through
        // collection, so the item did not loosen - and an author never typed
        // it, so it is not a comment node. See DEFINITION_PLACEHOLDER.
        if is_definition_placeholder(line) {
            cur.consume();

            return CommentBlock::Consumed(None);
        }
        let content = trim_ascii_start(line)
            .strip_prefix("%%")
            .unwrap_or_default()
            .trim_start()
            .to_string();
        let span_start = cur.pos;
        cur.consume();

        let mut pos = span_of(cur, span_start, cur.pos, options);
        if let Some(pos) = &mut pos {
            pos.start_column = pos.start_column.saturating_sub(leading_ws(line));
        }
        return CommentBlock::Consumed(Some(Box::new(BlockNode::Comment(Comment {
            block: false,
            delimited: false,
            content,
            pos,
        }))));
    }

    CommentBlock::NotAComment
}

fn parse_blocks(cur: &mut LineCursor, options: &Options<'_>) -> Vec<BlockNode> {
    // Recursion cap (see MAX_NESTING_DEPTH). Over the cap, flatten everything
    // still in the cursor into one paragraph rather than recursing further,
    // matching the carve-php degrade behavior.
    let Some(_depth) = DepthGuard::enter() else {
        let span_start = cur.pos;
        let mut rest: Vec<&str> = Vec::new();
        while let Some(line) = cur.consume() {
            rest.push(line);
        }
        let text = rest.join("\n");
        // Check line-wise: `is_blank_line` only trims spaces/tabs, so a joined
        // multi-line all-blank tail (which contains newlines) must be tested
        // per line, not on the joined string.
        if rest.iter().all(|line| is_blank_line(line)) {
            return Vec::new();
        }
        // The SECOND over-cap producer, and a real one. The colon-fence
        // document that named carve-rs#716 never reaches this branch - it
        // degrades through `parse_capped_colon_body` instead - but a deep quote
        // ladder and a deep list ladder both arrive here with positions on, and
        // published nothing either. The lines are contiguous and still in the
        // cursor, so the span is the ordinary one and the anchors are the
        // ordinary ones; `rest` holds the lines VERBATIM, so a line's anchor is
        // its own stripped column.
        let children = if options.positions {
            let anchors = rest
                .iter()
                .enumerate()
                .map(|(idx, line)| inline_anchor_for_line(cur, span_start + idx, line))
                .collect();
            parse_flattened_inline_with_anchors(&text, options, anchors)
        } else {
            parse_flattened_inline(&text, options)
        };
        return vec![BlockNode::Paragraph(Paragraph {
            attrs: None,
            children,
            pos: span_of(cur, span_start, cur.pos, options),
            ..Default::default()
        })];
    };
    let mut out = Vec::new();
    let mut pending_attrs: Option<Attrs> = None;
    // Where the pending run STARTED, for §15 A4's diagnostic. The FIRST block of
    // a stacked run, because that is where the author began writing attributes
    // that reach nothing - the run merges into one set and is reported once.
    let mut pending_attrs_pos: Option<Pos> = None;
    while !cur.eof() {
        let line = cur.peek().unwrap();
        // A standalone `{attr}` block opener fires only at the container's
        // content column (flush-left here, since the caller has dedented to that
        // column). An INDENTED `{attr}` line does NOT attach to the following
        // block; it folds as literal paragraph text (strict column-0 rule,
        // docs/divergence-from-djot.md §11), matching carve-php / carve-js.
        let line_flush = !line.starts_with([' ', '\t']);
        if is_blank_line(line) {
            cur.consume();
            continue;
        }
        // BOTH comment spellings, shared with `parse_block` so a comment
        // reached through a `+` continuation is the same node as one written at
        // top level. It was not: this loop had the arm and the single-block
        // parser did not, so a `+`-attached `%% c` fell through to a paragraph
        // wrapping an INLINE comment (carve-rs#678).
        match take_comment_block(cur, options) {
            CommentBlock::NotAComment => {}
            CommentBlock::Consumed(None) => continue,
            CommentBlock::Consumed(Some(node)) => {
                out.push(*node);
                continue;
            }
        }
        if line_flush {
            let attrs_start = cur.pos;
            if let Some(attrs) = parse_standalone_attrs_block(cur) {
                merge_attrs(&mut pending_attrs, attrs);
                if pending_attrs_pos.is_none() {
                    pending_attrs_pos = span_of(cur, attrs_start, cur.pos, options);
                }
                continue;
            }
        }
        let start_line = cur.pos;
        if let Some(node) = parse_block(cur, options) {
            let mut node = node;
            // §15 A2a: a floating attribute skips what renders NOTHING and
            // attaches to the next VISIBLE block. The other invisible kinds -
            // comments, reference and footnote definitions - never reach here
            // (they are consumed earlier, leaving the pending attrs alone), so
            // an abbreviation definition was the one that took them and, having
            // nowhere to put them, dropped them: `{#i}` / `*[A]: b` / blank /
            // `e` lost the id where carve-js and the spec publish
            // `<p id="i">e</p>` (carve-rs#511 item 2).
            let renders_nothing =
                matches!(node, BlockNode::AbbreviationDef(_) | BlockNode::Comment(_));
            if !renders_nothing {
                if let Some(attrs) = pending_attrs.take() {
                    apply_attrs_to_block(&mut node, attrs);
                    pending_attrs_pos = None;
                }
            }
            // Resolve a code fence's opener title to the `title` attribute (after
            // the preceding {title=...} line was applied, so that line wins), so
            // the title lives on the node attrs and survives every consumer: the
            // core renderer, a caption Figure, and a FencedRender extension that
            // rewrites the block (it clones the code block's attrs).
            resolve_code_title(&mut node);
            // Stamp blocks with their 1-based original source line for editor
            // preview scroll-sync. Synthetic extracted lines carry no map entry.
            if options.source_lines {
                if let Some(line) = cur.source_line(start_line) {
                    stamp_source_line(&mut node, line);
                }
            }
            out.push(node);
        }
    }
    // DROP IF DANGLING, AND SAY SO (§15 A4, markup-carve/carve#1281). The loop
    // has run out of lines, so there is no next block for this set to float to.
    // This one site covers both ways of running out, because a container's body
    // is parsed by its own call: at document level the input ended, and inside a
    // quote, an item, a `dd` or a footnote body the CONTAINER ended. Nothing is
    // emitted for the attribute either way - what changes is that the processor
    // now reports it rather than discarding it silently.
    if pending_attrs.is_some() {
        note_unattached_block_attrs(pending_attrs_pos);
    }
    out
}

/// True if `attrs` already carries a `title` key (case-insensitive, since HTML
/// attribute names are case-insensitive).
fn attrs_have_title(attrs: &Option<Attrs>) -> bool {
    attrs
        .as_ref()
        .is_some_and(|a| a.key_values.keys().any(|k| k.eq_ignore_ascii_case("title")))
}

/// Copy a code fence's opener `title` to the `title` attribute (unless a
/// preceding `{title=...}` line already set one, which wins). The `title` field
/// is left in place as the source of truth for non-HTML renderers.
fn copy_title_to_attr(cb: &mut CodeBlock) {
    let Some(title) = cb.title.clone() else {
        return;
    };
    if attrs_have_title(&cb.attrs) {
        return;
    }
    let attrs = cb.attrs.get_or_insert_with(Attrs::default);
    attrs.key_values.insert("title".to_string(), title);
    // NO ORDER SLOT. `attrs.order` is the source-appearance order of the slots
    // in a `{#id .class key=value}` block, and this title was written as fence
    // metadata - ``` rust "Example" - not as a slot in one. Recording it
    // claimed a position in a block the author never wrote (carve#785).
    // A title written in a real attribute block still takes its slot, because
    // that one goes through the attribute parser rather than here.
}

/// Resolve a code fence's opener title onto the node attrs so it renders
/// uniformly and survives a caption Figure and a FencedRender extension (which
/// clones the code block's attrs). For a captioned block a `{title=...}` line
/// attaches to the figure and wins, so the inner block's title is dropped.
fn resolve_code_title(node: &mut BlockNode) {
    match node {
        BlockNode::CodeBlock(cb) => copy_title_to_attr(cb),
        BlockNode::Figure(f) => {
            if let FigureTarget::CodeBlock(cb) = &mut *f.target {
                if attrs_have_title(&f.attrs) {
                    cb.title = None;
                } else {
                    copy_title_to_attr(cb);
                }
            }
        }
        _ => {}
    }
}

fn parse_block(cur: &mut LineCursor, options: &Options<'_>) -> Option<BlockNode> {
    // Checked FIRST, and through the same helper `parse_blocks` uses, so a
    // comment reached by a `+` continuation is the node the identical line
    // produces at top level (carve-rs#678).
    match take_comment_block(cur, options) {
        CommentBlock::NotAComment => {}
        CommentBlock::Consumed(node) => return node.map(|node| *node),
    }
    let line = cur.peek()?;
    if let Some(fence_marker) = detect_fence_open(line) {
        let fence_at = cur.pos;
        let block = parse_fence(cur, fence_marker, options);
        // A caption immediately after a fenced code block makes it a numbered
        // LISTING: wrap it in a figure like a captioned image/table.
        if let BlockNode::CodeBlock(cb) = block {
            if let Some(caption) = consume_caption(cur, options) {
                return Some(BlockNode::Figure(Figure {
                    attrs: None,
                    target: Box::new(FigureTarget::CodeBlock(cb)),
                    caption,
                    short_caption: None,
                    // From the opening fence through the end of the caption -
                    // the same extent a captioned image's figure takes.
                    pos: span_of(cur, fence_at, cur.pos, options),
                }));
            }
            return Some(BlockNode::CodeBlock(cb));
        }
        return Some(block);
    }
    if let Some(marker) = thematic_break_marker(line) {
        let span_start = cur.pos;
        cur.consume();
        return Some(BlockNode::ThematicBreak(ThematicBreak {
            marker: (marker != '-').then_some(marker),
            pos: span_of(cur, span_start, cur.pos, options),
            ..Default::default()
        }));
    }
    if let Some((level, first_text)) = detect_heading(line) {
        let span_start = cur.pos;
        cur.consume();
        // SINGLE-LINE HEADINGS (NORMATIVE, diverges from Djot): a heading ENDS
        // AT THE NEWLINE. Nothing folds into it -- not a plain line, not a
        // same-count `#` line -- so the following line begins whatever block it
        // begins, exactly as after any other closed block. Lazy continuation
        // therefore means one thing across the language: it continues an open
        // PARAGRAPH, and a heading is not one.
        let joined = first_text.to_string();
        let anchors = options
            .positions
            .then(|| vec![inline_anchor_for_line(cur, span_start, first_text)]);
        // djot-strict (spec PART 2 headings; matches carve-js #153): a heading
        // line carries NO trailing `{...}` attribute block -- a trailing brace
        // block is ordinary inline content, and the heading id derives from
        // the full literal text. Attributes attach via a PRECEDING
        // block-attribute line (the pending-attrs loop, PART 9 §15).
        // §756 (NORMATIVE): strip the line's trailing whitespace (trim_ascii_end
        // -- ASCII whitespace, so a trailing NBSP survives).
        let pos = span_of(cur, span_start, cur.pos, options);
        let inline_text = trim_ascii_end(&joined);
        let children = if let Some(anchors) = anchors {
            parse_inline_lines_with_anchor(inline_text, options, anchors)
        } else {
            parse_inline_with_options(inline_text, options)
        };
        return Some(BlockNode::Heading(Heading {
            attrs: None,
            level,
            children,
            pos,
        }));
    }
    if strip_blockquote_prefix(line).is_some() {
        return Some(parse_blockquote(cur, options));
    }
    if is_list_marker(line) {
        return Some(parse_list(cur, options));
    }
    // A table row opens a table only when FLUSH-LEFT (like a heading, quote or
    // `:: ` def-list term). `is_table_start` trims leading whitespace, so an
    // INDENTED row (`  |a|`) would otherwise wrongly open a table where the
    // reference renders a paragraph; a genuine table sits at its container's
    // content column and is already dedented to column 0 here.
    if !line.starts_with([' ', '\t']) && is_table_start(line) {
        return Some(parse_table(cur, options));
    }
    if is_definition_list_start(line) {
        return Some(parse_definition_list(cur, options));
    }
    // A `::: |` line block or `::: \` hard-break block opens ONLY flush-left
    // (at its container's content column), exactly like the div / admonition
    // container check below. `detect_line_block_open` and
    // `detect_hardbreaks_block_open` trim leading whitespace, so an INDENTED
    // colon fence (above the content column) would otherwise still open; the
    // strict column-0 rule (docs/divergence-from-djot.md §11) requires it to
    // fold as literal paragraph text instead. `line` is already dedented to
    // the content column here, so a fence sitting AT that column still opens.
    if !line.starts_with([' ', '\t']) {
        if detect_line_block_open(line).is_some() {
            return Some(parse_line_block(cur, options));
        }
        if detect_hardbreaks_block_open(line).is_some() {
            return Some(parse_hardbreaks_block(cur, options));
        }
    }
    // FLUSH-LEFT only: `detect_container_open` trims leading whitespace, so an
    // indented `::: note` below/above a list item's content column must fold as
    // lazy paragraph text (§24 C3), not open a nested container -- uniform with
    // the quote/heading/table checks. `line` is already dedented to the content
    // column here, so a `:::` at the content column opens.
    if !line.starts_with([' ', '\t']) && detect_container_open(line).is_some() {
        return Some(parse_container(cur, options));
    }
    if cur.at_document_level {
        if let Some(mut abbr) = detect_abbreviation_def(line) {
            let abbr_at = cur.pos;
            cur.consume();
            // Its own line, the same way the block image below takes one. #517
            // started publishing this node and it went out with no span at all,
            // which PART 12 §4 allows only for a node that was reassembled and has
            // no honest one - a definition the author wrote on line 1 has one.
            abbr.pos = span_of(cur, abbr_at, abbr_at + 1, options);
            return Some(BlockNode::AbbreviationDef(abbr));
        }
    }
    if let Some(mut img) = detect_block_image(line) {
        if image_is_block(cur) {
            let image_at = cur.pos;
            cur.consume();
            // The image's own line. An INLINE image gets its span from the
            // inline parser; a block image never goes through it, so it had
            // none at all.
            img.pos = span_of(cur, image_at, image_at + 1, options);
            if let Some(caption) = consume_caption(cur, options) {
                return Some(BlockNode::Figure(Figure {
                    attrs: None,
                    target: Box::new(FigureTarget::Image(img)),
                    caption,
                    short_caption: None,
                    // The figure runs from the image to the end of the caption
                    // the cursor just consumed.
                    pos: span_of(cur, image_at, cur.pos, options),
                }));
            }
            return Some(BlockNode::BlockImage(img));
        }
        // Not standalone: the image folds into a paragraph with the following
        // content (parse_paragraph below); a sole-image paragraph is still
        // promoted to a bare block image afterwards.
    }
    if let Some(matched) = try_extension_block(cur, options) {
        return Some(matched);
    }
    // A block whose sole content is a display-math span (`$$`…``) followed by a
    // caption is a numbered EQUATION. Diverted before the paragraph fallback so
    // parse_paragraph does not fold the caption line into the math paragraph.
    if trim_ascii_start(line).starts_with("$$`") {
        if let Some(eq) = parse_equation_block(cur, options) {
            return Some(eq);
        }
    }
    Some(parse_paragraph(cur, options))
}

/// Parse a standalone display-math line, wrapping it in a figure when a caption
/// follows (a numbered equation). Returns `None` when the line is not solely
/// display math, or when non-blank prose follows with no blank line (so the
/// line belongs to a normal multi-line paragraph instead).
fn parse_equation_block(cur: &mut LineCursor, options: &Options<'_>) -> Option<BlockNode> {
    let line = cur.peek()?;
    let inline_text = trim_ascii_start(line);
    let inline = if options.positions {
        parse_inline_lines_with_anchor(
            inline_text,
            options,
            vec![inline_anchor_for_line(cur, cur.pos, inline_text)],
        )
    } else {
        parse_inline_with_options(inline_text, options)
    };
    if inline.len() != 1 || !matches!(&inline[0], InlineNode::Math(m) if m.display) {
        return None;
    }
    // Non-blank, non-caption prose on the very next line: let parse_paragraph
    // fold the math and that text into one paragraph (preserve existing behavior).
    if let Some(next) = cur.lines.get(cur.pos + 1).copied() {
        if !is_blank_line(next) && caption_content(next).is_none() {
            return None;
        }
    }
    // Standalone display math: consume the line, then attach a caption if one
    // follows (directly or across a single blank line).
    let math_at = cur.pos;
    cur.consume();
    let target = FigureTarget::Paragraph(Paragraph {
        attrs: None,
        children: inline,
        // The paragraph is one line: the display-math line itself. It was built
        // with `..Default::default()`, so it had no span whether or not a
        // caption followed.
        pos: span_of(cur, math_at, math_at + 1, options),
        ..Default::default()
    });
    if let Some(caption) = consume_caption(cur, options) {
        return Some(BlockNode::Figure(Figure {
            attrs: None,
            target: Box::new(target),
            caption,
            short_caption: None,
            // Through the end of the caption, like the listing above.
            pos: span_of(cur, math_at, cur.pos, options),
        }));
    }
    match target {
        FigureTarget::Paragraph(p) => Some(BlockNode::Paragraph(p)),
        _ => unreachable!(),
    }
}

fn detect_heading(line: &str) -> Option<(u8, &str)> {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    if !(1..=6).contains(&hashes) {
        return None;
    }
    if hashes >= bytes.len() || bytes[hashes] != b' ' {
        return None;
    }
    // Skip all spaces after the marker (the delimiter is one-or-more spaces;
    // per the Carve grammar it is a space, not a tab).
    let mut start = hashes;
    while start < bytes.len() && bytes[start] == b' ' {
        start += 1;
    }
    // Return the content VERBATIM (leading tab kept, trailing kept): first-line
    // trailing is interior once continuation lines fold in, so it is stripped
    // only from the final assembled content (§756). The empty gate still tests a
    // trailing-stripped view so `# `, `#  `, `# \t` are not headings.
    let rest = &line[start..];
    if trim_ascii_end(rest).is_empty() {
        return None;
    }
    Some((hashes as u8, rest))
}

/// Any ATX heading marker line (`#`..`######` followed by a space or EOL) —
/// such a line starts a NEW heading rather than continuing the current one.
fn is_heading_marker_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    (1..=6).contains(&hashes) && (hashes == bytes.len() || bytes[hashes] == b' ')
}

/// A line that opens an ATX heading WITH content: `#`..`######`, then a single
/// space, then at least one non-whitespace character. Used to decide whether a
/// list item's marker-line remainder (`- # H`) opens a heading block. Bare (`#`)
/// or whitespace-only (`# `, `#  `) remainders and a tab separator (`#\tH`) are
/// NOT headings here -- they stay inline text, matching carve-js / carve-php on
/// the settled cases (the all-whitespace remainder is tracked separately).
fn heading_content_starts(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut hashes = 0usize;
    while hashes < bytes.len() && bytes[hashes] == b'#' {
        hashes += 1;
    }
    if !(1..=6).contains(&hashes) || bytes.get(hashes) != Some(&b' ') {
        return false;
    }
    line[hashes + 1..].bytes().any(|b| b != b' ' && b != b'\t')
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommentFenceOpen {
    fence_len: usize,
    tail: String,
}

/// A fenced-comment line is a leading run of 3+ `%`; any following text is
/// non-structural tail. The opener tail is preserved as comment content.
/// The last line index at which a comment fence of each length closes.
///
/// An UNTERMINATED comment fence is not a fenced comment: the block parser
/// degrades it to a single-line comment. A pre-pass that treats it as an open
/// fence stays open for the rest of the document, which suppressed every later
/// line block and took the definitions inside them with it.
///
/// Keyed by EXACT length, because that is what `is_comment_fence_close` tests -
/// a `%%%%` line does not close a `%%%` fence. And recorded for any line whose
/// leading `%` run is long enough, not only the bare ones: `%%% trailing` is a
/// closer to that function, so filtering to all-`%` lines missed real closers.
///
/// Precomputed because the forward scan it replaces is quadratic on exactly the
/// input the perf suite guards: `%%% x`, `%%%% x`, ... is all openers and no
/// closers, so every line scans to the end.
fn comment_fence_close_index(lines: &[&str]) -> std::collections::HashMap<usize, usize> {
    let mut last: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (i, line) in lines.iter().enumerate() {
        // The RAW line. This index only ever answers for a TOP-LEVEL opener -
        // the line-block state is entered nowhere else - so a `> %%%` inside a
        // container is not a closer for it, and stripping the prefix made one.
        // Leading whitespace is not part of the delimiter (an indented closer
        // closes an indented opener), but nothing else is stripped: the note
        // above is why a `> %%%` must not answer for a top-level opener.
        let run = trim_ascii_end(trim_ascii_start(line))
            .bytes()
            .take_while(|b| *b == b'%')
            .count();
        if run >= 3 {
            last.insert(run, i);
        }
    }
    last
}

/// The same fence seen from a position that CONSUMES it.
///
/// A comment is recognized at ANY column (carve#624), but the strict form above
/// still decides where a line ENDS an item or OPENS a block, so an indented
/// fence does not close the list it sits in. Reading it only at column 0 where
/// it is consumed left an indented opener to the `%%` line-comment path, which
/// took the opener and the closer one line at a time and rendered everything
/// between them - a comment that hid its delimiters and showed its contents
/// (carve-rs#573, the same defect carve-js#630 reported there).
fn detect_comment_fence_line_any_column(line: &str) -> Option<CommentFenceOpen> {
    detect_comment_fence_line(trim_ascii_start(line))
}

/// The closer counterpart of `detect_comment_fence_line_any_column`.
fn is_comment_fence_close_any_column(line: &str, fence_len: usize) -> bool {
    is_comment_fence_close(trim_ascii_start(line), fence_len)
}

/// The opener as the definition PRE-PASSES may read it: past leading
/// whitespace, and past a list marker on the fence's own line.
///
/// PART 9 §24 S1 places a line by the column it REACHES and never by its first
/// character, and §28 makes a comment fence's body verbatim and invisible
/// wherever the fence sits. Neither clause is scoped to column 0, and the block
/// parser already reads both spellings (it consumes `- %%%` through
/// `marker.content`). The pre-passes read only the strict column-0 form, so a
/// fence indented to a list item's content column was invisible to them and they
/// walked into its body and registered from it: `- item` / `  %%%` /
/// `  [r]: /url` / `  %%%` left the label live in the link table while the
/// comment rendered nothing, so a later `[r][]` resolved against text the author
/// had commented out (markup-carve/carve-rs#1047). A footnote definition there
/// was worse still - it produced a whole endnote section nobody wrote.
///
/// A BLOCKQUOTE prefix is deliberately NOT stripped here. `> %%%` /
/// `> [r]: /url` / `> %%%` registers in carve-js, carve-php and carve-rs alike
/// and only the executable oracle leaves it literal, so that shape is an open
/// cross-engine question rather than this engine's defect; widening the strip
/// would answer it unilaterally.
/// The widening is to a fence AT a live list item's content column, and no
/// further. Both of the shapes just outside it are refused, and both refusals
/// are load-bearing:
///
/// - An indented fence with NO live content column is a top-level comment, and a
///   top-level comment's body may sit BELOW its own fence
///   (`comment_body_is_relative_to_its_fence`). Its real closer can therefore be
///   at a column this line-based pass cannot bound, and guessing mispaired the
///   delimiters of ` %%%` / `x` / `  %%%`: the pass rejected the true pair, took
///   the second delimiter as an opener, and let a later `%%% tail` close it, so
///   the definition in between was swallowed.
/// - A fence BELOW a live content column keeps the item open without being the
///   item's content (§24 C3). Entering opacity there freezes the stale content
///   column across the blank line that actually ends the list, so a line the
///   block parser reads as a top-level paragraph gets stripped to the dead
///   column and registered.
///
/// Returns the fence and the COLUMN its `%` run starts at, which is what bounds
/// its body - see `container_comment_fence_closes`.
/// A comment fence opener, and the column it is written at, when that column
/// reaches a live list item.
///
/// WHICH COLUMN THE FENCE REACHES, not which one is innermost. `- - inner`
/// opens two items on one line and leaves BOTH content columns live, 2 and 4
/// (carve#655); a fence written at 2 is the outer item's, and asking the
/// innermost column alone declined it while the outer item was still open
/// (carve-rs#1054). `reached_by` is the question the definition scan beside this
/// one already asks, so both halves now measure against the same thing.
///
/// The gate that remains is the §24 C3 one: a fence reaching no live column at
/// all is below the container and is not its comment.
fn detect_comment_fence_opener_in_container(
    line: &str,
    columns: &ContentColumns,
) -> Option<(CommentFenceOpen, usize)> {
    let (open, col) = detect_comment_fence_opener_at_any_column(line)?;
    if col > 0 && columns.reached_by(col) == 0 {
        return None;
    }
    Some((open, col))
}

/// What bounds the body of a comment fence a definition pre-pass just opened.
///
/// The two containers end for different reasons, so they take different
/// questions. A COLUMN-scoped fence ends where a later line dedents past the
/// column its container holds. A QUOTE-scoped one ends at a blank line: a
/// blockquote does not survive one, so `> a` / blank / `> b` is two quotes, and
/// a run after the blank wears the same marker while belonging to a different
/// quote. For a quoted scope the blank line is therefore the whole test, where a
/// column scope keeps the dedent test - there a dedented line really can be a
/// lazy continuation.
#[derive(Clone, Copy)]
enum CommentFenceScope {
    /// The column the `%` run starts at. Zero is the document level.
    Column(usize),
    /// The blockquote markers the `%` run was written behind: how many, and the
    /// column the first of them starts at.
    Quoted { depth: usize, col: usize },
}

impl CommentFenceScope {
    fn quote_depth(self) -> usize {
        match self {
            CommentFenceScope::Quoted { depth, .. } => depth,
            CommentFenceScope::Column(_) => 0,
        }
    }
}

/// The fence a definition pre-pass opens, together with the scope bounding it.
///
/// `detect_comment_fence_opener_in_container` walks past a list marker but not
/// past a blockquote marker, so `> %%%` was not an opener to either pre-pass at
/// all - they walked into its body and registered from it. §28 makes a comment
/// fence's body verbatim wherever the fence sits and §24 C3 states the rule for
/// both delimiter spellings; neither is scoped by container, so nothing
/// distinguishes a fence reached through a `>` prefix from one reached through
/// indentation, and the block parser consumed the quoted one all along. That
/// left `> %%%` / `> [r]: /url` / `> %%%` with `r` live in the link table while
/// the comment rendered nothing (markup-carve/carve#1341).
fn detect_comment_fence_opener_scoped(
    line: &str,
    columns: &ContentColumns,
) -> Option<(CommentFenceOpen, CommentFenceScope)> {
    let (depth, col, quoted) = prepass_quote_scope(line);
    if depth > 0 {
        // Inside the quote the delimiter is read at any column, the same way the
        // column-scoped opener above reads it - the quote is the container, so
        // its own indentation does not have to reach anything further.
        let (open, _) = detect_comment_fence_opener_at_any_column(quoted)?;
        return Some((open, CommentFenceScope::Quoted { depth, col }));
    }
    let (open, col) = detect_comment_fence_opener_in_container(line, columns)?;
    Some((open, CommentFenceScope::Column(col)))
}

/// A comment fence a definition pre-pass has entered: its width, and the quote
/// depth its opener was written at so a closer can be read at the same depth.
#[derive(Clone, Copy)]
struct OpenCommentFence {
    fence_len: usize,
    quote_depth: usize,
}

/// Does `line` close `open`, read at the depth `open` was written at?
///
/// Depth 0 is `is_comment_fence_close_any_column` unchanged. Deeper, the markers
/// come off first, which is what `PrepassFenceTracker` in carve-php and the
/// quoted-code-fence closer in `extract_link_defs` already do: a closer is a
/// continuation line of the container its opener sits in, so it wears the same
/// prefix. A `> > %%%` therefore stays quoted comment content rather than
/// closing a `> %%%`.
fn closes_open_comment_fence(line: &str, open: OpenCommentFence) -> bool {
    let mut rest = line;
    for _ in 0..open.quote_depth {
        let Some(inner) = strip_prepass_blockquote_prefix(rest) else {
            return false;
        };
        rest = inner;
    }
    is_comment_fence_close_any_column(rest, open.fence_len)
}

/// Does the fence opened at `open_at` close inside the container bounding it?
///
/// The document level is the only scope the RAW closer index answers for, and
/// the only one it can: it reads raw lines, so a `> %%%` matches nothing in it,
/// and a `%%%` written back at column 0 is not the closer of a fence a container
/// holds. It was asked FIRST for every scope all the same, as a gate the bounded
/// scan then had to agree with, and that is what made a quoted fence read as
/// unterminated to the pre-passes alone. It is also what made the leak
/// contingent on unrelated text: a stray column-0 `%%%` later in the document
/// got the same quoted fence past the gate, and it then answered correctly -
/// which is why the already-pinned spellings passed while the reported one did
/// not (markup-carve/carve#1341).
///
/// A column-scoped query is unchanged by dropping the gate. For depth 0 the
/// bounded index holds exactly the runs the raw one does, so the gate was
/// redundant there rather than load-bearing.
fn comment_fence_scope_closes(
    scope: CommentFenceScope,
    lines: &[&str],
    open_at: usize,
    fence_len: usize,
    columns: &ContentColumns,
    raw_closers: &HashMap<usize, usize>,
    bounded: &mut Option<ContainerCommentClosers>,
) -> bool {
    match scope {
        CommentFenceScope::Column(0) => raw_closers
            .get(&fence_len)
            .is_some_and(|close_at| *close_at > open_at),
        CommentFenceScope::Column(col) => bounded
            .get_or_insert_with(|| ContainerCommentClosers::build(lines))
            .closes_in_column(lines, open_at, columns.reached_by(col), fence_len),
        CommentFenceScope::Quoted { depth, col } => bounded
            .get_or_insert_with(|| ContainerCommentClosers::build(lines))
            .closes_in_quote(lines, open_at, depth, col, fence_len),
    }
}

fn detect_comment_fence_opener_at_any_column(line: &str) -> Option<(CommentFenceOpen, usize)> {
    let mut rest = line;
    let mut col = 0usize;
    loop {
        let trimmed = trim_ascii_start(rest);
        col = advance_columns(&rest[..rest.len() - trimmed.len()], col);
        rest = trimmed;
        let Some(marker) = detect_list_marker_full(rest) else {
            break;
        };
        let consumed = marker.content.as_ptr() as usize - rest.as_ptr() as usize;
        if consumed == 0 {
            break;
        }
        col = advance_columns(&rest[..consumed], col);
        rest = marker.content;
    }
    detect_comment_fence_line(rest).map(|open| (open, col))
}

/// `indent_columns`, continued from a column already reached. Tab stops are
/// measured from the start of the line, so a marker's own width has to be walked
/// rather than counted in bytes.
fn advance_columns(prefix: &str, mut col: usize) -> usize {
    for byte in prefix.bytes() {
        if byte == b'\t' {
            col += 4 - (col % 4);
        } else {
            col += 1;
        }
    }
    col
}

// Counts the lines the container-comment dedent walk visits.
//
// The whole point of the index below is that this stays proportional to the
// DOCUMENT rather than to openers times container length, and a count is the
// only way to state that without a clock - see the note on `quote_prefix_calls`
// for why this repo counts work instead of timing it. Test-only, so a release
// build carries nothing.
#[cfg(test)]
thread_local! {
    pub(crate) static CONTAINER_DEDENT_STEPS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

/// Does a comment fence opened INSIDE a container close before that container
/// ends?
///
/// `comment_fence_close_index` answers "is there a closer of this length
/// anywhere later", which is the whole question for a column-0 opener: nothing
/// bounds its body but the end of input. A fence inside a container is bounded
/// by the container, and a `%%%` written back at column 0 does not close it -
/// the block parser has ended the item long before, and reads the indented fence
/// as an unterminated one-line comment instead. Entering the fence state on that
/// far closer swallowed everything in between, so an item holding `%%%` and
/// `hidden`, then a blank, then `[r]: /url`, then a blank and a column-0 `%%%`,
/// lost a definition that carve-rs and the oracle both register. That is a worse
/// defect than the one the container-aware opener fixes, which is why the bound
/// is part of the same change. (carve-js loses that definition today - a
/// separate divergence, not something to reproduce here.)
///
/// The container's extent is approximated the way the rest of this line-based
/// pre-pass approximates: the first non-blank line that dedents past the fence's
/// own column ends it, and blank lines are transparent. Only reached for a fence
/// that is not at column 0, so a column-0 opener costs exactly what it did
/// before.
///
/// INDEXED rather than scanned, for the reason `comment_fence_close_index`
/// exists: one container can hold many openers, and walking it once per opener
/// is the quadratic shape this file's perf suite guards. `m` fence widths above
/// `m * m` filler lines, with every matching closer only past the dedent, made
/// each opener walk the whole container - O(m^3) work for an O(m^2) document,
/// measured at 3x on 1.8 MB and widening with size.
///
/// Two facts answer the question without the walk. `closer` is the first
/// comment-fence closer of this exact width after the opener, from an index
/// built once. `dedent` is the first non-blank line after it whose indent falls
/// below the fence's column. The fence closes inside its container exactly when
/// `closer < dedent`: a closer past the dedent is outside the container, and a
/// dedented line that happens to be a closer is outside it too.
///
/// `dedent` is memoized per COLUMN, which is what makes a run of openers at one
/// column cost a single walk between them rather than one each: once the first
/// dedent below column `k` after line `p` is known, it is still the answer for
/// every query point in `p..dedent`. Openers at different columns keep separate
/// entries, so alternating columns do not evict each other.
///
/// A list MARKER inside the body is not a stop, and the block parser now agrees.
/// It used to end a contained comment at one, so `- item` / `  %%%` / `  - x` /
/// `  y` / `  %%%` rendered `x` and `y` where the oracle and carve-js render an
/// empty item. This scan stayed with the clause through that, which made the two
/// halves disagree: the definition stopped registering while the body still
/// leaked. carve-rs#1053 fixed the block parser's side - its content-column
/// marker gate now treats an open comment span as opaque, the way it already
/// treated a code fence - so both halves answer §28 the same way and the shape
/// is correct end to end.
#[derive(Default)]
struct ContainerCommentClosers {
    /// Line index of every comment-fence closer, keyed by its exact `%` run AND
    /// the quote depth it was written at, ascending within each key.
    by_width: HashMap<(usize, usize), Vec<usize>>,
    /// column -> (query point the answer was computed from, the answer).
    dedents: HashMap<usize, (usize, usize)>,
    /// (query point the answer was computed from, the answer), for the blank
    /// line that ends a quote. One entry rather than a map because the question
    /// carries no column.
    blank: Option<(usize, usize)>,
}

impl ContainerCommentClosers {
    fn build(lines: &[&str]) -> Self {
        let mut by_width: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (index, line) in lines.iter().enumerate() {
            // The same reading `comment_fence_close_index` uses, taken once per
            // quote DEPTH: leading whitespace is not part of the delimiter, a
            // tail is ignored, and nothing but the blockquote markers comes off.
            //
            // Depth is half the key because stripping the markers collapses
            // `> > %%%` and `> %%%` onto one run, and they close different
            // fences. A run shallower than the opener is the line that ENDED the
            // opener's quote; a deeper one is inside a quote of its own, which
            // the block parser reads as one. Accepting either would suppress a
            // definition the block parser publishes.
            //
            // Depth 0 is exactly the set this index held before depth was
            // tracked, so every column-scoped query answers as it did.
            let (depth, _, rest) = prepass_quote_scope(line);
            let run = trim_ascii_end(trim_ascii_start(rest))
                .bytes()
                .take_while(|byte| *byte == b'%')
                .count();
            if run >= 3 {
                by_width.entry((run, depth)).or_default().push(index);
            }
        }
        Self {
            by_width,
            dedents: HashMap::new(),
            blank: None,
        }
    }

    fn closes_in_column(
        &mut self,
        lines: &[&str],
        open_at: usize,
        open_col: usize,
        fence_len: usize,
    ) -> bool {
        let Some(closer) = self.next_closer(open_at, fence_len, 0) else {
            return false;
        };
        closer < self.first_dedent(lines, open_at, open_col)
    }

    /// The quoted twin of `closes_in_column`. The blank line carries the bound:
    /// a blockquote does not survive one, so a `> %%%` below a blank opens a
    /// different quote whose body the block parser never joins to this one.
    ///
    /// The dedent joins it only where the quote is itself inside something. A
    /// quote at column 0 has nothing to dedent below, so the blank is the whole
    /// test there; a `- > %%%` sits at column 2, and a later `> %%%` back at
    /// column 0 has left the item, which the depth alone cannot see because both
    /// runs are one marker deep.
    fn closes_in_quote(
        &mut self,
        lines: &[&str],
        open_at: usize,
        depth: usize,
        quote_col: usize,
        fence_len: usize,
    ) -> bool {
        let Some(closer) = self.next_closer(open_at, fence_len, depth) else {
            return false;
        };
        if closer >= self.first_blank(lines, open_at) {
            return false;
        }
        quote_col == 0 || closer < self.first_dedent(lines, open_at, quote_col)
    }

    fn next_closer(&self, open_at: usize, fence_len: usize, depth: usize) -> Option<usize> {
        let positions = self.by_width.get(&(fence_len, depth))?;
        let next = positions.partition_point(|at| *at <= open_at);
        positions.get(next).copied()
    }

    /// Memoized the way `first_dedent` is, and for the same reason: with no
    /// blank between `from` and the answer, the answer is still the answer for
    /// every query point in between, so a run of quoted openers costs one walk
    /// rather than one each.
    fn first_blank(&mut self, lines: &[&str], open_at: usize) -> usize {
        if let Some((from, at)) = self.blank {
            if from <= open_at && open_at < at {
                return at;
            }
        }
        let mut at = lines.len();
        for (offset, line) in lines[open_at + 1..].iter().enumerate() {
            #[cfg(test)]
            CONTAINER_DEDENT_STEPS.with(|c| c.set(c.get() + 1));
            if is_blank_line(line) {
                at = open_at + 1 + offset;
                break;
            }
        }
        self.blank = Some((open_at, at));
        at
    }

    fn first_dedent(&mut self, lines: &[&str], open_at: usize, open_col: usize) -> usize {
        if let Some(&(from, at)) = self.dedents.get(&open_col) {
            if from <= open_at && open_at < at {
                return at;
            }
        }
        let mut at = lines.len();
        for (offset, line) in lines[open_at + 1..].iter().enumerate() {
            #[cfg(test)]
            CONTAINER_DEDENT_STEPS.with(|c| c.set(c.get() + 1));
            if is_blank_line(line) {
                continue;
            }
            if indent_columns(line) < open_col {
                at = open_at + 1 + offset;
                break;
            }
        }
        self.dedents.insert(open_col, (open_at, at));
        at
    }
}

fn detect_comment_fence_line(line: &str) -> Option<CommentFenceOpen> {
    let line = trim_ascii_end(line);
    let fence_len = line.bytes().take_while(|b| *b == b'%').count();
    if fence_len < 3 {
        return None;
    }
    Some(CommentFenceOpen {
        fence_len,
        tail: trim_ascii_start(&line[fence_len..]).to_string(),
    })
}

/// A comment-fence closer matches by exact leading `%` run length; its tail is
/// ignored and discarded.
fn is_comment_fence_close(line: &str, fence_len: usize) -> bool {
    let line = trim_ascii_end(line);
    line.bytes().take_while(|b| *b == b'%').count() == fence_len
}

fn detect_thematic_break(line: &str) -> bool {
    thematic_break_marker(line).is_some()
}

fn thematic_break_marker(line: &str) -> Option<char> {
    // Grammar (spec §262): a col-0 run of 3+ of the SAME `-`/`*`/`_`,
    // CONTIGUOUS (no internal spaces), followed only by trailing whitespace.
    // No leading indent. So `***`/`----`/`___` are breaks, but `* * *` (spaces)
    // and ` ***` (indented) fall through to list/paragraph. A mixed run (`-*-`)
    // is not a break either.
    let bytes = line.as_bytes();
    let marker = match bytes.first() {
        Some(&b @ (b'-' | b'*' | b'_')) => b,
        _ => return None,
    };
    let mut count = 0usize;
    let mut i = 0;
    while i < bytes.len() && bytes[i] == marker {
        count += 1;
        i += 1;
    }
    if count < 3 {
        return None;
    }
    // Only trailing whitespace may follow the contiguous marker run.
    bytes[i..]
        .iter()
        .all(|&b| b == b' ' || b == b'\t')
        .then_some(marker as char)
}

#[derive(Debug, Clone, Copy)]
struct FenceOpen {
    fence_char: u8,
    fence_len: usize,
    content_col: usize,
    quoted: bool,
    lang_start: usize,
    lang_end: usize,
    title_start: Option<usize>,
    title_end: Option<usize>,
    label_start: Option<usize>,
    label_end: Option<usize>,
}

fn detect_fence_open(line: &str) -> Option<FenceOpen> {
    // A TRAILING whitespace run is dropped before the separator below is asked
    // anything (markup-carve/carve#1295, markup-carve/carve-rs#1022's `330-2`).
    //
    // Two clauses meet on this line and POSITION decides which governs. A tab
    // BEFORE content is the marker-to-content separator, which is the `space`
    // terminal and nothing else, so ```` ```<TAB>php ```` opens no fence - that
    // refusal is the `== b' '` test below and it stays. A tab at the END of the
    // line with nothing after it never reaches that slot: it is trailing
    // whitespace on a content line, PART 2 drops it, and what is left is the
    // bare opener. Read that way the two clauses never overlap.
    //
    // Trailing SPACES already behaved this way, by being eaten further down; a
    // tab had no such path and left a fence that refused to open.
    let line = trim_ascii_end(line);
    let bytes = line.as_bytes();
    let mut i = 0;
    if bytes.is_empty() {
        return None;
    }
    let fence_char = bytes[i];
    if fence_char != b'`' && fence_char != b'~' {
        return None;
    }
    let fence_start = i;
    while i < bytes.len() && bytes[i] == fence_char {
        i += 1;
    }
    let fence_len = i - fence_start;
    if fence_len < 3 {
        return None;
    }
    // Optional whitespace then info string:
    //   [language] ["header"] [[label]]
    // in that fixed order. With no language, a header or label may sit
    // directly against the fence; after a language each following token must
    // be whitespace-separated.
    //
    // THIS SLOT IS EXACTLY ONE SPACE. `fenced_code_block = code_fence_open,
    // [space], [code_fence_info]` spells it as one, and carve#912 ruled the
    // production right (carve#912). A two-space opener therefore matches no
    // shape and the INVALID-FENCE FALLBACK applies: an inline verbatim span in
    // a paragraph.
    //
    // The two metadata slots INSIDE `code_fence_info` are spelled `space+` and
    // are NOT in scope - a run stays legal at both, and the `separated` loops
    // below are deliberately left alone. The cardinality answer is
    // per-production, not global (carve#892 keeps the colon fence's separator a
    // run for the same reason).
    //
    // A run with no info after it is the line ending rather than this slot: the
    // remaining spaces are eaten by the `separated` loop and the fence stays
    // valid, which is what keeps ```` ```<SP><SP> ```` an ordinary empty fence.
    if bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    let lang_start = i;
    // Raw passthrough block: `=FORMAT` (§4.15, djot raw-block syntax) -- a
    // leading `=` immediately followed by the format name. The `=` is the block
    // parallel of the inline raw `{=format}` attribute; it is never part of a
    // language token, so this is unambiguous against an ordinary code block.
    // parse_fence recovers raw blocks by the leading `=` in this span. The `=`
    // and format name must be adjacent (`=html`); `= html` is not raw.
    if i < bytes.len() && bytes[i] == b'=' {
        i += 1;
        if i >= bytes.len() || !bytes[i].is_ascii_alphabetic() {
            return None;
        }
        i += 1;
        while i < bytes.len()
            && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
        {
            i += 1;
        }
        let lang_end = i;
        // Must be only whitespace after the format name
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        if i != bytes.len() {
            return None;
        }
        return Some(FenceOpen {
            fence_char,
            fence_len,
            content_col: 0,
            quoted: false,
            lang_start,
            lang_end,
            title_start: None,
            title_end: None,
            label_start: None,
            label_end: None,
        });
    }
    // Language token charset covers real-world tags with punctuation
    // (c++, c#, f#, asp.net, text/html); the token is still anchored (no
    // whitespace), so a multiword/quoted info string is not a fence. `/` is
    // allowed so MIME-like tags stay a single language token.
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric()
            || bytes[i] == b'_'
            || bytes[i] == b'-'
            || bytes[i] == b'+'
            || bytes[i] == b'#'
            || bytes[i] == b'.'
            || bytes[i] == b'/')
    {
        i += 1;
    }
    let lang_end = i;
    let has_lang = lang_start < lang_end;
    let mut title_start = None;
    let mut title_end = None;
    let mut label_start = None;
    let mut label_end = None;
    let mut separated = false;
    while i < bytes.len() && bytes[i] == b' ' {
        separated = true;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'"' {
        if has_lang && !separated {
            return None;
        }
        i += 1;
        let start = i;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                title_start = Some(start);
                title_end = Some(i);
                i += 1;
                break;
            }
            i += 1;
        }
        title_start?;
        separated = false;
        while i < bytes.len() && bytes[i] == b' ' {
            separated = true;
            i += 1;
        }
    }
    if i < bytes.len() && bytes[i] == b'[' {
        if (has_lang || title_start.is_some()) && !separated {
            return None;
        }
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i] != b']' {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        label_start = Some(start);
        label_end = Some(i);
        i += 1;
    }
    // Must be only whitespace after the info string
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    if i != bytes.len() {
        return None;
    }
    Some(FenceOpen {
        fence_char,
        fence_len,
        content_col: 0,
        quoted: false,
        lang_start,
        lang_end,
        title_start,
        title_end,
        label_start,
        label_end,
    })
}

fn parse_fence(cur: &mut LineCursor, open: FenceOpen, options: &Options<'_>) -> BlockNode {
    let span_start = cur.pos;
    let open_line = cur.consume().unwrap();
    let open_trim = open_line[open.lang_start..].trim();
    let raw_format = open_trim.strip_prefix('=').map(|f| f.trim().to_string());
    let lang = if raw_format.is_none() && open.lang_start < open.lang_end {
        Some(open_line[open.lang_start..open.lang_end].to_string())
    } else {
        None
    };
    let title = open
        .title_start
        .zip(open.title_end)
        .map(|(start, end)| unescape_quoted_header(&open_line[start..end]));
    let label = open
        .label_start
        .zip(open.label_end)
        .map(|(start, end)| open_line[start..end].to_string());
    let mut content_lines: Vec<String> = Vec::new();
    while let Some(line) = cur.peek() {
        if is_fence_close(line, open) {
            cur.consume();
            break;
        }
        cur.consume();
        content_lines.push(line.to_string());
    }
    // The span covers the opener, the body and the closer - the whole block as
    // the author wrote it, not just its content.
    let pos = span_of(cur, span_start, cur.pos, options);
    if let Some(format) = raw_format {
        BlockNode::RawBlock(RawBlock {
            format,
            content: content_lines.join("\n"),
            pos,
        })
    } else {
        BlockNode::CodeBlock(CodeBlock {
            attrs: None,
            lang,
            title,
            label,
            content: content_lines.join("\n"),
            pos,
        })
    }
}

fn unescape_quoted_header(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A CLOSER TAKES NO CONTENT, so its tail is the LINE ENDING and not a slot.
///
/// PART 2's NO TRAILING WHITESPACE governs it: the run is `whitespace`,
/// `' ' | '\t'`, it is dropped, and it is not content. That is the whole reason
/// a tab is accepted here while the OPENER refuses one - the opener's tab sits
/// before an info string, where it is a separator and MARKER SEPARATORS AND
/// PADDING SLOTS spells the terminal `space` alone. Position decides, not the
/// construct (carve#1295), so the two halves of the fence disagree on purpose.
///
/// A tab with content after it is still not a closer: the loop stops at the
/// first non-whitespace byte and the line then fails the end-of-line test, so
/// ```` ```<TAB>php ```` closes nothing.
fn is_fence_close(line: &str, open: FenceOpen) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    let start = i;
    while i < bytes.len() && bytes[i] == open.fence_char {
        i += 1;
    }
    if i - start < open.fence_len {
        return false;
    }
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    i == bytes.len()
}

fn exact_colon_fence_len(line: &str) -> Option<usize> {
    let t = trim_ascii_end(line);
    if !t.is_empty() && t.bytes().all(|b| b == b':') {
        Some(t.len())
    } else {
        None
    }
}

fn is_invalid_colon_fence_opener_text(line: &str) -> bool {
    let trimmed = trim_ascii_start(line);
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return false;
    }
    if exact_colon_fence_len(trimmed).is_some() {
        return false;
    }
    detect_container_open(trimmed).is_none()
        && detect_line_block_open(trimmed).is_none()
        && detect_hardbreaks_block_open(trimmed).is_none()
}

fn code_fence_has_closer(cur: &mut LineCursor<'_>, open: FenceOpen) -> bool {
    if !cur.has_code_closer_after(cur.pos, open.fence_char, open.fence_len) {
        return false;
    }
    let strip = leading_ws(cur.lines[cur.pos]);
    cur.lines[cur.pos + 1..]
        .iter()
        .any(|l| is_fence_close(&l[leading_ws(l).min(strip)..], open))
}

fn push_current_line(inner: &mut LineBuffer, cur: &LineCursor<'_>) {
    inner.push_at(
        cur.peek().unwrap().to_string(),
        cur.source_line(cur.pos),
        cur.source_col(cur.pos),
    );
}

/// Copy the opener line, then every line up to and including its closer.
///
/// The opener is taken BEFORE the first closer test on purpose. An opener with
/// no info string is closer-shaped itself - a bare ``` or a `%%%` - so testing
/// it would end the span on its own line and hand the span's contents back to
/// the block parser, where a fence-shaped line inside it closes the container
/// around it (carve#450).
fn take_opaque_span_into(
    inner: &mut LineBuffer,
    cur: &mut LineCursor<'_>,
    is_close: impl Fn(&str) -> bool,
) {
    push_current_line(inner, cur);
    cur.consume();
    while let Some(line) = cur.peek() {
        push_current_line(inner, cur);
        cur.consume();
        if is_close(line) {
            break;
        }
    }
}

fn skip_opaque_span_into(inner: &mut LineBuffer, cur: &mut LineCursor<'_>) -> bool {
    let Some(line) = cur.peek() else {
        return false;
    };
    if let Some(open) = detect_fence_open(line) {
        // A closer is required whether or not a paragraph is open. Only a fence
        // that closes is opaque; an unterminated one would otherwise consume the
        // container's own `:::` as content and run to the end of the document,
        // dragging every following block inside (carve#515, carve-rs#458).
        //
        // The comment-fence branch below has always required its closer. This
        // one asked only when a paragraph was open, so the rule held for a fence
        // that interrupted prose and lapsed for one that opened a body - which
        // is why the parameter that carried that state is gone.
        if code_fence_has_closer(cur, open) {
            take_opaque_span_into(inner, cur, |candidate| is_fence_close(candidate, open));
            return true;
        }
    }
    if let Some(open) = detect_comment_fence_line(line) {
        if cur.has_comment_closer_after(cur.pos + 1, open.fence_len) {
            let fence_len = open.fence_len;
            take_opaque_span_into(inner, cur, move |candidate| {
                is_comment_fence_close(candidate, fence_len)
            });
            return true;
        }
    }
    false
}

/// Returns the collected body and whether the container's own CLOSER was
/// consumed - `false` means end of input closed it (PART 9 §12). The flag
/// exists for the figure group, whose caption slot hangs on the closing fence
/// (§4c): a group closed by end of input has no closer line for a caption.
fn collect_colon_container_body(cur: &mut LineCursor<'_>, opener_len: usize) -> (LineBuffer, bool) {
    let mut inner = LineBuffer::default();
    let mut closed = false;
    let mut stack = vec![opener_len];
    while cur.peek().is_some() {
        let top = *stack.last().unwrap();
        // A CLOSER OF AN OPEN CONTAINER IS NOT ABSORBABLE. §12's absorption is
        // about a paragraph swallowing a would-be OPENER; the container this
        // line closes was opened BEFORE the malformed fence was ever read, so
        // no absorption can take its closer away (carve-rs#719).
        if exact_colon_fence_len(cur.peek().unwrap()) == Some(top) {
            if stack.len() == 1 {
                cur.consume();
                closed = true;
                break;
            }
            push_current_line(&mut inner, cur);
            cur.consume();
            stack.pop();
            continue;
        }
        if skip_opaque_span_into(&mut inner, cur) {
            continue;
        }
        let line = cur.peek().unwrap();
        if !line.starts_with([' ', '\t']) {
            if stack.len() < MAX_NESTING_DEPTH {
                if let Some(len) = detect_container_open(line)
                    .map(|open| open.fence_len)
                    .or_else(|| detect_line_block_open(line))
                    .or_else(|| detect_hardbreaks_block_open(line))
                {
                    stack.push(len);
                    push_current_line(&mut inner, cur);
                    cur.consume();
                    continue;
                }
            } else if detect_container_open(line).is_some()
                || detect_line_block_open(line).is_some()
                || detect_hardbreaks_block_open(line).is_some()
            {
                // Past the cap an opener DEGRADES to literal paragraph text
                // (§25) - it does not vanish. Consuming the line without
                // pushing it dropped every over-cap opener that had a closer
                // somewhere after it: 203 openers plus three closers published
                // 200 titles and no trace of the other three, while the same
                // input without the closers kept them (carve-rs#530).
                push_current_line(&mut inner, cur);
                cur.consume();
                continue;
            }
        }
        // No state survives a line here. This walk decides where the container
        // ENDS, and nothing a body line does can change that: the only line
        // that ends it is its own closer, and a closer is not absorbable. The
        // glued-colon tracking that used to sit here suppressed exactly that
        // closer, which is the defect (carve-rs#719).
        push_current_line(&mut inner, cur);
        cur.consume();
    }
    (inner, closed)
}

fn find_line_block_end(lines: &[&str], start: usize, fence_len: usize) -> usize {
    let mut idx = start + 1;
    while idx < lines.len() {
        if exact_colon_fence_len(lines[idx]) == Some(fence_len) {
            return idx + 1;
        }
        idx += 1;
    }
    lines.len()
}

fn find_colon_container_end(lines: &[&str], start: usize, fence_len: usize) -> usize {
    // Built on the first code fence seen, not up front: this function is called
    // often, and an unconditional O(lines) build here made a document of
    // unterminated `%%%` openers - which never reach the fence branch at all -
    // quadratic, as tests/perf_regressions.rs caught.
    let mut closer_index: Option<HashMap<u8, Vec<usize>>> = None;
    let mut stack = vec![fence_len];
    let mut idx = start + 1;
    while idx < lines.len() {
        let line = lines[idx];
        let top = *stack.last().unwrap();
        // A CLOSER OF AN OPEN CONTAINER IS NOT ABSORBABLE - the same rule as in
        // `collect_colon_container_body`, which walks this body for real. The
        // two must agree about where the container ends, and they answer with
        // the same test (carve-rs#719).
        //
        // NOT INDEPENDENTLY OBSERVABLE today, and changed anyway. This walk is
        // reached only from `parse_continuation_block`, where the extent it
        // reports decides where a `+`-attached block ends; leaving the old
        // absorbing test here is a GREEN mutation against 690 corpus documents
        // x six targets and 55223 generated `+`/colon documents. It moves
        // regardless, because the two walks disagreeing about a container's end
        // is itself the bug shape - carve#515 was exactly that, and the comment
        // in the fence branch below records it. A test asserting this line
        // alone would be a check that cannot fail (markup-carve/carve#755), so
        // the reason lives here instead.
        if exact_colon_fence_len(line) == Some(top) {
            stack.pop();
            idx += 1;
            if stack.is_empty() {
                return idx;
            }
            continue;
        }
        if let Some(open) = detect_fence_open(line) {
            // Same rule as `skip_opaque_span_into`, which walks this body for
            // real: only a fence that closes is opaque (carve#515). This copy
            // used to accept an unterminated one whenever no paragraph was
            // open, so the two walks disagreed about where the container ends.
            //
            // The index makes the `any` below skip-able. Without it this scan
            // runs from every opener to the end of the input, which is
            // quadratic in the number of unterminated openers.
            let index = closer_index.get_or_insert_with(|| build_code_closer_last_index(lines));
            let has_closer = code_closer_exists_after(index, idx, open.fence_char, open.fence_len)
                && lines[idx + 1..]
                    .iter()
                    .any(|candidate| is_fence_close(candidate, open));
            if has_closer {
                idx += 1;
                while idx < lines.len() {
                    let candidate = lines[idx];
                    idx += 1;
                    if is_fence_close(candidate, open) {
                        break;
                    }
                }
                continue;
            }
        }
        if let Some(open) = detect_comment_fence_line(line) {
            if let Some(close) =
                (idx + 1..lines.len()).find(|&j| is_comment_fence_close(lines[j], open.fence_len))
            {
                idx = close + 1;
                continue;
            }
        }
        if !line.starts_with([' ', '\t']) && stack.len() < MAX_NESTING_DEPTH {
            if let Some(len) = detect_container_open(line)
                .map(|open| open.fence_len)
                .or_else(|| detect_line_block_open(line))
                .or_else(|| detect_hardbreaks_block_open(line))
            {
                stack.push(len);
                idx += 1;
                continue;
            }
        }
        idx += 1;
    }
    lines.len()
}

/// The column a footnote body's continuation lines must reach.
///
/// PART 9 §16 asks for ">= 2", RELATIVE to the definition line. Measured from
/// column 0 instead, an INDENTED definition swallowed anything at column 2 -
/// including a `:::` closer belonging to the container the definition sits in,
/// which then rendered as an empty `<div>` inside the endnote and pushed the
/// backlink out of its paragraph (carve-rs#591). carve-js and carve-php both
/// measure it relative: `  [^f]: x` takes a `    more` continuation and leaves
/// a `  more` alone.
///
/// COLUMNS, not characters. Every caller compares this against
/// `indent_columns`, which expands a tab to its stop, so computing it with
/// `leading_ws` - a CHARACTER count - put the two sides of the comparison in
/// different units and a tab made them disagree. That is the class the
/// tabs-are-columns family has been correcting throughout: markup-carve/carve#692,
/// #796, #901, #905 and #893 were each a rule stated in one unit and
/// implemented in another (carve-rs#735).
///
/// UNREACHABLE TODAY, and fixed anyway. The two spellings can only differ on a
/// definition line whose own indentation contains a tab, and no such line
/// reaches here: an indented top-level definition is not collected at all (it
/// stays a literal paragraph), and a container-nested one skips the
/// continuation loop entirely (`if !in_container`). Measured - swapping the
/// units back changes nothing across the corpus or any input built for it.
///
/// It is corrected rather than left because the next change to which
/// definitions are collected makes it a live defect with nothing to catch it,
/// which is why carve-rs#735 was filed before it could bite. A test asserting
/// the ENGINE's output could not fail here, so the unit is pinned on this
/// function directly instead (markup-carve/carve#755).
fn footnote_body_floor(def_line: &str) -> usize {
    indent_columns(def_line) + 2
}

fn leading_ws(line: &str) -> usize {
    line.bytes()
        .take_while(|b| *b == b' ' || *b == b'\t')
        .count()
}

// Visual column of the leading whitespace, expanding tabs to the next
// CommonMark tab stop (a multiple of 4). For space-only indentation this
// equals leading_ws(). Used for list-nesting comparisons.
fn indent_columns(line: &str) -> usize {
    let mut col = 0;
    for b in line.bytes() {
        match b {
            b' ' => col += 1,
            b'\t' => col += 4 - (col % 4),
            _ => break,
        }
    }
    col
}

// Drop leading whitespace up to `cols` columns (tab-stop aware) and return the
// remainder. With keep_residual, the unconsumed columns of a straddling tab are
// re-emitted as spaces, so the line keeps the column the tab actually reached.
// For space-only indentation there is never a residual.
//
// THE RESIDUAL MATTERS INSIDE A LIST ITEM, and the note here used to say the
// opposite: that a straddling tab could be consumed whole because "Carve has no
// indent-sensitive block where the leftover column would change meaning". Every
// block opener in an item is indent-sensitive in exactly that way -- at the
// item's content column an opener parses, one column past it the line is
// paragraph text -- so dropping the residual let a tab landing PAST the column
// dedent flush and open a fence, heading, quote or thematic break that the same
// column written in spaces does not (carve-rs#889). PART 7: a leading tab
// indents to column 4, and an ordered item's content column is 3.
//
// Marker lines still pass it for their own reason: tab+space-aligned sibling
// markers keep the same visual column and the recursive parse re-derives the
// child base from it.
fn slice_columns(line: &str, cols: usize, keep_residual: bool) -> String {
    slice_columns_mapped(line, cols, keep_residual).0
}

/// `slice_columns`, plus what a caller needs to map its output back to source.
///
/// Returns `(sliced, consumed, synthetic)`: the characters taken off the front,
/// and the number of SPACES written in their place for a straddling tab's
/// residual. Those spaces are not in the source, so a caller that treats the
/// result as a plain suffix charges offsets to characters that do not exist,
/// and a span near the end runs past the end of the document (carve-rs#700).
///
/// The line's real content still IS a suffix, though, so the mapping is exact
/// with one subtraction: output position `p >= synthetic` sits at source
/// position `consumed + p - synthetic`. Only a position INSIDE the synthetic
/// run has no source, and nothing starts there - the run is whitespace and the
/// marker follows it. carve-js#771 fixed the same defect the same way.
fn slice_columns_mapped(line: &str, cols: usize, keep_residual: bool) -> (String, usize, usize) {
    let bytes = line.as_bytes();
    let mut col = 0;
    let mut i = 0;
    while i < bytes.len() && col < cols {
        match bytes[i] {
            b' ' => {
                col += 1;
                i += 1;
            }
            b'\t' => {
                col += 4 - (col % 4);
                i += 1;
            }
            _ => break,
        }
    }
    let consumed = line[..i].chars().count();
    if keep_residual && col > cols {
        let synthetic = col - cols;
        let mut s = " ".repeat(synthetic);
        s.push_str(&line[i..]);
        (s, consumed, synthetic)
    } else {
        (line[i..].to_string(), consumed, 0)
    }
}

/// Whether the quoted body so far leaves a paragraph open.
///
/// Deciding this means walking a quoted line down to its INNERMOST content,
/// which costs one `strip_blockquote_prefix` per quote marker still on the
/// line -- and every enclosing level of a nested quote repeats that walk over
/// the same markers, because each level strips one marker and hands the rest
/// down. Deciding it eagerly, on every quoted line, therefore cost
/// `depth^3 / 6` strips on a depth ladder: 1,556,994 at depth 200, against
/// carve-js's 20,100 on the same document (markup-carve/carve-rs#731).
///
/// The answer is READ only when a fence opener or an unprefixed line arrives,
/// which on a ladder is never. So the walk belongs at the read. The predicate
/// and its inputs are unchanged; only the moment it runs is.
///
/// PART 9 §12's absorption is part of the same answer, so it is decided here
/// rather than sampled around this value: `inherited_absorption` is the state
/// the line inherits, and the resolved verdict carries both whether the
/// paragraph is open and whether THIS line opens an absorption. Reading the
/// flag on every quoted line - which is what `suppress_colon_interrupt` did
/// when it stood outside - is precisely the eager read this defers
/// (markup-carve/carve-rs#738 landing under markup-carve/carve-rs#731).
enum ParaOpen<'a> {
    /// Closed, decided without consulting any line.
    Closed,
    /// Open iff `line`'s innermost quoted content is paragraph text, under the
    /// absorption state it inherits. `answer` caches the verdict,
    /// so re-reading the same state walks once.
    Deferred {
        line: &'a str,
        inherited_absorption: bool,
        answer: Option<Verdict>,
    },
}

/// What one resolved quoted line says about the paragraph and the absorption.
#[derive(Clone, Copy)]
struct Verdict {
    /// Whether the line leaves a paragraph open.
    open: bool,
    /// Whether the line is a glued colon fence that starts §12's absorption,
    /// which only an OPEN paragraph can carry (so this implies `open`).
    opens_absorption: bool,
}

impl<'a> ParaOpen<'a> {
    fn from_line(line: &'a str, inherited_absorption: bool) -> Self {
        ParaOpen::Deferred {
            line,
            inherited_absorption,
            answer: None,
        }
    }

    /// Whether resolving this line can change the absorption carried by the
    /// next one. Keeping this cheap guard preserves the deep-quote deferral.
    fn may_carry_absorption(&self) -> bool {
        match self {
            ParaOpen::Closed => false,
            ParaOpen::Deferred {
                line,
                inherited_absorption,
                ..
            } => *inherited_absorption || line.contains(":::"),
        }
    }

    /// Absorption inherited by the next quoted line.
    fn absorption(&mut self) -> bool {
        let inherited = match self {
            ParaOpen::Closed => return false,
            ParaOpen::Deferred {
                inherited_absorption,
                ..
            } => *inherited_absorption,
        };
        let verdict = self.resolve();
        verdict.open && (inherited || verdict.opens_absorption)
    }

    fn get(&mut self) -> bool {
        self.resolve().open
    }

    fn resolve(&mut self) -> Verdict {
        let ParaOpen::Deferred {
            line,
            inherited_absorption,
            answer,
        } = self
        else {
            return Verdict {
                open: false,
                opens_absorption: false,
            };
        };
        if let Some(known) = answer {
            return *known;
        }
        let suppressed = *inherited_absorption;
        // Look THROUGH any further quote markers before deciding. A lazy line
        // continues the innermost OPEN PARAGRAPH, however many containers it
        // failed to match, so what matters is whether the innermost quoted
        // content leaves a paragraph open - not whether this line opens
        // another quote.
        //
        // Without this, `>> b` stripped to `> b`, which reads as a container
        // opener, so the paragraph closed and a following bare line could not
        // fold. That made laziness work at depth 1 and not at depth 2, which
        // is not a reading of any rule: PART 1 S4's strict wording would close
        // the quote at depth 1 too, and nothing does that (markup-carve/carve#506).
        let mut innermost = *line;
        while let Some(rest) = strip_blockquote_prefix(innermost) {
            innermost = rest;
        }
        // An open paragraph requires plain paragraph text. A line that is
        // itself a block-opener (heading, thematic break, table row, `:::` div
        // / line block opener) leaves NO open paragraph -- so a following list
        // marker has nothing to fold into and must end the quote.
        // `interrupts_paragraph_with_rest` is the §10 predicate: a line that
        // would interrupt a paragraph is, by definition, not paragraph
        // continuation text.
        //
        // Only a FLUSH-LEFT `:::` container closes the quoted paragraph; an
        // INDENTED `::: note` / `:::` (above the quote's content column) is
        // literal paragraph text, so it keeps the paragraph open and lazy
        // continuation stays in the quote (strict column-0 rule,
        // docs/divergence-from-djot.md §11) -- uniform with the opener paths
        // in parse_block / interrupts_paragraph.
        //
        // The rest-of-body slice is empty: the only lookahead
        // `interrupts_paragraph_with_rest` consults is a fenced-code closer
        // probe, and this predicate is reached only from the branch where the
        // line is NOT a fence opener, so the caller never had a slice to give.
        // It passed a `Vec` built under `if detect_fence_open(stripped)`, which
        // the enclosing `else if let Some(open) = detect_fence_open(stripped)`
        // had already decided was `None` - so that `Vec` was always empty. An
        // assertion in its place held over the whole corpus and test suite.
        //
        // A fence-shaped line the open paragraph has already absorbed is
        // paragraph text, so it neither opens a block nor interrupts: §12's
        // absorption decides before the shape tests do (carve-rs#727).
        let absorbed = suppressed && is_suppressed_colon_fence_line(innermost);
        let open = !is_blank_line(innermost)
            && (absorbed
                || ((innermost.starts_with([' ', '\t'])
                    || detect_container_open(innermost).is_none())
                    && !trim_ascii_start(innermost).starts_with("%%")
                    && !interrupts_paragraph_with_rest(innermost, &[])));
        let verdict = Verdict {
            open,
            opens_absorption: open && is_invalid_colon_fence_opener_text(innermost),
        };
        *answer = Some(verdict);
        verdict
    }
}

fn parse_blockquote(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let span_start = cur.pos;
    let mut inner = LineBuffer::default();
    let mut para_open = ParaOpen::Closed;
    // PART 9 §12's absorption, tracked for the QUOTED paragraph the way
    // `parse_paragraph` tracks it for a top-level one: once a line has failed
    // the opener test, the paragraph absorbs the next fence-shaped line as text
    // "INSTEAD of being interrupted by it". The quote body collector already
    // pushed such a line into `inner`, where the nested parse folded it into the
    // paragraph correctly - but `para_open` was computed from the line's SHAPE
    // alone, so a bare `:::` set it false and the flush-left line under it could
    // not fold. That made the quote disagree with its own body about whether a
    // paragraph was open (carve-rs#727). `ParaOpen` owns that state now,
    // instead of requiring a second boolean whose validity depends on the
    // enum's current variant.
    //
    // It is a RUNNING state, and only a resolved `para_open` can advance it, so
    // it is carried by `ParaOpen` and advanced from the resolved verdict rather
    // than read on every quoted line. Reading it per line is what would put
    // markup-carve/carve-rs#731's cubic walk back.
    let mut in_fence: Option<FenceOpen> = None;
    // THE QUOTE'S OWN LAST BLOCK IS WHAT S4 ASKS ABOUT, and a continuation row
    // belongs to the table above it (§5 T6, markup-carve/carve#1348). `ParaOpen`
    // is handed ONE line and a `+ b |` line reads as prose on its own, so a
    // quote ending on a continuation row recorded an open paragraph and the
    // flush-left line below folded in - while the SAME table ending on a
    // standard row closed the quote. One question answered two ways by a
    // spelling. This carries the run so the two spellings answer alike.
    let mut table = TableRun::default();
    // ONE BLOCK IS ONE BLOCK HOWEVER MANY LINES IT TAKES (§15 A5). A wrapped
    // attribute block (`{.k` / `#x}`) closes the quoted paragraph exactly as the
    // single-line `{.k}` does, and the lines after its opener are its own
    // content rather than paragraph text. `ParaOpen` decides from ONE line, so
    // the block's extent is tracked here beside the code fence's - the same
    // shape, for the same reason (markup-carve/carve-rs#1050).
    let mut attrs_block_rest: usize = 0;
    // The first line index the block lookahead is still worth running on. A scan
    // that walked to a blank line, or to the end of the quoted run, without
    // meeting a line that could CLOSE an attribute block has proved the same for
    // every line it passed - see `quoted_attrs_block_len`.
    let mut attrs_scan_floor: usize = 0;
    while let Some(line) = cur.peek() {
        if let Some(stripped) = strip_blockquote_prefix(line) {
            let source_line = cur.source_line(cur.pos);
            let at = cur.pos;
            cur.consume();
            // The quote marker (and its optional space) is a pure prefix, so the
            // quoted line's columns are knowable in the document.
            let stripped_at = stripped_col(cur.source_col(at), line, stripped);
            if attrs_block_rest > 0 {
                // Inside a wrapped attribute block the opener already closed the
                // paragraph; its remaining lines close nothing further and open
                // nothing either.
                attrs_block_rest -= 1;
                para_open = ParaOpen::Closed;
                table = TableRun::default();
            } else if let Some(open) = in_fence {
                if is_fence_close(stripped, open) {
                    in_fence = None;
                }
                para_open = ParaOpen::Closed;
                table = TableRun::default();
                // The absorption belongs to ONE paragraph: whatever closed it
                // -- a blank line, a real opener, a code fence -- ends the
                // absorption too. Spelled at each site that closes the
                // paragraph rather than once after the chain, because after the
                // chain it would have to READ `para_open`, and reading it on
                // every quoted line is the walk this defers.
            } else if let Some(open) = detect_fence_open(stripped) {
                table = TableRun::default();
                if !para_open.get() {
                    // Fence at block start opens (unterminated renders to end).
                    in_fence = Some(open);
                    para_open = ParaOpen::Closed;
                } else {
                    // After an open paragraph a fence interrupts only with a
                    // matching closer ahead (§10); else it is inline verbatim.
                    let has_closer = cur.lines[cur.pos..]
                        .iter()
                        .take_while(|l| strip_blockquote_prefix(l).is_some())
                        .any(|l| {
                            let s = strip_blockquote_prefix(l).unwrap_or(l);
                            is_fence_close(s, open)
                        });
                    if has_closer {
                        in_fence = Some(open);
                        para_open = ParaOpen::Closed;
                    }
                    // No closer: the fence is inline verbatim and the paragraph
                    // (already resolved OPEN by the test above) stays open, so
                    // the absorption survives - as it did when the reset was
                    // written `if !para_open`.
                }
            } else {
                // Whether this line leaves a paragraph open is decided by
                // `ParaOpen::get`, which walks down to the innermost quoted
                // content. Record the line rather than the answer: the walk
                // costs one strip per remaining quote marker, and answering
                // here answers a question nothing may ask
                // (markup-carve/carve-rs#731). The absorption state the line
                // inherits goes WITH it, so §12 is decided by the same walk
                // instead of forcing one of its own.
                let inherited_absorption = if para_open.may_carry_absorption() {
                    para_open.absorption()
                } else {
                    false
                };
                // The run is advanced on every ordinary quoted line, so the
                // answer is about the BLOCK this line lands in rather than
                // about the line. A continuation row appends to the table above
                // it and opens no paragraph, exactly as the row it appends to
                // does; anything else resets the run, which is what keeps a
                // `+ b |` with no table above it ordinary prose
                // (markup-carve/carve#1345).
                if table.observe(stripped) {
                    para_open = ParaOpen::Closed;
                    inner.push_at(stripped.to_string(), source_line, stripped_at);
                    continue;
                }
                para_open = ParaOpen::from_line(stripped, inherited_absorption);
                // A WRAPPED ATTRIBUTE BLOCK CLOSES THE PARAGRAPH TOO, and only
                // the lines after its opener can tell it apart from prose that
                // happens to start with a brace. `ParaOpen` is handed ONE line
                // and passes an empty rest slice to
                // `interrupts_paragraph_with_rest`, so the single-line `{.k}`
                // closed the quoted paragraph while `{.k` / `#x}` did not: the
                // quote went on collecting, the column-0 line folded in, and the
                // author's attributes landed on it INSIDE the container they
                // were written to end (markup-carve/carve-rs#1050). §15 A5 makes
                // one block one block however many lines it takes, so the two
                // spellings answer alike.
                if at >= attrs_scan_floor {
                    match quoted_attrs_block_len(stripped, &cur.lines[cur.pos..]) {
                        QuotedAttrsBlock::Block(len) => {
                            para_open = ParaOpen::Closed;
                            attrs_block_rest = len - 1;
                        }
                        // The scan already proved no block can start inside the
                        // window it walked, so the next lines skip it. That is
                        // what keeps a run of brace-shaped lines that close
                        // nothing linear instead of scanned once per line.
                        QuotedAttrsBlock::NoneWithin(window) => {
                            attrs_scan_floor = at + window;
                        }
                        QuotedAttrsBlock::No => {}
                    }
                }
                // Advancing the absorption flag needs the verdict, and getting
                // the verdict is the walk. So force it only where the answer
                // can change the flag. While the flag is OFF, a line carrying
                // no COLON RUN settles it either way without being read, and
                // the flag stays off whether the paragraph turns out open or
                // closed. Both §12 predicates need `:::`:
                //
                // - `is_invalid_colon_fence_opener_text` returns early unless
                //   the trimmed line opens with `fence_len >= 3` colons;
                // - `is_suppressed_colon_fence_line` is `exact_colon_fence_len`
                //   AND `is_colon_fence_opener_shape`, and each of the three
                //   detectors the latter ORs (`detect_container_open`,
                //   `detect_line_block_open`, `detect_hardbreaks_block_open`)
                //   returns `None` on that same `fence_len < 3` test.
                //
                // `innermost` is a suffix of `stripped`, so testing `stripped`
                // is conservative: it can only force a walk that was not
                // needed, never skip one that was. Ordinary quoted prose - a
                // `Note:`, a `12:30`, an `https://` - therefore stays on the
                // deferred path, which is what lets markup-carve/carve-rs#738
                // ride on the deferral instead of undoing it.
            }
            inner.push_at(stripped.to_string(), source_line, stripped_at);
            continue;
        }
        // Continuation marker (Carve, PART 9 §17): a lone `+` at column 0 after
        // a quoted line attaches the FOLLOWING flush-left block to the quote --
        // the un-prefixed analogue of the list-item form, so a real block (list,
        // fenced code, table, ...) joins the quote without repeating `>`. Collect
        // the block's lines (up to a blank line, a `>` line, or a further `+`)
        // and splice them into the quote body behind a blank-line separator, so
        // they parse as their own block instead of folding into the quoted
        // paragraph. The marker only attaches; a blank line still ends the quote
        // and a `+` outside a container stays literal.
        if trim_ascii(line) == "+" && indent_columns(line) == 0 {
            cur.consume();
            let mut attached = LineBuffer::default();
            let cursor_lines = cur.lines;
            let end = attached_block_end(
                cursor_lines,
                cur.pos,
                &mut cur.comment_closer_last_index,
                &mut |next, _| {
                    is_blank_line(next)
                        || strip_blockquote_prefix(next).is_some()
                        || (trim_ascii(next) == "+" && indent_columns(next) == 0)
                },
            );
            // ONE BLOCK, AND THE BOUNDARY IS THAT BLOCK'S EXTENT (§17 L3, ruled
            // in markup-carve/carve#1290). The scan above finds the boundary -
            // the next blank line, `>` line or further `+` - and the marker
            // attaches ONE block up to it, not everything up to it. The block
            // may still be many lines long: a wrapped paragraph, a list, a
            // nested quote, a fenced block. So the extent is measured by parsing
            // one block out of it rather than by taking the whole run.
            //
            // The list-item form already counted this way - `parse_continuation_
            // block` calls the single-block parser and advances by what it
            // consumed - and this branch was the one place where the two
            // spellings of the same clause disagreed: `> quoted` / `+` / `para`
            // / `# H` pulled the heading into the quote as well.
            //
            // Under a MEASUREMENT PROBE the whole extent is spliced instead. The
            // probe only wants a line count, and this marker's own division of
            // its content cannot change how many lines the block above it spans
            // - measuring here as well would double the work per nesting level.
            let attach_end = if measuring_attached_block() {
                end
            } else {
                cur.pos + attached_one_block_lines(&cursor_lines[cur.pos..end], options)
            };
            while cur.pos < attach_end {
                let next = cur.lines[cur.pos];
                // Attached lines are spliced in verbatim, so the container took
                // nothing beyond whatever an outer one already had.
                attached.push_at(
                    next.to_string(),
                    cur.source_line(cur.pos),
                    cur.source_col(cur.pos),
                );
                cur.pos += 1;
            }
            if !attached.lines.is_empty() {
                // `inner` always holds the quote's first content line, so a
                // leading blank separates the attached block from it.
                inner.push_synthetic_blank();
                inner.lines.extend(attached.lines);
                inner.line_map.extend(attached.line_map);
                // Must extend in lockstep with `lines`: a col_map that lags by
                // one entry hands every later block a wrong column.
                inner.col_map.extend(attached.col_map);
                inner.push_synthetic_blank();
                para_open = ParaOpen::Closed;
                table = TableRun::default();
            }
            continue;
        }
        // Lazy continuation: a non-`>` line folds into an OPEN paragraph. A
        // blank line, a caption, or a line that starts a block ends the quote.
        // A list marker FOLDS into the open quoted paragraph as literal text --
        // the quoted paragraph follows the same rule as a top-level paragraph,
        // where a list marker does not interrupt (it needs a blank line before
        // it). `interrupts_paragraph` is the shared predicate for that decision,
        // and it already returns false for bullet/task/ordered markers, so we
        // simply defer to it. A heading is the sole construct a list marker
        // would otherwise end, and headings still interrupt via that predicate.
        if !para_open.get() || is_blank_line(line) || caption_content(line).is_some() || {
            let line_owned = line.to_string();
            interrupts_lazy_continuation(cur, &line_owned)
        } {
            break;
        }
        let source_line = cur.source_line(cur.pos);
        // A lazy continuation line carries no quote marker, so nothing was
        // stripped from it beyond what an outer container already took.
        let source_col = cur.source_col(cur.pos);
        cur.consume();
        inner.push_at(line.to_string(), source_line, source_col);
    }
    let inner = inner.into_source();
    let children = parse_mapped_source(&inner, options);
    let quote = BlockQuote {
        pos: span_of(cur, span_start, cur.pos, options),
        attrs: None,
        children,
    };
    if let Some(caption) = consume_caption(cur, options) {
        BlockNode::Figure(Figure {
            attrs: None,
            target: Box::new(FigureTarget::BlockQuote(quote)),
            caption,
            short_caption: None,
            pos: span_of(cur, span_start, cur.pos, options),
        })
    } else {
        BlockNode::BlockQuote(quote)
    }
}

fn is_list_marker(line: &str) -> bool {
    detect_task(line).is_some()
        || detect_unordered(line).is_some()
        || detect_ordered(line).is_some()
}

/// Read an attribute block abutting a list marker (`-{.c}` / `3.{#x}`):
/// the parsed attributes (`None` for an empty `{}` block) and the byte index
/// just past the closing `}`. Returns `None` when there is no closing brace
/// or the content is not a valid attribute list -- in which case the marker
/// is not a list item (the line is ordinary text, grammar `item_attributes`).
fn read_list_item_attrs(bytes: &[u8], start: usize) -> Option<(Option<Attrs>, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut i = start + 1;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escaped {
            escaped = false;
        } else if b == b'\\' {
            escaped = true;
        } else if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else if b == b'"' || b == b'\'' {
            quote = Some(b);
        } else if b == b'}' {
            let inner = std::str::from_utf8(&bytes[start + 1..i]).ok()?;
            let end = i + 1;
            return if inner.trim().is_empty() {
                Some((None, end))
            } else {
                Some((Some(parse_attrs(inner)?), end))
            };
        }
        i += 1;
    }
    None
}

/// The text after a list marker: an optional abutting attribute block, then
/// the marker's required single space, then the item content. Returns the
/// content (trailing whitespace trimmed) and the item attributes. `None` when
/// the required space is missing or an abutting `{...}` is not a valid
/// attribute block (so the line is not a list item). A SPACE before `{` is
/// ordinary content, not an item-attribute, so it is handled by the plain
/// space branch and the `{...}` stays in the content.
fn marker_tail(line: &str, marker_end: usize) -> Option<(&str, Option<Attrs>)> {
    let bytes = line.as_bytes();
    let (content, attrs) = match bytes.get(marker_end) {
        Some(&b' ') => {
            let mut content_start = marker_end;
            while matches!(bytes.get(content_start), Some(b' ' | b'\t')) {
                content_start += 1;
            }
            (trim_ascii_end(&line[content_start..]), None)
        }
        Some(&b'{') => {
            let (attrs, end) = read_list_item_attrs(bytes, marker_end)?;
            if bytes.get(end) != Some(&b' ') {
                return None;
            }
            let mut content_start = end;
            while matches!(bytes.get(content_start), Some(b' ' | b'\t')) {
                content_start += 1;
            }
            (trim_ascii_end(&line[content_start..]), attrs)
        }
        _ => return None,
    };
    // A marker with no same-line content is not a list item -- a bare `- `
    // (or `-{.c} `) is ordinary text (matches carve-js / carve-php; a list
    // item carries its content on the marker line).
    if content.is_empty() {
        return None;
    }
    Some((content, attrs))
}

fn detect_unordered(line: &str) -> Option<(&str, Option<Attrs>, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let c = bytes[i];
    // `+` is the list continuation marker, not a bullet (#80).
    if c != b'-' && c != b'*' {
        return None;
    }
    let (content, attrs) = marker_tail(line, i + 1)?;
    Some((content, attrs, &line[i..i + 1]))
}

fn detect_ordered(line: &str) -> Option<&str> {
    detect_ordered_full(line).map(|(content, _, _, _, _, _)| content)
}

#[allow(clippy::type_complexity)]
fn detect_ordered_full(
    line: &str,
) -> Option<(
    &str,
    Option<usize>,
    Option<OrderedListType>,
    Option<Attrs>,
    u8,
    &str,
)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let marker_start = i;
    while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
        i += 1;
    }
    if i == marker_start {
        if bytes.get(i) != Some(&b'.') {
            return None;
        }
        let (content, attrs) = marker_tail(line, i + 1)?;
        return Some((content, None, None, attrs, b'.', ""));
    }
    if bytes.get(i) != Some(&b'.') && bytes.get(i) != Some(&b')') {
        return None;
    }
    let delim = bytes[i];
    // The required space may be preceded by an abutting attribute block
    // (`3.{#x} item`); `marker_tail` enforces the space and rejects an
    // invalid block.
    let (content, attrs) = marker_tail(line, i + 1)?;
    let marker = &line[marker_start..i];
    if marker.bytes().all(|b| b.is_ascii_digit()) {
        return Some((
            content,
            marker.parse::<usize>().ok().filter(|n| *n != 1),
            None,
            attrs,
            delim,
            marker,
        ));
    }
    // A single letter is ALPHA by default, EXCEPT a lone `i`/`I`, which defaults
    // to roman (§11 ambiguous-letter rule; the list parser may re-classify
    // either way when a consecutive sibling disambiguates).
    if marker.len() == 1 && !marker.eq_ignore_ascii_case("i") {
        let b = marker.as_bytes()[0];
        if b.is_ascii_lowercase() {
            return Some((
                content,
                Some((b - b'a' + 1) as usize).filter(|n| *n != 1),
                Some(OrderedListType::LowerAlpha),
                attrs,
                delim,
                marker,
            ));
        }
        if b.is_ascii_uppercase() {
            return Some((
                content,
                Some((b - b'A' + 1) as usize).filter(|n| *n != 1),
                Some(OrderedListType::UpperAlpha),
                attrs,
                delim,
                marker,
            ));
        }
    }
    let roman = roman_to_int(marker)?;
    Some((
        content,
        Some(roman).filter(|n| *n != 1),
        Some(if marker.chars().all(|c| c.is_ascii_uppercase()) {
            OrderedListType::UpperRoman
        } else {
            OrderedListType::LowerRoman
        }),
        attrs,
        delim,
        marker,
    ))
}

fn roman_to_int(s: &str) -> Option<usize> {
    let mut total = 0isize;
    let mut prev = 0isize;
    for ch in s.chars().rev() {
        let val = match ch.to_ascii_lowercase() {
            'i' => 1,
            'v' => 5,
            'x' => 10,
            'l' => 50,
            'c' => 100,
            'd' => 500,
            'm' => 1000,
            _ => return None,
        };
        if val < prev {
            total -= val;
        } else {
            total += val;
            prev = val;
        }
    }
    (total > 0).then_some(total as usize)
}

fn detect_task(line: &str) -> Option<(bool, &str, Option<Attrs>, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let c = bytes[i];
    // `+` is the list continuation marker, not a bullet (#80).
    if c != b'-' && c != b'*' {
        return None;
    }
    // An attribute block abuts the bullet, BEFORE the task marker:
    // `-{.c} [ ] text`. `marker_tail` consumes the optional block and the
    // bullet's required space; the task box `[x] ` then opens the content.
    let (after, attrs) = marker_tail(line, i + 1)?;
    let ab = after.as_bytes();
    if ab.len() < 4 || ab[0] != b'[' || ab[2] != b']' || ab[3] != b' ' {
        return None;
    }
    // PART 9 enumerates the states exhaustively:
    //
    //   task_state = ' ' | 'x' | 'X' | '-' | '_' | '>' | '?' ;
    //
    // The bracket shape was checked and the state byte was not, so any single
    // character opened a task item - and `- [!] urgent` did not merely
    // reinterpret the marker, it DELETED the `[!]` and rendered a checkbox
    // nobody wrote (carve-rs#471). Two characters were already rejected by the
    // `ab[2] != b']'` test; only the one-character case was open.
    if !matches!(ab[1], b' ' | b'x' | b'X' | b'-' | b'_' | b'>' | b'?') {
        return None;
    }
    let checked = matches!(ab[1], b'x' | b'X');
    Some((checked, trim_ascii_end(&after[4..]), attrs, &line[i..i + 1]))
}

/// Lower-alpha index of a single letter (`a`=1 … `z`=26), case-insensitive.
fn alpha_index(m: &str) -> Option<usize> {
    if m.len() != 1 {
        return None;
    }
    let b = m.as_bytes()[0].to_ascii_lowercase();
    (b.is_ascii_lowercase()).then(|| (b - b'a' + 1) as usize)
}

/// Resolve the §11 ambiguous-letter tie-break for an ordered list's FIRST
/// marker, returning its `(start, ol_type)`. A single roman-letter marker
/// (i/v/x/l/c/d/m) is reclassified to ROMAN when the next sibling is the
/// consecutive roman numeral, to ALPHA when the next is the consecutive letter;
/// otherwise the detector's default stands (lone `i`/`I` roman, others alpha).
fn resolve_ordered_first(
    first: &ListMarker<'_>,
    cur: &LineCursor,
    base_indent: usize,
) -> (Option<usize>, Option<OrderedListType>) {
    if !first.ordered || !is_ambiguous_roman_letter(first.marker) {
        return (first.start, first.ol_type);
    }
    // Find the next sibling ordered marker at the same indent, skipping the
    // first item's own body (blank lines and lines indented past the base).
    let mut sibling = None;
    for l in &cur.lines[cur.pos + 1..] {
        if is_blank_line(l) {
            continue;
        }
        if indent_columns(l) > base_indent {
            continue; // part of the first item's body
        }
        sibling = detect_list_marker_full(l).filter(|m| m.ordered && m.indent == base_indent);
        break;
    }
    let upper = first.marker.chars().all(|c| c.is_ascii_uppercase());
    let roman_type = if upper {
        OrderedListType::UpperRoman
    } else {
        OrderedListType::LowerRoman
    };
    let alpha_type = if upper {
        OrderedListType::UpperAlpha
    } else {
        OrderedListType::LowerAlpha
    };
    if let Some(sib) = sibling {
        let first_roman = roman_to_int(first.marker);
        let sib_roman = roman_to_int(sib.marker).filter(|_| !sib.marker.is_empty());
        if let (Some(fr), Some(sr)) = (first_roman, sib_roman) {
            // sibling is itself a roman-shaped marker and the consecutive value
            if sr == fr + 1 {
                return (Some(fr).filter(|n| *n != 1), Some(roman_type));
            }
        }
        if let (Some(fa), Some(sa)) = (alpha_index(first.marker), alpha_index(sib.marker)) {
            if sa == fa + 1 {
                return (Some(fa).filter(|n| *n != 1), Some(alpha_type));
            }
        }
    }
    (first.start, first.ol_type)
}

fn has_indexed_comment_closer_after(
    lines: &[&str],
    comment_closers: &mut Option<HashMap<usize, usize>>,
    start: usize,
    fence_len: usize,
) -> bool {
    if comment_closers.is_none() {
        *comment_closers = Some(build_comment_closer_last_index(lines));
    }
    comment_closers
        .as_ref()
        .and_then(|index| index.get(&fence_len).copied())
        .is_some_and(|last| last >= start)
}

/// The end index of the ONE block a `+` continuation marker attaches
/// (PART 9 §17 L3), scanning from `start` over `lines`.
///
/// A BOUNDARY LINE INSIDE AN OPEN FENCE DOES NOT END THE CONTAINER
/// (markup-carve/carve#983 corpus category 279). L3 bounds the attachment "up
/// to the next blank line, sibling marker, or a further `+`", and those bound
/// THE BLOCK: a fenced block ends at its CLOSER, which is what makes it one
/// block, so a boundary line written between an opener and its closer is fence
/// content and ends nothing.
///
/// ONE SPELLING FOR EVERY CONTAINER. `is_boundary` is the only per-container
/// part; the four fence shapes are not. This scan is
/// `parse_continuation_block`'s, moved here unchanged so that collector keeps
/// its exact behavior and the other four gain it. The second closure argument
/// is the index of the line being tested.
///
/// `comment_closers` is the caller's lazily built exact-width `%%%` closer
/// index. It is a parameter rather than a local because REBUILDING it per call
/// is quadratic on a document full of comment openers.
/// How many of `slice`'s lines the ONE block a `+` continuation marker attaches
/// occupies (§17 L3, markup-carve/carve#1290).
///
/// `slice` is the marker's EXTENT, already bounded by the caller's blank-line /
/// sibling / further-`+` scan. Within it the marker takes one block, which is
/// exactly what the single-block parser consumes - a wrapped paragraph, a list,
/// a quote and a fenced block are each one block and each many lines.
///
/// The block is parsed here only to be MEASURED; the caller splices the lines
/// into the container's body, where they parse again in their real context.
/// Measuring by re-parsing rather than by a second line scan keeps one
/// definition of where a block ends - a scan would be a copy of the block
/// grammar that could drift from it silently.
///
/// A LEADING ATTRIBUTE RUN IS PART OF THE BLOCK IT FLOATS ONTO. Only
/// `parse_blocks` owns a pending-attribute slot, and this is a `parse_block`
/// call, so an attribute line left to it reads as a paragraph and the
/// measurement stops in front of the block the attributes were written for.
/// `> q` / `+` / `{.x}` / `# h` then attached the attribute line ALONE, dropped
/// the attributes and left the heading outside the quote - where the list form
/// attaches an attributed heading. The run is consumed here so the block behind
/// it is what gets measured, exactly as `parse_continuation_block` does it.
///
/// A SELF-DELIMITING BLOCK IS NOT PARSED AT ALL. A fence and a colon container
/// end at a CLOSER, which is a line-level fact that `attached_block_end` already
/// reads with these same helpers - so their extent is taken from the lines and
/// the body is never walked. This is what keeps a deeply nested attachment
/// affordable: parsing the body would re-walk the whole subtree at every level
/// above it. Measured on `> q` / `+` / `::: d` nested to the cap: 9.52 s when the
/// probe parsed the container, 0.2 s when it reads the closer, against 0.22 s
/// for the same document before this clause was implemented at all.
///
/// MEASURING DOES NOT NEST either. The probe parses the attached block and the
/// caller then parses those same lines again, so an inner `+` under a probe
/// would be measured twice per level - doubling per level, at 0.02 s / 0.06 s /
/// 0.26 s / 1.05 s for depths 8 / 10 / 12 / 14 against a flat 0.00 s before. An
/// inner marker under a probe therefore splices its whole extent instead. That
/// cannot change the answer here: what this returns is a LINE COUNT, and a
/// block's line extent is decided by closers, quote prefixes and indentation -
/// never by how an inner marker divided its own content.
///
/// At least one line, always. A parser that consumed nothing would leave the
/// caller's cursor where it was, and the container loop would see the same line
/// forever.
fn attached_one_block_lines(slice: &[&str], options: &Options<'_>) -> usize {
    if slice.is_empty() {
        return 0;
    }
    // A cache OF ITS OWN, never the caller's. The closer index is keyed by line
    // number, and `slice` is renumbered from the container's own lines - handing
    // it an index built over those would read a closer at the wrong place.
    let mut comment_closers: Option<HashMap<usize, usize>> = None;
    let _measuring = MeasuringGuard::enter();
    let mut sub = LineCursor::new_with_cols(slice, None, None);
    while let Some(line) = sub.peek() {
        if line.starts_with([' ', '\t']) || !line.starts_with('{') {
            break;
        }
        if parse_standalone_attrs_block(&mut sub).is_none() {
            break;
        }
    }
    if let Some(end) = self_delimiting_block_end(slice, sub.pos, &mut comment_closers) {
        return end.clamp(1, slice.len());
    }
    parse_block(&mut sub, options);
    sub.pos.clamp(1, slice.len())
}

/// One past the last line of the self-delimiting block opening at `slice[start]`,
/// or `None` when that line opens none.
///
/// The five openers and the four helpers are the ones [`attached_block_end`]
/// skips regions with, reused rather than restated: a fence or colon container
/// runs to its closer, or to end of input when it has none, and nothing inside
/// it can shorten that.
fn self_delimiting_block_end(
    slice: &[&str],
    start: usize,
    comment_closers: &mut Option<HashMap<usize, usize>>,
) -> Option<usize> {
    let line = *slice.get(start)?;
    if let Some(open) = detect_fence_open(line) {
        let mut i = start + 1;
        while i < slice.len() {
            if is_fence_close(slice[i], open) {
                return Some(i + 1);
            }
            i += 1;
        }
        return Some(slice.len());
    }
    if let Some(open) = detect_comment_fence_line(line) {
        if has_indexed_comment_closer_after(slice, comment_closers, start + 1, open.fence_len) {
            if let Some(close) =
                (start + 1..slice.len()).find(|&j| is_comment_fence_close(slice[j], open.fence_len))
            {
                return Some(close + 1);
            }
        }
        // §28: with no closer ahead it is not a fence, it is a `%%` line
        // comment - one line, and the lines after it are just lines.
        return Some(start + 1);
    }
    if let Some(fence_len) = detect_line_block_open(line) {
        return Some(find_line_block_end(slice, start, fence_len));
    }
    if let Some(fence_len) = detect_hardbreaks_block_open(line) {
        return Some(find_colon_container_end(slice, start, fence_len));
    }
    if let Some(open) = detect_container_open(line) {
        return Some(find_colon_container_end(slice, start, open.fence_len));
    }
    None
}

thread_local! {
    // Set while [`attached_one_block_lines`] is measuring, so a `+` marker
    // reached inside the probe splices its extent rather than running a probe of
    // its own. See that function for why the count is unaffected and for the
    // cost this avoids.
    //
    // Plain initializer for the same MSRV reason as `NESTING_DEPTH`.
    #[allow(clippy::missing_const_for_thread_local)]
    static MEASURING_ATTACHED_BLOCK: Cell<bool> = const { Cell::new(false) };
}

/// RAII flag for the measurement probe, restoring the previous value on drop
/// (panic unwind included), the discipline [`DepthGuard`] keeps for the depth
/// counter.
struct MeasuringGuard {
    previous: bool,
}

impl MeasuringGuard {
    fn enter() -> MeasuringGuard {
        MeasuringGuard {
            previous: MEASURING_ATTACHED_BLOCK.with(|m| m.replace(true)),
        }
    }
}

impl Drop for MeasuringGuard {
    fn drop(&mut self) {
        MEASURING_ATTACHED_BLOCK.with(|m| m.set(self.previous));
    }
}

fn measuring_attached_block() -> bool {
    MEASURING_ATTACHED_BLOCK.with(|m| m.get())
}

fn attached_block_end(
    lines: &[&str],
    start: usize,
    comment_closers: &mut Option<HashMap<usize, usize>>,
    is_boundary: &mut dyn FnMut(&str, usize) -> bool,
) -> usize {
    let mut end = start;
    let mut in_fence: Option<FenceOpen> = None;
    while end < lines.len() {
        let line = lines[end];
        if let Some(open) = in_fence {
            if is_fence_close(line, open) {
                in_fence = None;
            }
            end += 1;
            continue;
        }
        if let Some(open) = detect_fence_open(line) {
            in_fence = Some(open);
            end += 1;
            continue;
        }
        if let Some(open) = detect_comment_fence_line(line) {
            if has_indexed_comment_closer_after(lines, comment_closers, end + 1, open.fence_len) {
                if let Some(close) = (end + 1..lines.len())
                    .find(|&j| is_comment_fence_close(lines[j], open.fence_len))
                {
                    end = close + 1;
                    continue;
                }
            }
        }
        // A colon fence is a self-delimiting block; skip the whole region so a
        // `+` inside it is content, not the parent's boundary. Unterminated
        // colon containers close at end of input.
        if let Some(fence_len) = detect_line_block_open(line) {
            end = find_line_block_end(lines, end, fence_len);
            continue;
        }
        if let Some(fence_len) = detect_hardbreaks_block_open(line) {
            end = find_colon_container_end(lines, end, fence_len);
            continue;
        }
        if let Some(open) = detect_container_open(line) {
            end = find_colon_container_end(lines, end, open.fence_len);
            continue;
        }
        if is_boundary(line, end) {
            break;
        }
        end += 1;
    }
    end
}

/// Parse ONE block attached by a list `+` continuation marker, bounded to the
/// lines before the next lone `+` marker at the item's base indent. The scan is
/// fence-aware -- a `+` inside a nested fenced code block is content, not a
/// boundary -- so a greedy block (e.g. a block quote's lazy continuation)
/// cannot swallow the following `+` and its block. `- a / + / >q1 / + / >q2`
/// then yields two separate quotes. Advances `cur` by the lines consumed.
fn parse_continuation_block(
    cur: &mut LineCursor,
    options: &Options<'_>,
    base_indent: usize,
) -> Option<BlockNode> {
    // A nested list manages its OWN `+` continuations -- the boundary scan
    // cannot tell a child list's `+` from the parent's, so a list is parsed
    // unbounded. (Code / colon fences are handled INSIDE the scan: code fences
    // with matching closers are skipped, and colon fences close at their closer
    // or EOF, so inner `+` lines are content rather than parent boundaries.)
    if let Some(line) = cur.peek() {
        if let Some(nm) = detect_list_marker_full(line) {
            // A marker indented past the base nests as a child list of THIS
            // item, so parse it unbounded (it manages its own `+`). But a
            // marker AT or BELOW the outer base column is a SIBLING of the
            // outer list, not content of this `+`-attached block: bound the
            // block to empty so the outer list takes the marker as a sibling
            // item rather than nesting it (matches carve-php for `+`-then-
            // marker, e.g. `- a / + / text / + / - b`).
            if nm.indent > base_indent {
                return parse_block(cur, options);
            }
            return None;
        }
    }
    let lines = cur.lines;
    let start = cur.pos;
    let end = attached_block_end(
        lines,
        start,
        &mut cur.comment_closer_last_index,
        &mut |line, end| {
            // A FURTHER `+` ends this attachment, on the first line as on any other.
            // §17 L3 lists the terminators as "the next blank line, sibling marker,
            // or a further `+`" and makes no exception for an attachment that has
            // taken nothing yet. Requiring `end > cur.pos` here swallowed the
            // second marker as CONTENT of the first, so `- a` / `+` / `+` / `b`
            // rendered a literal `+` in the item where carve-js, carve-php and the
            // executable spec all attach `b` (carve-rs#704). The first marker
            // simply attaches an empty block, which adds nothing.
            //
            // The sibling-marker guard below keeps its `end > cur.pos` for a
            // different reason: a list marker on the first attached line is a
            // sibling ITEM, and ending the attachment there is what makes it one.
            if trim_ascii(line) == "+" && indent_columns(line) == base_indent {
                return true;
            }
            // A list marker at (or below) the base column is a SIBLING item of the
            // outer list, not part of this `+`-attached block. Bound the block here
            // so it is not absorbed -- now that a bullet does not interrupt, a
            // `> quote` (or other) block would otherwise swallow a following
            // `- next` as lazy continuation. Matches carve-js.
            end > start
                && indent_columns(line) <= base_indent
                && detect_list_marker_full(line).is_some()
        },
    );
    let slice: Vec<&str> = cur.lines[cur.pos..end].to_vec();
    let line_map: Vec<Option<usize>> = cur
        .line_map
        .map(|map| map[cur.pos..end].to_vec())
        .unwrap_or_default();
    // The attached lines are taken VERBATIM - nothing is stripped from them -
    // so the parent's column widths apply unchanged. Without this the sub-cursor
    // had no column map at all, and every block a `+` attached came out
    // unplaced: the code block, quote or table after the marker, and everything
    // inside it.
    let col_map: Vec<Option<isize>> = cur
        .col_map
        .map(|map| map[cur.pos..end].to_vec())
        .unwrap_or_default();
    let mut sub = LineCursor::new_with_cols(
        &slice,
        cur.line_map.is_some().then_some(line_map.as_slice()),
        cur.col_map.is_some().then_some(col_map.as_slice()),
    );
    // A BLOCK-ATTRIBUTE LINE IS A BLOCK, AND IT FLOATS TO THE NEXT ONE.
    // PART 2 lists `block_attributes` among the alternatives of `block`, so
    // PART 11's `continuation_marker_block = continuation_marker, block` admits
    // one after the marker, and PART 9 §15 sends it to the block that follows.
    // `parse_blocks` owns the only pending-attribute slot and this is a
    // `parse_block` call, so the line arrived here with nothing to read it and
    // fell through to a paragraph: `- a` / `+` / `{.x}` / `> q` rendered a
    // literal `{.x}` in the item and left the quote OUTSIDE it, where carve-js
    // and carve-php put the quote inside carrying the class
    // (markup-carve/carve-rs#1020, rule in markup-carve/carve#1238).
    //
    // AT THE MARKER'S OWN COLUMN, the same guard `parse_blocks` spells as
    // `line_flush`. These lines are taken VERBATIM (see above), so "flush"
    // is measured against `base_indent` rather than against column 0 - a `+`
    // inside a nested list attaches at ITS column. A line one column further in
    // is ordinary text (strict column-0 rule, docs/divergence-from-djot.md §11).
    //
    // Attribute blocks STACK, so this takes the whole leading run and merges
    // it, the way `parse_blocks` merges consecutive lines into one slot.
    let mut floating_attrs: Option<Attrs> = None;
    let mut floating_attrs_pos: Option<Pos> = None;
    while let Some(line) = sub.peek() {
        if indent_columns(line) != base_indent || !trim_ascii_start(line).starts_with('{') {
            break;
        }
        let attrs_start = sub.pos;
        let Some(attrs) = parse_standalone_attrs_block(&mut sub) else {
            break;
        };
        merge_attrs(&mut floating_attrs, attrs);
        if floating_attrs_pos.is_none() {
            floating_attrs_pos = span_of(&sub, attrs_start, sub.pos, options);
        }
    }
    // The block starts where the attribute run ended, which is what the source
    // stamp below has to name: `parse_blocks` likewise takes `start_line` after
    // its own attribute lines are consumed. Stamping the slice's first line
    // would point an editor's scroll-sync at the `{…}` line instead.
    let block_start = sub.pos;
    let mut block = parse_block(&mut sub, options);
    if let Some(attrs) = floating_attrs {
        // AN EMPTY PARAGRAPH IS NOT A BLOCK THE AUTHOR WROTE THESE FOR. It is
        // what a `parse_block` call returns when the extent held no content -
        // `- a` / `+` / `{.x}` / blank / `> q`, where §17 L3 bounds the
        // attachment at the blank - and `parse_list` filters it out again, so
        // attributes applied to it were discarded one step later without
        // anything noticing. The set reaches nothing; §15 A4 drops it and says
        // so (markup-carve/carve#1281).
        //
        // This does NOT change a byte of HTML or of the AST: the node it used
        // to attach to never reached the tree. What it changes is that the loss
        // is now reported, which is the whole of the rule.
        let reaches_nothing = match &block {
            None => true,
            Some(BlockNode::Paragraph(p)) => p.children.is_empty(),
            // AND A BLOCK THAT TAKES NO ATTRIBUTES IS NOT ONE EITHER.
            // `apply_attrs_to_block` ends in `_ => {}`, so handing it a comment
            // or an abbreviation definition discards the set exactly as having
            // no block at all would - and §15 A2a's "float past what renders
            // nothing" cannot save it here, because a `+` attaches ONE block and
            // there is no next one to float to. `- a` / `+` / `{.x}` / `%% c`
            // dropped the attributes with nothing reporting it, while the
            // document-level twin `{.x}` / blank / `%% c` reported them.
            Some(BlockNode::Comment(_)) | Some(BlockNode::AbbreviationDef(_)) => true,
            Some(_) => false,
        };
        if reaches_nothing {
            note_unattached_block_attrs(floating_attrs_pos);
        } else if let Some(node) = &mut block {
            // The invisible-block guard is now ABOVE, in `reaches_nothing`.
            // It was removed from here once, correctly: it could not change a
            // byte of HTML or of the AST, and a check that cannot fail is the
            // defect class markup-carve/carve#755 catalogs. §15 A4's diagnostic
            // is what gives it something to change - the set is still dropped
            // either way, and now the drop is reported.
            apply_attrs_to_block(node, attrs);
        }
    }
    if options.source_lines {
        if let Some(block) = &mut block {
            if let Some(line) = line_map.get(block_start).copied().flatten() {
                stamp_source_line(block, line);
            }
        }
    }
    cur.pos += sub.pos;
    block
}

/// A list's extent, taken from the items it holds.
///
/// Used only when the cursor cannot supply one. Both ends have to exist, or the
/// range would start or stop somewhere arbitrary - so a list whose first or last
/// item is unplaced stays unplaced itself rather than reporting a partial span.
fn span_across_items(items: &[ListItem]) -> Option<Pos> {
    let first = items.first()?.pos?;
    let last = items.last()?.pos?;
    Some(Pos {
        end_line: last.end_line,
        end_column: last.end_column,
        end_offset: last.end_offset,
        ..first
    })
}

/// Widen each item's span to cover the blocks inside it.
///
/// An item's span is fixed when the item is BUILT, and the loop then attaches
/// later blocks - an indented paragraph, a nested list, a quote, a `+`
/// continuation - to `items.last_mut()`. Every one of those landed after the
/// span was taken, so the item claimed its marker line and the rest of the item
/// sat outside it: 55 nodes across the spec corpus, all of them invisible,
/// because a span is compared against source text for `text` nodes alone
/// (carve#565).
///
/// Widening after the fact rather than re-taking the span at each of the six
/// push sites: the sites disagree about what they have in scope, and a rule
/// that runs once cannot be half-applied by the next branch someone adds.
fn widen_items_over_children(items: &mut [ListItem]) {
    for item in items.iter_mut() {
        let Some(mut pos) = item.pos else { continue };
        let mut last_owned: Option<Pos> = None;
        for child in &item.children {
            let Some(child_pos) = crate::ast_json::block_pos(child) else {
                continue;
            };
            // LINE and COLUMN, not offsets: a span carries the pair at parse
            // time and `fill_offsets` turns it into offsets afterwards, so
            // comparing offsets here compares two zeroes and widens nothing.
            let is_later = match last_owned {
                None => true,
                Some(last) => {
                    (child_pos.end_line, child_pos.end_column) > (last.end_line, last.end_column)
                }
            };
            if is_later {
                last_owned = Some(*child_pos);
            }
        }
        if let Some(last) = last_owned {
            pos.end_line = last.end_line;
            pos.end_column = last.end_column;
            pos.end_offset = last.end_offset;
        }
        item.pos = Some(pos);
    }
}

fn parse_list(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let span_start = cur.pos;
    let first = cur.peek().unwrap();
    let first_marker = detect_list_marker_full(first).unwrap();
    let base_indent = first_marker.indent;
    let is_task = first_marker.checked.is_some();
    let is_ordered = first_marker.ordered;
    // §11 ambiguous-letter tie-break: a single roman-letter first marker is
    // roman or alpha depending on its consecutive sibling. Resolve it against
    // the next sibling marker before fixing the list's type.
    let (start, ol_type) = resolve_ordered_first(&first_marker, cur, base_indent);
    let first_delim = first_marker.delim;
    let first_dialect = ol_dialect(ol_type);
    let mut items: Vec<ListItem> = Vec::new();
    let mut tight = true;
    let mut pending_blank = false;
    // A block-attribute block that ended a continuation chunk, waiting for the
    // SUB-LIST it was written in front of. The chunk boundary is what separated
    // them (see `split_trailing_attrs`), so this carries them across it.
    //
    // It survives blank lines, because a blank does not break attachment
    // anywhere else either - `{.x}` / blank / `- b` at document level attaches.
    // Everything that opens an item or attaches a block of its own clears it,
    // so a set of attributes can only ever reach the sub-list that directly
    // follows it, and one that reaches nothing is dropped exactly as before.
    let mut pending_attrs: Option<Attrs> = None;
    // Where that set was written, for §15 A4's diagnostic. This slot holds
    // attributes LIFTED off a chunk (`split_trailing_attrs`), so the span comes
    // from the chunk's own maps rather than from the cursor.
    let mut pending_attrs_pos: Option<Pos> = None;
    // The current item's content column (where its content begins after the
    // marker). Nested content and sub-blocks of the last item dedent by this, so
    // it persists across iterations and is updated as each item is opened.
    let mut content_col = base_indent + 2;
    // The current item's own fenced code block, still OPEN. A FENCED BODY IS
    // NOT A PARAGRAPH, so nothing below the content column folds into the item
    // while one is open: PART 9 §24's S1 stops at the ITEM, S2 never fires, and
    // S4's lazy branch wants a paragraph a verbatim body is not - the item and
    // the list close, and the residue re-parses in the surviving context
    // (markup-carve/carve#950). It lives here, beside `content_col`, because the
    // guard is on the OPEN FENCE rather than on where the fence was opened: the
    // opener may be the MARKER line, which the collectors never see, or a later
    // CONTINUATION line, after the item's paragraph state has reopened.
    let mut item_open_fence: Option<FenceOpen> = None;
    while let Some(line) = cur.peek() {
        if is_blank_line(line) {
            // A blank alone does not loosen the list; it loosens only when the
            // next line is a sibling item (handled at the marker branch via
            // pending_blank) or an indented second paragraph (below). A blank
            // before any other indented block keeps it compact (#74).
            pending_blank = true;
            cur.consume();
            continue;
        }
        // Lone `+` continuation marker (Carve): attaches the next flush-left
        // block to the current item without indentation.
        if trim_ascii(line) == "+" && indent_columns(line) == base_indent {
            cur.consume();
            pending_blank = false;
            // A marker is not the block these were written for either (§15 A4).
            note_unattached_block_attrs(
                pending_attrs_pos.take().filter(|_| pending_attrs.is_some()),
            );
            pending_attrs = None;
            if let Some(block) = parse_continuation_block(cur, options, base_indent) {
                // An EMPTY paragraph is not content and must not become one of
                // the item's blocks. It arises when the attached block's whole
                // content was a collected definition: the prepass blanks that
                // line, so what parses is a paragraph with no children.
                //
                // Filtering it in the RENDERER (carve-rs#670) fixed the HTML and
                // left the tree carrying the node - which is the trace spec
                // markup-carve/carve#801 says a collected definition must not
                // leave, and the three-way shape comparison shows this engine
                // standing alone on corpus 226 because of it. carve-js and
                // carve-php build no node at all.
                let is_empty_paragraph = matches!(
                    &block,
                    BlockNode::Paragraph(p) if p.children.is_empty()
                );
                if let Some(last) = items.last_mut() {
                    if !is_empty_paragraph {
                        last.children.push(block);
                    }
                }
            }
            continue;
        }
        let Some(marker) = detect_list_marker_full(line) else {
            let indent = indent_columns(line);
            if indent > base_indent {
                // After a blank line, lazy continuation no longer applies: a line
                // must reach the item's content column to keep belonging to it
                // (PART 9 §24 C3). A line BELOW content_column ends the list and
                // parses at document level (corpus 81-list-lazy-5). content_col
                // is the item's true column (`- `=2, `1. `=3, `10. `=4), NOT a
                // fixed base+2 -- an ordered item's body sits deeper.
                if pending_blank && indent < content_col {
                    break;
                }
                // The item holds an OPEN fenced body and this line is below its
                // content column, so §24 closes the item and the list here
                // rather than folding the line in (#950). The collector below
                // stops on the same state and would otherwise return nothing,
                // leaving the cursor where it is.
                if item_open_fence.is_some() && indent < content_col {
                    break;
                }
                if let Some(last) = items.last_mut() {
                    let mut nested = collect_item_continuation_block_mapped(
                        cur,
                        base_indent,
                        content_col,
                        &mut item_open_fence,
                    );
                    // A heading keeps the item open for flush-left lazy text.
                    // The heading itself ends at its newline and absorbs
                    // nothing (PART 2 SINGLE-LINE HEADINGS), but when the
                    // indented block ends in a heading the following flush-left
                    // lines still belong to the ITEM, so pull them in: they
                    // become the item's own content beside the heading rather
                    // than the list ending and the text floating to the top
                    // level (corpus 73-list-nesting-and-looseness-4; matches
                    // carve-php / carve-js, carve#326). A blank BEFORE the
                    // heading is irrelevant to whether text AFTER it belongs to
                    // the item, so this is not gated on pending_blank
                    // (collect_trailing_lazy still stops at a blank of its own).
                    // Only headings keep the item open this way: a code block or
                    // table leaves its trailing text a separate top-level block.
                    //
                    // CommonMark lazy continuation otherwise: the dedented
                    // non-blank line folds into the nested block's deepest open
                    // paragraph (e.g. a block quote's or an unterminated div's
                    // trailing paragraph) so it stays INSIDE the item. The
                    // recursive block parse absorbs it.
                    //
                    // And then collection RESUMES, because S4's lazy branch
                    // closes nothing -- see fold_lazy_run_and_resume. This
                    // branch used to fold once and stop, which put the line
                    // AFTER the folded one outside the container it never left
                    // (carve#980, carve-rs#813).
                    fold_lazy_run_and_resume(
                        cur,
                        &mut nested,
                        content_col,
                        |src, below| collected_body_takes_the_lazy_line(src, below, options),
                        |cur| {
                            collect_item_continuation_block_mapped(
                                cur,
                                base_indent,
                                content_col,
                                &mut item_open_fence,
                            )
                        },
                    );
                    let definition_ended_paragraph = nested
                        .source
                        .lines()
                        .any(|line| trim_ascii(line) == DEFINITION_PLACEHOLDER);
                    // A `{…}` block that ENDS this chunk was written in front of
                    // whatever comes next, and what comes next is in the next
                    // chunk - the collector broke on the sub-list marker. Hold
                    // it rather than letting the chunk's own pending slot drop
                    // it (carve-rs#1007). Split BEFORE parsing, so the block it
                    // was going to attach to inside this chunk is unaffected.
                    //
                    // Repeatedly, because attribute blocks STACK: `{.x}` /
                    // `{#i}` in front of one block merges into a single set
                    // everywhere else, and lifting only the last one off would
                    // have made the nested list the one target that keeps just
                    // the final block. Each split shortens the chunk, so this
                    // terminates.
                    let mut split_stack: Vec<(Attrs, Option<Pos>)> = Vec::new();
                    while let Some(split) = split_trailing_attrs(&mut nested) {
                        split_stack.push(split);
                    }
                    let mut nested_children = parse_mapped_source(&nested, options);
                    // Attributes held from an EARLIER chunk attach here when
                    // this one opens with a block, which is the case where a
                    // blank line sits between the `{…}` and its target. The
                    // sub-list branch is the other consumer; between them, a set
                    // of attributes reaches the first block written after it
                    // whichever chunk that landed in.
                    if pending_attrs.is_some() {
                        if let Some(target) = nested_children
                            .iter_mut()
                            .find(|block| !matches!(block, BlockNode::Comment(_)))
                        {
                            apply_attrs_to_block(target, pending_attrs.take().unwrap());
                            pending_attrs_pos = None;
                        }
                    }
                    // Back into SOURCE order, so the merge resolves a repeated
                    // key the way `parse_blocks` resolves it at top level.
                    for (attrs, pos) in split_stack.into_iter().rev() {
                        if pending_attrs.is_none() {
                            pending_attrs_pos = pos;
                        }
                        merge_attrs(&mut pending_attrs, attrs);
                    }
                    // A blank before an indented sub-block loosens only when it
                    // is a genuine second paragraph (#74 compact list blocks).
                    // Skip what renders NOTHING when looking for that
                    // paragraph. An invisible line does not cancel the blank
                    // line above it (PART 9 §17 L1b, markup-carve/carve#630):
                    // `- a` / blank / `  %% n` / `  text` holds a second
                    // paragraph with a comment in front of it, and reading only
                    // the FIRST child found the comment and called the item
                    // tight. Matches carve-js.
                    let first_visible = nested_children
                        .iter()
                        .find(|block| !matches!(block, BlockNode::Comment(_)));
                    if pending_blank && matches!(first_visible, Some(BlockNode::Paragraph(_))) {
                        tight = false;
                    }
                    // A blank ABSORBED inside the collected continuation (e.g. a
                    // fence/div/table followed by a blank and then trailing text)
                    // loosens the item when a plain paragraph follows the blank
                    // (§17 L1). The outer `pending_blank` only sees a blank BEFORE
                    // this chunk, so this covers the blank-after / blank-both case.
                    if continuation_source_loosens(&nested.source) {
                        tight = false;
                    }
                    // An INVISIBLE continuation is not the item's second block
                    // (§17 L2 - it renders nothing), but it does not consume the
                    // blank either: the blank still sits between this item and
                    // whatever follows, and a blank before a SIBLING loosens the
                    // list. Clearing it here made `- a` / blank / `  %% note` /
                    // `- b` tight, where carve-js and the corpus have it loose
                    // (carve-rs#557, corpus 87-compact-list-blocks-6).
                    let renders_nothing = nested_children.iter().all(|block| {
                        matches!(
                            block,
                            BlockNode::Comment(_)
                                | BlockNode::AbbreviationDef(_)
                                | BlockNode::LinkReferenceDefinition(_)
                        )
                    });
                    if !renders_nothing {
                        pending_blank = false;
                    }
                    last.children.extend(nested_children);
                    // A collected definition is an I5 block, not the comment
                    // exception. If the collector stopped on a nonzero line
                    // below the item's content column, no paragraph remains
                    // for that line to continue (markup-carve/carve#1376).
                    if definition_ended_paragraph
                        && cur.peek().is_some_and(|line| {
                            let indent = indent_columns(line);
                            indent > 0 && indent < content_col
                        })
                    {
                        break;
                    }
                    continue;
                }
            }
            // §24 C3: a comment is recognized at ANY column and renders nothing,
            // so a FLUSH-LEFT one does not close the item it follows - nor the
            // list. `- a` / `%% c` / `b` keeps `b` in the item as a second
            // paragraph, and a following sibling marker resumes the SAME list,
            // as carve-js, carve-php and the executable spec all have it
            // (carve-rs#562). Collecting the comment into the item also stops a
            // TRAILING one from being hoisted to document level, where the other
            // engines leave it inside the item that preceded it.
            //
            // Two neighbours are deliberately excluded, both measured against
            // all three: a comment FENCE (`%%%`) ends the list, and so does a
            // comment after a BLANK line - past a blank the item is closed
            // already, and nothing reopens it.
            if !pending_blank && is_flush_line_comment(line) {
                // A collected definition at this container's column zero is
                // not the comment exception below. It is a column-scoped I5
                // interrupter: below the open item's content column it closes
                // the item, then the surrounding block parser consumes the
                // placeholder and parses what follows as a sibling. At the
                // item's content column the indented continuation branch above
                // already collected it inside the item (corpus 228).
                if trim_ascii(line) == DOCUMENT_DEFINITION_PLACEHOLDER {
                    break;
                }
                if let Some(last) = items.last_mut() {
                    let mut nested = MappedSource::new_line_at(
                        line.to_string(),
                        cur.source_line(cur.pos),
                        cur.source_col(cur.pos),
                    );
                    cur.consume();
                    // The lazy text after the comment belongs to the item too,
                    // and parsing it TOGETHER with the comment is what makes it
                    // a second paragraph rather than a continuation of the
                    // first: a comment ends the paragraph above it (§10) while
                    // leaving its container open.
                    collect_trailing_lazy(cur, &mut nested);
                    last.children.extend(parse_mapped_source(&nested, options));
                    continue;
                }
            }
            break;
        };
        if marker.indent < base_indent {
            break;
        }
        if marker.indent > base_indent {
            // A marker indented past the base nests as a sub-list. (An ordered
            // marker BELOW the content column never reaches here -- it folds
            // into the item paragraph in the per-item loop below, §10. Unordered
            // and task markers always interrupt, so they nest at any indent.)
            if pending_blank && marker.indent < content_col {
                break;
            }
            // The same fenced-body guard the non-marker branch above applies. A
            // MARKER below the content column is a line below the content
            // column, and §24 stops at the ITEM for both: S1 walks the stack by
            // the indentation a line supplies, which has nothing to do with
            // what the line says. Whether the residue is a sub-list or prose is
            // decided in the surviving context, AFTER the containers close - so
            // reading the marker first nested it inside the very item the open
            // fence closes (markup-carve/carve#950).
            if item_open_fence.is_some() && marker.indent < content_col {
                break;
            }
            if let Some(last) = items.last_mut() {
                let sub_indent = marker.indent;
                let mut nested = collect_indented_block_mapped(cur, base_indent, content_col);
                // A column-0 lazy-continuation line folds into the sub-list's
                // last open paragraph (e.g. `inner` / `lazy`). It must NOT close
                // the sub-list: a following sibling marker at the sub-list's own
                // column (`2. sibling`) resumes the SAME list. Loop folding the
                // lazy line, then resume collecting the sub-list continuation, so
                // the sibling joins the open list rather than starting a new one
                // (corpus 05-lists-17, matches carve-php / carve-js).
                //
                // Only fold the column-0 lazy line when the collected content
                // still ends in an OPEN paragraph OR a heading (a heading folds
                // trailing plain text as continuation, carve#326). After a
                // CLOSED block (fenced code, table, closed div) there is
                // neither, so the dedented line ends the item -> top-level.
                fold_lazy_run_and_resume(
                    cur,
                    &mut nested,
                    content_col,
                    |src, below| collected_body_takes_the_lazy_line(src, below, options),
                    |cur| collect_indented_block_mapped(cur, sub_indent - 1, content_col),
                );
                let mut nested_children = parse_mapped_source(&nested, options);
                // The block-attribute block written in front of this sub-list,
                // carried across the chunk boundary that separated them
                // (carve-rs#1007). It lands on the nested LIST, not on the item
                // and not on the outer list: `apply_attrs_to_block` already has
                // the arm, and a list is a block like the paragraph, quote and
                // fence in this position that have always taken them.
                //
                // The first non-comment child, matching how the looseness check
                // below picks the block that counts - a comment renders nothing
                // and is not what the author wrote the attributes for.
                if let Some(attrs) = pending_attrs.take() {
                    pending_attrs_pos = None;
                    if let Some(target) = nested_children
                        .iter_mut()
                        .find(|block| !matches!(block, BlockNode::Comment(_)))
                    {
                        apply_attrs_to_block(target, attrs);
                    }
                }
                // A blank line INSIDE the outer item -- swallowed into the nested
                // source by the collection above -- that directly separates the
                // sub-list from a following PARAGRAPH still attached to the outer
                // item makes the OUTER item loose. This is the same paragraph-only
                // rule the plain-continuation branch applies via `pending_blank`
                // (matches carve-js). The check is precise: the blank must
                // directly precede outer-item content (not inner-item content or a
                // sibling marker -- corpus 142: nested looseness does not
                // propagate) and that content must begin a paragraph (a `<hr>`,
                // block quote, or other block opener does not loosen).
                if sublist_source_loosens_outer_item(&nested.source) {
                    tight = false;
                }
                // The blank BEFORE this sub-list is consumed by it and must not
                // survive to loosen a later sibling marker (§17 L2: a blank
                // before an item's sub-block keeps the item tight). Without
                // this, `- a` / blank / `  - b` / `- c` loosened at `- c`
                // because pending_blank leaked past the sub-list, while the same
                // blank before a plain continuation block cleared it below.
                // Matches carve-js / carve-php (carve-rs#286). A blank AFTER the
                // sub-list still re-raises pending_blank in the blank branch, so
                // a genuine blank BETWEEN items keeps loosening.
                pending_blank = false;
                last.children.extend(nested_children);
                continue;
            }
            break;
        }
        // Past the sub-list branch, so this marker opens a SIBLING item. Any
        // attributes still held were written in front of something that never
        // came, and a sibling is not that something - drop them here rather
        // than let them reach a sub-list further down the list, which is not
        // where the author put them (carve-rs#1007). Reported rather than
        // silent, like every other way of running out (§15 A4,
        // markup-carve/carve#1281).
        note_unattached_block_attrs(pending_attrs_pos.take().filter(|_| pending_attrs.is_some()));
        pending_attrs = None;
        if marker.ordered != is_ordered || marker.checked.is_some() != is_task {
            break;
        }
        if !is_ordered && marker.marker != first_marker.marker {
            break;
        }
        // §11: an ordered item whose delimiter (`.` vs `)`) or dialect family
        // (decimal / alpha / roman, case included) differs from the list's first
        // item starts a NEW sibling list. Skip the FIRST item: its own detected
        // dialect may differ from the list's resolved (tie-broken) dialect
        // (`v.` detects alpha but the list is roman), and it can never split
        // from itself.
        if is_ordered
            && !items.is_empty()
            && (marker.delim != first_delim || !dialect_compatible(first_dialect, &marker))
        {
            break;
        }
        if pending_blank {
            tight = false;
            pending_blank = false;
        }
        // This item's content column. For ordered/unordered it is where the
        // marker content begins (`- `=2, `1. `=3, `10. `=4). For a TASK the
        // checkbox is content, not marker, so the column is the bullet width
        // (`- `/`* ` = 2) -- a child indented to 2 nests, matching the spec's
        // task attribute/continuation convention (`- [x] x` / `  {.c}`).
        content_col = if marker.checked.is_some() {
            base_indent + 2
        } else {
            let l = cur.peek().unwrap();
            let byte_off = (marker.content.as_ptr() as usize).saturating_sub(l.as_ptr() as usize);
            indent_columns(l) + byte_off.saturating_sub(leading_ws(l))
        };
        // A new item carries none of the previous item's fence state.
        item_open_fence = None;
        let item_source_line = cur.source_line(cur.pos);
        let item_at = cur.pos;
        cur.consume();
        let item_attrs = source_line_attrs(marker.attrs.clone(), item_source_line, options);
        // First-block form `- +` (grammar §17): a lone `+` as the marker
        // content means the item's first block is the following flush-left
        // block (no inline paragraph).
        if trim_ascii(marker.content) == "+" {
            let mut item = ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                children: Vec::new(),

                pos: None,
            };
            if let Some(block) = parse_continuation_block(cur, options, base_indent) {
                item.children.push(block);
            }
            // The item runs from its marker line through the block the `+`
            // attached - it was the only list item built with a hardcoded
            // `None`, so an item written this way had no span while its
            // siblings and its own contents did.
            item.pos = span_of(cur, item_at, cur.pos, options);
            items.push(item);
            continue;
        }
        // When the item's content BEGINS, on the marker line, with another list
        // marker (`- - A`, `* - A`, `1. - A`, ...), the lead is itself a
        // sub-list, not a paragraph. Parse the lead together with every
        // following dedented line as ONE block stream so the marker-line
        // sub-list behaves exactly like a sub-list opened on a *following* line:
        // following same-indent markers MERGE into it as siblings, and
        // post-blank indented blocks are ABSORBED into its items. This MATCHES
        // reference djot.js (@djot/djot 0.3.2) and CommonMark, which both treat
        // a marker-line sub-list as a normal nested list. It corrects Carve's
        // prior line-scoping (which split the sub-list from following items and
        // leaked later indented blocks to the parent row) -- a bug inherited
        // from djot-php, whose marker-line handling deviates from reference
        // djot. The combined stream reuses the normal nested-list/absorption
        // logic (collect_indented_block + recursive parse) -- no separate path.
        if marker.content.starts_with('>') {
            let mut stream = item_marker_source(cur, marker.content, item_at);
            stream.append(collect_indented_block_mapped(cur, base_indent, content_col));
            // A column-0 line after the item folds into the quote's open
            // paragraph, exactly as it does when the quote is written on the
            // NEXT line - the shape this branch used to disagree with itself
            // about, ending the list where every other path continued it
            // (carve#572). PART 1 S4 folds a lazy continuation into the
            // innermost OPEN paragraph, and the guard is that there is one: an
            // empty quote has none, so `- >` + text still ends the item.
            // A BLANK line ends the item, so a line after one is not lazy.
            // The collector consumes a trailing blank when the item collected
            // nothing indented, so the test is the LINE JUST CONSUMED rather
            // than the collected text, which no longer shows it.
            // And collection RESUMES after the fold, because S4 closes nothing:
            // `- > a` / `d` / `  > b` is ONE quote holding one paragraph, not a
            // quote followed by a second one (carve#980, carve-rs#813).
            let after_blank = cur.pos > 0 && is_blank_line(cur.lines[cur.pos - 1]);
            fold_lazy_run_and_resume(
                cur,
                &mut stream,
                content_col,
                |src, below| !after_blank && nested_ends_with_open_paragraph(src, below, options),
                |cur| collect_indented_block_mapped(cur, base_indent, content_col),
            );
            // A blank line between the item's blocks loosens the list, whatever
            // the marker-line lead happens to be. This branch and the two beside
            // it build their item and `continue` past the loosening test the
            // normal path runs, so `- {a=b}` / `x` / blank / `Body.` stayed tight
            // where `- x` / blank / `Body.` went loose - on the same blank line
            // (carve-rs#476).
            if continuation_source_loosens(&stream.source) {
                tight = false;
            }
            let children = parse_mapped_source(&stream, options);
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                children,

                // The item runs from its marker to the last line its body
                // consumed - the bullet is part of the item, unlike the
                // paragraph inside it, which starts at the text.
                pos: span_of(cur, item_at, cur.pos, options),
            });
            continue;
        }
        // Braces ALONE on the marker line are a block-attribute line for the
        // item's first block, not lead text (corpus 170). `- {a=b .c}` +
        // `  # H` therefore attributes the heading, exactly as the same two
        // lines do at document level. Braces followed by TEXT stay literal
        // (`- {.c} literal text`), which is why the whole content must parse as
        // a standalone attribute line. Routed through the block stream so the
        // pending-attribute machinery, not this lead-paragraph path, sees it.
        if parse_standalone_attrs(marker.content).is_some() {
            let mut stream = item_marker_source(cur, marker.content, item_at);
            stream.append(collect_indented_block_mapped(cur, base_indent, content_col));
            // A FLUSH-LEFT line is not the block this attribute floats onto.
            //
            // This used to pull that line INTO the item so the attributes could
            // reach it - which is the same absorb-a-flush-left-block behavior
            // PART 1 S4 refuses (markup-carve/carve#1280), and it made the item
            // swallow a block the author wrote outside it purely because an
            // attribute line was looking for a target. `- {.k}` / `# H`
            // published `<li><h1 class="k">` where the heading belongs at
            // document level, and `- {.k}` / `tail` published a classed
            // paragraph inside the item.
            //
            // An attribute line leaves no open paragraph, so the flush-left
            // line ends the item and the attribute has nothing left in scope: it
            // is dropped where it was written rather than travelling out of its
            // container (§15 A4, markup-carve/carve#1281). The indented
            // spelling - `- {a=b .c}` / `  # H`, corpus 170 - reaches the item's
            // content column and is collected above, so it still attaches.
            // A blank line between the item's blocks loosens the list, whatever
            // the marker-line lead happens to be. This branch and the two beside
            // it build their item and `continue` past the loosening test the
            // normal path runs, so `- {a=b}` / `x` / blank / `Body.` stayed tight
            // where `- x` / blank / `Body.` went loose - on the same blank line
            // (carve-rs#476).
            if continuation_source_loosens(&stream.source) {
                tight = false;
            }
            let children = parse_mapped_source(&stream, options);
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                children,
                pos: span_of(cur, item_at, cur.pos, options),
            });
            continue;
        }
        if detect_list_marker_full(marker.content).is_some() {
            let mut stream = item_marker_source(cur, marker.content, item_at);
            let before_block = cur.pos;
            stream.append(collect_indented_block_mapped(cur, base_indent, content_col));
            // A blank line closes the sub-list's last paragraph, so the next
            // flush-left line starts a NEW top-level block instead of folding in
            // (carve-rs#490). The collected source keeps no trace of a trailing
            // blank, so `nested_ends_with_open_paragraph` below still reports the
            // paragraph as open and the lazy loop swallowed `- - a` / blank / `b`
            // into the inner item, where carve-js and carve-php end the list.
            let mut ended_on_blank =
                cur.pos > before_block && is_blank_line(cur.lines[cur.pos - 1]);
            // A column-0 lazy-continuation line following the marker-line
            // sub-list folds into its last open paragraph (`- - b` / `lazy` ->
            // `<li>b\nlazy</li>`), and a following sibling marker at the
            // sub-list's column resumes the SAME list. This is the same
            // lazy-fold / resume loop the following-line nested-list path runs
            // above; reused here so the marker-line sub-list behaves identically.
            loop {
                if ended_on_blank {
                    break;
                }
                let has_lazy = if let Some(line) = cur.peek() {
                    let line = line.to_string();
                    !is_blank_line(&line)
                        && indent_columns(&line) == 0
                        && !is_list_marker(&line)
                        && !interrupts_paragraph(cur, &line)
                } else {
                    false
                };
                if !has_lazy {
                    break;
                }
                if !nested_ends_with_open_paragraph(
                    &stream.source,
                    last_consumed_line_below_column(cur, content_col),
                    options,
                ) {
                    break;
                }
                let before = cur.pos;
                collect_trailing_lazy(cur, &mut stream);
                if cur.pos == before {
                    break;
                }
                let before_block = cur.pos;
                stream.append(collect_indented_block_mapped(
                    cur,
                    content_col - 1,
                    content_col,
                ));
                ended_on_blank = cur.pos > before_block && is_blank_line(cur.lines[cur.pos - 1]);
            }
            // A blank line between the item's blocks loosens the list, and a
            // sub-list lead is no exception: the item holds the sub-list and
            // whatever follows the blank at THIS item's content column
            // (carve-rs#490). #476 fixed the attribute-block, quote and heading
            // leads beside this one and left the sub-list lead open, because
            // carve-php agreed with this engine at the time and carve-js did not.
            // Content at or past the SUB-LIST's content column is the sub-list's
            // own business - `sublist_source_loosens_outer_item` is the same
            // non-propagating test the following-line sub-list path uses, not the
            // blanket `continuation_source_loosens` the other leads can afford.
            if sublist_source_loosens_outer_item(&stream.source) {
                tight = false;
            }
            let children = parse_mapped_source(&stream, options);
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                children,

                // The item runs from its marker to the last line its body
                // consumed - the bullet is part of the item, unlike the
                // paragraph inside it, which starts at the text.
                pos: span_of(cur, item_at, cur.pos, options),
            });
            continue;
        }
        if marker_content_starts_block(marker.content, cur, content_col)
            || marker_content_is_attr_line(marker.content)
        {
            let mut stream = item_marker_source(cur, marker.content, item_at);
            let before_block = cur.pos;
            // A fence can open on the MARKER LINE, where its opener is the
            // marker-line content rather than a collected line - so the
            // collector's fenced-body guard is seeded from it (corpus 276).
            item_open_fence = detect_fence_open(marker.content);
            // A COMMENT FENCE OPENED ON THE MARKER LINE TAKES ITS BODY FROM THE
            // CONTENT COLUMN AND NOWHERE ELSE.
            //
            // The code fence beside it gets this from the collector's open-fence
            // guard (`fence.is_some() && indent < strip_cols`), which is keyed on
            // a `FenceOpen` and so never sees a `%%%` opener. Left at the
            // ordinary floor the collector took BELOW-column lines as the item's,
            // and the recursive parse then read them as the comment's body - so
            // `- %%%` / ` hidden` / ` %%%` published an empty item and DELETED
            // `hidden`. A line below the content column reaches no container
            // (§24 C3): it ends the item and re-parses outside it, exactly as
            // `- ``` ` / ` x` / ` ``` ` already does.
            //
            // Raising the floor to `content_col - 1` says that directly, and it
            // is the same expression the sub-list and lazy-resume collectors use
            // for "at or past this column only".
            let body_floor = if detect_comment_fence_line(marker.content).is_some() {
                content_col.saturating_sub(1)
            } else {
                base_indent
            };
            stream.append(collect_indented_block_mapped_after_fence(
                cur,
                body_floor,
                content_col,
                &mut item_open_fence,
            ));
            // A heading on the marker line (`- # H`) folds its trailing
            // flush-left plain text as heading continuation (`- # H\nlazy` ->
            // one `<h1>H\nlazy</h1>` inside the item), matching carve-js /
            // carve-php and the indented-heading-in-item path above. Only a
            // heading folds this way; the other marker-line block openers
            // (fence, table, thematic, container) keep their trailing text as a
            // separate block, so the guard is heading-only. A blank line closes
            // the heading (§heading rule 2), so skip the fold once one was
            // consumed while collecting -- `- # H\n\nsep` keeps `sep` as its own
            // top-level block.
            // True only when the block collection ended by swallowing a TRAILING
            // blank separator -- the single-line marker-line block case (heading,
            // thematic break), where the last consumed line is that blank. A
            // blank INSIDE a multiline block (e.g. a fenced code block with an
            // interior blank) is NOT a separator: there the last consumed line is
            // block content, and any real trailing separator is left for the
            // outer loop, so this stays false and tightness is unaffected.
            // And collection RESUMES after the fold, on the same S4 reading the
            // other three call sites take: nothing closed, so a line back at the
            // content column is still the item's (carve#980, carve-rs#813).
            let swallowed_blank_separator =
                cur.pos > before_block && is_blank_line(cur.lines[cur.pos - 1]);
            let marker_line_was_the_whole_block = cur.pos == before_block;
            fold_lazy_run_and_resume(
                cur,
                &mut stream,
                content_col,
                |src, _below| {
                    if swallowed_blank_separator {
                        return false;
                    }
                    // NO OPEN PARAGRAPH, NO LAZY LINE (PART 1 S4, ruled in
                    // markup-carve/carve#1280). The marker line's content is the
                    // item's FIRST BLOCK, so `- # H` writes a heading there
                    // exactly as `- ` plus an indented `# H` would, and a block
                    // that leaves no open paragraph leaves none wherever it was
                    // written. Ask S4's one question of the body and let the
                    // answer decide, instead of enumerating the kinds that fold.
                    //
                    // The enumeration this replaces listed the heading, table,
                    // thematic break, comment, link reference definition,
                    // footnote definition and attribute-block spellings as
                    // FOLDING - eight kinds, none of which holds an open
                    // paragraph, and every one of which the same engine already
                    // ended on in a block quote (`> # H` / `tail`). One rule
                    // stated for one container and not the other.
                    //
                    // Only when the marker line IS the whole block. Once the
                    // item collected lines at its CONTENT COLUMN the same
                    // question has a different answer under S4, and the clause
                    // leaves that half deliberately open (corpus
                    // 75-list-nesting-and-looseness-4 pins the folding answer
                    // for a nested spelling of it) - so that path keeps the
                    // enumeration it had.
                    if marker_line_was_the_whole_block {
                        return body_ends_with_open_paragraph(src, options);
                    }
                    // S4 DOES NOT ASK WHETHER THE OPEN PARAGRAPH IS THE
                    // CONTAINER'S FIRST BLOCK (markup-carve/carve#1370, a
                    // clarifying passage on the same clause). The half left open
                    // above is settled: an item whose first block is a table, a
                    // fence or a heading and whose next line is prose holds an
                    // open paragraph exactly as an item that began with prose
                    // does. The blocks before that paragraph are spent - they
                    // answered S4 while they were the item's last block and
                    // stopped answering it when prose reopened one.
                    //
                    // Without this the engine rendered `b` AS the item's prose
                    // and then declined to treat its paragraph as open, which is
                    // one line answered two ways in a single parse
                    // (carve-rs#1098).
                    //
                    // A direct trailing heading is bounded and supplements
                    // nothing (carve#1377). If it belongs to a nested
                    // definition or item, the enclosing paragraph may still
                    // take the line after that inner container closes.
                    body_ends_with_open_paragraph(src, options)
                        || nested_ends_with_heading(src, options)
                },
                |cur| collect_indented_block_mapped(cur, base_indent, content_col),
            );
            // A blank absorbed inside the marker-line block's continuation that
            // is followed by a plain paragraph loosens the item (§17 L1), the
            // same rule the plain-continuation branch applies -- e.g. a
            // marker-line fence with blank-separated trailing text
            // (`- ```\n  c\n  ```\n\n  tail`).
            if continuation_source_loosens(&stream.source) {
                tight = false;
            }
            let children = parse_mapped_source(&stream, options);
            items.push(ListItem {
                attrs: item_attrs,
                checked: marker.checked,
                children,

                // The item runs from its marker to the last line its body
                // consumed - the bullet is part of the item, unlike the
                // paragraph inside it, which starts at the text.
                pos: span_of(cur, item_at, cur.pos, options),
            });
            // A single-line marker-line block (heading, thematic break) leaves
            // no indented continuation, so collect_indented_block_mapped above
            // swallows the trailing blank separator before the outer loop can
            // see it. Re-raise pending_blank so a following sibling item still
            // loosens the list (`- # H\n\n- b` / `- ---\n\n- b` render `<li>` as
            // loose), matching carve-js / carve-php. A multi-line block (`:::`
            // container) leaves the blank for the outer loop, so
            // swallowed_blank_separator is false there and this does not
            // double-loosen.
            if swallowed_blank_separator {
                pending_blank = true;
            }
            continue;
        }
        // The item's first paragraph is the marker content plus any
        // immediately-following indented prose lines (lazy continuation).
        // It stops at a blank line or a list marker: a nested sublist still
        // interrupts (the one Carve deviation, grammar §10). Block openers that
        // begin ON the marker line are handled above by
        // marker_content_starts_block (heading, fence, thematic, container,
        // table); a would-be opener on a LATER lazy-continuation line stays
        // paragraph text.
        let mut para_lines = vec![marker.content.to_string()];
        let mut anchors = options
            .positions
            .then(|| vec![inline_anchor_for_line(cur, item_at, marker.content)]);
        let literal_colon_opener = detect_container_open(marker.content)
            .map(|open| open.fence_len)
            .or_else(|| detect_line_block_open(marker.content))
            .or_else(|| detect_hardbreaks_block_open(marker.content));
        while let Some(next) = cur.peek() {
            // A `+` ends the lead paragraph only where it can ACT as a
            // continuation marker - at or below the item's base column. An
            // INDENTED one is not consumed as a marker by any engine (all three
            // render it), so it is lazy text and folds like any other
            // continuation line. Breaking on it at any indent made `- a` / `  +`
            // two paragraphs here and one in carve-js and carve-php
            // (carve-rs#672, markup-carve/carve#812).
            if is_blank_line(next)
                || (trim_ascii(next) == "+" && indent_columns(next) <= base_indent)
            {
                break;
            }
            if let Some(fence_len) = literal_colon_opener {
                if indent_columns(next) <= base_indent
                    && exact_colon_fence_len(next) == Some(fence_len)
                {
                    break;
                }
            }
            if let Some(nm) = detect_list_marker_full(next) {
                // A marker indented past the base but BELOW this item's content
                // column is lazy continuation, not a sub-list: under symmetric
                // §10 no list marker (bullet, task, or ordered) interrupts a
                // paragraph, so fold it. A marker AT or ABOVE the content column
                // nests; one at the base column is a sibling (ends the paragraph).
                let folds = nm.indent > base_indent && nm.indent < content_col;
                if !folds {
                    break;
                }
            }
            let indent = indent_columns(next);
            // A FENCED BODY IS NOT A PARAGRAPH (§24, markup-carve/carve#950).
            // The lead paragraph may have ABSORBED a fence opener as text - §10
            // I4 keeps a fence with no closer left in the item from interrupting
            // - but the LAYOUT still reads a fence there, so a line below the
            // content column closes the item instead of folding in. The item's
            // fence state is one variable across all three of its collection
            // sites, so the outer loop below sees the same open fence and closes
            // the list at this line.
            if item_open_fence.is_some() && indent < content_col {
                break;
            }
            if indent < content_col {
                // NO SPECIAL CASE FOR AN ABSORBED FENCE. This used to break
                // when the paragraph held an invalid colon fence and ended in a
                // bare one, which ended the item and made the flush-left line a
                // document paragraph. PART 1 S4 says the opposite: `:::note`
                // fails §12's opener test so it is paragraph text, §12 then has
                // the paragraph absorb the bare fence below it as text too, and
                // a paragraph nothing ever interrupted is still OPEN when the
                // flush-left line arrives (carve#891, corpus
                // `86-list-lazy-continuation-9`). What decides is whether a
                // block was opened, never the shape of the line that tried.
                // BELOW content_column (§24 C3): a line -- flush-left OR indented
                // short of the content column -- lazily continues the item's open
                // paragraph and folds in as text. A block opener here is NOT a
                // block opener (the residual/absent indent disqualifies it), so it
                // never interrupts; only a genuine lazy-continuation interrupt
                // (blank, sibling marker, ...) ends it.
                let next_owned = next.to_string();
                if interrupts_lazy_continuation_as_container(cur, &next_owned) {
                    break;
                }
                let inline_line = trim_ascii_start(next);
                if let Some(anchors) = &mut anchors {
                    anchors.push(inline_anchor_for_line(cur, cur.pos, inline_line));
                }
                para_lines.push(inline_line.to_string());
                cur.consume();
                continue;
            }
            if indent > content_col {
                // ABOVE content_column (§24 C3): the line is lazy paragraph text,
                // never a block opener. Fully strip its indent and fold it into
                // the lead paragraph (inline-parsed, so a would-be opener like
                // `> q` renders as literal text, and no residual indent leaks).
                // A sibling/nesting marker above the content column was already
                // handled above; only a genuine lazy interrupt ends the fold.
                let next_owned = next.to_string();
                if interrupts_lazy_continuation_as_container(cur, &next_owned) {
                    break;
                }
                let inline_line = trim_ascii_start(next);
                if let Some(anchors) = &mut anchors {
                    anchors.push(inline_anchor_for_line(cur, cur.pos, inline_line));
                }
                para_lines.push(inline_line.to_string());
                cur.consume();
                continue;
            }
            // AT content_column: a block opener interrupts the lead paragraph and
            // nests as a child block; plain text dedents to the body's column 0.
            let dedented = slice_columns(next, content_col, false);
            let suppress_colon_interrupt = para_lines
                .iter()
                .any(|line| is_invalid_colon_fence_opener_text(line));
            if interrupts_paragraph_as_container(cur, &dedented)
                && !(suppress_colon_interrupt && is_suppressed_colon_fence_line(&dedented))
            {
                break;
            }
            if let Some(anchors) = &mut anchors {
                anchors.push(inline_anchor_for_line(cur, cur.pos, &dedented));
            }
            track_collected_fence(&mut item_open_fence, &dedented, true);
            para_lines.push(dedented);
            cur.consume();
        }
        // Trailing ASCII whitespace is discarded before inline escape
        // resolution.  Do this per physical line: waiting until after `join`
        // leaves an escaped space on an intermediate list-item line, which the
        // inline parser then turns into NBSP instead of the intended hard break
        // (carve-rs#855, markup-carve/carve#1028).
        let normalized_para_lines: Vec<&str> =
            para_lines.iter().map(|line| trim_ascii_end(line)).collect();
        let para_text = normalized_para_lines.join("\n");
        let para_text = para_text.trim_end_matches([' ', '\t']);
        let children = if let Some(anchors) = anchors {
            parse_inline_lines_with_anchor(para_text, options, anchors)
        } else {
            parse_inline_with_options(para_text, options)
        };
        let mut paragraph = BlockNode::Paragraph(Paragraph {
            attrs: None,
            children,
            // This paragraph BEGINS at the item's content column by construction:
            // its first line is the marker line's own content. Leaving the field
            // at its `false` default gated off the image + `^ caption` promotion
            // in `promote_block_images`, so a caption inside a list item rendered
            // as literal text (carve-rs#610). Lines folded in from BELOW the
            // content column are lazy continuations and never start this
            // paragraph, so the flag describes the first line correctly.
            at_content_column: true,
            pos: item_paragraph_span(
                cur,
                item_at,
                cur.pos.saturating_sub(1),
                marker.content,
                options,
            ),
        });
        if options.source_lines {
            if let Some(line) = item_source_line {
                stamp_source_line(&mut paragraph, line);
            }
        }
        items.push(ListItem {
            attrs: item_attrs,
            checked: marker.checked,
            children: vec![paragraph],

            pos: span_of(cur, item_at, cur.pos, options),
        });
    }
    // Indentation that places a list at this cursor level belongs to each item.
    // `MappedSource` anchors the stripped marker at the content column, so move
    // the start back across that placing indentation before offsets are filled.
    if base_indent > 0 {
        for item in &mut items {
            if let Some(pos) = &mut item.pos {
                pos.start_column = pos.start_column.saturating_sub(base_indent);
            }
        }
    }
    // THE LIST ENDED, so a set still held here was written in front of a block
    // that never came (§15 A4). This is the list's own slot, separate from the
    // one `parse_blocks` keeps for a container BODY - attributes reach it only
    // by being lifted off a chunk, and only this loop can place them.
    if pending_attrs.is_some() {
        note_unattached_block_attrs(pending_attrs_pos);
    }
    widen_items_over_children(&mut items);
    let mut list_pos =
        span_of(cur, span_start, cur.pos, options).or_else(|| span_across_items(&items));
    if base_indent > 0 {
        if let Some(pos) = &mut list_pos {
            pos.start_column = pos.start_column.saturating_sub(base_indent);
        }
    }
    BlockNode::List(List {
        // The cursor's own span when it can give one, else the extent of the
        // items themselves. A list inside a `+`-continued blockquote sits on
        // lines whose stripped width is unknown, so `span_of` refuses - but the
        // items were placed by other means, and a list that runs from its first
        // item to its last is not a guess.
        pos: list_pos,
        attrs: None,
        ordered: is_ordered,
        start,
        ol_type,
        bare_marker: is_ordered && first_marker.marker.is_empty(),
        delim: first_delim.map(char::from),
        bullet_char: if is_ordered {
            None
        } else {
            first_marker
                .marker
                .chars()
                .next()
                .filter(|c| *c == '-' || *c == '*')
        },
        tight,
        items,
    })
}

fn marker_content_starts_block(content: &str, cur: &LineCursor<'_>, content_col: usize) -> bool {
    // Reference and footnote definitions are invisible blocks at a container's
    // content column. On a marker line they must enter the nested block parser,
    // not the lead-paragraph scanner (`r. [f]: t` is an empty list item).
    if parse_footnote_def_line(content).is_some()
        || parse_link_def_line(content).is_some_and(|(_, target)| !trim_ascii(target).is_empty())
    {
        return true;
    }
    // A thematic break as the marker-line content is a block (`1. ---` ->
    // <li><hr></li>), not inline text -- otherwise smart punctuation turns
    // `---` into an em-dash. Matches carve-js / carve-php.
    if detect_thematic_break(content) {
        return true;
    }
    // A heading WITH content as the marker-line first block (`- # H` ->
    // <li><h1>H</h1></li>), matching carve-js / carve-php. A heading is a single
    // line, so no multi-line close scan is needed. Bare `#`, a `# ` with no
    // content, or a tab (not the required space) stay inline text.
    if heading_content_starts(content) {
        return true;
    }
    // A fence is the item's FIRST content, so there is no open paragraph for it
    // to interrupt and the I4 closer lookahead does not apply: an unterminated
    // fence opens a code block that runs to the end, exactly as it does at the
    // top level and inside a block quote. Requiring a closer here made a list
    // item the one container that rendered it as inline verbatim, disagreeing
    // with carve-rs's own handling of the other two (carve-rs#458).
    if detect_fence_open(content).is_some() {
        return true;
    }
    // A COMMENT fence is an opener on the same terms as the code fence above
    // it: it OPENS, closer or no closer. This used to scan ahead for a closer
    // AT THE CONTENT COLUMN and fall through to the lead-paragraph path when it
    // found none, which made `- %%%` / `c` / `%%%` collect `c` as item text -
    // the item reaching past its own end for a body written at column 0, which
    // is the shape PART 1 S4 refuses (markup-carve/carve#1280). Its closer
    // travels with its opener: the item holds an empty comment and what follows
    // re-parses at document level, the same derivation `- ``` ` already gets
    // (§S4 A FENCED BODY IS NOT A PARAGRAPH) and the same output the quote
    // spelling `> %%%` already produced.
    if detect_comment_fence_line(content).is_some() {
        return true;
    }
    // A LINE comment, like the comment FENCE just above it. Left out, `- %% c`
    // routed to the lead-PARAGRAPH path, where the inline scanner consumed the
    // comment and left the item holding an EMPTY paragraph: the comment was
    // absent from the AST where carve-js publishes a `comment` node, the
    // canonical writer saw an item with no content and substituted the
    // CONTINUATION MARKER (`- +` - a different construct, one that takes a
    // body), and the empty paragraph rendered a blank line inside the item
    // (carve-rs#511 item 7, carve-rs#532).
    if trim_ascii_start(content).starts_with("%%") {
        return true;
    }
    let colon_fence_len = detect_container_open(content)
        .map(|open| open.fence_len)
        .or_else(|| detect_line_block_open(content))
        .or_else(|| detect_hardbreaks_block_open(content));
    if colon_fence_len.is_some() {
        // An opener OPENS, closer or no closer (carve#514), and an empty body
        // is a container with nothing in it (carve#570). What can stop it is
        // the STRICT CONTENT-COLUMN rule: a following line BELOW the content
        // column is lazy item text, and it folds the fence in with it, so
        // `- :::` / `x` is the literal `::: x`.
        //
        // Only a line that would fold counts. Nothing following at all, a blank
        // (which ends the item's content), a sibling marker, or a flush-left
        // block opener all leave the fence standing on its own - and `- :::`
        // alone published `<li>:::</li>` here where carve-js, carve-php and the
        // executable spec publish an empty `<div>` (carve-rs#511 item 4).
        let Some(next) = cur.lines.get(cur.pos) else {
            return true;
        };
        if is_blank_line(next) {
            return true;
        }
        if indent_columns(next) >= content_col {
            return true;
        }
        // Only a FLUSH-LEFT line ends the item. An indented one is item-lazy
        // text whatever its shape - corpus 161 is `- ::: note` / ` - para text`
        // / ` :::`, where the marker one column in folds and takes the fence
        // with it.
        if indent_columns(next) > 0 {
            return false;
        }
        return is_list_marker(next) || interrupts_paragraph_with_rest(next, &[]);
    }
    if is_table_start(content) {
        return true;
    }
    // A definition TERM. PART 9 §24 C3 names it in the same uniform block-opener
    // set as the branches above - "block quote, heading, thematic break, fenced
    // code, colon fence / admonition, TABLE, and DEFINITION LIST (a `:: term`
    // opener)" - and it was the one member missing here, so `* :: t` kept the
    // term as literal item text where carve-js and carve-php open a list.
    //
    // No lookahead: a term stands alone as a `<dl>` holding only a `<dt>`, and
    // `is_definition_list_start` already requires the `:: ` separator and
    // non-blank content, so `:::` (a div) and a bare `::` are both excluded.
    if is_definition_list_start(content) {
        return true;
    }
    false
}

/// Only a COMPLETE, VALID single-line block counts. The multi-line form (`{#id`
/// on the marker line, `.foo}` on the next) is deliberately excluded: routing it
/// here on the guess that it closes later would send an invalid run
/// (`- {not attrs` / `lazy`) down the block path, where the lazy line is not
/// collected and escapes the item as a top-level paragraph. It stays literal, as
/// it was before this rule existed -- a pre-existing divergence from carve-js,
/// which attaches it, and one no corpus case pins.
fn marker_content_is_attr_line(content: &str) -> bool {
    trim_ascii(content).starts_with('{') && parse_standalone_attrs(content).is_some()
}

#[derive(Clone)]
struct ListMarker<'a> {
    indent: usize,
    ordered: bool,
    checked: Option<bool>,
    start: Option<usize>,
    ol_type: Option<OrderedListType>,
    content: &'a str,
    attrs: Option<Attrs>,
    /// Ordered-marker delimiter (`.` or `)`); `None` for bullets/tasks. A change
    /// in delimiter starts a new sibling list (§11).
    delim: Option<u8>,
    /// The raw ordered marker text (`i`, `iv`, `3`, `b`); used to re-classify an
    /// ambiguous single roman-letter via its sibling (§11 tie-break).
    marker: &'a str,
}

/// Coarse ordered-list dialect family for the §11 same-list test: decimal,
/// alphabetic, or roman (case included). A change splits the list.
#[derive(PartialEq, Eq, Clone, Copy)]
enum OlDialect {
    Decimal,
    Alpha(bool),
    Roman(bool),
}

fn ol_dialect(ol_type: Option<OrderedListType>) -> OlDialect {
    match ol_type {
        None => OlDialect::Decimal,
        Some(OrderedListType::LowerAlpha) => OlDialect::Alpha(false),
        Some(OrderedListType::UpperAlpha) => OlDialect::Alpha(true),
        Some(OrderedListType::LowerRoman) => OlDialect::Roman(false),
        Some(OrderedListType::UpperRoman) => OlDialect::Roman(true),
    }
}

/// Does an ordered `marker` keep the list's dialect (no §11 dialect split)? A
/// non-ambiguous marker must match the family exactly; an ambiguous single
/// roman-letter is compatible with EITHER a roman or an alpha list of the same
/// case (it continues as that dialect), but never a decimal list.
fn dialect_compatible(first: OlDialect, marker: &ListMarker<'_>) -> bool {
    if marker.marker.is_empty() {
        return first == OlDialect::Decimal;
    }
    if is_ambiguous_roman_letter(marker.marker) {
        let upper = marker.marker.chars().all(|c| c.is_ascii_uppercase());
        match first {
            OlDialect::Roman(u) | OlDialect::Alpha(u) => u == upper,
            OlDialect::Decimal => false,
        }
    } else {
        ol_dialect(marker.ol_type) == first
    }
}

/// Is `m` a single roman-letter marker (i/v/x/l/c/d/m, either case)? Such a
/// marker is dialect-AMBIGUOUS: roman or alpha depending on its sibling (§11).
fn is_ambiguous_roman_letter(m: &str) -> bool {
    m.len() == 1
        && matches!(
            m.to_ascii_lowercase().as_str(),
            "i" | "v" | "x" | "l" | "c" | "d" | "m"
        )
}

/// Visual column (tab-aware) at which a list ITEM's continuation content begins,
/// mirroring `parse_list` exactly: for ordered/unordered it is where the marker
/// content begins (`- ` -> 2, `1. ` -> 3, `10. ` -> 4); for a TASK the checkbox
/// counts as content, not marker, so the column is the bullet width (`- ` -> 2).
/// Returns `None` when `line` is not a list marker.
fn marker_content_col(line: &str) -> Option<usize> {
    let m = detect_list_marker_full(line)?;
    if m.checked.is_some() {
        return Some(m.indent + 2);
    }
    let content_off = (m.content.as_ptr() as usize).saturating_sub(line.as_ptr() as usize);
    Some(indent_columns(line) + content_off.saturating_sub(leading_ws(line)))
}

/// Whether `line` (in the dedented sub-list coordinate space) begins a plain
/// PARAGRAPH rather than a block opener. Any indented line is paragraph text
/// under the strict column-0 rule; a flush-left line is a paragraph only when it
/// matches none of the block openers.
pub(crate) fn line_starts_paragraph(line: &str) -> bool {
    if is_blank_line(line) {
        return false;
    }
    if line.starts_with([' ', '\t']) {
        // An indented line is outer-item paragraph content ONLY if it does not
        // open a block. An indented sibling marker belongs to the nested list,
        // not to the outer item, so treating it as paragraph content propagated
        // the nested list's looseness outwards - which PART 9 section 17 says
        // it must not (corpus 142). The 2-space form dedents to column 0 and
        // reached the marker check below; the 4-space form kept its indent and
        // short-circuited here.
        // ...but only a LIST MARKER disqualifies it. Unordered and task markers
        // nest at any indent, so an indented sibling marker belongs to the
        // nested list. Every other opener needs its own column, so an indented
        // `> q` or `# h` is literal paragraph text and DOES loosen (corpus 160).
        return detect_list_marker_full(line).is_none();
    }
    detect_heading(line).is_none()
        && !detect_thematic_break(line)
        && !line.starts_with('>')
        && detect_fence_open(line).is_none()
        && detect_container_open(line).is_none()
        && detect_line_block_open(line).is_none()
        && detect_hardbreaks_block_open(line).is_none()
        && detect_comment_fence_line(line).is_none()
        && !is_table_start(line)
        && !is_definition_list_start(line)
        && detect_list_marker_full(line).is_none()
        // A flush-left block-attribute line floats forward to the block below
        // it (§15), which is exactly why `interrupts_paragraph` lets it end an
        // open paragraph. Missing it here is the same omission read from the
        // other side: the writer asks this function whether a child's first
        // line would FOLD into the paragraph above, and answered `{.x}` with
        // yes. It then wrote the child behind a `+` at the item's MARKER
        // column, where a following `- b` is a SIBLING item rather than the
        // nested list the attributes were reaching for, so
        // `- a` / `  {.x}` / `  - b` came back as two items and a literal
        // `{.x}` (corpus 322/323). The indented branch above is deliberately
        // untouched: under the strict column-0 rule an INDENTED attr line is
        // lazy paragraph text, not a floater.
        && parse_standalone_attrs(line).is_none()
        && !trim_ascii_start(line).starts_with("%%")
}

/// True when the dedented sub-list source carries a blank line that DIRECTLY
/// precedes a PARAGRAPH attached to the OUTER item -- i.e. below the sub-list's
/// own content column and plain paragraph text. That blank is internal to the
/// outer item and loosens it. A blank that precedes inner-item content (reaching
/// the sub-list's content column), a sibling marker, or a non-paragraph outer
/// block (e.g. a `<hr>` or block quote) does NOT loosen the outer item (corpus
/// 142: looseness does not propagate; and the paragraph-only rule the plain
/// continuation branch applies via `pending_blank`).
fn sublist_source_loosens_outer_item(source: &str) -> bool {
    let lines: Vec<&str> = source.split('\n').collect();
    let Some(inner_content_col) = lines
        .iter()
        .find(|l| !is_blank_line(l))
        .and_then(|l| marker_content_col(l))
    else {
        return false;
    };
    let mut prev_blank = false;
    for line in &lines {
        if is_blank_line(line) {
            prev_blank = true;
            continue;
        }
        if prev_blank && indent_columns(line) < inner_content_col && line_starts_paragraph(line) {
            return true;
        }
        prev_blank = false;
    }
    false
}

fn detect_list_marker_full(line: &str) -> Option<ListMarker<'_>> {
    let indent = indent_columns(line);
    if let Some((checked, content, attrs, marker)) = detect_task(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: Some(checked),
            start: None,
            ol_type: None,
            content,
            attrs,
            delim: None,
            marker,
        });
    }
    if let Some((content, start, ol_type, attrs, delim, marker)) = detect_ordered_full(line) {
        return Some(ListMarker {
            indent,
            ordered: true,
            checked: None,
            start,
            ol_type,
            content,
            attrs,
            delim: Some(delim),
            marker,
        });
    }
    if let Some((content, attrs, marker)) = detect_unordered(line) {
        return Some(ListMarker {
            indent,
            ordered: false,
            checked: None,
            start: None,
            ol_type: None,
            content,
            attrs,
            delim: None,
            marker,
        });
    }
    None
}

/// Whether a collected body's deepest trailing block is a heading inside a
/// nested list. The enclosing item must collect the flush-left line so it can
/// close that inner item and continue the still-open outer paragraph; the
/// heading's own collector no longer takes it (carve#1377).
fn nested_ends_with_heading(nested: &str, options: &Options<'_>) -> bool {
    block_ends_with_heading(probe_blocks(nested, options).last())
}

fn block_ends_with_heading(block: Option<&BlockNode>) -> bool {
    match block {
        Some(BlockNode::List(l)) => {
            let trailing = l.items.last().and_then(|it| it.children.last());
            matches!(trailing, Some(BlockNode::Heading(_))) || block_ends_with_heading(trailing)
        }
        Some(BlockNode::DefinitionList(dl)) => block_ends_with_heading(
            dl.items
                .last()
                .and_then(|item| item.definitions.last())
                .and_then(|d| d.children.last()),
        ),
        _ => false,
    }
}

/// S4's lazy question for a body collected at a list item's CONTENT COLUMN:
/// does a following flush-left line fold into it?
///
/// DEPTH IS NOT A PARAMETER (carve-rs#1025). When the body's last line is a
/// SUB-ITEM's marker line, the innermost container's last block is that
/// marker's CONTENT, so S4's question is asked of it directly - the same
/// question `- # H` / `tail` answers one level up (markup-carve/carve#1280).
/// Without this the two levels disagree: the sub-item refuses the line on
/// re-parse while this collector has already claimed it, and the line lands in
/// the OUTER item, which is a third answer no engine produces.
///
/// A heading at the sub-item's CONTENT COLUMN is a bounded block too. It leaves
/// no paragraph open, so the line ends the inner item (carve#1377).
fn collected_body_takes_the_lazy_line(
    src: &str,
    trailing_below_column: bool,
    options: &Options<'_>,
) -> bool {
    if let Some(content) = trailing_marker_line_content(src) {
        return body_ends_with_open_paragraph(&content, options);
    }
    nested_ends_with_open_paragraph(src, trailing_below_column, options)
        || nested_ends_with_heading(src, options)
}

/// Whether the line JUST CONSUMED was written BELOW the container's content
/// column.
///
/// §24 C3's comment exception turns on the column, and the collected body
/// cannot say what it was: the body is DEDENTED by whatever each line supplied,
/// up to the content column, so a line at column 1 under a content column of 2
/// and one at column 2 both arrive flush.
///
/// Read from the CURSOR rather than from the collector's `col_map`, which looks
/// like it carries this and does not: the map is only built when positions are
/// on, so on the plain `--html` path it is EMPTY and every answer read from it
/// would be the same one. That is a check that cannot fire, and it is the same
/// reason the `after_blank` test beside the quote path reads the line just
/// consumed instead of the collected text.
fn last_consumed_line_below_column(cur: &LineCursor, content_col: usize) -> bool {
    if content_col == 0 || cur.pos == 0 {
        return false;
    }
    indent_columns(cur.lines[cur.pos - 1]) < content_col
}

/// The marker-line CONTENT of a collected body whose last line opens a list
/// item, if that is what its last line is.
///
/// `- a` / `  - # N` collects `a` / `- # N`, and the block a following
/// flush-left line would have to fold into is the sub-item's - which is the
/// marker's content, `# N`. A body whose last line is anything else (a line at
/// the sub-item's content column, a paragraph, a fence) is not this shape.
fn trailing_marker_line_content(body: &str) -> Option<String> {
    let last = body.lines().rev().find(|line| !is_blank_line(line))?;
    let marker = detect_list_marker_full(last)?;
    Some(marker.content.to_string())
}

/// PART 1 S4's one question, asked of a container's WHOLE body: does the last
/// block leave a paragraph open?
///
/// [`nested_ends_with_open_paragraph`] answers the same question for a body
/// collected BESIDE a marker line that is still holding a paragraph of its own,
/// which is why it looks PAST a trailing run of comments: there the open
/// paragraph is the marker line's, and an invisible block did not close it.
/// Here the body is all there is - `- %% c` writes the comment as the item's
/// first and only block, so there is no earlier paragraph for it to leave open,
/// and looking past it would find one that was never written
/// (markup-carve/carve#1280).
fn body_ends_with_open_paragraph(body: &str, options: &Options<'_>) -> bool {
    let blocks = parse_blocks_with_options(body, options);
    block_ends_with_open_paragraph(blocks.last(), colon_fences_left_open(body))
}

/// Whether the collected nested block ends in an OPEN paragraph -- i.e. its
/// last block is a paragraph, or a container (block quote / div / admonition)
/// whose last child recursively ends in a paragraph. CommonMark lazy
/// continuation folds a following dedented non-blank line into the deepest open
/// paragraph: when a list item's last block is a block quote whose trailing
/// block is a paragraph, the dedented line is the quote's own lazy continuation
/// and must stay INSIDE the item rather than ending it. A code block or table
/// has no open paragraph, so it does NOT fold (the dedented line ends the item).
fn nested_ends_with_open_paragraph(
    nested: &str,
    trailing_below_column: bool,
    options: &Options<'_>,
) -> bool {
    let blocks = probe_blocks(nested, options);
    // A COMMENT AT THE CONTENT COLUMN IS A BLOCK, AND A BLOCK ENDS THE PARAGRAPH
    // IT SITS UNDER, whatever it renders (PART 1 S4, markup-carve/carve#1364).
    // §24 C3 makes the content column the container body's own column 0, so a
    // line there is read as a block - and a block ends the paragraph above it
    // whether or not anything reaches the page. This walk used to look PAST a
    // trailing run of comments at whatever they were sitting on, which is the
    // reading that made `- a` / `  %% c` / `tail` fold while the `dd` spelling
    // one construct over did not.
    //
    // The §17 L1a objection does not reach it: whether a line IS a paragraph
    // (markup-carve/carve#621, #625) and whether it ENDS one are different
    // questions, and the ATTRIBUTE BLOCK settles it by measurement - it is
    // invisible, it ends the item, and every implementation already agrees.
    //
    // BELOW the content column nothing changes, because a line down there never
    // reaches this predicate as a block: `- a` / `  %% c` / ` b` keeps `b` in
    // the item (corpus 358, and 189/192 for the nested spelling).
    let mut end = blocks.len();
    if trailing_below_column {
        // §24 C3's COMMENT EXCEPTION, which the clause above says is unchanged.
        // A comment written BELOW the content column is not a block of this
        // container - it reaches the container only through S4's lazy fold - so
        // it ends nothing and the line under it still folds: `- a` / ` %% c` /
        // `b` keeps `b` in the item (corpus 183, and 192 for the fence
        // spelling). The dedented body cannot say which of the two it was, since
        // a line at column 1 under a content column of 2 and one at column 2
        // both arrive flush; the caller reads it off the collector's own map.
        while end > 0 && matches!(blocks[end - 1], BlockNode::Comment(_)) {
            end -= 1;
        }
        if end == 0 {
            // Everything collected renders nothing, so the still-open paragraph
            // is the one on the MARKER line. An empty collection is a different
            // thing and keeps the old answer.
            return !blocks.is_empty();
        }
    } else if end == 0 {
        return false;
    }

    // AN UNTERMINATED DIV IS A CONTAINER LIKE ANY OTHER (carve#939). PART 1 S4
    // folds a flush-left line into the innermost OPEN PARAGRAPH, and a `::: `
    // div that no `:::` line closed still holds one - so the line folds into
    // the div, not out of the item.
    //
    // A div CLOSED by its fence is the case the catch-all below is right about:
    // the fence closes the paragraph inside it, and a dedented line after it
    // ends the item. One line of body decides it two ways, which is why the
    // termination has to be read from the SOURCE rather than from the node -
    // the AST records what a div holds, not whether the author closed it.
    // DEPTH IS NOT A PARAMETER of this rule (carve#506), so the unterminated
    // fence is looked through wherever the recursion below reaches it -- an
    // item holding an item holding an open div answers the same as a bare one
    // (markup-carve/carve#980). The termination is read from the SOURCE rather
    // than from the node either way: the AST records what a div holds, not
    // whether the author closed it.
    block_ends_with_open_paragraph(blocks.get(end - 1), colon_fences_left_open(nested))
}

/// How many colon fences does this collected body end INSIDE, unclosed?
///
/// A colon fence closes on an EXACT length match, so the widths are tracked as
/// a stack rather than a count. Only what an opener actually opens is pushed:
/// a `:::`-shaped line that fails the opener test is absorbed paragraph text
/// and opens nothing (PART 7's separator narrowing, carve#900/#905), which is
/// the reading that made `:::note` stop being a container.
///
/// The DEPTH is what the caller needs, not just "any". An unterminated fence can
/// only be the last block at its level, so the open ones are exactly the last
/// N containers on the last-child chain - and a CLOSED container nested inside
/// an open one must still read as closed. Answering `true` for the whole source
/// let `:::: outer` / `::: inner` / `a` / `:::` fold a flush-left line into the
/// inner div's paragraph, which its own `:::` line had already closed.
///
/// A VERBATIM BODY OPENS NOTHING. A colon-shaped line inside a code fence is
/// code text and one inside a LINE BLOCK is verse text, so this scan carries
/// both bodies' state. Without it, `- x` / `  ``` ` / `  :::` / `  ::::` /
/// `  ``` ` charged two containers the parser never opened, and the properly
/// closed divs after the fence were then looked through as though they were
/// open. The line-block spelling is the same defect one construct over, and it
/// is the one that made the whole scan wrong rather than merely optimistic: a
/// line block's body is where a colon-shaped line is MOST likely to be content.
fn colon_fences_left_open(nested: &str) -> usize {
    let mut open: Vec<usize> = Vec::new();
    let mut code: Option<FenceOpen> = None;
    let mut verse: Option<usize> = None;
    for line in nested.lines() {
        let mut trimmed = trim_ascii_start(line);
        // A container nested in a QUOTE carries the quote's marker on every one
        // of its lines, so the fence is only visible past it.
        while let Some(rest) = trimmed.strip_prefix('>') {
            trimmed = trim_ascii_start(rest);
        }
        if let Some(fence) = code {
            if is_fence_close(trimmed, fence) {
                code = None;
            }
            continue;
        }
        if let Some(fence_len) = verse {
            // Only the EXACT closer ends it, and every other line is content
            // whatever it looks like - the same test the definition prepass
            // applies to a line block's body.
            if exact_colon_fence_len(trimmed) == Some(fence_len) {
                verse = None;
            }
            continue;
        }
        if let Some(fence) = detect_fence_open(trimmed) {
            code = Some(fence);
            continue;
        }
        if let Some(fence_len) = detect_line_block_open(trimmed) {
            verse = Some(fence_len);
            continue;
        }
        if let Some(len) = exact_colon_fence_len(trimmed) {
            if open.last() == Some(&len) {
                open.pop();
                continue;
            }
        }
        if let Some(container) = detect_container_open(trimmed) {
            open.push(container.fence_len);
        }
    }
    open.len()
}

fn block_ends_with_open_paragraph(block: Option<&BlockNode>, colon_open: usize) -> bool {
    match block {
        Some(BlockNode::Paragraph(_)) => true,
        // A blockquote has no explicit closer: lazy continuation keeps its
        // trailing paragraph open, so a dedented line folds into it.
        Some(BlockNode::BlockQuote(q)) => {
            block_ends_with_open_paragraph(q.children.last(), colon_open)
        }
        // A list's last item can hold an open paragraph (the deepest open
        // paragraph a dedented line continues, e.g. a sub-list item's text).
        Some(BlockNode::List(l)) => block_ends_with_open_paragraph(
            l.items.last().and_then(|it| it.children.last()),
            colon_open,
        ),
        // A definition list has no explicit closer either: its last item stays
        // open -- a term still awaiting its `:  ` definition, or a definition
        // whose body ends in a paragraph. A following flush-left `:  ` line (at
        // any column at or below the term) attaches as a `<dd>`, and lazy body
        // text folds into the open definition. This is the lenient def-attach
        // rule shared with carve-php / carve-js: a definition marker is not
        // subject to the column-0-exits rule that ends a list item.
        Some(BlockNode::DefinitionList(dl)) => match dl.items.last() {
            None => false,
            // Bare term, no definition yet: open (awaiting `:  def`).
            Some(item) if item.definitions.is_empty() => true,
            // Otherwise the last definition's body must end in an open paragraph.
            Some(item) => block_ends_with_open_paragraph(
                item.definitions.last().and_then(|d| d.children.last()),
                colon_open,
            ),
        },
        // AN UNTERMINATED DIV IS A CONTAINER LIKE ANY OTHER (carve#939, carve#909).
        // PART 1 S4 folds a flush-left line into the innermost OPEN PARAGRAPH,
        // and a `::: ` div that no `:::` line closed still holds one.
        //
        // A div CLOSED by its fence is the case the catch-all below is right
        // about: the fence closes the paragraph inside it, and a dedented line
        // after it ends the item (like code/table). One line of body decides it
        // two ways, which is why `colon_open` is read from the SOURCE.
        Some(BlockNode::Div(d)) if colon_open > 0 => {
            block_ends_with_open_paragraph(d.children.last(), colon_open - 1)
        }
        Some(BlockNode::Admonition(a)) if colon_open > 0 => {
            block_ends_with_open_paragraph(a.children.last(), colon_open - 1)
        }
        // A bare `::: figure` fence is a container like the two above; left
        // unterminated it still holds its open paragraph (§4c defers to §12's
        // container rules for body and closer discipline).
        Some(BlockNode::FigureGroup(g)) if colon_open > 0 => {
            block_ends_with_open_paragraph(g.children.last(), colon_open - 1)
        }
        _ => false,
    }
}

/// §17 L1/L2: within a list item's collected continuation body, a blank line
/// that is followed by a PLAIN paragraph (a line that opens no sub-block)
/// loosens the list, exactly as a blank-separated second paragraph does. A
/// blank followed by a sub-block opener (fence, `:::` div, table, block quote,
/// heading, thematic break, definition term, or a nested list marker) keeps the
/// item tight (§17 L2). This mirrors the executable-spec oracle's line-based
/// `opensSubBlock` scan, which -- like carve-js -- is purely textual: it does
/// not track whether a blank sits inside a fenced block, so a fenced block that
/// contains an interior blank line loosens its item too. `source` is the
/// continuation dedented to column 0, so block openers are recognized flush.
/// One past the closer of a colon container opened at `start`, but ONLY when it
/// really closes.
///
/// `find_colon_container_end` and `find_line_block_end` both answer end-of-input
/// for an unterminated opener, which is right for an extent scan and wrong for
/// a state pass: an opener with no closer would latch the pass and swallow every
/// later line, so a genuinely CLOSED fence below an unterminated one would go
/// unmarked. Proving the last line is an exact closer is the whole difference.
fn closed_colon_span_end(
    lines: &[&str],
    start: usize,
    fence_len: usize,
    line_block: bool,
) -> Option<usize> {
    let end = if line_block {
        find_line_block_end(lines, start, fence_len)
    } else {
        find_colon_container_end(lines, start, fence_len)
    };
    if end > start && end <= lines.len() && exact_colon_fence_len(lines[end - 1]) == Some(fence_len)
    {
        return Some(end);
    }
    None
}

fn continuation_source_loosens(source: &str) -> bool {
    let lines: Vec<&str> = source.split('\n').collect();
    // A blank line INSIDE AN OPEN FENCE is that block's own content, not an
    // interior block separator, so it must not loosen the item (carve-php#404
    // family; matches carve-js / carve-php). A blank AFTER the fence closes
    // still loosens against a following paragraph.
    //
    // ALL THREE FENCE KINDS. This knew only the code fence, which is the same
    // one-kind-of-three read corpus category 279 pins for the `+` collectors:
    // a blank inside an item's own `%%%` or `:::` body loosened the item that
    // held it, where the identical code fence kept it tight
    // (markup-carve/carve#985). §28 makes a comment body verbatim and a colon
    // container is one block; neither is two blocks with a separator between
    // them.
    //
    // ONE STATEFUL LEFT-TO-RIGHT PASS, not one scan per line: each closed span
    // is jumped over whole, and spans never overlap, so the walk stays linear.
    let mut fence: Option<FenceOpen> = None;
    let mut comment_closers: Option<HashMap<usize, usize>> = None;
    let mut i = 0;
    while i < lines.len() {
        if let Some(open) = fence {
            if is_fence_close(lines[i], open) {
                fence = None;
            }
            i += 1;
            continue;
        }
        if let Some(open) = detect_fence_open(lines[i]) {
            fence = Some(open);
            i += 1;
            continue;
        }
        if let Some(open) = detect_comment_fence_line(lines[i]) {
            // AN OPENER WITH NO CLOSER AHEAD OPENS NOTHING (§28) and must not
            // latch this pass.
            let closers =
                comment_closers.get_or_insert_with(|| build_comment_closer_last_index(&lines));
            let has_closer = closers
                .get(&open.fence_len)
                .copied()
                .is_some_and(|last| last > i);
            if has_closer {
                if let Some(close) =
                    (i + 1..lines.len()).find(|&j| is_comment_fence_close(lines[j], open.fence_len))
                {
                    i = close + 1;
                    continue;
                }
            }
        }
        if let Some(fence_len) = detect_line_block_open(lines[i]) {
            if let Some(end) = closed_colon_span_end(&lines, i, fence_len, true) {
                i = end;
                continue;
            }
        }
        if let Some(fence_len) = detect_hardbreaks_block_open(lines[i]) {
            if let Some(end) = closed_colon_span_end(&lines, i, fence_len, false) {
                i = end;
                continue;
            }
        }
        if let Some(open) = detect_container_open(lines[i]) {
            if let Some(end) = closed_colon_span_end(&lines, i, open.fence_len, false) {
                i = end;
                continue;
            }
        }
        // Start at 1: a leading blank is not an interior separator between blocks.
        if i == 0 || !is_blank_line(lines[i]) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < lines.len() && is_blank_line(lines[j]) {
            j += 1;
        }
        if j >= lines.len() {
            // Only trailing blank(s) follow: no second block to loosen against.
            i += 1;
            continue;
        }
        if !continuation_line_opens_sub_block(lines[j], &lines[j + 1..]) {
            return true;
        }
        i += 1;
    }
    false
}

/// Whether `line` (already dedented to column 0) begins a sub-block that, when
/// it follows a blank line inside a list item, keeps the item tight (§17 L2).
/// A nested list marker counts (a sub-list after a blank attaches tight); a
/// plain paragraph does not (it loosens, §17 L1). Mirrors the oracle's
/// `opensSubBlock` plus its marker handling.
fn continuation_line_opens_sub_block(line: &str, rest: &[&str]) -> bool {
    if is_list_marker(line) {
        return true;
    }
    if interrupts_paragraph_with_rest(line, rest) {
        return true;
    }
    false
}

/// A flush-left single-line comment (`%% ...`), the one construct §24 C3
/// recognizes at any column that leaves its container open. A comment FENCE
/// (`%%%`) is NOT one: it opens a multi-line block, and all three engines let it
/// end the list it follows.
fn is_flush_line_comment(line: &str) -> bool {
    line.starts_with("%%") && detect_comment_fence_line(line).is_none()
}

/// Is the next line one `collect_trailing_lazy` could actually fold?
///
/// A cheap peek, kept separate because the open-paragraph test that follows it
/// reparses the whole collected subtree: it must run only when there is a lazy
/// line pending, else deeply nested lists blow up.
fn lazy_line_pending(cur: &mut LineCursor) -> bool {
    let Some(line) = cur.peek() else { return false };
    let line = line.to_string();
    !is_blank_line(&line)
        && indent_columns(&line) == 0
        && !is_list_marker(&line)
        && !interrupts_paragraph(cur, &line)
}

/// AND NOTHING CLOSES MEANS THE CONTAINER GOES ON COLLECTING (PART 1 S4,
/// markup-carve/carve#980).
///
/// S4's lazy branch folds a flush-left line into the innermost OPEN PARAGRAPH
/// and ends with "and NOTHING closes". That binds the lines AFTER the folded
/// one as much as the folded one itself, so a line that comes back to the
/// container's content column is still that container's content. Fold the lazy
/// run, RESUME collecting into the same stream, and repeat.
///
/// Folding once and stopping renders a document nothing in the stack ever
/// closed: `- x` / `  :::` / `  a` / `d` / `  b` / `  :::` folded `d` into the
/// div's paragraph and then left `b` beside the div with a stray empty one
/// after it (markup-carve/carve-rs#813).
///
/// `open` is the governing parameter and the caller supplies it, because the
/// three call sites qualify it differently -- a pending blank closes the
/// paragraph in one, and a trailing heading keeps the container open for
/// flush-left text in two (carve#326). What none of them may do is read the
/// FENCE KIND: a code fence body simply never holds an open paragraph, which
/// is the whole of the asymmetry (carve#980).
fn fold_lazy_run_and_resume(
    cur: &mut LineCursor,
    nested: &mut MappedSource,
    content_col: usize,
    mut open: impl FnMut(&str, bool) -> bool,
    mut resume: impl FnMut(&mut LineCursor) -> MappedSource,
) {
    loop {
        if !lazy_line_pending(cur) {
            break;
        }
        // The column the run ended at, taken before the fold consumes anything.
        let below = last_consumed_line_below_column(cur, content_col);
        if !open(&nested.source, below) {
            break;
        }
        let before = cur.pos;
        collect_trailing_lazy(cur, nested);
        if cur.pos == before {
            break;
        }
        let before_resume = cur.pos;
        nested.append(resume(cur));
        if cur.pos == before_resume {
            break;
        }
    }
}

fn collect_trailing_lazy(cur: &mut LineCursor, nested: &mut MappedSource) {
    while let Some(line) = cur.peek() {
        if is_blank_line(line)
            || indent_columns(line) > 0
            || is_list_marker(line)
            || trim_ascii(line) == "+"
            || {
                let line_owned = line.to_string();
                interrupts_lazy_continuation(cur, &line_owned)
            }
        {
            break;
        }
        // The guard above already required column 0, so nothing is taken off
        // this line beyond whatever an outer container removed. Recording it
        // keeps `span_of` able to end a lazily continued block correctly.
        nested.push_newline_at(
            trim_ascii_start(line).to_string(),
            cur.source_line(cur.pos),
            cur.source_col(cur.pos),
        );
        cur.consume();
    }
}

fn collect_item_continuation_block_mapped(
    cur: &mut LineCursor,
    parent_indent: usize,
    content_col: usize,
    open_fence: &mut Option<FenceOpen>,
) -> MappedSource {
    collect_indented_block_mapped_with(cur, parent_indent, content_col, true, open_fence)
}

/// How far to dedent a collected line, given the container's content column.
///
/// At or past that column the line IS item content and lands flush at column 0.
/// Below it the line never reached the column, so it is lazy paragraph text: it
/// keeps its OWN indentation instead, since dedenting it to 0 is what let the
/// recursive parse read it as a block. `- - a` / ` # H` published an `<h1>`, and
/// ` - b` a sibling item, where a below-column line at the TOP level already
/// folded as text (carve-rs#512).
fn dedent_for_collection(line: &str, indent: usize, strip_cols: usize) -> usize {
    if indent >= strip_cols {
        return strip_cols;
    }
    // Two exceptions dedent all the way. A DEFINITION (`:  `) attaches to the
    // term above it from ANY column, which is why an under-indented one is a
    // `<dd>` and not lazy text (corpus 154) - its TERM (`:: `) is not lenient
    // and folds like everything else. And a comment is invisible wherever it
    // sits: this engine finds it after trimming, so keeping the column would
    // leave its blank line in a different place without making it text.
    let trimmed = trim_ascii_start(line);
    if trimmed.starts_with(":  ") || trimmed.starts_with("%%") {
        return indent;
    }
    // Plain text dedents all the way too: its leading whitespace is not
    // significant, and keeping it would publish a `<dt>term\n wrapped</dt>`
    // where the corpus wants the wrapped line flush (corpus 156). Only a
    // block-SHAPED line needs its column back - that column is the whole
    // reason it is text rather than the block it looks like.
    if line_starts_paragraph(trimmed) {
        return indent;
    }
    // A block-shaped line keeps exactly ONE column, whatever it was indented
    // by. §24 C3: BELOW the content column a marker folds as lazy item text and
    // no other opener nests either - the depth of the indent does not enter
    // into it, and Rule B's "any indent" is scoped to where a TOP-LEVEL list
    // may open (C4), not to nesting. Keeping the ORIGINAL column let a line two
    // columns in reach the SUB-list's content column inside the re-parsed
    // stream and open a list there, which is what all three engines used to do
    // (carve#603). One column can reach no content column at all, so the fold
    // holds at every depth.
    indent.saturating_sub(1)
}

fn collect_indented_block_mapped(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
) -> MappedSource {
    collect_indented_block_mapped_with(cur, parent_indent, strip_cols, false, &mut None)
}

/// The same collection, told that a fenced code block is ALREADY OPEN because
/// its opener was the item's MARKER LINE (`- ``` `) - a line this collector
/// never sees. Without the seed the guard below has nothing to guard, and the
/// body's first below-column line folds in (PART 9 §24, markup-carve/carve#950).
fn collect_indented_block_mapped_after_fence(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
    open_fence: &mut Option<FenceOpen>,
) -> MappedSource {
    collect_indented_block_mapped_with(cur, parent_indent, strip_cols, false, open_fence)
}

fn collect_indented_block_mapped_with(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
    stop_at_content_column_marker: bool,
    fence: &mut Option<FenceOpen>,
) -> MappedSource {
    if cur.line_map.is_none() {
        return MappedSource {
            source: collect_indented_block_plain_with(
                cur,
                parent_indent,
                strip_cols,
                stop_at_content_column_marker,
                fence,
            ),
            line_map: Vec::new(),
            col_map: Vec::new(),
        };
    }
    let mut lines = Vec::new();
    let mut line_map = Vec::new();
    let mut col_map: Vec<Option<isize>> = Vec::new();
    let mut block_indent: Option<usize> = None;
    // A COLON CONTAINER IS THE THIRD OPEN FENCE at the item's content column,
    // and this collector tracked only the code one. §24 S1 MATCH PREFIXES and
    // S2 place a line by the COLUMN it reaches and never by its first
    // character, so a marker at the body's own column inside an open `:::` is
    // the same continuation a plain `x` is - which is exactly what the code
    // spelling beside it already answers (corpus category 278). Without it
    // `- x` / `  :::` / `  a` / `  - m` / `  b` / `  :::` split the div around a
    // nested list and published a spurious empty `div` for the closer (corpus
    // category 279 row 5).
    //
    // A STACK of exact widths, not a depth count: a colon fence closes on an
    // EXACT length match (markup-carve/carve#455), so a wider run nests rather
    // than closes.
    let mut colon_open: Vec<usize> = Vec::new();
    let mut comment_fence: Option<(usize, usize)> = None;
    let mut comment_fence_strip: Option<usize> = None;
    let mut definition_ended_paragraph = false;
    while let Some(line) = cur.peek() {
        if is_blank_line(line) {
            // INSIDE AN OPEN FENCE A BLANK IS CONTENT. Mirrors the plain
            // collector: the lookahead below asks whether the ITEM continues,
            // which is the wrong question while a fence is running. The plain
            // path was fixed in #911 and this one was not, so `--html` (which
            // takes the plain path) and `--json`/`fmt` (which take this one)
            // parsed the same document differently (markup-carve/carve-rs#908).
            if fence.is_some() {
                lines.push(String::new());
                if cur.line_map.is_some() {
                    line_map.push(cur.source_line(cur.pos));
                }
                col_map.push(cur.source_col(cur.pos));
                cur.consume();
                continue;
            }
            // Lazy continuation does not cross a blank line: after a blank, only
            // keep collecting if the next non-blank line is still indented to the
            // block's own level. A shallower line (e.g. a dedent landing below a
            // sublist) ends the block and is left for the caller, so it can close
            // the list rather than fold in (grammar §10, corpus 81-list-lazy-5).
            {
                // No block collected yet means the item's content is all on the
                // MARKER line (`- - a`, `- # H`), so there is no block indent to
                // compare against - and this guard used to be skipped entirely,
                // letting a post-blank line BELOW the content column be collected
                // as part of the item. `- - a` / blank / ` b` put `b` in the
                // outer item where carve-js and the executable spec end the list
                // (#578, corpus 190). The content column is the right floor: it
                // is what a line has to reach to still belong to the item.
                let bi = block_indent.unwrap_or(strip_cols);
                let mut k = cur.pos + 1;
                while k < cur.lines.len() && is_blank_line(cur.lines[k]) {
                    k += 1;
                }
                // The item's CONTENT COLUMN outright. It used to be
                // `min(block_indent, content_col)`, for the sibling-marker case
                // (carve-rs#301): the collected block is indented DEEPER than
                // the column and a marker below it still belongs to the item.
                // But there both spellings give the column, so the min only ever
                // differed when the block was SHALLOWER - and a block below the
                // content column is text, not a block (§24 C3). Taking the
                // column directly is the same rule with nothing left to get
                // wrong, and it is what made the comment special-casing this
                // guard once needed redundant.
                let threshold = strip_cols;
                let _ = bi;
                let continues = k < cur.lines.len() && indent_columns(cur.lines[k]) >= threshold;
                if !continues {
                    break;
                }
            }
            lines.push(String::new());
            if cur.line_map.is_some() {
                line_map.push(cur.source_line(cur.pos));
            }
            col_map.push(cur.source_col(cur.pos));
            cur.consume();
            continue;
        }
        let indent = indent_columns(line);
        if indent <= parent_indent {
            break;
        }
        if definition_ended_paragraph && indent < strip_cols {
            break;
        }
        // A FENCED BODY IS NOT A PARAGRAPH, so nothing below the content column
        // folds while one is open (PART 9 §24, markup-carve/carve#950). §24's
        // STEP algorithm says it twice over: a below-column line supplies none
        // of the body's indentation, so S1 MATCH PREFIXES stops at the ITEM and
        // S2 FENCED BODY never fires - S2 wants the innermost MATCHED container
        // to be the body. S4 governs, and its lazy branch continues an open
        // PARAGRAPH, which a verbatim body is not. The unmatched containers
        // close: the item holds an EMPTY code block and the residue re-parses in
        // the surviving context.
        //
        // The guard is on the OPEN FENCE, not on where the fence was opened.
        // Seeding it from the marker line alone leaves the identical shape with
        // the fence opened at the content column still folding, because a reader
        // that asks "is a paragraph open" sees one again as soon as the body
        // collects a line (corpus 276 row 6). And the fence need not be the
        // item's first block at all - one opened on a CONTINUATION line closes
        // the item at the same place (row 7).
        if fence.is_some() && indent < strip_cols {
            break;
        }
        let is_marker = detect_list_marker_full(line).is_some();
        // INSIDE AN OPEN FENCE THE MARKER IS CODE TEXT (corpus category 278,
        // markup-carve/carve#975). PART 9 §24's S1 MATCH PREFIXES and S2 FENCED
        // BODY place a line by the COLUMN it reaches; neither reads the line's
        // first character. So a list marker at the item's content column, inside
        // a fence that item opened, is the same continuation a plain `x` is -
        // which corpus 276 row 3 already pins, and which category 278 differs
        // from by two characters. Without the guard the marker severed the
        // verbatim body: the fence closed empty, the marker opened a sub-list,
        // and the fence's own closer became an empty code span.
        //
        // A COMMENT BODY IS VERBATIM FOR THE SAME REASON (§28, carve-rs#1053).
        // §28 makes a comment's body opaque exactly as §24 makes a code fence's,
        // so the marker inside one is comment text and severing the chunk there
        // is the same defect in a second spelling. Left out, the opener landed
        // in a chunk of its own and re-parsed as an unterminated fence - which
        // §28 degrades to a `%%` line comment - the marker opened a sub-list,
        // and the closer degraded the same way in the next chunk. Both
        // delimiters vanished and the body rendered: the one outcome a comment
        // may never have. A heading or a block quote in that position was
        // already correct, but by omission - they never set `is_marker` - so the
        // asymmetry was in this gate, not in any opener set.
        //
        // `comment_fence` is the state the PRECEDING lines left, because the
        // tracker below has not run for this line yet, which is what makes it
        // the right question here: it is set only where a closer really follows
        // (§28), so an unterminated `%%%` still degrades and a marker under it
        // still stops the chunk.
        if stop_at_content_column_marker
            && is_marker
            && indent >= strip_cols
            && fence.is_none()
            && comment_fence.is_none()
            && colon_open.is_empty()
        {
            break;
        }
        // A comment is not a block, so it does not set the block indent. It
        // renders nothing and may sit BELOW the content column (§24 C3), and
        // taking its column here lowered the post-blank threshold below the
        // content column - which is how `- - a` / ` %% c` / blank / ` b` kept
        // `b` in the item after the bare form had been fixed (#578, corpus 190).
        if block_indent.is_none() {
            block_indent = Some(indent);
        }
        // Dedent by the item's content column so a nested block (sub-list, block
        // quote, heading) reaches column 0 and parses. A sub-list marker line is
        // dedented residual-aware so tab+space-aligned siblings keep the same
        // visual column (the recursive parse re-derives the child base); other
        // lines use whole-tab dedent so they land flush at column 0.
        // A comment fence travels as ONE span, opener through closer, at ONE
        // dedent. Per-line dedenting takes the opener's whole indent (the `%%`
        // exception in `dedent_for_collection` matches `%%%` too) and leaves it
        // on the frame's base column, where it ends the list nested there:
        // `- - a` / ` %%% c` / ` %%%` / ` b` closed the inner item and moved `b`
        // out of it. Dedenting the opener but not its body is no better - the
        // body then sits outside its own fence and renders as item text
        // (carve-rs#581, corpus 191).
        //
        // Below the content column the fence reached no container, so the span
        // keeps its columns; at or past it the span is item content and dedents
        // like everything else.
        let was_in_comment_span = comment_fence_strip.is_some();
        if let Some((fence_len, _)) = comment_fence {
            if is_comment_fence_close_any_column(line, fence_len) {
                comment_fence = None;
            }
        } else if let Some(open) =
            detect_comment_fence_line_any_column(line).filter(|_| fence.is_none())
        {
            // A `%%%` INSIDE A CODE FENCE IS CODE TEXT, so it opens no span
            // (§24, and the same reading of §28 the marker gate above applies).
            // `fence` is the state the preceding lines left, so the code fence's
            // own opener and closer still read as themselves and only body lines
            // are excluded. Without the filter the span opened on code text and
            // was still open at the code fence's end, which suppressed the
            // marker gate for the REAL comment that followed: `- item` / a code
            // fence holding `%%%` / `  - x` / `  %%%` / `  - z` / `  %%%` put
            // `z` on the page, where the delimiters around it should have hidden
            // it. The dedent below reads the same state, so opening it on code
            // text was already wrong before the gate started asking.
            //
            // §28: a fence with NO closer ahead is not a fence - it degrades to
            // an ordinary `%%` line comment, and the lines after it are just
            // lines. Opening the span anyway dedented the next one by the span's
            // strip, which lifted a BELOW-column line to the body's column 0 and
            // parsed it as a block: `- a` / `  %%% x` / ` # h` published an
            // `<h1>` where every other engine keeps `# h` as text
            // (carve-rs#586).
            if cur.has_comment_closer_after(cur.pos + 1, open.fence_len) {
                comment_fence = Some((open.fence_len, indent));
                comment_fence_strip = Some(if indent < strip_cols { 0 } else { strip_cols });
            }
        }
        let in_comment_span = was_in_comment_span || comment_fence.is_some();
        let stripped = match (in_comment_span, comment_fence_strip) {
            (true, Some(span_strip)) => span_strip.min(indent),
            _ => dedent_for_collection(line, indent, strip_cols),
        };
        if comment_fence.is_none() {
            comment_fence_strip = None;
        }
        let (sliced, consumed, synthetic) = slice_columns_mapped(line, stripped, true);
        definition_ended_paragraph = trim_ascii(&sliced) == DEFINITION_PLACEHOLDER;
        // A COLON LINE INSIDE A CODE OR COMMENT SPAN OPENS AND CLOSES NOTHING:
        // §28 makes both bodies verbatim, which is the same reading that keeps a
        // marker in one from being a marker. Read before the code tracker
        // advances, so the CLOSER line counts as inside its own fence.
        //
        // Without it a `:::` written inside a code fence pushed onto the stack
        // and never came off, so the item's next REAL `:::` opener matched that
        // ghost as a closer, the stack emptied one level early, and the marker
        // gate severed the very div this tracker exists to keep whole.
        let opaque_here = fence.is_some() || in_comment_span;
        track_collected_fence(fence, &sliced, indent >= strip_cols);
        track_collected_colon_fence(
            &mut colon_open,
            &sliced,
            indent >= strip_cols && !opaque_here,
        );
        lines.push(sliced);
        if cur.line_map.is_some() {
            line_map.push(cur.source_line(cur.pos));
        }
        // The enclosing container may already have stripped something, so the
        // widths accumulate; an unknown parent width keeps this unknown too.
        //
        // A straddling tab's residual is re-emitted as SPACES that are not in
        // the source, so the plain `outer + stripped` charged offsets past them
        // to characters that do not exist and spans ran off the end of the
        // document (carve-rs#700). The content after the run is still a suffix,
        // so the anchor moves back by the synthetic width instead: what the
        // slice actually consumed, minus what was written in its place.
        //
        // Only a position INSIDE the run has no source, and nothing starts
        // there - it is whitespace and the marker follows it.
        //
        // The difference is SIGNED and no longer bails out when the slice wrote
        // more than it consumed. It used to `checked_sub` and drop the line's
        // anchor, because the map could not hold a negative constant - the same
        // limit that lost a tab-indented footnote continuation its positions
        // (carve-rs#736).
        col_map.push(
            cur.source_col(cur.pos)
                .map(|outer| outer + consumed as isize - synthetic as isize),
        );
        cur.consume();
    }
    // TERMINATE while a fence is still open, so the blank collected above
    // survives the round trip back through `str::lines()`. Same rule the plain
    // collector applies (markup-carve/carve-rs#908).
    let mut source = lines.join("\n");
    if fence.is_some() && lines.last().is_some_and(|line| line.is_empty()) {
        source.push('\n');
    }
    MappedSource {
        col_map,
        source,
        line_map,
    }
}

/// Follow the fenced code block a collected line opens or closes, so the
/// below-column guard above knows whether one is open.
///
/// Only a line AT OR PAST the content column can do either: below it the line
/// never reached the container, so a fence-shaped one is paragraph text and
/// opens nothing (§24 C3). The test runs on the DEDENTED line for the same
/// reason `detect_fence_open` refuses an indented one - a residual column is
/// still a column.
/// Advance a collector's OPEN COLON CONTAINER stack over one collected line.
///
/// The code counterpart is `track_collected_fence`, and this is deliberately
/// written beside it: one construct answered in two places is what let a `:::`
/// body sever on a marker where a code body did not.
///
/// EXACT WIDTHS, so the widths in flight are a stack (markup-carve/carve#455):
/// a bare run of the innermost width closes, any other opener nests. Only a
/// line that reaches the content column can open or close one - below it the
/// line is text (§24 C3), which is the same condition the code tracker takes.
fn track_collected_colon_fence(open: &mut Vec<usize>, line: &str, at_content_column: bool) {
    if !at_content_column {
        return;
    }
    if let Some(len) = exact_colon_fence_len(line) {
        if open.last() == Some(&len) {
            open.pop();
            return;
        }
    }
    if let Some(len) = detect_line_block_open(line) {
        open.push(len);
        return;
    }
    if let Some(len) = detect_hardbreaks_block_open(line) {
        open.push(len);
        return;
    }
    if let Some(container) = detect_container_open(line) {
        open.push(container.fence_len);
    }
}

fn track_collected_fence(fence: &mut Option<FenceOpen>, dedented: &str, at_content_column: bool) {
    if !at_content_column {
        return;
    }
    match *fence {
        Some(open) => {
            if is_fence_close(dedented, open) {
                *fence = None;
            }
        }
        None => {
            if let Some(open) = detect_fence_open(dedented) {
                *fence = Some(open);
            }
        }
    }
}

fn collect_indented_block_plain_with(
    cur: &mut LineCursor,
    parent_indent: usize,
    strip_cols: usize,
    stop_at_content_column_marker: bool,
    fence: &mut Option<FenceOpen>,
) -> String {
    let mut lines = Vec::new();
    let mut block_indent: Option<usize> = None;
    // A COLON CONTAINER IS THE THIRD OPEN FENCE at the item's content column,
    // and this collector tracked only the code one. §24 S1 MATCH PREFIXES and
    // S2 place a line by the COLUMN it reaches and never by its first
    // character, so a marker at the body's own column inside an open `:::` is
    // the same continuation a plain `x` is - which is exactly what the code
    // spelling beside it already answers (corpus category 278). Without it
    // `- x` / `  :::` / `  a` / `  - m` / `  b` / `  :::` split the div around a
    // nested list and published a spurious empty `div` for the closer (corpus
    // category 279 row 5).
    //
    // A STACK of exact widths, not a depth count: a colon fence closes on an
    // EXACT length match (markup-carve/carve#455), so a wider run nests rather
    // than closes.
    let mut colon_open: Vec<usize> = Vec::new();
    let mut comment_fence: Option<(usize, usize)> = None;
    let mut comment_fence_strip: Option<usize> = None;
    let mut definition_ended_paragraph = false;
    while let Some(line) = cur.peek() {
        if is_blank_line(line) {
            // INSIDE AN OPEN FENCE A BLANK IS CONTENT, not a gap between blocks.
            // The lookahead below asks whether the ITEM continues, and answers
            // no when nothing after the blank reaches the content column - which
            // is right for spacing and wrong for a fence still running, whose
            // body continues to its closer or to the container's end. A fence
            // that ended with the item therefore lost its trailing blank before
            // any join could preserve it (markup-carve/carve-rs#908).
            if fence.is_some() {
                lines.push(String::new());
                cur.consume();
                continue;
            }
            {
                // No block collected yet means the item's content is all on the
                // MARKER line (`- - a`, `- # H`), so there is no block indent to
                // compare against - and this guard used to be skipped entirely,
                // letting a post-blank line BELOW the content column be collected
                // as part of the item. `- - a` / blank / ` b` put `b` in the
                // outer item where carve-js and the executable spec end the list
                // (#578, corpus 190). The content column is the right floor: it
                // is what a line has to reach to still belong to the item.
                let bi = block_indent.unwrap_or(strip_cols);
                let mut k = cur.pos + 1;
                while k < cur.lines.len() && is_blank_line(cur.lines[k]) {
                    k += 1;
                }
                // The item's CONTENT COLUMN outright - see the mapped
                // collector above (carve-rs#301).
                let threshold = strip_cols;
                let _ = bi;
                let continues = k < cur.lines.len() && indent_columns(cur.lines[k]) >= threshold;
                if !continues {
                    break;
                }
            }
            lines.push(String::new());
            cur.consume();
            continue;
        }
        let indent = indent_columns(line);
        if indent <= parent_indent {
            break;
        }
        if definition_ended_paragraph && indent < strip_cols {
            break;
        }
        // Same fenced-body guard as the mapped collector above (§24, #950).
        if fence.is_some() && indent < strip_cols {
            break;
        }
        let is_marker = detect_list_marker_full(line).is_some();
        // INSIDE AN OPEN FENCE THE MARKER IS CODE TEXT (corpus category 278,
        // markup-carve/carve#975). PART 9 §24's S1 MATCH PREFIXES and S2 FENCED
        // BODY place a line by the COLUMN it reaches; neither reads the line's
        // first character. So a list marker at the item's content column, inside
        // a fence that item opened, is the same continuation a plain `x` is -
        // which corpus 276 row 3 already pins, and which category 278 differs
        // from by two characters. Without the guard the marker severed the
        // verbatim body: the fence closed empty, the marker opened a sub-list,
        // and the fence's own closer became an empty code span.
        // Same comment-body guard as the mapped collector above (§28, #1053).
        if stop_at_content_column_marker
            && is_marker
            && indent >= strip_cols
            && fence.is_none()
            && comment_fence.is_none()
            && colon_open.is_empty()
        {
            break;
        }
        // A comment is not a block, so it does not set the block indent. It
        // renders nothing and may sit BELOW the content column (§24 C3), and
        // taking its column here lowered the post-blank threshold below the
        // content column - which is how `- - a` / ` %% c` / blank / ` b` kept
        // `b` in the item after the bare form had been fixed (#578, corpus 190).
        if block_indent.is_none() {
            block_indent = Some(indent);
        }
        // Same one-dedent-per-span rule as the mapped collector above.
        let was_in_comment_span = comment_fence_strip.is_some();
        if let Some((fence_len, _)) = comment_fence {
            if is_comment_fence_close_any_column(line, fence_len) {
                comment_fence = None;
            }
        } else if let Some(open) =
            detect_comment_fence_line_any_column(line).filter(|_| fence.is_none())
        {
            // See the mapped collector: a fence with no closer ahead degrades to
            // a line comment (§28), so it opens no span (carve-rs#586); and a
            // `%%%` inside a code fence is code text, so it opens none either.
            if cur.has_comment_closer_after(cur.pos + 1, open.fence_len) {
                comment_fence = Some((open.fence_len, indent));
                comment_fence_strip = Some(if indent < strip_cols { 0 } else { strip_cols });
            }
        }
        let in_comment_span = was_in_comment_span || comment_fence.is_some();
        let stripped = match (in_comment_span, comment_fence_strip) {
            (true, Some(span_strip)) => span_strip.min(indent),
            _ => dedent_for_collection(line, indent, strip_cols),
        };
        if comment_fence.is_none() {
            comment_fence_strip = None;
        }
        let sliced = slice_columns(line, stripped, true);
        definition_ended_paragraph = trim_ascii(&sliced) == DEFINITION_PLACEHOLDER;
        // A COLON LINE INSIDE A CODE OR COMMENT SPAN OPENS AND CLOSES NOTHING:
        // §28 makes both bodies verbatim, which is the same reading that keeps a
        // marker in one from being a marker. Read before the code tracker
        // advances, so the CLOSER line counts as inside its own fence.
        //
        // Without it a `:::` written inside a code fence pushed onto the stack
        // and never came off, so the item's next REAL `:::` opener matched that
        // ghost as a closer, the stack emptied one level early, and the marker
        // gate severed the very div this tracker exists to keep whole.
        let opaque_here = fence.is_some() || in_comment_span;
        track_collected_fence(fence, &sliced, indent >= strip_cols);
        track_collected_colon_fence(
            &mut colon_open,
            &sliced,
            indent >= strip_cols && !opaque_here,
        );
        lines.push(sliced);
        cur.consume();
    }
    // TERMINATE while a fence is still open, so the blank the loop above just
    // collected survives the round trip back through `str::lines()`. Outside a
    // fence a trailing blank is spacing and the plain join is right
    // (markup-carve/carve-rs#908).
    let mut source = lines.join("\n");
    if fence.is_some() && lines.last().is_some_and(|line| line.is_empty()) {
        source.push('\n');
    }
    source
}

fn detect_block_image(line: &str) -> Option<Image> {
    if !line.starts_with("![") {
        return None;
    }
    let bytes = line.as_bytes();
    let bracket_matches = compute_bracket_matches(bytes);
    // Block-image detection runs once on a single line (not in a per-position
    // loop), so full-slice last-occurrence scans are fine here.
    let bounds = InlineBounds {
        matches: &bracket_matches,
        last_close_paren: bytes.iter().rposition(|&b| b == b')'),
        last_close_brace: bytes.iter().rposition(|&b| b == b'}'),
        last_close_bracket: bytes.iter().rposition(|&b| b == b']'),
        last_gt: bytes.iter().rposition(|&b| b == b'>'),
        delim_brace: [None; DELIM_BRACE_SLOTS],
    };
    let (img, consumed) = parse_image_at(bytes, 0, &bounds)?;
    let after = &line[consumed..];
    if !trim_ascii(after).is_empty() {
        return None;
    }
    Some(img)
}

fn parse_paragraph(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let span_start = cur.pos;
    // Whether the first line sits at the container's content column (flush-left
    // here, since the caller has dedented to that column). Only a content-column
    // image + `^ caption` promotes to a `<figure>` later; an indented one stays
    // literal (strict column-0 rule).
    let at_content_column = cur.peek().is_some_and(|l| !l.starts_with([' ', '\t']));
    let mut lines: Vec<&str> = Vec::new();
    let mut suppress_colon_interrupt = false;
    while let Some(line) = cur.peek() {
        if is_blank_line(line) {
            break;
        }
        // First line is always part of the paragraph; from the second on, a
        // visible block opener interrupts (§10).
        let line_owned = line.to_string();
        if !lines.is_empty()
            && interrupts_paragraph(cur, &line_owned)
            && !(suppress_colon_interrupt && is_suppressed_colon_fence_line(&line_owned))
        {
            break;
        }
        cur.consume();
        // Leading indentation is not significant in a paragraph (djot has no
        // indented code blocks); strip it so an indented line like ` c` renders
        // as `<p>c</p>`, matching list-item continuation handling.
        //
        // TRAILING WHITESPACE IS DROPPED ON EVERY CONTENT LINE, not just the
        // block's last (PART 2 NO TRAILING WHITESPACE, carve#926). The two
        // documents `abc<newline>def` and `abc<SP><newline>def` are the same
        // document. This engine stripped only the joined END, which is the
        // paragraph-final line, so a run before a SOFT BREAK survived into the
        // output.
        //
        // carve-rs#359 limited stripping to block-final lines on the strength of
        // PART 12 §7, which claimed `a` + SPACE + newline + `b` renders
        // `<p>a \nb</p>` and argued from that claim that stripping breaks
        // `to_html(fmt(x)) == to_html(x)`. §7 has been corrected: the executable
        // spec does not render it that way, and the PARSER is the half that
        // moves.
        //
        // `trim_ascii_end` is space-and-tab, deliberately not `str::trim_end`:
        // every other invisible character is CONTENT and survives, however
        // invisible - a no-break space, a zero-width space, a byte order mark,
        // an en quad, an ideographic space, a form feed and a vertical tab. An
        // implementation using a Unicode whitespace property (or a language's
        // legacy `\s`) fails seven of those rows, and a plain-space fixture
        // cannot see it.
        let trimmed = trim_ascii_end(trim_ascii_start(line));
        suppress_colon_interrupt |= is_invalid_colon_fence_opener_text(trimmed);
        lines.push(trimmed);
    }
    // A paragraph never carries its OWN trailing attribute block: a standalone
    // `{...}` line floats forward (handled via interrupts_paragraph + the
    // pending-attrs loop), and a trailing same-line `{...}` with no abutting
    // host stays literal inline content (§14). Paragraph attributes come only
    // from a preceding block-attribute line (§15), applied by the caller.
    // Every line arrives already stripped at both ends (see the loop above), so
    // the join needs no further trimming. A HARD BREAK is unaffected: it is a
    // trailing BACKSLASH, which is content and not whitespace, so the line does
    // not end in whitespace at all and nothing is dropped in front of it. Carve
    // has no two-space hard break to lose - `a<SP><SP>` + newline + `b` is one
    // paragraph with a soft break in the executable spec.
    let joined = lines.join("\n");
    let joined = joined.as_str();
    let children = if options.positions {
        let anchors = lines
            .iter()
            .enumerate()
            .map(|(idx, line)| inline_anchor_for_line(cur, span_start + idx, line))
            .collect();
        parse_inline_lines_with_anchor(joined, options, anchors)
    } else {
        parse_inline_with_options(joined, options)
    };
    let pos = span_of(cur, span_start, cur.pos, options);
    BlockNode::Paragraph(Paragraph {
        attrs: None,
        children,
        at_content_column,
        pos,
    })
}

/// Whether `line`, seen while accumulating a paragraph, ends it and starts a
/// new block (grammar §10, post-Markdown default).
///
/// A VISIBLE block interrupts an open paragraph with no blank line, at the top
/// level and nested: heading, thematic break, block quote, bullet/task list, a
/// valid table row, a fenced code block that has a matching closer ahead, and
/// a valid flush-left colon-fence block. INVISIBLE constructs
/// (comments, abbreviation definitions) interrupt too. ORDERED lists do NOT
/// interrupt, `+` is the continuation marker not a bullet, and a bare image
/// stays inline.
fn interrupts_paragraph(cur: &mut LineCursor<'_>, line: &str) -> bool {
    // §10 (post-Markdown default): a VISIBLE block interrupts an open paragraph
    // with no blank line. Invisible constructs (comments, abbreviation defs)
    // interrupt too. Ordered lists do NOT interrupt, `+` is the continuation
    // marker not a bullet, and a bare image stays inline.
    if trim_ascii_start(line).starts_with("%%")
        || (cur.at_document_level && detect_abbreviation_def(line).is_some())
    {
        return true;
    }
    // A standalone block-attribute line floats forward to the next block (or is
    // dropped when none follows, §15), so it interrupts the paragraph rather
    // than folding in as literal text -- but only FLUSH-LEFT, like the
    // quote/heading/table checks below. `parse_standalone_attrs` trims leading
    // whitespace, so without this guard an INDENTED `{...}` line would interrupt
    // where an indented `> q` / `# h` does not; an indented attr line is lazy
    // paragraph text under the strict column-0 rule (§24 C3), not a floater.
    //
    // The WRAPPED spelling (`{.k` / `#x}`) interrupts on the same grounds. Only
    // the single-line form was tested here, so `{.k` folded into the paragraph
    // as literal text: the author's braces reached the page and the attributes
    // reached nothing (markup-carve/carve-rs#1039). The continuation lines come
    // from the CURSOR, and only when `line` is the line it is parked on - every
    // other caller passes a line this cursor is not standing at, and guessing a
    // block's extent from an unrelated position is how a probe reads a closer
    // that is not there.
    if !line.starts_with([' ', '\t']) {
        if parse_standalone_attrs(line).is_some() {
            return true;
        }
        if cur.lines.get(cur.pos).copied() == Some(line)
            && standalone_attrs_block_len(&cur.lines[cur.pos..]).is_some()
        {
            return true;
        }
    }
    // Symmetric §10: a list marker (bullet OR task OR ordered) does NOT
    // interrupt a paragraph -- a list needs a blank line before it. Only the
    // other visible blocks interrupt.
    if detect_heading(line).is_some()
        || detect_thematic_break(line)
        || strip_blockquote_prefix(line).is_some()
        // A definition-list term `:: ` is a first-class block opener (§24 C3):
        // it interrupts at column 0 and nests at the content column, uniform
        // with quote/heading/fence/table. `is_definition_list_start` requires a
        // flush-left `:: `, so an indented term folds as lazy text like the rest.
        || is_definition_list_start(line)
        // A table row interrupts only when FLUSH-LEFT, like the quote/heading
        // checks above -- `is_table_start` trims leading whitespace, so without
        // this guard an INDENTED row (`  |a|`) would interrupt where an indented
        // `> q` / `# h` does not. An indented row below/above a list item's
        // content column is lazy paragraph text (§24 C3), not a nested table.
        || (!line.starts_with([' ', '\t']) && is_table_start(line))
    {
        return true;
    }
    // Fenced code interrupts only with a matching closer ahead. The
    // opener `line` has been dedented to its container's content column by the
    // caller (a list item's lead paragraph dedents by that column), but the
    // closer probe runs over the RAW remaining lines -- so dedent each by the
    // same amount before the column-exact `is_fence_close`, or a closer that
    // carries the container indent is missed and the fence never interrupts.
    // For a flush (column-0) opener the strip is 0, so top-level fences are
    // unaffected; a strict opener only matches when `line` is flush, so the
    // strip comes from the raw current line's own indentation.
    if let Some(open) = detect_fence_open(line) {
        // The index answers "no closer anywhere ahead" in constant time. The
        // exact probe below still decides, but without this gate it runs from
        // every opener to the end of the document, so a file of unterminated
        // openers costs O(n^2) - the shape `comment_closer_last_index` already
        // removed for `%%%`.
        if !cur.has_code_closer_after(cur.pos, open.fence_char, open.fence_len) {
            return false;
        }
        let strip = leading_ws(cur.lines[cur.pos]);
        let rest = &cur.lines[cur.pos + 1..];
        // THE CLOSER HAS TO BE INSIDE THE SAME CONTAINER. Once this fence
        // opens, a line below the container's content column closes the
        // container: a fenced body is not a paragraph, so nothing folds into it
        // from below (PART 9 §24, markup-carve/carve#950). A closer past that
        // line is therefore not this fence's closer, and by §10 I4 a fence with
        // no closer left does not interrupt - the delimiter run stays paragraph
        // text.
        //
        // `strip` IS that column here: the caller dedents an item's line by the
        // content column before asking, and `detect_fence_open` refuses a line
        // that still carries indentation, so a fence reaching this point sat
        // exactly at the column. At document level the strip is 0 and no line
        // is below it, which leaves top-level fences untouched. A BLANK line is
        // not below anything - inside an open fence it is verbatim content.
        if rest
            .iter()
            .take_while(|l| is_blank_line(l) || leading_ws(l) >= strip)
            .any(|l| is_fence_close(&l[leading_ws(l).min(strip)..], open))
        {
            return true;
        }
    }
    if is_colon_fence_opener_shape(line) {
        return true;
    }
    false
}

fn interrupts_lazy_continuation(cur: &mut LineCursor<'_>, line: &str) -> bool {
    // A caption line (`^ …`) ends a list/blockquote item's lazy continuation
    // rather than folding in: a caption is a heading/figure terminator, not
    // plain prose the item absorbs. It becomes its own top-level block, matching
    // carve-js / carve-php (carve#326). Top-level caption-to-figure attachment
    // runs in the block parser, not this lazy-continuation path.
    interrupts_paragraph(cur, line)
        || is_colon_fence_opener_shape(line)
        || caption_content(line).is_some()
}

fn interrupts_paragraph_as_container(cur: &mut LineCursor<'_>, line: &str) -> bool {
    let saved = cur.at_document_level;
    cur.at_document_level = false;
    let interrupts = interrupts_paragraph(cur, line);
    cur.at_document_level = saved;
    interrupts
}

fn interrupts_lazy_continuation_as_container(cur: &mut LineCursor<'_>, line: &str) -> bool {
    let saved = cur.at_document_level;
    cur.at_document_level = false;
    let interrupts = interrupts_lazy_continuation(cur, line);
    cur.at_document_level = saved;
    interrupts
}

fn is_colon_fence_opener_shape(line: &str) -> bool {
    // Only a FLUSH-LEFT colon fence ends lazy continuation (grammar PART 9
    // §10). An INDENTED colon-shaped line (the detectors trim leading
    // whitespace) is still within the container, so it folds as lazy text
    // instead of escaping the container.
    if line.starts_with(' ') || line.starts_with('\t') {
        return false;
    }
    detect_container_open(line).is_some()
        || detect_line_block_open(line).is_some()
        || detect_hardbreaks_block_open(line).is_some()
}

/// Whether a glued colon fence earlier in the paragraph (`:::note`, `:::]`)
/// keeps THIS line from interrupting it.
///
/// Only a BARE fence (`:::`, nothing after the colons) is held back: it is
/// closer-shaped, and the paragraph it would close is literal text, so it stays
/// literal text too. A real opener - a type word, `::: |`, `::: \` - opens its
/// block as usual, glued predecessor or not (carve-rs#496). Matches carve-js
/// (`RE_ADMONITION_CLOSE` + `isLiteralColonFenceLine`) and carve-php.
fn is_suppressed_colon_fence_line(line: &str) -> bool {
    is_colon_fence_opener_shape(line) && exact_colon_fence_len(line).is_some()
}

fn interrupts_paragraph_with_rest(line: &str, rest: &[&str]) -> bool {
    if trim_ascii_start(line).starts_with("%%") {
        return true;
    }
    // Flush-left only (see interrupts_paragraph): an indented `{...}` line is
    // lazy paragraph text under the strict column-0 rule, not a floating attr.
    //
    // The WRAPPED spelling interrupts too, which is what `rest` is for. Only the
    // single-line form was tested here, so `{.k` opened nothing and folded into
    // the paragraph as literal text - the author's braces reached the page and
    // the attributes reached nothing (markup-carve/carve-rs#1039, and the half
    // of markup-carve/carve#1281's `329-...-6` that the container's end does not
    // answer on its own).
    //
    // The brace test comes FIRST so the common line costs nothing: this
    // predicate is asked of every blank-separated block, and copying `rest`
    // for a heading or a paragraph line would make that quadratic.
    if !line.starts_with([' ', '\t']) && trim_ascii(line).starts_with('{') {
        let mut block: Vec<&str> = Vec::with_capacity(rest.len() + 1);
        block.push(line);
        block.extend_from_slice(rest);
        if standalone_attrs_block_len(&block).is_some() {
            return true;
        }
    }
    if detect_heading(line).is_some()
        || detect_thematic_break(line)
        || strip_blockquote_prefix(line).is_some()
        // A definition-list term `:: ` is a first-class block opener (§24 C3):
        // it interrupts at column 0 and nests at the content column, uniform
        // with quote/heading/fence/table. `is_definition_list_start` requires a
        // flush-left `:: `, so an indented term folds as lazy text like the rest.
        || is_definition_list_start(line)
        // A table row interrupts only when FLUSH-LEFT, like the quote/heading
        // checks above -- `is_table_start` trims leading whitespace, so without
        // this guard an INDENTED row (`  |a|`) would interrupt where an indented
        // `> q` / `# h` does not. An indented row below/above a list item's
        // content column is lazy paragraph text (§24 C3), not a nested table.
        || (!line.starts_with([' ', '\t']) && is_table_start(line))
    {
        return true;
    }
    if let Some(open) = detect_fence_open(line) {
        if rest.iter().any(|l| is_fence_close(l, open)) {
            return true;
        }
    }
    // A FLUSH-LEFT colon-fence family opener interrupts blockquote lazy
    // continuation like any block opener. An INDENTED colon fence (above the
    // quote's content column) is literal paragraph text under the strict
    // column-0 rule, so lazy continuation stays inside the quote.
    if !line.starts_with([' ', '\t'])
        && (detect_container_open(line).is_some()
            || detect_line_block_open(line).is_some()
            || detect_hardbreaks_block_open(line).is_some())
    {
        return true;
    }
    false
}

/// A `- ` / `* ` bullet, including the attributed form `-{.c} ` (NOT `+`, the
/// continuation marker; not ordered).
///
/// Delegates to `detect_unordered` so an attributed bullet interrupts a
/// paragraph just like a plain one (and an attributed task already does via
/// `detect_task`). Leading tabs are skipped as well as spaces: a bullet opens
/// a list at any indentation (Rule B), so a tab-indented bullet interrupts a
/// paragraph too.
fn is_definition_list_start(line: &str) -> bool {
    line.strip_prefix(":: ")
        .is_some_and(|term| !is_blank_line(term))
}

fn parse_definition_list(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let list_start = cur.pos;
    let mut items = Vec::new();
    while let Some(line) = cur.peek() {
        let Some(term) = line.strip_prefix(":: ") else {
            break;
        };
        if is_blank_line(term) {
            break;
        }
        // The SEPARATOR is the space, and whitespace past it is not content -
        // the same rule `#`, `>` and `-` already follow, and the one carve#513
        // and carve#530 restated twice as a property of the separator rather
        // than of any one marker. `stripped_col` locates the column by matching
        // this slice as a suffix of the raw line, so trimming here moves the
        // term's own position with it instead of leaving it on the marker.
        let term = trim_ascii_start(term);
        let term_source_line = cur.source_line(cur.pos);
        let term_start = cur.pos;
        cur.consume();
        // A term folds a following plain line like a heading (soft break), so a
        // wrapped term line does not strand the definition. A blank line, a new
        // marker (`::` / `:  `), a list marker, or a block opener ends the term.
        let mut term_text = trim_ascii_end(term).to_string();
        let mut term_anchors = options
            .positions
            .then(|| vec![inline_anchor_for_line(cur, term_start, term)]);
        while let Some(next) = cur.peek() {
            if is_blank_line(next)
                || next.strip_prefix(":: ").is_some()
                || next.strip_prefix(":  ").is_some()
                || is_list_marker(next)
            {
                break;
            }
            let owned = next.to_string();
            if interrupts_paragraph(cur, &owned) {
                break;
            }
            term_text.push('\n');
            // A term's continuation line is a CONTENT LINE, so its trailing
            // whitespace run does not reach the output - the same rule the
            // term's FIRST line already follows two statements up, and the one
            // markup-carve/carve#926 made general (markup-carve/carve#1289).
            // Stripping here, at the source layer, is what keeps the exception
            // intact: spaces INSIDE a verbatim run are the construct's content
            // and end at its closing delimiter, so an all-space `` `  ` `` term
            // is untouched by a trim that only ever sees the end of a line.
            term_text.push_str(trim_ascii_end(&owned));
            if let Some(term_anchors) = &mut term_anchors {
                term_anchors.push(inline_anchor_for_line(cur, cur.pos, &owned));
            }
            cur.consume();
        }
        let children = if let Some(term_anchors) = term_anchors {
            parse_inline_lines_with_anchor(&term_text, options, term_anchors)
        } else {
            parse_inline_with_options(&term_text, options)
        };
        // The span covers the `:: ` marker and every line the term folded, the
        // same way a heading's covers its `#`.
        let mut terms = vec![DefinitionTerm {
            attrs: source_line_attrs(None, term_source_line, options),
            children,
            pos: span_of(cur, term_start, cur.pos, options),
        }];

        // CONSECUTIVE terms share an entry, which is what `:: a` / `:: b` on
        // adjacent lines means and what the rendered `<dl>` shows - a run of
        // `<dt>` followed by the `<dd>`s they share. This engine used to open a
        // new entry per term line, so the same document grouped differently
        // here and in carve-js while both rendered the same list; the grouping
        // was an internal nobody could see, until the AST became something
        // engines hand each other (PART 12).
        while let Some(next) = cur.peek() {
            let Some(next_term) = next.strip_prefix(":: ") else {
                break;
            };
            if is_blank_line(next_term) {
                break;
            }
            // Same separator rule as the item's first term above. A list can
            // hold several terms and each one strips its own padding.
            let next_term = trim_ascii_start(next_term);
            let next_source_line = cur.source_line(cur.pos);
            let next_start = cur.pos;
            cur.consume();
            let mut text = trim_ascii_end(next_term).to_string();
            let mut anchors = options
                .positions
                .then(|| vec![inline_anchor_for_line(cur, next_start, next_term)]);
            while let Some(following) = cur.peek() {
                if is_blank_line(following)
                    || following.strip_prefix(":: ").is_some()
                    || following.strip_prefix(":  ").is_some()
                    || is_list_marker(following)
                {
                    break;
                }
                let owned = following.to_string();
                if interrupts_paragraph(cur, &owned) {
                    break;
                }
                text.push('\n');
                // Same rule as the first term's continuation above: a CONSECUTIVE
                // term folds its lines the same way, so it drops the same run.
                text.push_str(trim_ascii_end(&owned));
                if let Some(anchors) = &mut anchors {
                    anchors.push(inline_anchor_for_line(cur, cur.pos, &owned));
                }
                cur.consume();
            }
            let children = if let Some(anchors) = anchors {
                parse_inline_lines_with_anchor(&text, options, anchors)
            } else {
                parse_inline_with_options(&text, options)
            };
            terms.push(DefinitionTerm {
                attrs: source_line_attrs(None, next_source_line, options),
                children,
                pos: span_of(cur, next_start, cur.pos, options),
            });
        }

        let mut defs = Vec::new();

        loop {
            // A blank line before a `:  ` definition is a separator (djot
            // parity): a definition may be separated from its term or a
            // previous definition by a blank line. A blank not followed by a
            // `:  ` definition ends the entry.
            if matches!(cur.peek(), Some(l) if is_blank_line(l)) {
                let mut look = 0usize;
                while matches!(cur.lines.get(cur.pos + look).copied(), Some(l) if is_blank_line(l))
                {
                    look += 1;
                }
                match cur.lines.get(cur.pos + look).copied() {
                    Some(after) if after.strip_prefix(":  ").is_some() => {
                        for _ in 0..look {
                            cur.consume();
                        }
                    }
                    _ => break,
                }
            }
            let Some(line) = cur.peek() else {
                break;
            };
            let Some(def) = line.strip_prefix(":  ") else {
                break;
            };
            if is_blank_line(def) {
                break;
            }
            let def_source_line = cur.source_line(cur.pos);
            let def_start = cur.pos;
            // The `:  ` marker is three codepoints; add whatever an enclosing
            // container already took so a nested block maps back to the document.
            let def_source_col = cur.source_col(cur.pos).map(|c| c + 3);
            cur.consume();
            let def_trimmed = trim_ascii_end(def);
            // First-block form (`:  +`, mirroring the list `- +`): when the sole
            // content is a lone `+`, seed the body with the FOLLOWING flush-left
            // block (no `+` literal), with no indentation. `:  \+` stays literal.
            let mut body = if is_plus_marker(def_trimmed) {
                let mut fb = LineBuffer::default();
                let lines = cur.lines;
                let end = attached_block_end(
                    lines,
                    cur.pos,
                    &mut cur.comment_closer_last_index,
                    &mut |a, _| {
                        is_blank_line(a)
                            || is_plus_marker(a)
                            || a.strip_prefix(":: ").is_some()
                            || a.strip_prefix(":  ").is_some()
                    },
                );
                while cur.pos < end {
                    let a = cur.lines[cur.pos];
                    fb.push_at(
                        a.to_string(),
                        cur.source_line(cur.pos),
                        cur.source_col(cur.pos),
                    );
                    cur.consume();
                }
                fb.into_source()
            } else {
                MappedSource::new_line_at(def_trimmed.to_string(), def_source_line, def_source_col)
            };
            // A FENCED BODY IS NOT A PARAGRAPH, and a definition body is such a
            // container (PART 0 S4, markup-carve/carve#956). The guard is on the
            // OPEN FENCE rather than on where the fence was opened, so it is
            // seeded from the `:  ` MARKER line - which no collector ever sees -
            // and then followed by the collector on the lines it takes. The
            // first-block (`:  +`) form seeds nothing: its body is the following
            // flush-left block, which supplies no indentation for the rule to
            // measure against.
            let mut fence = DefinitionBodyFence {
                open: if is_plus_marker(def_trimmed) {
                    None
                } else {
                    detect_fence_open(def_trimmed)
                },
                closed_last: false,
            };
            let seed = body.source.clone();
            body.append(collect_definition_body(cur, &mut fence, &seed, options));
            // The span covers the `:  ` marker through the last line the body
            // consumed, so a multi-line definition is one region rather than
            // just its opening line. `collect_definition_body` has already
            // advanced the cursor past those lines.
            let children = parse_mapped_source(&body, options);
            let mut pos = span_of(cur, def_start, cur.pos, options);
            if let (Some(pos), Some(last)) = (
                pos.as_mut(),
                children.iter().rev().find_map(crate::ast_json::block_pos),
            ) {
                pos.end_line = last.end_line;
                pos.end_column = last.end_column;
                pos.end_offset = last.end_offset;
            }
            defs.push(DefinitionDef {
                attrs: source_line_attrs(None, def_source_line, options),
                children,
                pos,
            });
        }

        items.push(DefinitionItem {
            // NOT placed, though the cursor could say where it is. The wire
            // format flattens items into a flat run of definition_term and
            // definition_description nodes, so the item is regrouped on the way
            // back in and any span here would not survive a round-trip
            // (PART 12 section 6). A field that is Some before a round-trip and
            // None after is worse than one that is always None.
            terms,
            definitions: defs,
            pos: None,
        });

        let saved = cur.pos;
        while matches!(cur.peek(), Some(line) if is_blank_line(line)) {
            cur.consume();
        }
        if !cur.peek().is_some_and(is_definition_list_start) {
            cur.pos = saved;
            break;
        }
    }
    BlockNode::DefinitionList(DefinitionList {
        attrs: None,
        // The cursor has rolled back past the trailing blanks it looked through
        // for another item, so it points one line past the last definition -
        // the span stops at the content, not at the gap after it.
        pos: span_of(cur, list_start, cur.pos, options),
        items,
    })
}

/// A lone `+` (optionally followed by spaces/tabs) is the continuation marker
/// (PART 9 §17): it attaches the following flush-left block to the open
/// container.
fn is_plus_marker(line: &str) -> bool {
    line.strip_prefix('+')
        .is_some_and(|rest| rest.bytes().all(|b| b == b' ' || b == b'\t'))
}

/// Collect the continuation of a definition body. A definition continues like a
/// list item (PART 9 §17): form A folds an indented block in (a blank line is
/// tolerated when a later line still continues), form B attaches a lone `+`
/// pull-left flush-left block with no indentation, and a flush-left line with no
/// blank before it that does not start an interrupting block lazily continues
/// the open paragraph (matching list items, block quotes and djot). Returned
/// lines carry blank separators so the block sub-parse yields multiple paragraphs.
///
/// The body's own fenced code block, for the S4 test below. `open` carries it
/// while it is still OPEN; `closed_last` records that the last thing collected at
/// the body's column was its CLOSER, with nothing after it yet. Both states leave
/// the body without an open paragraph (PART 0 S4, markup-carve/carve#956).
struct DefinitionBodyFence {
    open: Option<FenceOpen>,
    closed_last: bool,
}

impl DefinitionBodyFence {
    /// S4's lazy branch continues an open PARAGRAPH, and a verbatim body is not
    /// one - neither while it runs nor once it has finished. A CLOSED code block
    /// is a finished block, not a paragraph, so a below-column line after it has
    /// nothing to fold into either.
    fn holds_no_paragraph(&self) -> bool {
        self.open.is_some() || self.closed_last
    }

    /// Follow the fence over one line collected at the body's column.
    fn track(&mut self, dedented: &str) {
        let was_open = self.open.is_some();
        track_collected_fence(&mut self.open, dedented, true);
        // The closer is the line that took the fence from open to closed; any
        // other line at the column is content, and content after a closed block
        // opens a paragraph the fold can reach again.
        self.closed_last = was_open && self.open.is_none();
    }
}

/// Does the definition body collected so far end in something a flush-left line
/// can FOLD INTO?
///
/// PART 0 S4's lazy branch continues an open PARAGRAPH. `DefinitionBodyFence`
/// above answers that for the body's own code fence and has to, because this
/// test reads the body collected SO FAR: an unterminated fence has no closer
/// yet, and PART 9 S10 degrades it to a paragraph, which would answer the open
/// half backwards (carve-rs#785, #789). Everything else the body can end in -
/// an empty block quote, a closed div or admonition, a table, a thematic break,
/// a line block, a block-attribute line that left no block at all - holds no
/// open paragraph either, and this is where those are answered
/// (markup-carve/carve#956, carve-rs#790).
///
/// THE HEADING IS AN EXCEPTION AND IT IS SPELLED HERE, not in
/// `block_ends_with_open_paragraph`. A heading is not a paragraph, so the shared
/// predicate says false for one - yet `- one` / `  :: term` / `  :  # H` /
/// `lazy` keeps the line inside the body, and so does every list spelling beside
/// it (`heading_folds_lazy.rs`, carve#326). The list path arrives at that answer
/// without consulting the shared predicate at all, so an arm added THERE would
/// move the list's pinned answers as well as this one. Written here, it reaches
/// the definition body and nothing else.
/// Whether `source`'s last non-blank line is a standalone block-attribute line,
/// written flush at the body's own column.
///
/// The source is already dedented to that column, so "flush" is column 0 here -
/// the strict column-0 rule, which is what keeps an INDENTED `{…}` ordinary
/// paragraph text.
/// The body's last line is an INTERRUPTER that leaves no block behind.
///
/// A DEFINITION BODY IS A CONTAINER WITH A CONTENT COLUMN (PART 9 §24 C3,
/// markup-carve/carve#1350), and a construct §10 I5 makes an interrupter ends
/// the paragraph it sits under wherever that column is. Most interrupters leave
/// a node the block-level test below can see and need nothing here. These do
/// not, and the tree records what a body HOLDS rather than what was taken out
/// of it, so they are read from the SOURCE:
///
/// - a BLOCK-ATTRIBUTE line (§15 A1), which is applied to the block it precedes
///   and is never a block itself;
/// - a COMMENT (§24 C3), recognized at any column and publishing nothing;
/// - the PLACEHOLDER a collected link or footnote definition leaves in its
///   place, which `parse_comment_block` consumes without building a node so a
///   definition leaves no trace (markup-carve/carve#801).
///
/// The last of the three is why one answer closes both spellings the ruling
/// names: `:  a` / `   [r]: /u` reaches this predicate as `a` over a comment
/// line, exactly as `:  a` / `   %% c` does, because the prepass has already
/// swapped the definition out. Reading the definition line HERE as well would
/// be a second rule for a shape that is already one.
///
/// An OPEN verbatim body cannot reach this: a body whose fence is still open is
/// ended by `DefinitionBodyFence` before the fold is ever asked, so a `%%` line
/// inside code is not read as a comment here.
fn body_ends_with_an_unnoded_interrupter(source: &str) -> bool {
    if body_ends_with_an_attribute_line(source) {
        return true;
    }
    source
        .lines()
        .rev()
        .find(|line| !is_blank_line(line))
        .is_some_and(|last| trim_ascii_start(last).starts_with("%%"))
}

fn body_ends_with_an_attribute_line(source: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let Some(end) = lines.iter().rposition(|line| !is_blank_line(line)) else {
        return false;
    };
    // The WRAPPED spelling (`{.k` / `#x}`) counts too. This used to test the
    // last line alone, so a block that closed on it but OPENED on an earlier one
    // was invisible here and the body was read as ending in a paragraph
    // (markup-carve/carve#1281's `329-...-6`, markup-carve/carve-rs#1039).
    //
    // The scan walks back only over the current run of non-blank lines, because
    // an attribute block is refused at a blank line - so the start can never be
    // further back than that.
    let mut start = end;
    loop {
        if standalone_attrs_block_len(&lines[start..]) == Some(end - start + 1) {
            return true;
        }
        if start == 0 || is_blank_line(lines[start - 1]) {
            return false;
        }
        start -= 1;
    }
}

/// `marker_line_is_the_whole_body` is true while nothing has been collected at
/// the body's own column, so `source` is the `:  ` marker line's content and
/// nothing else. THAT is the half PART 1 S4 rules on (markup-carve/carve#1280,
/// carve-rs#1049): the marker line's content is the body's FIRST BLOCK, so
/// `:  # H` writes a heading there exactly as `:  ` plus an indented `# H`
/// would, and a block that leaves no open paragraph leaves none wherever it was
/// written. Ask S4's one question and let the answer decide.
///
/// The enumeration below answered it from a LIST of kinds instead, and the list
/// disagreed with itself: a table, a thematic break and an attribute block ended
/// the body, while a HEADING and a COMMENT folded - and the LIST spelling of
/// both of those ends, in this same engine, one clause over. So the same
/// document got two answers depending on which of the five kinds sat on the
/// marker.
///
/// Once lines ARE collected at the body's content column the clause leaves the
/// question deliberately open - corpus 75-list-nesting-and-looseness-4 pins the
/// folding answer for the list spelling of that half - so that path keeps the
/// enumeration, exactly as the list path does one call site over.
fn definition_body_takes_the_fold(
    source: &str,
    marker_line_is_the_whole_body: bool,
    options: &Options<'_>,
) -> bool {
    // A BLOCK-ATTRIBUTE LINE LEAVES NO NODE, so the block-level test below
    // cannot see it - and it INTERRUPTS an open paragraph (§15 A1), which is the
    // one thing that test is asking about. `d` / `{.k}` parses to a single
    // paragraph, exactly as `d` alone does, so a body ending in an attribute
    // line reported the paragraph the attribute had already closed, and a
    // flush-left line folded in: `:: t` / `:  d` / `   {.k}` / `tail` published
    // `tail` INSIDE the `dd`, wearing a class the author wrote for a block that
    // never came (markup-carve/carve#1281; the question is S4's, which that
    // ruling leaves to markup-carve/carve#1280).
    //
    // The docblock above already listed "a block-attribute line that left no
    // block at all" among the things this answers. It did - for a body that is
    // ONLY the attribute line, where there is no paragraph node either. With
    // content in front of it there was one, and it answered for that.
    //
    // The WRAPPED spelling (`{.k` / `#x}`) is deliberately NOT covered: a
    // multi-line attribute block interrupts no open paragraph anywhere in this
    // engine, at document level as much as here, and making it do so is a §10 I5
    // question rather than this one.
    if body_ends_with_an_unnoded_interrupter(source) {
        return false;
    }
    // S4's question, asked of the WHOLE body. `body_ends_with_open_paragraph`
    // is the same predicate the list's marker-line path asks, and it does NOT
    // look past a trailing run of comments: there the open paragraph would have
    // to be one written EARLIER on the marker line, and here the comment IS the
    // marker line, so there is no earlier paragraph for it to leave open
    // (`:  %% c` / `tail`, matching `- %% c` / `tail`).
    if if marker_line_is_the_whole_body {
        body_ends_with_open_paragraph(source, options)
    } else {
        nested_ends_with_open_paragraph(source, false, options)
    } {
        return true;
    }
    let blocks = probe_blocks(source, options);
    let mut end = blocks.len();
    // Past a trailing run of comments, for the reason
    // `nested_ends_with_open_paragraph` gives: a comment renders nothing, so it
    // closes no paragraph and cannot be what the body ends in (S24 C3).
    while end > 0 && matches!(blocks[end - 1], BlockNode::Comment(_)) {
        end -= 1;
    }
    if end == 0 {
        return false;
    }
    matches!(
        blocks[end - 1],
        // See above. A HEADING FOLDS ONLY AT THE CONTENT COLUMN, which is the
        // half carve#1280 leaves open; on the marker line S4 has already
        // answered above, and the answer is that it does not.
        BlockNode::Heading(_) if !marker_line_is_the_whole_body
    ) || matches!(
        blocks[end - 1],
        // AN IMAGE BLOCK IS ONLY A BLOCK UNTIL SOMETHING FOLDS INTO IT.
        // `image_is_block` makes a bare image line a block ONLY when the
        // next line does not fold, and a `^ ` caption is an inline
        // continuation a following line extends. Both are decided by the
        // line AFTER the body, which is exactly the line this predicate is
        // being asked about and which the collected source therefore does
        // not contain yet. Reading the block off a body that stops one line
        // early turned `:  ![a](i.png)` / `lazy` into a standalone image
        // plus a top-level paragraph, where the list twin folds - the same
        // read-the-body-so-far trap the fence guard exists to avoid.
        BlockNode::BlockImage(_) | BlockNode::Figure(_)
    )
}

/// `fence` carries the body's own fenced code block, seeded by the caller from
/// the `:  ` marker line - which no collector ever sees - and followed here on
/// every line collected at the body's column. `seed` is that same marker-line
/// content (or, for the `:  +` form, the attached flush-left block), because the
/// paragraph test below needs the WHOLE body and this collector only ever sees
/// the lines after it.
fn collect_definition_body(
    cur: &mut LineCursor,
    fence: &mut DefinitionBodyFence,
    seed: &str,
    options: &Options<'_>,
) -> MappedSource {
    let mut lines: Vec<String> = Vec::new();
    // A LAZY LINE THAT FOLDED LEFT A PARAGRAPH OPEN, by construction: it reached
    // the fold only by being flush-left and not interrupting, which is what
    // paragraph text is. So the next flush-left line's answer is already known
    // and does not need asking.
    //
    // It has to be known, rather than merely nice to know. `so_far` is rebuilt
    // from every line collected so far and then PARSED, once per lazy line, so a
    // paragraph continued lazily under a `:  ` body cost O(n^2) in both the copy
    // and the parse: 32 KB took 22.9 seconds, growing 4x per doubling. That is
    // the §25 shape - a document a reader could plausibly write, degrading
    // superlinearly - and the LIST twin one call site over is linear on the same
    // input, which is what says it is a defect rather than the price of the
    // rule.
    //
    // Only a lazy fold may set it. Every other way a line joins the body -- a
    // collected line at the body's own column, a `+` attached block -- can put a
    // fence, a table or a container at the end of it, so each of those clears it
    // and the next question is asked in full.
    let mut folded_a_lazy_line = false;
    let mut line_map: Vec<Option<usize>> = Vec::new();
    // Codepoints taken off the front of each line, kept in lockstep with
    // `lines`. `None` means unknown, and a block starting there gets no
    // position rather than a guessed one (PART 12 section 4).
    let mut col_map: Vec<Option<isize>> = Vec::new();
    while let Some(line) = cur.peek() {
        // Form B: `+` pull-left continuation.
        if is_plus_marker(line) {
            cur.consume();
            let mut attached = LineBuffer::default();
            let cursor_lines = cur.lines;
            let end = attached_block_end(
                cursor_lines,
                cur.pos,
                &mut cur.comment_closer_last_index,
                &mut |a, _| {
                    is_blank_line(a)
                        || is_plus_marker(a)
                        || a.strip_prefix(":: ").is_some()
                        || a.strip_prefix(":  ").is_some()
                },
            );
            while cur.pos < end {
                let a = cur.lines[cur.pos];
                attached.push_at(
                    a.to_string(),
                    cur.source_line(cur.pos),
                    cur.source_col(cur.pos),
                );
                cur.consume();
            }
            if !attached.lines.is_empty() {
                folded_a_lazy_line = false;
                lines.push(String::new());
                line_map.push(None);
                col_map.push(None);
                lines.extend(attached.lines);
                line_map.extend(attached.line_map);
                col_map.extend(attached.col_map);
            }
            continue;
        }
        // Form A: an indented continuation line (no intervening blank).
        if !is_blank_line(line) {
            let indent = indent_columns(line);
            if indent >= 3 {
                // A STRADDLING TAB LEAVES ITS RESIDUAL COLUMNS IN FRONT OF THE
                // LINE (PART 9 §24 C1 gives a tab a column value, carve-rs#793).
                // With `keep_residual` false a single tab - which reaches column
                // 4, one PAST the body's column - was consumed whole and the
                // residue landed FLUSH LEFT, where a `>` is a block opener. The
                // four-space spelling of the same column keeps its fourth space,
                // so the line sits at column 1 and is lazy text. Same column,
                // two answers.
                //
                // The residual is written back as the spaces the tab bought past
                // the margin, which is what makes the two spellings agree.
                let sliced = slice_columns(line, 3.min(indent), true);
                // Count what was actually removed rather than assuming three:
                // `slice_columns` works in COLUMNS, and a tab is one codepoint
                // spanning several of them. The difference in LENGTH is the
                // right quantity for both cases: with a residual it is
                // `consumed - synthetic`, which is exactly the base the mapping
                // in `slice_columns_mapped` documents.
                col_map.push(cur.source_col(cur.pos).map(|c| {
                    c + line.chars().count().saturating_sub(sliced.chars().count()) as isize
                }));
                fence.track(&sliced);
                folded_a_lazy_line = false;
                lines.push(sliced);
                line_map.push(cur.source_line(cur.pos));
                cur.consume();
                continue;
            }
            // A new term/definition marker ends the definition (the outer loop
            // picks it up).
            if line.strip_prefix(":: ").is_some() || line.strip_prefix(":  ").is_some() {
                break;
            }
            // A FENCED BODY IS NOT A PARAGRAPH (PART 0 S4,
            // markup-carve/carve#956). This line is below the body's content
            // column, so it supplies none of the body's indentation: S1 MATCH
            // PREFIXES stops at the DEFINITION ENTRY and S2 FENCED BODY never
            // fires, S2 wanting the innermost MATCHED container to be the body.
            // S4 governs, and its lazy branch continues an open PARAGRAPH, which
            // a verbatim body is not. The containers close, the `dd` holds an
            // EMPTY code block, and the residue re-parses at document level -
            // byte for byte the answer corpus 276 pins for the list spelling,
            // which this engine already gives (#772).
            //
            // NEITHER WHILE IT RUNS NOR ONCE IT HAS FINISHED. A CLOSED code
            // block is a finished block, so the body has no open paragraph after
            // it either and the same derivation ends the body there. The worked
            // example in the clause shows the open half; the rule is stated on
            // the paragraph, not on the delimiter.
            if fence.holds_no_paragraph() {
                break;
            }
            // AND NEITHER DOES ANY OTHER BODY THAT ENDS IN A FINISHED BLOCK.
            // The fence guard above is one shape of "the body holds no open
            // paragraph"; an empty block quote, a closed div or admonition, a
            // table, a thematic break, a line block and a body a
            // block-attribute line left with no block at all are the others,
            // and each of them folded here while the LIST twin closed
            // (carve-rs#790). Same clause, same answer.
            let mut so_far = String::new();
            if !folded_a_lazy_line {
                so_far.push_str(seed);
                for collected in &lines {
                    so_far.push('\n');
                    so_far.push_str(collected);
                }
            }
            // NOTHING HAS BEEN COLLECTED AT THE BODY'S COLUMN YET, so the body
            // is the marker line's own content and S4's one question is asked of
            // it directly (PART 1 S4, ruled uniform in markup-carve/carve#1280).
            // That is the half `marker_line_was_the_whole_block` answers for a
            // list item one call site over; a definition body IS such a
            // container (markup-carve/carve#956) and the container KIND is not a
            // parameter of the rule (markup-carve/carve#920).
            if !folded_a_lazy_line
                && !definition_body_takes_the_fold(&so_far, lines.is_empty(), options)
            {
                break;
            }
            // BELOW THE BODY'S COLUMN THE BODY ENDS (markup-carve/carve#932).
            // `definition_indent` states the floor as column arithmetic; this is
            // the other side of it. A line indented 1 or 2 columns reaches
            // neither band above: it is not the body's own block content, and it
            // is not lazy text either, because `lazy_continuation_line` is
            // spelled as a FLUSH-LEFT line. So the body ends and the line is
            // classified in the surviving context, where PART 2's COLUMN-EXACT
            // DELIMITERS makes an indented block opener plain text.
            //
            // Without this, BELOW and PAST are one band: the fold never looked at
            // indentation (carve-rs#734 recorded exactly that when it labelled
            // the no-blank shape a control), so `:  body` / ` > q` and
            // `:  body` / `    > q` produced the same bytes and the floor of
            // three columns was unobservable on this side. The footnote body,
            // which the clause names as the precedent, already answers this way.
            if indent > 0 {
                break;
            }
            // Lazy continuation: a flush-left line with no blank before it that
            // does not start an interrupting block folds into the open
            // paragraph (the same rule list items and block quotes use, matching
            // djot). A block opener ends the definition.
            let owned = line.to_string();
            if !interrupts_paragraph(cur, &owned) {
                folded_a_lazy_line = true;
                lines.push(owned);
                line_map.push(cur.source_line(cur.pos));
                col_map.push(cur.source_col(cur.pos));
                cur.consume();
                continue;
            }
            break;
        }
        // Blank line: absorb it as a paragraph separator ONLY when a later line
        // still continues the definition (form A); otherwise leave it for the
        // entry separator / outer block stream.
        let mut look = 0usize;
        while matches!(cur.lines.get(cur.pos + look).copied(), Some(l) if is_blank_line(l)) {
            look += 1;
        }
        match cur.lines.get(cur.pos + look).copied() {
            Some(after) if !is_blank_line(after) && indent_columns(after) >= 3 => {
                folded_a_lazy_line = false;
                for _ in 0..look {
                    lines.push(String::new());
                    line_map.push(cur.source_line(cur.pos));
                    col_map.push(cur.source_col(cur.pos));
                    cur.consume();
                }
            }
            _ => break,
        }
    }
    debug_assert_eq!(col_map.len(), lines.len());
    MappedSource {
        col_map,
        source: lines.join("\n"),
        line_map,
    }
}

/// A bare image line is a block image (or figure) ONLY when it stands alone --
/// the next line is blank / EOF, a `^ ` caption, or a paragraph interrupter.
/// When the next line FOLDS (plain text, list marker, another bare image), the
/// image stays inline in a paragraph with that content, per grammar §1722 I3
/// ("an image is not a block of its own; it stays inline in the paragraph").
fn image_is_block(cur: &mut LineCursor) -> bool {
    let Some(next) = cur.lines.get(cur.pos + 1).copied() else {
        return true;
    };
    if is_blank_line(next) || caption_content(next).is_some() {
        return true;
    }
    // Peek-1 interruption: test the next line as if it were current, then rewind.
    let next_owned = next.to_string();
    let saved = cur.pos;
    cur.pos += 1;
    let interrupts = interrupts_paragraph(cur, &next_owned);
    cur.pos = saved;
    interrupts
}

fn consume_caption(cur: &mut LineCursor, options: &Options<'_>) -> Option<Vec<InlineNode>> {
    consume_caption_slot(cur, options, true)
}

fn consume_caption_slot(
    cur: &mut LineCursor,
    options: &Options<'_>,
    caption_context: bool,
) -> Option<Vec<InlineNode>> {
    let saved = cur.pos;
    // PART 9 §4 (NORMATIVE): `caption_slot = [blank_line], caption` carries at
    // most ONE optional blank line. A caption line adjacent to its host, or
    // separated from it by exactly one blank line, attaches; TWO blank lines
    // DETACH and leave the `^ ` line an ordinary paragraph. Scanning blank
    // lines in a loop attaches across any number of them (carve-rs#830), which
    // no corpus document could distinguish because every captioned document
    // had zero or one blank line.
    if matches!(cur.peek(), Some(line) if is_blank_line(line)) {
        cur.consume();
    }
    let Some(line) = cur.peek() else {
        cur.pos = saved;
        return None;
    };
    let Some(text) = caption_content(line) else {
        cur.pos = saved;
        return None;
    };
    let mut joined = text.to_string();
    // One anchor per folded line, the same shape a paragraph builds. The first
    // entry accounts for the `^ ` marker, which `inline_anchor_for_line`
    // derives by comparing the full line against the inline text.
    let mut anchors = options
        .positions
        .then(|| vec![inline_anchor_for_line(cur, cur.pos, text)]);
    cur.consume();
    // A caption is multi-line inline content, so it folds following lines like a
    // PARAGRAPH (§10), NOT like a heading: a list marker FOLDS in (djot -- a
    // list needs a blank line to interrupt), while a heading / blockquote /
    // table / fenced code / `:::` div / thematic break / `%%%` comment
    // interrupts and ends the caption. A blank line or a further `^ ` caption
    // line also ends it. Continuation lines join with `\n`.
    while let Some(next) = cur.peek() {
        if is_blank_line(next) || caption_content(next).is_some() {
            break;
        }
        let next_owned = next.to_string();
        if interrupts_paragraph(cur, &next_owned) {
            break;
        }
        joined.push('\n');
        joined.push_str(next);
        if let Some(anchors) = &mut anchors {
            anchors.push(inline_anchor_for_line(cur, cur.pos, next));
        }
        cur.consume();
    }
    // §756 (NORMATIVE): strip trailing whitespace - on EVERY line the caption
    // spans, not only its last (PART 2 NO TRAILING WHITESPACE, carve#926; a
    // table caption is one of the contexts that ruling names, and one the
    // executable spec itself was missing until it was measured). Stripping only
    // ever shortens a line's END, so it cannot shift any anchor.
    let joined = joined
        .split('\n')
        .map(trim_ascii_end)
        .collect::<Vec<_>>()
        .join("\n");
    let text = trim_ascii_end(&joined);
    Some(match anchors {
        Some(anchors) => parse_caption_inline_with_anchor(text, options, anchors, caption_context),
        None => parse_caption_inline_with_options(text, options, caption_context),
    })
}

fn is_table_start(line: &str) -> bool {
    // A standard table row opens AND closes with `|` (grammar standard_row; a
    // `|=` cell is a header cell). A stray leading `|` with no closing `|`
    // (`| a`) is ordinary paragraph text, not a table. (`+` multi-line-cell
    // continuations are consumed inside parse_table; a `+` line never starts a
    // table, #80.)
    //
    // A row may also carry a `{...}` attribute block glued to its closing pipe
    // (`| a |{.x}` -> <tr class="x">); split_row_attrs validates it, so a line
    // ending in a valid row-attribute block also opens a table.
    let trimmed = trim_ascii(line);
    if trimmed.len() < 2 || !trimmed.starts_with('|') {
        return false;
    }
    if trimmed == "||" {
        return false;
    }
    let (_, body) = split_row_attrs(trimmed);
    if !body.ends_with('|') {
        return false;
    }
    let interior = &body[1..body.len() - 1];
    if !interior.contains('|') && trim_ascii(interior).is_empty() {
        return false;
    }
    true
}

/// A `{...}` attribute block GLUED to the row's closing `|` sets the row's
/// `<tr>` attributes -- the row-level twin of a cell's opening-pipe block. The
/// whole payload must be a valid attribute block running to end of line;
/// otherwise the `{` is ordinary content. Returns the parsed attributes and the
/// line body up to and including the closing pipe (with the block removed).
fn split_row_attrs(content: &str) -> (Option<Attrs>, &str) {
    if let Some(idx) = content.rfind('|') {
        let bytes = content.as_bytes();
        if bytes.get(idx + 1) == Some(&b'{') {
            let last_close_brace = bytes.iter().rposition(|&b| b == b'}');
            if let Some((attrs, next)) = read_attrs_at(bytes, idx + 1, last_close_brace) {
                if next == content.len() {
                    return (Some(attrs), &content[..=idx]);
                }
            }
        }
    }
    (None, content)
}

/// A cell padding slot is a run of U+0020 (spec PART 7). A tab there is not
/// padding, so it stays as cell content.
fn trim_cell_padding(s: &str) -> &str {
    s.trim_matches(' ')
}

/// A cell padding slot is a run of U+0020 (spec PART 7). A tab there is not
/// padding, so it stays as cell content.
fn trim_cell_padding_start(s: &str) -> &str {
    s.trim_start_matches(' ')
}

fn parse_table(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let span_start = cur.pos;
    let mut rows: Vec<TableRow> = Vec::new();
    // GFM-style header separator: a delimiter row directly after the first row
    // turns that row into a header and sets per-column alignment. The colons land
    // on the HEADER cells only, matching what the native `|=<` markers produce.
    // The first row must not itself be a delimiter row.
    //
    // They used to be applied to every body row as well, so the same logical table
    // parsed to two different trees depending on which separator syntax the author
    // used, and the writer then serialized the propagated values as per-cell
    // markers nobody wrote (carve#352, corpus 09-tables-3). Nothing is lost: the
    // HTML renderer inherits column alignment for a body cell whose own align is
    // unset, which is how the native path has always rendered aligned body cells.
    // A genuine per-cell override sets the cell's own align and is untouched.
    let mut first_is_delim = false;
    let mut saw_separator = false;
    while let Some(line) = cur.peek() {
        // ONLY a `|` row opens an iteration. A `+` continuation is taken by the
        // row it continues, below, so reaching one here means the row above did
        // not want it - after a delimiter row, where a continuation has only a
        // HEADER to attach to and the table ends instead.
        if !is_table_start(line) {
            break;
        }
        let row_at = cur.pos;
        cur.consume();
        if rows.is_empty() {
            first_is_delim = is_delim_row(line);
        } else if rows.len() == 1 && !saw_separator && !first_is_delim && is_delim_row(line) {
            // The separator row: make the first row the header, drop the row.
            saw_separator = true;
            let column_aligns = parse_delim_aligns(line);
            for cell in &mut rows[0].cells {
                cell.header = true;
            }
            apply_column_aligns(&mut rows[0], &column_aligns);
            continue;
        }
        // THE CONTINUATIONS ARE TAKEN WITH THE ROW, not applied to a finished
        // one. A `+` row extends a CELL, and the cell is the block an unclosed
        // inline run reaches the end of (carve#1293), so the cell's content
        // must be assembled before it is parsed - which means the row parser
        // has to see every line the row runs to at once.
        let mut conts: Vec<(&str, Option<(usize, isize)>)> = Vec::new();
        let mut last_cont_at = None;
        while let Some(next) = cur.peek() {
            if !is_table_continuation(next) {
                break;
            }
            let cont_at = cur.pos;
            cur.consume();
            conts.push((
                next,
                cur.source_line(cont_at)
                    .zip(cur.source_col(cont_at))
                    .filter(|_| options.positions),
            ));
            last_cont_at = Some(cont_at);
        }
        let mut row = parse_table_row(
            line,
            &conts,
            options,
            options
                .positions
                .then(|| (cur.source_line(row_at), cur.source_col(row_at)))
                .and_then(|(l, c)| Some((l?, c?))),
        );
        // The cursor is the only place that knows where these lines sit, which
        // is why `parse_table_row` cannot place the row itself.
        row.pos = span_of(cur, row_at, row_at + 1, options);
        // The row RUNS to its last continuation. It stays one contiguous range
        // that no sibling row overlaps, so it keeps a position - unlike the
        // cell a continuation extends, whose content sits in two column ranges
        // with another column's content between them.
        if let (Some(pos), Some(end)) = (
            row.pos.as_mut(),
            last_cont_at.and_then(|at| span_of(cur, at, at + 1, options)),
        ) {
            pos.end_line = end.end_line;
            pos.end_column = end.end_column;
            pos.end_offset = end.end_offset;
        }
        rows.push(row);
    }
    // The caption FIRST, then the span: a caption is one of the table's
    // children and sits after the last row, so a span taken before it is
    // consumed stops short and leaves the caption's inlines outside their own
    // parent. Struct-field order made that the default, and nothing could see
    // it - a span is compared against source text for `text` nodes alone
    // (carve#565).
    let caption = consume_caption(cur, options);
    // The `if caption.is_some()` that stood here returned `BlockNode::Table`
    // and fell through to `BlockNode::Table`: two arms, one behavior, no way to
    // fail. Removed rather than corrected, since the two are now genuinely the
    // same.
    BlockNode::Table(Table {
        pos: span_of(cur, span_start, cur.pos, options),
        attrs: None,
        caption,
        short_caption: None,
        columns: Vec::new(),
        rows,
        row_groups: None,
    })
}

/// The table a line-based scan is currently inside, mirrored from the row loop
/// in [`parse_table`].
///
/// PART 5 T6 gives a continuation row `table_cell`s and joins them onto the row
/// above, so it is as much a part of the table as the row it appends to, and
/// PART 1 S4 asks what a container's last BLOCK is rather than how its last line
/// is written. A scan that reads one line at a time cannot answer that without
/// carrying the run: `+ b |` is a ROW under a table and ORDINARY PROSE under
/// anything else (markup-carve/carve#1345), and the rule has to give both
/// answers rather than one of them everywhere.
///
/// The row loop is mirrored rather than approximated because the two disagree
/// in a place that is easy to miss: a continuation directly after the GFM
/// DELIMITER row is not taken, since the separator `continue`s past the
/// continuation loop and the table ends there. Answering "row" for it is
/// markup-carve/carve#1354 in this engine's own code.
#[derive(Default)]
struct TableRun {
    /// Rows consumed so far, the delimiter row excluded exactly as the row loop
    /// excludes it. `None` when no table is open.
    rows: Option<usize>,
    first_is_delim: bool,
    saw_separator: bool,
    /// Whether the last line consumed was a row that goes on to take
    /// continuations. False right after the separator row.
    takes_continuations: bool,
    /// How many quote markers the run's own lines sit behind, inside the
    /// container being read. A run does not survive a change of depth.
    depth: usize,
}

impl TableRun {
    /// Feed the next line of the container's own content, already stripped of
    /// the container prefix. Answers whether the line is a CONTINUATION ROW of
    /// the table above it - the one shape whose meaning the line alone does not
    /// carry.
    ///
    /// Indented lines are not rows at all: `is_table_start` trims, so without
    /// the flush-left guard an indented `  |a|` would open a table where an
    /// indented `> q` or `# h` opens nothing (PART 9 section 24 C3). The same
    /// guard is on the continuation, so the two halves of one table cannot be
    /// read at two different columns.
    fn observe(&mut self, line: &str) -> bool {
        // A PIPE IS THE PRE-TEST, and it has to be one. Both `is_table_start`
        // and `is_table_continuation` require a closing `|`, so a line holding
        // no pipe at all is neither a row nor a continuation row and no run
        // survives it - which is answerable from the BYTES, before anything is
        // stripped.
        //
        // Without it the walk below ran on every quoted line at every depth,
        // which is one strip per marker per line: a depth ladder of ordinary
        // prose went from 16 strips per unit of work to 79.5, and
        // `a_depth_ladder_costs_strips_in_proportion_to_its_markers` caught it.
        // Same shape as §12's absorption pre-test one function over
        // (markup-carve/carve-rs#738), and conservative in the same direction:
        // it can only force a walk that was not needed, never skip one that was.
        //
        // The reset is unconditional here rather than depth-aware, which costs
        // nothing: with no run open the depth is re-read from the next line that
        // opens one.
        if !line.as_bytes().contains(&b'|') {
            *self = TableRun::default();
            return false;
        }
        // A QUOTE INSIDE A QUOTE IS ASKED WHAT IT ENDS ON (markup-carve/carve#1355,
        // corpus 356). The line is read at the depth it sits at: `> > | a |` in
        // an outer quote arrives here as `> | a |`, which is not a row at this
        // level and is a row one level in. Stripping to the innermost content is
        // the same walk `ParaOpen::resolve` makes for the same reason - a lazy
        // line continues the innermost open paragraph however many containers it
        // failed to match (markup-carve/carve#506).
        //
        // The DEPTH is carried with it, because a run lives inside ONE container.
        // Without it `> > | a |` over `> + b |` would read as two lines of one
        // table, where the second is content of the OUTER quote written after the
        // inner one ended - and a `+ ...|` with no table above it AT ITS OWN
        // LEVEL is prose (markup-carve/carve#1345).
        let mut innermost = line;
        let mut depth = 0usize;
        while let Some(rest) = strip_blockquote_prefix(innermost) {
            depth += 1;
            innermost = rest;
        }
        if self.rows.is_some() && depth != self.depth {
            *self = TableRun::default();
        }
        self.depth = depth;
        let line = innermost;
        let flush = !line.starts_with([' ', '\t']);
        if flush && is_table_start(line) {
            let rows = self.rows.unwrap_or(0);
            if rows == 0 {
                self.first_is_delim = is_delim_row(line);
                self.rows = Some(1);
                self.takes_continuations = true;
            } else if rows == 1 && !self.saw_separator && !self.first_is_delim && is_delim_row(line)
            {
                self.saw_separator = true;
                self.takes_continuations = false;
            } else {
                self.rows = Some(rows + 1);
                self.takes_continuations = true;
            }
            return false;
        }
        if self.takes_continuations && flush && is_table_continuation(line) {
            return true;
        }
        let depth = self.depth;
        *self = TableRun::default();
        self.depth = depth;
        false
    }
}

fn is_table_continuation(line: &str) -> bool {
    // `continuation_row` ends in `'|'` just like `standard_row`, so the closing
    // pipe is required here too: `+ c | d` is prose and ends the table. Unlike a
    // standard row it has no `row_attributes` slot, so a trailing `|{.x}` does
    // NOT stand in for the closing pipe.
    let trimmed = trim_ascii(line);
    if trimmed.len() < 2 || !trimmed.starts_with('+') || !trimmed.ends_with('|') {
        return false;
    }
    trimmed != "+|"
}

/// A GFM delimiter cell: an optional leading colon, one or more dashes, an
/// optional trailing colon, and nothing else.
fn is_delim_cell(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && b[i] == b':' {
        i += 1;
    }
    let dash_start = i;
    while i < b.len() && b[i] == b'-' {
        i += 1;
    }
    if i == dash_start {
        return false; // need at least one dash
    }
    if i < b.len() && b[i] == b':' {
        i += 1;
    }
    i == b.len()
}

/// Strip a row's CLOSING pipe, unless that pipe is escaped.
///
/// An escaped closing pipe is still an escape (markup-carve/carve#1293). The
/// row closes there either way, because the line ends in a pipe; what the
/// escape decides is what the CELL holds, which is a literal pipe and not an
/// orphaned backslash. Leaving the `\|` pair in the content is what says that:
/// the splitter already reads the escape at every other position and keeps the
/// pair, so the cell ends up holding one pipe and the row is not split there.
///
/// Cutting the pipe off blindly left a trailing `\` behind, which the inline
/// parser then read as a hard break - `| a b \|` published a `<br>` where the
/// author wrote a pipe.
///
/// A pipe preceded by an EVEN number of backslashes is not escaped: the
/// backslashes escape each other, so the pipe is the plain closer.
fn strip_row_closing_pipe(content: &str) -> &str {
    let Some(stripped) = content.strip_suffix('|') else {
        return content;
    };
    let backslashes = stripped.len() - stripped.trim_end_matches('\\').len();
    if backslashes % 2 == 1 {
        return content;
    }
    stripped
}

/// A delimiter row: every cell is a delimiter cell (and there is at least one).
fn is_delim_row(line: &str) -> bool {
    let mut content = trim_ascii(line);
    content = content.strip_prefix('|').unwrap_or(content);
    content = content.strip_suffix('|').unwrap_or(content);
    let cells = split_table_cells(content);
    // PART 7: cell padding is U+0020 only, so a tab makes the cell content and
    // the cell stops being a delimiter cell -- which unmakes the whole row.
    !cells.is_empty() && cells.iter().all(|c| is_delim_cell(trim_cell_padding(c)))
}

/// Per-column alignment from a delimiter row's colons.
fn parse_delim_aligns(line: &str) -> Vec<Option<TableAlign>> {
    let mut content = trim_ascii(line);
    content = content.strip_prefix('|').unwrap_or(content);
    content = content.strip_suffix('|').unwrap_or(content);
    split_table_cells(content)
        .iter()
        .map(|c| {
            // PART 7: cell padding is U+0020 only. UNREACHABLE-BY-CONSTRUCTION
            // today -- this runs only behind `is_delim_row`, whose own
            // space-only trim already rejected any row with a tab in a cell's
            // padding, so no tab can arrive here. It is spelled the same way
            // anyway rather than left as `.trim()`: one rule spelled two ways in
            // two functions is how this family drifts (markup-carve/carve#755),
            // and if the gate above is ever restructured this site is already
            // right. Reverting it alone is a GREEN mutation, recorded as such
            // rather than pinned by a test that could not fail.
            let t = trim_cell_padding(c);
            match (t.starts_with(':'), t.ends_with(':')) {
                (true, true) => Some(TableAlign::Center),
                (false, true) => Some(TableAlign::Right),
                (true, false) => Some(TableAlign::Left),
                (false, false) => None,
            }
        })
        .collect()
}

/// Apply a column default alignment to each cell that has no alignment of its
/// own (a native `|<` marker wins over the column default).
fn apply_column_aligns(row: &mut TableRow, aligns: &[Option<TableAlign>]) {
    for (i, cell) in row.cells.iter_mut().enumerate() {
        if cell.align.is_none() {
            if let Some(a) = aligns.get(i).copied().flatten() {
                cell.align = Some(a);
            }
        }
    }
}

/// A cell's content contributed by ONE `+` continuation row, with the anchor
/// its own source line gives it.
struct CellFragment {
    text: String,
    anchor: Option<(usize, isize)>,
}

/// `base` is the row line's (source line, columns already stripped by an
/// enclosing container). Without it a cell cannot be placed: this function is
/// handed an already-split line and cannot know where it sits.
///
/// `conts` are the `+` CONTINUATION lines that belong to this row, each with
/// the same pair for its own line. They are taken here rather than applied
/// afterwards because a continuation extends a CELL, and the cell is the block
/// an unclosed inline run reaches the end of (carve#1293) - so the cell's
/// content has to be assembled BEFORE it is parsed, not parsed twice and
/// concatenated. Parsing each fragment on its own closed the run at the row's
/// pipe and opened a fresh one for the continuation, which published an empty
/// `<code></code>` that no clause in the language produces.
fn parse_table_row(
    line: &str,
    conts: &[(&str, Option<(usize, isize)>)],
    options: &Options<'_>,
    base: Option<(usize, isize)>,
) -> TableRow {
    let mut content = trim_ascii(line);
    let (attrs, body) = split_row_attrs(content);
    content = body;
    if let Some(stripped) = content.strip_prefix('|') {
        content = stripped;
    }
    content = strip_row_closing_pipe(content);
    // Where `content` starts inside `line`, in CHARS: it is a slice of `line`
    // after trimming, the row-attribute split and the outer pipes, so the byte
    // distance between them is exact.
    let content_off = base.map(|_| char_offset_of(line, content));
    let split = split_table_cells_ranged(content);
    let mut fragments: Vec<Vec<CellFragment>> =
        (0..split.cells.len()).map(|_| Vec::new()).collect();
    collect_continuation_fragments(conts, &split, &mut fragments);
    let cells = split
        .cells
        .into_iter()
        .enumerate()
        .map(|(idx, slice)| {
            // The cell's own start column, which is also the anchor its inline
            // content is parsed against: the cell text is a verbatim slice of
            // the row now that the escaped pipe is preserved, so an offset
            // inside it maps straight back to the document.
            let cell_anchor = match (base, content_off) {
                (Some((line_no, stripped)), Some(off)) => {
                    Some((line_no, stripped + (off + slice.start) as isize))
                }
                _ => None,
            };
            let extra = &fragments[idx];
            let mut cell = parse_table_cell(&slice.text, options, cell_anchor, extra);
            if !extra.is_empty() {
                // Its value is now reassembled from discontiguous fragments; no
                // single exact source extent exists for the cell.
                cell.pos = None;
            } else if let (Some((line_no, stripped)), Some(off)) = (base, content_off) {
                cell.pos = Some(Pos {
                    start_line: line_no,
                    end_line: line_no,
                    start_column: document_column(stripped, off + slice.start),
                    end_column: document_column(stripped, off + slice.end),
                    // Filled from the line table once the document is parsed.
                    start_offset: 0,
                    end_offset: 0,
                });
            }
            cell
        })
        .collect();
    TableRow {
        cells,
        attrs,
        pos: None,
    }
}

/// Where `part` starts inside `whole`, in CHARS.
///
/// `part` must be a SUB-SLICE of `whole` - it always is at the call sites, which
/// only ever narrow - so the byte distance between the two pointers is exact and
/// converts without re-scanning.
fn char_offset_of(whole: &str, part: &str) -> usize {
    let bytes = (part.as_ptr() as usize).saturating_sub(whole.as_ptr() as usize);
    whole[..bytes].chars().count()
}

/// Cut each continuation line into cells and file every non-empty one under the
/// column it extends.
///
/// The verbatim run a line leaves OPEN is carried into the next one: a `+` row
/// continues the cell, so a run the row's closing pipe did not close is still
/// open where the continuation picks that cell up (carve#1293). An EMPTY cell
/// contributes nothing - not even the joining space - which is what lets a
/// continuation address one column of a wide row.
fn collect_continuation_fragments(
    conts: &[(&str, Option<(usize, isize)>)],
    row: &RowSplit,
    fragments: &mut [Vec<CellFragment>],
) {
    // The run's WIDTH travels with it, not just the fact that one is open: a
    // run closes on a run of exactly its own length, on the continuation row as
    // much as on the row that opened it (carve-rs#1051).
    let mut open_len = row.open_len;
    let mut open_run_at = row.open_run_at;
    for (line, base) in conts {
        let mut content = trim_ascii(line);
        if let Some(stripped) = content.strip_prefix('+') {
            content = stripped;
        }
        // The same rule as the row's own closer above: the escape is honored
        // wherever it appears, and a continuation row's last pipe is not an
        // exception (markup-carve/carve#1293).
        content = strip_row_closing_pipe(content);
        let content_off = base.map(|_| char_offset_of(line, content));
        let split = split_table_cells_seeded(content, open_len, open_run_at);
        open_len = split.open_len;
        open_run_at = split.open_run_at;
        for (idx, cell) in split.cells.iter().enumerate() {
            let text = trim_cell_padding(&cell.text); // PART 7: cell padding is U+0020 only.
            if text.is_empty() {
                continue;
            }
            // Trimming moved the start; count what it took so the anchor lands
            // on the first character of the text and not on the padding before
            // it.
            let lead =
                cell.text.chars().count() - trim_cell_padding_start(&cell.text).chars().count(); // PART 7: cell padding is U+0020 only.
            let anchor = match (base, content_off) {
                (Some((line_no, stripped)), Some(off)) => {
                    Some((*line_no, stripped + (off + cell.start + lead) as isize))
                }
                _ => None,
            };
            if let Some(slot) = fragments.get_mut(idx) {
                slot.push(CellFragment {
                    text: text.to_string(),
                    anchor,
                });
            }
        }
    }
}

/// A cell as it sits in the row: its resolved text, and the CHAR range of the
/// source it came from.
///
/// The range is not derivable from the text: `\|` resolves to one character, so
/// a cell holding an escaped pipe is shorter than the source it spans.
struct CellSlice {
    text: String,
    start: usize,
    end: usize,
}

fn split_table_cells(content: &str) -> Vec<String> {
    split_table_cells_ranged(content)
        .cells
        .into_iter()
        .map(|c| c.text)
        .collect()
}

/// A row line cut into cells, together with the verbatim run its closing pipe
/// did NOT close.
///
/// The leftover run is what a `+` continuation row is scanned with. A row's
/// closing pipe closes the row even with a run still open (carve#1284), so the
/// run survives the row boundary - and by construction it sits in the LAST
/// cell, since once open it swallows every `|` but the closer.
struct RowSplit {
    cells: Vec<CellSlice>,
    /// The WIDTH of the verbatim run still open at end of line, or `None` when
    /// none is.
    ///
    /// THE WIDTH, not a flag. A run closes on a run of EXACTLY its own length
    /// (PART 9 §22), which is the rule this scanner already applies WITHIN a
    /// line - and it used to carry only "a run is open" across the row
    /// boundary, re-seeding the continuation at one backtick. So a run opened
    /// with two was closed by a single one on the continuation row, the pipe
    /// behind it split again, and the segment after it had no column to join:
    /// content loss, by a different route than the fresh scanner the per-column
    /// carry replaced (carve-rs#1051).
    open_len: Option<usize>,
    /// The cell index that run is open in: the last one.
    open_run_at: usize,
}

fn split_table_cells_ranged(content: &str) -> RowSplit {
    split_table_cells_seeded(content, None, 0)
}

/// `seed_len` / `open_run_at` seed the scanner with a verbatim run left OPEN by
/// the row this line CONTINUES (carve#1293), and with the WIDTH that run was
/// opened at.
///
/// A `+` continuation extends the cell, so the block an unclosed run reaches
/// the end of is that whole cell, continuation included: the pipes it spans on
/// the continuation row are its content, exactly as they are on the row that
/// opened it. Scanning a continuation with a fresh scanner cut the line at a
/// pipe INSIDE the run, and every segment past the first was then dropped for
/// want of a column to join - content loss rather than a different answer.
///
/// THE SEED BELONGS TO ONE COLUMN. The run was open in the row above's LAST
/// cell, and a continuation joins PER COLUMN, so the columns before it are
/// scanned normally and the pipe that ends them still separates. Seeding the
/// whole line instead swallows those separators and pushes the continuation
/// into the wrong cell.
///
/// AND THE SEED CARRIES ITS WIDTH. A run closes on a run of EXACTLY its own
/// length, which is the rule the scanner below already applies within a line;
/// re-seeding the continuation at one backtick made the boundary the one place
/// where the same scanner answered the width question a second way, and a run
/// of two was then closed by a single one on the continuation row
/// (carve-rs#1051).
fn split_table_cells_seeded(
    content: &str,
    seed_len: Option<usize>,
    open_run_at: usize,
) -> RowSplit {
    let mut cells = Vec::new();
    let mut buf = String::new();
    // The WIDTH of the verbatim run this scanner is inside, or `None` when it
    // is outside one. A parity toggle stood here and counted single backticks,
    // so a run of two closed itself the moment it opened and the pipe after it
    // split the row (markup-carve/carve#1284, corpus `328-...-4`). The tell was
    // that one backtick and three worked while two did not - the signature of
    // parity rather than of a delimiter.
    //
    // A verbatim run is opened by a RUN of N backticks and closed by a run of
    // EXACTLY N; a run of any other width inside it is content. That is the
    // same rule the inline parser already applies, and this scanner only needs
    // it to know which pipes are separators.
    //
    // ACROSS THE ROW BOUNDARY TOO, which is why the seed is a width and not a
    // flag: the run the continuation picks up was opened at some length on the
    // row above, and only a run of that same length closes it here.
    let mut open_len: Option<usize> = if open_run_at == 0 { seed_len } else { None };
    let mut index = 0usize;
    let mut cell_start = 0usize;
    let mut chars = content.chars().peekable();
    while let Some(ch) = chars.next() {
        index += 1;
        if ch == '`' {
            let mut run = 1usize;
            buf.push(ch);
            while chars.peek() == Some(&'`') {
                chars.next();
                index += 1;
                run += 1;
                buf.push('`');
            }
            match open_len {
                None => open_len = Some(run),
                Some(width) if width == run => open_len = None,
                Some(_) => {}
            }
            continue;
        }
        if ch == '\\' {
            // An escaped PIPE does not split the row, but the escape is KEPT
            // rather than resolved here: the inline parser turns it into an
            // `escaped_text` node, which is what carve-js publishes and what
            // the vocabulary defines. Resolving it here produced a single
            // `text` node holding a bare `|`, losing both the node and the
            // author's intent - and, because the cell text was then no longer
            // a verbatim slice of the row, nothing inside the cell could carry
            // a position either (carve-rs#333).
            //
            // Every other backslash escape was already preserved for the same
            // reason; this only makes the pipe consistent with them.
            if chars.peek() == Some(&'|') {
                buf.push('\\');
                buf.push('|');
                chars.next();
                index += 1;
            } else {
                buf.push('\\');
            }
            continue;
        }
        if ch == '|' && open_len.is_none() {
            cells.push(CellSlice {
                text: std::mem::take(&mut buf),
                start: cell_start,
                // The separator is not part of the cell.
                end: index - 1,
            });
            cell_start = index;
            if cells.len() == open_run_at {
                // Re-seed at the column the row above left open, AT THE WIDTH
                // that row opened it. `None` here is not a seed and leaves the
                // scanner outside a run, which is the same thing it was doing
                // for every other column.
                open_len = seed_len;
            }
            continue;
        }
        buf.push(ch);
    }
    cells.push(CellSlice {
        text: buf,
        start: cell_start,
        end: index,
    });
    let open_run_at = cells.len() - 1;
    RowSplit {
        cells,
        open_len,
        open_run_at,
    }
}

/// Parse a cell's inline content, anchored when the caller knows where the
/// cell sits in the document.
///
/// `slice` must be a SUB-SLICE of `cell` (it always is here: trimming and
/// marker removal only ever narrow it), so the byte distance between the two
/// pointers is exact and converts to a char offset without re-scanning.
fn parse_cell_inlines(
    cell: &str,
    slice: &str,
    options: &Options<'_>,
    anchor: Option<(usize, isize)>,
    extra: &[CellFragment],
) -> Vec<InlineNode> {
    let base = anchor
        .map(|(line_no, base_col)| (line_no, base_col + char_offset_of(cell, slice) as isize));
    parse_cell_content(slice, options, base, extra)
}

/// The cell's content as ONE inline block, its `+` continuation fragments
/// included.
///
/// THE CELL IS THE BLOCK (carve#1293). An unclosed inline verbatim run runs to
/// the end of the block it is in, and a `+` continuation extends the cell, so
/// the run has to be able to close on the continuation - which it cannot do if
/// each fragment is parsed by itself and the results concatenated. That is what
/// published `a <code>b</code> c<code></code>`: the run closed at the row's
/// pipe and a fresh one opened for the continuation, leaving an empty span no
/// clause in this language produces.
///
/// The JOINER is a manufactured space, because the source has a line break here
/// and the cell does not. It belongs to no source line, so it gets an anchor
/// segment of its own carrying NO anchor: a node whose span crosses it can then
/// place neither end and carries no position at all, which is the rule already
/// in force for a run joined across a gap (PART 12 section 4, absent beats
/// wrong). A node that lies wholly inside one fragment keeps its own position.
fn parse_cell_content(
    slice: &str,
    options: &Options<'_>,
    base: Option<(usize, isize)>,
    extra: &[CellFragment],
) -> Vec<InlineNode> {
    if extra.is_empty() {
        let Some(anchor) = base else {
            return parse_inline_with_options(slice, options);
        };
        return parse_inline_lines_with_anchor(slice, options, vec![Some(anchor)]);
    }
    let mut text = String::from(slice);
    let mut lines = vec![base];
    let mut breaks = Vec::new();
    for fragment in extra {
        if !text.is_empty() {
            // The joiner is the LAST character of the segment before it, which
            // is where a newline sits in an ordinary multi-line text. Opening
            // the next segment on it instead puts the exclusive end of a span
            // that stops just short of it into the following segment, and every
            // node at the end of a fragment loses its position.
            text.push(' ');
        }
        breaks.push(text.len());
        text.push_str(&fragment.text);
        lines.push(fragment.anchor);
    }
    parse_inline_segments_with_anchor(&text, options, lines, breaks)
}

fn parse_table_cell(
    cell: &str,
    options: &Options<'_>,
    anchor: Option<(usize, isize)>,
    extra: &[CellFragment],
) -> TableCell {
    // A leading `=` marks a HEADER cell, but only when GLUED to the `|` (no
    // leading whitespace), per grammar §20. `| =x |` (space before `=`) is a
    // literal `<td>`, matching carve-js / carve-php; check the RAW cell, not
    // the trimmed one.
    let header = cell.starts_with('=');
    // NOT trimmed: the alignment marker is GLUED to the opening `|` (grammar
    // §20, `data_cell` / `header_cell`), so whether whitespace precedes it is
    // the whole distinction. Reading the marker off the TRIMMED cell threw that
    // away and made `| < x |` left-aligned where carve-js and carve-php both
    // render a literal `<`, and made `| << |` a colspan by stripping the first
    // `<` as alignment and matching the second as a lone span marker
    // (carve-rs#459).
    let body = if header { &cell[1..] } else { cell };
    let trimmed = trim_cell_padding(body); // PART 7: cell padding is U+0020 only.
                                           // A whitespace-delimited lone marker is a span cell rather than alignment,
                                           // which is the one case a glued marker does not win: `|<|` is a colspan in
                                           // every engine. A header cell is already marked by its `=`, so a marker
                                           // glued after it is alignment even when it is the whole content (`|=<|`).
    let lone_span = !header && (trimmed == "<" || trimmed == "^");
    let mut after_markers = body;
    let (align, valign) = if lone_span {
        (None, None)
    } else {
        let run = body
            .bytes()
            .take_while(|marker| matches!(marker, b'>' | b'<' | b'~' | b'^' | b'v' | b'?'))
            .count();
        let inherited_horizontal = run == 2
            && body.as_bytes().first() == Some(&b'?')
            && body
                .as_bytes()
                .get(1)
                .is_some_and(|marker| matches!(marker, b'^' | b'~' | b'v'));
        let mut saw_horizontal = false;
        let mut saw_vertical = false;
        let mut axes_valid = !matches!(body.as_bytes().first(), Some(b'^' | b'v'))
            && !(body.as_bytes().first() == Some(&b'~')
                && body
                    .as_bytes()
                    .get(1)
                    .is_some_and(|marker| matches!(marker, b'>' | b'<')));
        for (index, marker) in body.bytes().take(run).enumerate() {
            if marker == b'?' {
                if inherited_horizontal && index == 0 {
                    continue;
                }
                axes_valid = false;
                break;
            } else if marker == b'~'
                && !saw_horizontal
                && !saw_vertical
                && body
                    .as_bytes()
                    .get(index + 1)
                    .is_some_and(|next| matches!(next, b'>' | b'<'))
            {
                saw_vertical = true;
            } else if matches!(marker, b'>' | b'<' | b'~') {
                if !saw_horizontal {
                    saw_horizontal = true;
                } else if marker == b'~' && !saw_vertical {
                    saw_vertical = true;
                } else {
                    axes_valid = false;
                    break;
                }
            } else if !saw_vertical {
                saw_vertical = true;
            } else {
                axes_valid = false;
                break;
            }
        }
        let markers = &body.as_bytes()[..run];
        let terminated = body
            .as_bytes()
            .get(run)
            .is_some_and(|b| *b == b' ' || *b == b'{');
        let valid = run > 0 && axes_valid && (saw_horizontal || inherited_horizontal) && terminated;
        if valid {
            after_markers = &body[run..];
        }
        let horizontal_marker = (valid && !inherited_horizontal)
            .then(|| {
                markers.iter().enumerate().find_map(|(index, marker)| {
                    let vertical_first_middle = *marker == b'~'
                        && index == 0
                        && markers
                            .get(1)
                            .is_some_and(|next| matches!(next, b'>' | b'<'));
                    (!vertical_first_middle && matches!(marker, b'>' | b'<' | b'~'))
                        .then_some(*marker)
                })
            })
            .flatten();
        let align = horizontal_marker.map(|marker| match marker {
            b'>' => TableAlign::Right,
            b'<' => TableAlign::Left,
            _ => TableAlign::Center,
        });
        let valign = if !valid {
            None
        } else if markers.contains(&b'^') {
            Some(TableVerticalAlign::Top)
        } else if markers.contains(&b'v') {
            Some(TableVerticalAlign::Bottom)
        } else if markers.len() == 2 {
            Some(TableVerticalAlign::Middle)
        } else {
            None
        };
        (align, valign)
    };
    // CELL ATTRIBUTES BIND LAST (grammar §20 T10, corpus
    // 319-cell-attributes-bind-after-the-kind-and-alignment-markers). A `{...}`
    // block sets the cell's attributes and is GLUED to whatever precedes it: to
    // the marker run where the cell has one (`|=<{.x} ...`), to the opening `|`
    // where it has none (`|{.x} ...`). One order, both productions.
    //
    // Reading it AHEAD of the markers instead left an attributed HEADER cell
    // unspellable: the only shape available, `|{#x}=R|`, is ambiguous by
    // construction and this grammar reads it as a data cell whose content
    // starts with `=`, so the canonical writer's spelling for `<th id="x">R</th>`
    // came back as `<td id="x">=R</td>` and the PART 11 round-trip invariant
    // failed on it. Once `=` has committed the cell to header, everything after
    // it is unambiguous.
    //
    // `read_attrs_at` is quote-aware and validates the whole payload, so a
    // partially-invalid or empty block reads as None and the `{` stays content.
    // A space before the brace (`| {.x}`, `|= {.x}`) is also ordinary content.
    let mut attrs = None;
    let mut rest = after_markers;
    if after_markers.as_bytes().first() == Some(&b'{') {
        let after_bytes = after_markers.as_bytes();
        let last_close_brace = after_bytes.iter().rposition(|&b| b == b'}');
        if let Some((read, next)) = read_attrs_at(after_bytes, 0, last_close_brace) {
            attrs = Some(read);
            rest = &after_markers[next..];
        }
    }
    let text = trim_cell_padding(rest); // PART 7: cell padding is U+0020 only.
                                        // `span_cell` is an ALTERNATIVE to `data_cell` in the grammar, not a suffix
                                        // of one, so a cell that already carried an alignment marker cannot also be
                                        // a span: whatever follows the marker is content. Without this, `|<<|` read
                                        // its first `<` as alignment and its second as a lone colspan marker and
                                        // emitted an empty cell (carve-rs#459). A cell carrying attributes is never
                                        // a bare span marker either -- its content is literal even if it is just
                                        // `^` or `<`.
    let span = match text {
        _ if align.is_some() || attrs.is_some() => None,
        "^" => Some(TableCellSpan::Rowspan),
        "<" => Some(TableCellSpan::Colspan),
        _ => None,
    };
    TableCell {
        header,
        span,
        align,
        valign,
        attrs,
        children: if span.is_some() {
            // A span cell renders nothing of its own - it widens a neighbour -
            // so its own content is empty. A continuation still files its
            // fragments under this column, and they stay in the tree where they
            // always were.
            parse_cell_content("", options, None, extra)
        } else {
            parse_cell_inlines(cell, text, options, anchor, extra)
        },
        // The caller places the cell: it knows where the row line sits.
        pos: None,
    }
}

struct ContainerOpen {
    fence_len: usize,
    kind: Option<String>,
    title: Option<String>,
    /// Codepoint offset of the title's first character within the opener line,
    /// when the title is a VERBATIM slice of it. A quoted title carrying an
    /// escape is rebuilt rather than sliced, so no column in it maps back and
    /// this stays `None` (PART 12 section 4).
    title_col: Option<usize>,
    label: Option<String>,
    attrs: Option<Attrs>,
}

fn detect_container_open(line: &str) -> Option<ContainerOpen> {
    let trimmed = trim_ascii(line);
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return None;
    }
    let after_fence = &trimmed[fence_len..];
    // THE SEPARATOR IS A RUN OF U+0020, and the token starts where that run
    // ends. #720 tested only the FIRST character (`after_fence.starts_with(' ')`)
    // and then read the token out of `after_fence.trim()`, whose Unicode trim
    // swallowed whatever came after that space. So a lone leading tab was
    // rejected while `::: <TAB>note` still opened an admonition, and
    // `::: <NBSP>note` did too - a first-character test wearing a run test's
    // clothes, the same shape found in carve-php's copy of this rule (#722,
    // corpus 254-2 and 254-5). Splitting the run here is what makes the check
    // about the rule: `rest` now begins at the token, so every test below sees
    // the character the grammar actually puts there.
    //
    // A RUN, not one space: `:::  note` is a two-space opener and a valid
    // admonition (corpus 254-10), so narrowing this to a single U+0020 would
    // break it alone.
    // `trimmed` is `line.trim()`, so there is no trailing whitespace left to
    // strip here and `rest` is exactly the token and what follows it.
    let sep_len = after_fence.bytes().take_while(|b| *b == b' ').count();
    let rest = &after_fence[sep_len..];
    // STRICT (djot): the opener is the colon fence, an optional type word,
    // and an optional quoted title -- and NOTHING else. A trailing `{...}`
    // (or any other non-title text) makes the line an ordinary paragraph,
    // not a fence; attributes attach via a preceding block-attribute line.
    if rest.is_empty() {
        return Some(ContainerOpen {
            fence_len,
            kind: None,
            title: None,
            title_col: None,
            label: None,
            attrs: None,
        });
    }
    // `div_open = colon_fence, [[space], label]` - the label is either GLUED to
    // the fence or separated from it by spaces, and nothing else. This branch
    // runs before the separator check below, so it used to match against a
    // Unicode-trimmed copy and open a labelled div for a tabbed opener; #720
    // guarded it with its own copy of the separator test instead. With `rest`
    // now starting where the space run ends, `parse_bare_label`'s own
    // `starts_with('[')` decides it: a tab or any other character between the
    // fence and the bracket is simply still there, so the label does not parse.
    // The extra guard would be a check that can no longer fail (#722).
    if let Some(label) = parse_bare_label(rest) {
        return Some(ContainerOpen {
            fence_len,
            kind: None,
            title: None,
            title_col: None,
            label: Some(label),
            attrs: None,
        });
    }
    // THE SEPARATOR IS A SPACE, U+0020 and nothing else. PART 7's MARKER
    // SEPARATORS AND PADDING SLOTS is normative: this slot stands between the
    // fence run and the token that SELECTS which of the four blocks the line
    // opens, which makes it a marker separator rather than padding. A tab was
    // accepted here, so a tabbed opener opened an admonition, a div, a line
    // block or a local hard-break block where the grammar makes the line a
    // paragraph (#712, spec carve#886).
    //
    // A typeless label may still be glued to the fence (`:::[x]`), and
    // `:::note` is ordinary paragraph text. The opener's METADATA slots are
    // the other role, and PART 7 on carve main spells those `space` too - see
    // `after_kind` below.
    if sep_len == 0 {
        return None;
    }
    // A type word is a grammar identifier: `(letter | '_'), {letter | digit
    // | '_' | '-'}`. It must START with a letter or underscore, so a
    // digit-first token (`123`) or a non-identifier opener (`::: {.x}`,
    // `:::{k=v}`) is not a fence -- the line is an ordinary paragraph.
    if !rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    let id_end = rest
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_'))
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    let kind = rest[..id_end].to_string();
    let after_kind = &rest[id_end..];
    // THE TITLE SLOT IS SPACES. PART 7 decides this terminal by POSITION, not
    // by the slot's role: a tab is syntax only inside a line's leading
    // indentation run, and from the first non-whitespace character onward it is
    // not syntax at all. Every metadata slot on this line sits after the fence,
    // so `admonition_open = colon_fence, space, admonition_type,
    // [space+, quoted_title], [space+, label]` spells all of them `space`.
    //
    // #720 read carve#886, which said a padding slot took `whitespace`, and
    // narrowed this from `char::is_whitespace` to `[' ', '\t']`. carve#901
    // landed as carve#905 and reverted that clause; the Unicode half of the
    // narrowing was right and is kept by going to `' '`, which admits neither
    // a tab nor U+00A0 nor a form feed (#722, corpus 255 and 255-2).
    if !after_kind.is_empty() && !after_kind.starts_with(' ') {
        return None;
    }
    let mut after = after_kind.trim_start_matches(' ');
    let mut title_col = None;
    let title = if after.starts_with('"') {
        // Where the text after the quote sits in the ORIGINAL line. These are
        // all subslices of `line`, so the byte offset is a pointer difference;
        // columns are codepoints (PART 12 section 4).
        let quote_at = (after.as_ptr() as usize) - (line.as_ptr() as usize);
        let text_at = quote_at + 1;
        let (title, remainder) = parse_quoted_metadata(after)?;
        // Only when the title is the source verbatim. An escaped quote makes
        // `parse_quoted_metadata` build a new string, and then no column in it
        // maps back.
        if line[text_at..].starts_with(&title) {
            title_col = Some(line[..text_at].chars().count());
        }
        // THE LABEL SLOT IS SPACES, for the same reason as the title slot
        // above. This one was missed entirely by #720, which narrowed only the
        // slot before the title: `trim_start` is `char::is_whitespace`, so this
        // admitted a tab AND every Unicode space. `::: note "T"` followed by
        // U+00A0 or U+2003 and a `[label]` opened an admonition here (#722,
        // corpus 255-3 and 255-4).
        after = remainder.trim_start_matches(' ');
        Some(title)
    } else {
        None
    };
    let label = if after.starts_with('[') {
        let close = after.find(']')?;
        let label = after[1..close].to_string();
        if !trim_ascii(&after[close + 1..]).is_empty() {
            return None;
        }
        Some(label)
    } else {
        if !after.is_empty() {
            return None;
        }
        None
    };
    Some(ContainerOpen {
        fence_len,
        kind: Some(kind),
        title,
        title_col,
        label,
        attrs: None,
    })
}

fn parse_bare_label(s: &str) -> Option<String> {
    let close = s.find(']')?;
    if !s.starts_with('[') || !s[close + 1..].trim().is_empty() {
        return None;
    }
    Some(s[1..close].to_string())
}

fn parse_quoted_metadata(s: &str) -> Option<(String, &str)> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Some((unescape_quoted_header(&s[1..i]), &s[i + 1..]));
        }
        i += 1;
    }
    None
}

fn parse_container(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let open = detect_container_open(cur.peek().unwrap()).unwrap();
    let span_start = cur.pos;
    cur.consume();
    // PART 9 §4c: a BARE `::: figure` opener opens a COMPOSITE FIGURE, unless
    // it sits inside an open group's body (groups do not nest - the inner one
    // stays the generic container the Admonition arm below builds). The body
    // and closer follow the unchanged container rules; what §4c adds is the
    // caption slot hanging on the CLOSING fence, this kind only.
    if is_bare_figure_open(&open) && !IN_FIGURE_GROUP.with(Cell::get) {
        let (children, closed) = {
            let _guard = FigureGroupGuard::enter();
            let (inner, closed) = collect_colon_container_body(cur, open.fence_len);
            (parse_capped_colon_body(inner, options), closed)
        };
        // The slot hangs on the closer. A group left open at end of input
        // closed there without one, so there is no line for a caption to
        // attach to (§4c).
        let caption = if closed {
            consume_caption(cur, options)
        } else {
            None
        };
        // Through the caption the cursor just consumed, like a figure's span.
        let pos = span_of(cur, span_start, cur.pos, options);
        return BlockNode::FigureGroup(FigureGroup {
            attrs: None,
            children,
            caption,
            pos,
        });
    }
    let (inner, _closed) = collect_colon_container_body(cur, open.fence_len);
    let children = parse_capped_colon_body(inner, options);
    // The span covers the opening fence through the closing one.
    let pos = span_of(cur, span_start, cur.pos, options);
    if let Some(kind) = open.kind {
        BlockNode::Admonition(Admonition {
            attrs: open.attrs,
            kind,
            // The title is a slice of the opener line, so its inlines can be
            // placed - but only when the opener told us which column it starts
            // at. `inline_anchor_for_line` cannot: it works by suffix, and a
            // title sits in the MIDDLE of its line, between quotes.
            title: open.title.map(|t| {
                let anchor = open.title_col.and_then(|col| {
                    Some((
                        cur.source_line(span_start)?,
                        cur.source_col(span_start)? + col as isize,
                    ))
                });
                parse_inline_lines_with_anchor(&t, options, vec![anchor])
            }),
            label: open.label,
            children,
            pos,
        })
    } else {
        BlockNode::Div(Div {
            attrs: open.attrs,
            label: open.label,
            children,
            pos,
        })
    }
}

/// A `::: |` line-block (verse) opener: a colon fence (3+) then a bare pipe and
/// nothing else (grammar PART 9 §23). Returns the fence length.
fn detect_line_block_open(line: &str) -> Option<usize> {
    let trimmed = trim_ascii(line);
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return None;
    }
    // grammar: `line_block_open = colon_fence, space, "|"` -- a SPACE between
    // the fence and the pipe is REQUIRED, so `:::|` is not a line block and
    // neither is a tabbed opener. The production always said `space`; this read
    // it as "space or tab", which PART 7's MARKER SEPARATORS AND PADDING SLOTS
    // now rules out explicitly - the pipe is the token that selects this block,
    // so the slot before it is a marker separator (#712, spec carve#886).
    let after = &trimmed[fence_len..];
    let trimmed_after = after.trim_start_matches(' ');
    if trimmed_after.len() == after.len() {
        return None; // no space before the pipe
    }
    // `trimmed` already had its trailing SPACE and TAB run removed by
    // `trim_ascii`, so a `trim_end` here can only ever strip what that helper
    // deliberately left: a VERTICAL TAB, a FORM FEED or a NO-BREAK SPACE, every
    // one of them CONTENT (PART 7, carve#977). The comparison is direct.
    if trimmed_after == "|" {
        Some(fence_len)
    } else {
        None
    }
}

/// Count a line's leading whitespace in visual columns (tab = next 4-stop).
fn leading_ws_columns(line: &str) -> usize {
    let mut columns = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' => columns += 1,
            '\t' => columns += 4 - (columns % 4),
            _ => break,
        }
    }
    columns
}

/// Remove up to `cols` columns of leading whitespace (tab-aware). When a tab
/// straddles the boundary its unconsumed columns are re-inserted as spaces, so
/// a verse line's relative indentation is preserved exactly (the residual-aware
/// dedent Carve uses on indentation it must keep).
fn strip_leading_columns(line: &str, cols: usize) -> String {
    let mut columns = 0usize;
    for (i, ch) in line.char_indices() {
        if columns >= cols {
            return line[i..].to_string();
        }
        match ch {
            ' ' => columns += 1,
            '\t' => {
                let next = columns + (4 - columns % 4);
                if next > cols {
                    // Tab crosses the reference column: keep the leftover columns.
                    return " ".repeat(next - cols) + &line[i + 1..];
                }
                columns = next;
            }
            _ => return line[i..].to_string(),
        }
    }
    String::new()
}

/// Expand the whitespace a line block preserves to non-breaking spaces, so a
/// verse line's layout survives; tabs advance to the next 4-column stop.
///
/// Leading whitespace is preserved down to a single column. An INNER or
/// TRAILING run of TWO OR MORE columns is a medial gap - the alignment a
/// caesura or a column of aligned text is made of - and is preserved too
/// (grammar §23). A lone inner space stays an ordinary, collapsible space so a
/// long line can still wrap between words.
///
/// Uses the generated-NBSP placeholder (HTML folds it to `&nbsp;`; plain/ANSI
/// turn it back into an ASCII space), so it stays distinct from a literal
/// U+00A0 typed in the source.
fn expand_line_block_ws(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut columns = 0usize;
    let mut seen_content = false;
    let mut chars = line.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch != ' ' && ch != '\t' {
            out.push(ch);
            seen_content = true;
            columns += 1;
            continue;
        }

        let mut width = if ch == '\t' { 4 - (columns % 4) } else { 1 };
        while let Some((_, next)) = chars.peek() {
            match next {
                ' ' => width += 1,
                '\t' => width += 4 - ((columns + width) % 4),
                _ => break,
            }
            chars.next();
        }
        columns += width;

        if !seen_content || width >= 2 {
            for _ in 0..width {
                out.push(crate::NBSP_PLACEHOLDER);
            }
        } else if chars.peek().is_some() {
            out.push(' ');
        }
        // ...and a ONE-COLUMN run at the END of the line is dropped, like
        // trailing whitespace anywhere else (PART 2 NO TRAILING WHITESPACE,
        // carve#926). The ORDER is what decides this line: §23 converts an
        // inner or trailing run of TWO OR MORE columns into NBSP CONTENT first,
        // and content is not whitespace - so the rule never reaches those, and
        // `abc<SP><SP>` still ends in two non-breaking spaces. What it does
        // reach is the one-column case, which §23 leaves as an ordinary space.
        //
        // A trailing TAB is not the one-column case: it expands to the next tab
        // stop, which is at least two columns from anywhere it can start, so it
        // becomes NBSP content and survives.
    }

    out
}

/// Parse a `::: |` line block into a `<div class="line-block">`: each stanza
/// (blank-line-separated run) is a paragraph whose soft breaks become hard
/// breaks and whose per-line leading whitespace is preserved (grammar §23).
/// One line-block stanza: its lines, its own span, and the SOURCE columns each
/// line starts and ends at.
///
/// The two column vectors are recorded whatever `placeable_indent` decided,
/// because a break's span is line geometry and survives a tab that makes the
/// line's TEXT unplaceable (carve-rs#480).
struct Stanza {
    lines: LineBuffer,
    at: Option<Pos>,
    end_cols: Vec<Option<isize>>,
    start_cols: Vec<Option<isize>>,
    /// The comment-only body lines this stanza had, as `(line index, node)`.
    /// Their text is already gone from `lines` - see `verse_comment_line`.
    comments: Vec<(usize, Comment)>,
}

/// A verse body line that is nothing but a comment (grammar PART 9 §23, IT IS
/// REMOVED AT THE BLOCK LAYER). Returns the comment's content.
///
/// ONLY a line whose FIRST character is `%` qualifies: `comment_line`'s optional
/// `[whitespace]` prefix has nothing to consume in verse, where a leading run is
/// CONTENT, so an indented `%%` line is ordinary text. The line is measured
/// AFTER the fence's own structural indent is stripped, which is the reference
/// column the rest of the block measures against.
fn verse_comment_line(stripped: &str) -> Option<String> {
    let rest = stripped.strip_prefix("%%")?;
    Some(rest.trim_start().to_string())
}

/// Put the comment-only lines back, at the content position of the line each
/// one came from.
///
/// The block layer emptied those lines before the stanza reached the inline
/// parser, which is the whole point of the clause: no inline run can reach a
/// comment, so an unclosed verbatim run opened on an EARLIER line cannot claim
/// it (carve#1333). The node is still published, because the author wrote it and
/// PART 12 has a `comment` node for it.
///
/// Line k begins just after the k-th boundary. Where a stanza has no k-th
/// boundary the comment is dropped: the only way that happens is an UNCLOSED
/// verbatim run swallowing the rest of the stanza, and inside that run there is
/// no place for a node - the run's own value carries the emptied line as a
/// newline, like every other break it swallows.
///
/// COUNT A BOUNDARY HOWEVER IT IS SPELLED, AND AT EVERY DEPTH. This walk used to
/// count top-level `HardBreak` nodes, which is one of the three spellings a
/// stanza boundary has, and the two it missed each lost a comment:
///
/// - An inline container that opens on one body line and closes on a later one
///   holds the boundaries between them as its OWN children, so a top-level walk
///   never sees them and a comment whose line ended under a `strong` found
///   nowhere to sit (markup-carve/carve-rs#1079). The boundary is the same
///   boundary at either depth, so the node goes back at it at either depth.
/// - A CLOSED verbatim run spanning a boundary carries that newline in its
///   value rather than as a node, so counting nodes alone reported a line number
///   short and every comment after it was placed a line late. `carve fmt` then
///   wrote it onto the following line, merging the author's comment text into
///   the next line's text.
///
/// The soft-to-hard conversion is a SEPARATE walk over the same slots
/// (`harden_verse_breaks`), and it always was - it just used to stop at the
/// stanza's top level. markup-carve/carve#1351 ruled that it does not, so both
/// walks now reach the same depths. This one is unchanged by that: it asks where
/// the boundary IS and never what it is called, which is why either answer kept
/// it.
///
/// ONE PASS, building new vectors rather than inserting into the old ones. The
/// comments arrive in line order, at most one per line, so the walk can consume
/// them alongside the boundaries - and a stanza of nothing but comment lines is
/// a document an author can write, where repeated `Vec::insert` would be
/// quadratic in the block's length.
/// PART 9 §23: every SOFT line break inside a stanza becomes a HARD break, at
/// EVERY DEPTH (markup-carve/carve#1351, corpus `348`).
///
/// The conversion used to run on the stanza's top-level nodes only, so a closed
/// inline construct spanning a boundary kept the bare newline: `*a` / `b*`
/// rendered `a\nb` inside the `strong` while the same two lines without the
/// emphasis got a `<br>`. This engine broke the clause's own invariant against
/// itself - `*a\` / `b*` DID emit the `<br>` inside the `strong`, so one line
/// boundary in one container gave two different answers depending on how it was
/// spelled.
///
/// DRIVEN BY NODE KIND, which is what §23's neighbour A BACKSLASH BREAK IS NOT
/// ADDITIVE fixes as the test. Both worked exemptions there turn on there being
/// no node: a backslash consumes its own newline, and a verbatim run carries the
/// newline as its content, so "there is no boundary left in the tree". An
/// emphasis run consumes nothing - the boundary is a node beside its text - so
/// the exemption never reached it, and the difference is in KIND rather than in
/// depth.
///
/// Both exemptions therefore need no code here and get none. A verbatim run has
/// no children to walk and no break node inside it, so `a `b` / `c` d` keeps its
/// bare newline (corpus `348-2`); and a `hard_break` is already hard, so a
/// backslash inside emphasis still produces exactly one `<br>` (corpus `348-4`).
///
/// The slots are the ones `splice_verse_comments_into` walks, for the same
/// reason: a boundary is a child of whatever construct spans it, whichever slot
/// that construct keeps its inlines in.
fn harden_verse_breaks(inlines: Vec<InlineNode>) -> Vec<InlineNode> {
    inlines
        .into_iter()
        .map(|node| match node {
            // The hard break here IS the source's line ending, so it keeps the
            // soft break's span rather than being rebuilt without one.
            InlineNode::SoftBreak(b) => InlineNode::HardBreak(b),
            InlineNode::Emphasis(mut n) => {
                n.children = harden_verse_breaks(n.children);
                InlineNode::Emphasis(n)
            }
            InlineNode::Link(mut n) => {
                n.children = harden_verse_breaks(n.children);
                InlineNode::Link(n)
            }
            InlineNode::Span(mut n) => {
                n.children = harden_verse_breaks(n.children);
                InlineNode::Span(n)
            }
            InlineNode::Extension(mut n) => {
                n.children = harden_verse_breaks(n.children);
                InlineNode::Extension(n)
            }
            InlineNode::CriticInsert(mut n) => {
                n.children = harden_verse_breaks(n.children);
                InlineNode::CriticInsert(n)
            }
            InlineNode::CriticDelete(mut n) => {
                n.children = harden_verse_breaks(n.children);
                InlineNode::CriticDelete(n)
            }
            InlineNode::Footnote(mut n) => {
                n.inline = n.inline.map(harden_verse_breaks);
                InlineNode::Footnote(n)
            }
            InlineNode::CitationGroup(mut group) => {
                for item in &mut group.items {
                    for field in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        *field = harden_verse_breaks(std::mem::take(field));
                    }
                }
                InlineNode::CitationGroup(group)
            }
            other => other,
        })
        .collect()
}

fn splice_verse_comments(
    inlines: Vec<InlineNode>,
    comments: Vec<(usize, Comment)>,
) -> Vec<InlineNode> {
    if comments.is_empty() {
        return inlines;
    }
    let mut pending = comments.into_iter().peekable();
    let mut line = 0usize;
    let mut out = splice_verse_comments_into(inlines, &mut pending, &mut line);
    // A COMMENT ON THE STANZA'S LAST LINE has no boundary after it to sit
    // before, so it goes at the end - the boundary that OPENS its line is still
    // there, which is what says the line is still there. The walk drains before
    // each node and so never reaches this one.
    //
    // STILL GATED ON THE LINE. Anything left over after that is a comment whose
    // line has no boundary at all, which is the open verbatim run swallowing the
    // rest of the stanza: the run carries the emptied line as a newline and
    // there is no place inside it for a node. Those stay dropped, as they were -
    // pushing the leftovers unconditionally puts the note back at a position the
    // author never wrote.
    while let Some((_, comment)) = pending.next_if(|(at, _)| *at == line) {
        out.push(InlineNode::Comment(comment));
    }
    out
}

/// One level of `splice_verse_comments`, sharing the caller's line counter so a
/// boundary counts once wherever in the tree it turns up.
fn splice_verse_comments_into(
    inlines: Vec<InlineNode>,
    pending: &mut std::iter::Peekable<std::vec::IntoIter<(usize, Comment)>>,
    line: &mut usize,
) -> Vec<InlineNode> {
    let mut out = Vec::with_capacity(inlines.len());
    for mut node in inlines {
        // The comment sits BEFORE the boundary that ends its line: the line is
        // empty now, so there is nothing else on it.
        while let Some((_, comment)) = pending.next_if(|(at, _)| *at == *line) {
            out.push(InlineNode::Comment(comment));
        }
        // Nothing left to place, so nothing left to count for: the rest of this
        // level is carried through without descending into it.
        if pending.peek().is_none() {
            out.push(node);
            continue;
        }
        match &mut node {
            InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) => *line += 1,
            // A verbatim run's newlines are boundaries it ATE. §23 says the run
            // "carries the break, and what it carries is a NEWLINE", so each one
            // ends a body line exactly as a break node does.
            InlineNode::Code(n) => *line += n.value.matches('\n').count(),
            InlineNode::Math(n) => *line += n.content.matches('\n').count(),
            InlineNode::RawInline(n) => *line += n.content.matches('\n').count(),
            InlineNode::LiteralInline(n) => *line += n.content.matches('\n').count(),
            // EVERY SLOT AN INLINE NODE HOLDS INLINES IN, not just `children`.
            // An inline footnote carries its body in `inline` and a citation
            // item carries three arrays of its own, so a walk that knew only
            // `children` missed those containers and the emptied line stayed
            // pending - `^[a` / `%% secret` / `c]` wrote the line back as an
            // empty one (markup-carve/carve-js#1184 found the same two slots).
            InlineNode::Emphasis(n) => {
                n.children =
                    splice_verse_comments_into(std::mem::take(&mut n.children), pending, line)
            }
            InlineNode::Link(n) => {
                n.children =
                    splice_verse_comments_into(std::mem::take(&mut n.children), pending, line)
            }
            InlineNode::Span(n) => {
                n.children =
                    splice_verse_comments_into(std::mem::take(&mut n.children), pending, line)
            }
            InlineNode::Extension(n) => {
                n.children =
                    splice_verse_comments_into(std::mem::take(&mut n.children), pending, line)
            }
            InlineNode::Footnote(n) => {
                if let Some(inline) = &mut n.inline {
                    *inline = splice_verse_comments_into(std::mem::take(inline), pending, line);
                }
            }
            InlineNode::CitationGroup(group) => {
                for item in &mut group.items {
                    for field in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        *field = splice_verse_comments_into(std::mem::take(field), pending, line);
                    }
                }
            }
            InlineNode::CriticInsert(n) => {
                n.children =
                    splice_verse_comments_into(std::mem::take(&mut n.children), pending, line)
            }
            InlineNode::CriticDelete(n) => {
                n.children =
                    splice_verse_comments_into(std::mem::take(&mut n.children), pending, line)
            }
            _ => {}
        }
        out.push(node);
    }
    out
}

/// Give every line-block hard break a span, even where the stanza's TEXT has
/// none.
///
/// A tab expands to placeholders and shifts every column after it within the
/// line, so the anchor machinery refuses that line and its inlines come out
/// unplaced. That is right for text: the value stops being a slice of the
/// source. A break is not content on the line, though - it is the newline
/// ENDING it, and tab expansion does not move a line ending. The k-th break is
/// the newline after stanza line k, which is pure line geometry, so it stays
/// derivable exactly where the text is not (carve-rs#480).
///
/// Only breaks still MISSING a span are filled: where the anchors placed one it
/// is already the same fact, computed the same way.
fn place_line_block_breaks(
    inlines: Vec<InlineNode>,
    lines: &LineBuffer,
    end_cols: &[Option<isize>],
    start_cols: &[Option<isize>],
    emptied: &[usize],
    options: &Options<'_>,
) -> Vec<InlineNode> {
    if !options.positions {
        return inlines;
    }
    let mut break_index = 0usize;
    inlines
        .into_iter()
        .map(|node| {
            let InlineNode::HardBreak(brk) = node else {
                return node;
            };
            let k = break_index;
            break_index += 1;
            // A break ending a line the block layer EMPTIED is recomputed even
            // when the inline parser gave it one: what that parser measured is
            // the emptied line, which ends at column 1, and the newline the
            // author wrote is at the end of the comment they wrote
            // (grammar PART 9 §23).
            if brk.pos.is_some() && emptied.binary_search(&k).is_err() {
                return InlineNode::HardBreak(brk);
            }
            // Start: just past the last character of line k. End: the first
            // column of line k+1, which is where the next line's content
            // begins. Both are 1-based, matching every other span.
            let pos = match (
                lines.line_map.get(k).copied().flatten(),
                end_cols.get(k).copied().flatten(),
                lines.line_map.get(k + 1).copied().flatten(),
                start_cols.get(k + 1).copied().flatten(),
            ) {
                (Some(start_line), Some(end_col), Some(end_line), Some(next_col)) => Some(Pos {
                    start_line,
                    start_column: document_column(end_col, 0),
                    end_line,
                    end_column: document_column(next_col, 0),
                    ..Default::default()
                }),
                _ => None,
            };
            InlineNode::HardBreak(Break {
                pos: pos.or(brk.pos),
            })
        })
        .collect()
}

fn parse_line_block(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let span_start = cur.pos;
    let opener = cur.peek().unwrap();
    let fence_len = detect_line_block_open(opener).unwrap();
    // Verse indentation is measured RELATIVE TO THE FENCE (grammar §23
    // REFERENCE COLUMN): strip the opener's own structural indent from each
    // body line before preserving the author's intra-verse whitespace.
    let base_indent = leading_ws_columns(opener);
    cur.consume();
    let mut stanzas: Vec<Stanza> = Vec::new();
    let mut stanza: Vec<String> = Vec::new();
    let mut stanza_line_map: Vec<Option<usize>> = Vec::new();
    let mut stanza_col_map: Vec<Option<isize>> = Vec::new();
    let mut stanza_end_cols: Vec<Option<isize>> = Vec::new();
    let mut stanza_start_cols: Vec<Option<isize>> = Vec::new();
    let mut stanza_comments: Vec<(usize, Comment)> = Vec::new();
    // Where the open stanza began, and where it ended - the CURSOR's own line
    // indices, which still point at the source. The rewritten verse text cannot
    // give a column back, but the lines a stanza occupies are not in doubt.
    let mut stanza_start: Option<usize> = None;
    let mut stanza_end = cur.pos;
    while let Some(line) = cur.peek() {
        if exact_colon_fence_len(line) == Some(fence_len) {
            cur.consume();
            break;
        }
        let source_line = cur.source_line(cur.pos);
        let line_at = cur.pos;
        cur.consume();
        if is_blank_line(line) {
            if !stanza.is_empty() {
                let at = stanza_start.take();
                let end_cols = std::mem::take(&mut stanza_end_cols);
                let start_cols = std::mem::take(&mut stanza_start_cols);
                stanzas.push(Stanza {
                    lines: LineBuffer {
                        lines: std::mem::take(&mut stanza),
                        line_map: std::mem::take(&mut stanza_line_map),
                        col_map: std::mem::take(&mut stanza_col_map),
                        // Built line by line from the source; nothing synthetic.
                        last_is_synthetic: false,
                    },
                    at: at.and_then(|start| span_of(cur, start, stanza_end, options)),
                    end_cols,
                    start_cols,
                    comments: std::mem::take(&mut stanza_comments),
                });
            }
            continue;
        }
        stanza_start.get_or_insert(line_at);
        stanza_end = cur.pos;
        let stripped = strip_leading_columns(line, base_indent);
        // A COMMENT-ONLY BODY LINE IS DECIDED HERE, with the other block-layer
        // decisions and BEFORE any inline content exists (grammar PART 9 §23, IT
        // IS REMOVED AT THE BLOCK LAYER). Leaving it to the inline parser let
        // §21's verbatim exclusion claim the line, so a stray backtick anywhere
        // above PUBLISHED the comment's own text inside the code span - the one
        // outcome a comment may never have (carve#1333).
        //
        // The line stays: what is removed is its TEXT, so the stanza keeps its
        // shape and the boundary below it still hardens into the `<br>` the
        // author wrote. The comment itself is put back after the inline run, by
        // `splice_verse_comments`.
        let comment = verse_comment_line(&stripped);
        let expanded = expand_line_block_ws(&stripped);
        // A verse line stays placeable only while the rewrite is a SPACE
        // PROMOTED IN PLACE: one space becomes one placeholder, so every column
        // still maps back one to one and the node's value is still the source
        // read differently. Anything else refuses.
        //
        // A TAB is the case that refuses. It expands to up to four placeholders
        // from one source character, which pushes every column after it too far
        // right - and even where the arithmetic happens to yield exactly one
        // column, the character changed, so the value stops being a slice of
        // the source and a consumer asked to highlight it gets a mismatch.
        // carve-js publishes no position for the same lines.
        //
        // The check used to look only at the LEADING run, which was right while
        // the rule only preserved the indent. Medial and trailing gaps are
        // preserved too now (PART 9 §23), so a tab anywhere in the line has to
        // count - `tab\tgap` kept a position while its value read `tab gap`.
        //
        // Per line, not per stanza: one tab-bearing line does not cost its
        // neighbours their positions.
        //
        // A DROPPED TRAILING ONE-COLUMN RUN is not a rewrite. §23 leaves a lone
        // space at the end of a verse line as an ordinary space and PART 2 then
        // drops it, so `def<SP>` arrives as `def` - SHORTER than its source
        // line, where an equal-length test reads a refusal. Nothing in front of
        // it moved, so the constant is unaffected and the line is still
        // placeable; the equal-length test cost `def` its position while the
        // identical line without the space kept one (markup-carve/carve#961).
        // Only the DROPPED tail may differ, and it must be whitespace: a
        // trailing tab expands to at least two columns, becomes NBSP content
        // and makes `expanded` longer, which still refuses here.
        //
        // THE WHITESPACE HALF OF THAT TEST CANNOT FAIL TODAY, recorded rather
        // than presented as a fix (markup-carve/carve#755). Widening it to
        // accept any dropped tail changes no corpus document and no output over
        // 6125 generated verse-line shapes, and fails no test: the only way
        // `expanded` gets shorter is `expand_line_block_ws` dropping a
        // one-column run, which is whitespace by construction. It stays because
        // the length arithmetic alone would not notice a future rewrite that
        // dropped CONTENT off the end, and that one would move the columns.
        let dropped = stripped
            .chars()
            .count()
            .checked_sub(expanded.chars().count());
        let placeable_indent = dropped.is_some_and(|dropped| {
            stripped
                .chars()
                .rev()
                .take(dropped)
                .all(|c| c == ' ' || c == '\t')
        }) && expanded
            .chars()
            .zip(stripped.chars())
            .all(|(e, s)| e == s || (s == ' ' && e == crate::NBSP_PLACEHOLDER));
        stanza_col_map.push(if placeable_indent {
            stripped_col(cur.source_col(line_at), line, &stripped)
        } else {
            None
        });
        // Where this line ENDS in the source, recorded whatever the indent
        // check decided. A hard break is the newline ENDING a line, not content
        // on it, and tab expansion does not move a line ending -- so the break's
        // span stays derivable on exactly the lines whose text is not
        // (carve-rs#480). Measured on `line`, which is the source text before
        // `expand_line_block_ws` rewrites gaps into placeholders.
        stanza_end_cols.push(
            cur.source_col(line_at)
                .map(|stripped_cols| stripped_cols + line.chars().count() as isize),
        );
        stanza_start_cols.push(cur.source_col(line_at));
        if let Some(content) = comment {
            let lead = (line.chars().count() - stripped.chars().count()) as isize;
            let start = cur.source_col(line_at).map(|col| col + lead);
            let pos = match (options.positions.then_some(()).and(source_line), start) {
                (Some(at), Some(start)) => Some(Pos {
                    start_line: at,
                    start_column: document_column(start, 0),
                    end_line: at,
                    end_column: document_column(start, stripped.chars().count()),
                    ..Default::default()
                }),
                _ => None,
            };
            stanza_comments.push((
                stanza.len(),
                Comment {
                    block: false,
                    delimited: false,
                    content,
                    pos,
                },
            ));
            stanza.push(String::new());
        } else {
            stanza.push(expanded);
        }
        stanza_line_map.push(source_line);
    }
    if !stanza.is_empty() {
        let at = stanza_start.take();
        stanzas.push(Stanza {
            lines: LineBuffer {
                lines: stanza,
                line_map: stanza_line_map,
                col_map: stanza_col_map,
                // Built line by line from the source; nothing synthetic.
                last_is_synthetic: false,
            },
            at: at.and_then(|start| span_of(cur, start, stanza_end, options)),
            end_cols: stanza_end_cols,
            start_cols: stanza_start_cols,
            comments: stanza_comments,
        });
    }

    let children = stanzas
        .into_iter()
        .map(
            |Stanza {
                 lines,
                 at,
                 end_cols,
                 start_cols,
                 comments,
             }| {
                let source_line = lines.line_map.first().copied().flatten();
                let anchors: Vec<Option<(usize, isize)>> = lines
                    .line_map
                    .iter()
                    .zip(lines.col_map.iter())
                    .map(|(line_no, col)| Some(((*line_no)?, (*col)?)))
                    .collect();
                // AT EVERY DEPTH (PART 9 section 23, markup-carve/carve#1351).
                // See `harden_verse_breaks` for why the test is node kind and
                // not depth, and why both exemptions need no code.
                let inlines = harden_verse_breaks(parse_inline_lines_with_anchor(
                    &lines.lines.join("\n"),
                    options,
                    anchors,
                ));
                let emptied: Vec<usize> = comments.iter().map(|(line, _)| *line).collect();
                let inlines = place_line_block_breaks(
                    inlines,
                    &lines,
                    &end_cols,
                    &start_cols,
                    &emptied,
                    options,
                );
                let inlines = splice_verse_comments(inlines, comments);
                let mut node = BlockNode::Paragraph(Paragraph {
                    attrs: None,
                    children: inlines,
                    pos: at,
                    ..Default::default()
                });
                if options.source_lines {
                    if let Some(line) = source_line {
                        stamp_source_line(&mut node, line);
                    }
                }
                node
            },
        )
        .collect();

    // No inline opener attributes (strict djot); a preceding block-attribute
    // line merges onto this node in parse_blocks.
    BlockNode::LineBlock(LineBlock {
        pos: span_of(cur, span_start, cur.pos, options),
        attrs: None,
        children,
    })
}

/// A `::: \` local hard-break block opener: a colon fence (3+) then a bare
/// backslash and nothing else (grammar PART 9 §23). Returns the fence length.
/// Deliberately smaller than a line block: it converts soft breaks in DIRECT
/// paragraph children to hard breaks, but does NOT preserve leading whitespace,
/// keeps the stanza/block structure of its body, and does not affect nested
/// blocks. Mirrors carve-js `RE_HARDBREAKS_BLOCK_OPEN` / `parseHardBreaksBlock`.
fn detect_hardbreaks_block_open(line: &str) -> Option<usize> {
    let trimmed = trim_ascii(line);
    let fence_len = trimmed.bytes().take_while(|b| *b == b':').count();
    if fence_len < 3 {
        return None;
    }
    // `hardbreaks_block_open = colon_fence, space, "\"` -- a SPACE between the
    // fence and the backslash is REQUIRED, so a glued opener is not one and
    // neither is a tabbed one. The production always said `space`; this read it
    // as "space or tab", which PART 7's MARKER SEPARATORS AND PADDING SLOTS now
    // rules out explicitly - the backslash is the token that selects this
    // block, so the slot before it is a marker separator (#712, spec
    // carve#886).
    let after = &trimmed[fence_len..];
    let trimmed_after = after.trim_start_matches(' ');
    if trimmed_after.len() == after.len() {
        return None; // no space before the backslash
    }
    // Same as the line block above: `trim_ascii` has already taken the trailing
    // space-or-tab run, so a `trim_end` could only strip content (PART 7,
    // carve#977).
    if trimmed_after == "\\" {
        Some(fence_len)
    } else {
        None
    }
}

/// Parse a `::: \` local hard-break block into a `<div class="hardbreaks">`:
/// the body is parsed as ordinary blocks, then every soft break in a DIRECT
/// paragraph child becomes a hard break. Unlike a line block, leading
/// whitespace is not preserved and nested blocks keep ordinary soft breaks.
fn parse_hardbreaks_block(cur: &mut LineCursor, options: &Options<'_>) -> BlockNode {
    let opener = cur.peek().unwrap();
    let fence_len = detect_hardbreaks_block_open(opener).unwrap();
    let span_start = cur.pos;
    cur.consume();
    let (inner, _closed) = collect_colon_container_body(cur, fence_len);
    // The span covers the opening fence through the closing one, like any other
    // colon fence.
    let pos = span_of(cur, span_start, cur.pos, options);
    let mut children = parse_capped_colon_body(inner, options);
    for child in &mut children {
        if let BlockNode::Paragraph(para) = child {
            for node in &mut para.children {
                if let InlineNode::SoftBreak(brk) = node {
                    // Carry the break's span across. Building a fresh
                    // `hard_break()` here threw it away, and the loss was
                    // invisible: the two render identically in this block, so
                    // only the tree showed it.
                    *node = InlineNode::HardBreak(Break { pos: brk.pos });
                }
            }
        }
    }
    // No inline opener attributes (strict djot); a preceding block-attribute
    // line merges onto this div in parse_blocks.
    BlockNode::Div(Div {
        attrs: Some(Attrs {
            id: None,
            classes: vec!["hardbreaks".to_string()],
            key_values: BTreeMap::new(),
            order: vec![AttrSlot::Class],
        }),
        label: None,
        children,
        // The author DID write this block: the opener is a colon fence carrying
        // a lone \, and a matching fence closes it. Refusing a position on the
        // grounds that it is a synthesized wrapper had it backwards - the
        // `.hardbreaks` class is synthesized, the fence is not.
        pos,
    })
}

fn detect_abbreviation_def(line: &str) -> Option<AbbreviationDef> {
    let rest = line.strip_prefix("*[")?;
    let (abbr, expansion) = rest.split_once("]:")?;
    let expansion = expansion.strip_prefix(' ')?;
    // THE SEPARATOR IS A RUN, AND IT IS ASCII SPACES. PART 5 spells it
    // `space+`: a tab is still not a separator, so `*[HTML]:<TAB>x` stayed a
    // paragraph at the `strip_prefix` above, but a run of spaces is one
    // separator rather than a separator plus content (carve#892).
    //
    // The first character that is not a space ENDS the separator and BEGINS the
    // content, which is the half this engine had wrong: the expansion was
    // `trim()`ed, so a no-break space or a tab after the run was eaten and
    // `*[HTML]: <NBSP>Hyper Text` expanded to a title the author did not write.
    // An `abbreviation_expansion` is a raw string, so both survive into it.
    //
    // Trimmed with `trim_start_matches(' ')` and not a whitespace `trim_start`
    // for exactly that reason - the terminal is the ASCII space, not the
    // Unicode property.
    let expansion = expansion.trim_start_matches(' ');
    // ASCII, because `letter` is: the grammar enumerates it as `a`..`z` plus
    // `A`..`Z`, and `digit` as `0`..`9`. `char::is_alphanumeric` is
    // Unicode-aware, so `*[ß]:` and `*[日本]:` were definitions here and
    // paragraph text in carve-js and carve-php - and a definition renders
    // nothing, so the document lost either the line or the expansion when it
    // moved (carve#791). Same reading that makes `*[e.g.]:` a paragraph, which
    // all three already agree on, and the same ASCII rule
    // `is_attr_ident_start` below applies to attribute identifiers.
    if abbr.is_empty() || !abbr.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    // `abbreviation_expansion = {character - newline}+` - ONE or more. An empty
    // expansion is not a definition, and consuming the line DELETED it from the
    // document (carve-js keeps it as `<p>*[A]:</p>`).
    //
    // Deliberately `is_empty()` and not a trimmed test: a space is a character,
    // so `*[A]:··` has a one-character expansion and IS a definition. That is
    // the production as written, and it is what carve-js does.
    if expansion.is_empty() {
        return None;
    }
    Some(AbbreviationDef {
        abbr: abbr.to_string(),
        // Only the TRAILING side is trimmed here: the leading run was consumed
        // above as the separator it is. The TERMINAL is space-or-tab, the one
        // whitespace definition (PART 7, carve#977) - `trim_end` is the Unicode
        // White_Space property, which ate a trailing NO-BREAK SPACE, VERTICAL
        // TAB or FORM FEED out of the expansion, all three of them content. The
        // note that used to stand here deferred this to carve#926; PART 7 is
        // where the terminal was settled.
        expansion: trim_ascii_end(expansion).to_string(),
        pos: None,
    })
}

/// First byte of an attribute identifier (id/class/key): a letter or `_`
/// (matches `is_identifier`'s first-char rule). Non-ASCII bytes are never a
/// start here (`is_identifier` uses `is_ascii_alphabetic`).
#[inline]
fn is_attr_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

/// Continuation byte of an attribute identifier: a letter, digit, `_`, or `-`
/// (matches `is_identifier`'s tail rule). Non-ASCII → false, ending the run.
#[inline]
fn is_attr_ident_part(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Whether the attribute payload following the `{` at `brace` provably cannot
/// parse -- i.e. `read_attrs_at`'s scan+`parse_attrs` would return `None`.
///
/// It walks the SAME token grammar `attr_tokens`/`parse_attrs` accept and bails
/// at the first byte that cannot continue a valid token, so a doomed payload is
/// rejected in O(1) per opener instead of the char walk running to a far `}`
/// (O(n²) on `[x]{`×n + `}`, `[x]{a `×n + `}`, `[x]{.a `×n + `}`, `[x]{k= `×n +
/// `}`, …). It is a pure SKIP filter: it returns `true` ONLY when the payload is
/// provably invalid; on a `}` (a candidate close), a newline, a quote, an
/// escape, a `key=<value>` with a real value, or ANY non-ASCII byte (a possible
/// Unicode-whitespace separator or non-ASCII content), it returns `false` and
/// the unchanged scan/`parse_attrs` path decides -- so every accepted block, and
/// its output, is byte-identical. A nested `{`/`[` (or any other invalid
/// boundary byte) ends the walk, so each byte is visited O(1) times -> O(n)
/// total. Deferring on non-ASCII keeps it correct without decoding chars (only
/// the ASCII pathological shapes need the O(1) bail; a non-ASCII payload is rare
/// and still handled correctly by the full scan). Mirrors carve-js
/// `spanAttrProvablyInvalid`, matched to carve-rs's first-`}` (non-balancing)
/// acceptance.
fn attr_payload_provably_invalid(bytes: &[u8], brace: usize) -> bool {
    let n = bytes.len();
    let mut i = brace + 1;
    while i < n {
        let c = bytes[i];
        // Non-ASCII: a Unicode-whitespace separator, a non-ASCII value byte, or
        // other subtle content. Defer to the full scan/parse (byte-identical;
        // non-ASCII is never the repeated ASCII pathological shape).
        if !c.is_ascii() {
            return false;
        }
        match c {
            // A candidate close at a token boundary: let the real scan decide.
            b'}' => return false,
            // A newline ends an inline block (read_attrs_at bails); defer.
            b'\n' => return false,
            // Other ASCII whitespace separates tokens (attr_tokens treats
            // char::is_whitespace as a separator); skip it and continue.
            b' ' | b'\t' | 0x0B | 0x0C | b'\r' => i += 1,
            // Quotes and escapes are subtle -- defer.
            b'"' | b'\'' | b'\\' => return false,
            // `#id` / `.class`: an identifier MUST follow, else the token (and
            // the whole payload) is invalid (§14).
            b'#' | b'.' => {
                match bytes.get(i + 1) {
                    Some(&d) if is_attr_ident_start(d) => {}
                    _ => return true,
                }
                i += 2;
                while i < n && is_attr_ident_part(bytes[i]) {
                    i += 1;
                }
            }
            // `:TAG` / `:` semantic language attribute. The full parser owns
            // the structural subtag envelope; this fast path only avoids
            // rejecting the newly claimed token before it gets there.
            b':' => return false,
            // A bareword: a boolean attribute, or the name in `key=value`.
            _ if is_attr_ident_start(c) => {
                i += 1;
                while i < n && is_attr_ident_part(bytes[i]) {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'=') {
                    // `key=` with an EMPTY value (EOF, `}`, or ASCII whitespace
                    // next) leaves a dangling `=` -> invalid. A bare value
                    // (>=1 non-space) or a quoted value: defer (a valid bare
                    // value is consumed whole by the scan -> linear). Non-ASCII
                    // after `=` (a value byte or Unicode space) also defers.
                    match bytes.get(i + 1) {
                        None => return true,
                        Some(&b'}') => return true,
                        Some(&v) if v.is_ascii() && v.is_ascii_whitespace() => return true,
                        _ => return false,
                    }
                }
                // else continue to the next token.
            }
            // Any other ASCII byte cannot begin a valid token at a boundary
            // (`[`, `{`, `(`, a digit, `-`, `+`, `=`, `,`, …): invalid.
            _ => return true,
        }
    }
    // Ran off the end without a `}`: the scan would fail too.
    true
}

/// Read an inline attribute block `{...}` at `start` (which must index a `{`).
///
/// `last_close_brace` is the index of the last `}` in `bytes` (or `None` if
/// there is none). A block can only close on a `}`, so when no `}` lies at or
/// after `start` the scan could only walk to end-of-text and fail -- it is
/// skipped in O(1). This keeps a run of unclosed `{`-attribute openers
/// (`[x]{`×n, `*a*{`×n, `:a:{`×n, …) linear instead of O(n^2). Callers scanning
/// a fresh slice (block attributes, table cells) pass that slice's own last-`}`
/// index. Skipping only elides a call that would return `None`, so output is
/// byte-identical.
/// The INLINE `attributes` production, whose interior is SPACE-ONLY.
///
/// Every whitespace slot of the inline block takes `space` (PART 4 THE INLINE
/// INTERIOR IS SPACE-ONLY, carve#906): the run after `{`, the run between two
/// attributes, the run before `}`, the boundary after an unquoted value, and
/// the blessed empty block `{ }`. All five sit AFTER the first non-whitespace
/// character of their line, which is where PART 7's rule already says a tab is
/// not syntax.
fn read_attrs_at(
    bytes: &[u8],
    start: usize,
    last_close_brace: Option<usize>,
) -> Option<(Attrs, usize)> {
    read_attrs_at_with(bytes, start, last_close_brace, true)
}

/// `read_attrs_at`, with the interior's whitespace rule as a parameter.
///
/// `space_only` is TRUE for the inline production and FALSE for the
/// block-attribute LINE, which keeps `whitespace` at all three of its slots -
/// and that distinction is the ruling rather than an omission. The block line
/// is the one construct whose interior can hold a leading indentation run:
/// after a `continuation`, the next line's leading whitespace IS indentation,
/// and the rule that narrows the inline block is the same rule that protects
/// this one.
fn read_attrs_at_with(
    bytes: &[u8],
    start: usize,
    last_close_brace: Option<usize>,
    space_only: bool,
) -> Option<(Attrs, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    if last_close_brace.map_or(true, |p| p < start) {
        return None;
    }
    // Reject, in O(1) per opener, a payload that provably cannot parse before the
    // char walk below runs to a far `}`. Without this, a run of never-validating
    // openers whose only `}` lies far ahead (`[x]{`×n + `}`, `[x]{a `×n + `}`, …)
    // walks to that `}` AND re-parses the whole tail at every opener -- O(n^2).
    // The filter only reports "invalid" where `parse_attrs` would return `None`
    // too, so output is byte-identical.
    if attr_payload_provably_invalid(bytes, start) {
        return None;
    }
    let mut i = start + 1;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    while i < bytes.len() {
        // An inline attribute block is single-line (grammar): a newline before
        // the closing `}` means this is not an inline attr -- the `{` stays
        // literal (`[x]{.a\n.b}` is text). Matches carve-js. Block-attribute
        // lines, which may span lines, are read by a separate path.
        if bytes[i] == b'\n' {
            return None;
        }
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if let Some(q) = quote {
            if bytes[i] == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            quote = Some(bytes[i]);
            i += 1;
            continue;
        }
        if bytes[i] == b'}' {
            break;
        }
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let inner = std::str::from_utf8(&bytes[start + 1..i]).ok()?;
    Some((parse_attrs_with(inner, space_only)?, i + 1))
}

/// An attribute name (id, class, key) is a grammar identifier: it must start
/// with a letter or underscore (not a digit -- a `class="123"` / `id="1"` is
/// also invalid CSS). A name that fails this (including an empty one) makes
/// the whole block invalid, so it stays literal (§14). A digit after the
/// first character is fine. Stricter than djot (jgm/djot#399).
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_attrs(src: &str) -> Option<Attrs> {
    parse_attrs_with(src, true)
}

fn parse_attrs_with(src: &str, space_only: bool) -> Option<Attrs> {
    // A TAB AT ANY OF THE FIVE INLINE POSITIONS MAKES THE BLOCK UNRECOGNIZED,
    // and its braces show. One test rather than five, because all five are the
    // same question - is this interior character syntax or not - and narrowing
    // them one at a time is how the executable spec left `[x]{<TAB>}` a valid
    // EMPTY block after its separator had already been narrowed. Inside a
    // QUOTED value the character is content and does not move.
    if space_only {
        let mut quote: Option<char> = None;
        let mut escaped = false;
        for ch in src.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            match quote {
                Some(q) => {
                    if ch == '\\' {
                        escaped = true;
                    } else if ch == q {
                        quote = None;
                    }
                }
                None => match ch {
                    '\\' => escaped = true,
                    '"' | '\'' => quote = Some(ch),
                    c if c.is_whitespace() && c != ' ' => return None,
                    _ => {}
                },
            }
        }
    }
    if src.trim().is_empty() {
        return None;
    }
    let mut attrs = Attrs::default();
    for token in attr_tokens(src) {
        if let Some(tag) = token.strip_prefix(':') {
            if !is_language_tag(tag) {
                return None;
            }
            if !attrs.key_values.contains_key("lang") {
                attrs.order.push(AttrSlot::Key("lang".to_string()));
            }
            attrs.key_values.insert("lang".to_string(), tag.to_string());
        } else if let Some(id) = token.strip_prefix('#') {
            if !is_identifier(id) {
                return None;
            }
            if attrs.id.is_none() {
                attrs.order.push(AttrSlot::Id);
            }
            attrs.id = Some(id.to_string());
        } else if let Some(class) = token.strip_prefix('.') {
            if !is_identifier(class) {
                return None;
            }
            if attrs.classes.is_empty() {
                attrs.order.push(AttrSlot::Class);
            }
            attrs.classes.push(class.to_string());
        } else if let Some((key, value)) = token.split_once('=') {
            if !is_identifier(key) {
                return None;
            }
            if value.is_empty() {
                return None;
            }
            // A quoted value unescapes ANY backslash-escaped ASCII punctuation
            // (grammar: escaped_char = '\' ascii_punctuation), not just \" / \'.
            // Route it through the same scan link/image titles use, matching
            // carve-js / carve-php. A bare value carries no escapes.
            let value = if let Some(inner) = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            {
                unescape_title(inner)
            } else {
                value.to_string()
            };
            if key == "id" {
                // `id=value` is the same attribute as `#id`: it feeds the id
                // slot, last-wins (§15), instead of emitting a second `id="…"`
                // (invalid HTML). `id` never enters key_values, so a bare `id`
                // boolean (below) cannot leave a stale duplicate. Matches
                // carve-php.
                if attrs.id.is_none() {
                    attrs.order.push(AttrSlot::Id);
                }
                attrs.id = Some(value);
            } else {
                if !attrs.key_values.contains_key(key) {
                    attrs.order.push(AttrSlot::Key(key.to_string()));
                }
                attrs.key_values.insert(key.to_string(), value);
            }
        } else if is_identifier(&token) {
            if token == "id" {
                // A bare boolean `id` also feeds the id slot (value ""), last-wins
                // and single -- `{id id=j}` -> `id="j"`, `{id}` -> `id=""`.
                if attrs.id.is_none() {
                    attrs.order.push(AttrSlot::Id);
                }
                attrs.id = Some(String::new());
            } else {
                // Boolean attribute: a bare word with no value, rendered name="".
                // (Matched last so `k=v` is a key/value, not a bare `k`.)
                if !attrs.key_values.contains_key(&token) {
                    attrs.order.push(AttrSlot::Key(token.clone()));
                }
                attrs.key_values.insert(token, String::new());
            }
        } else {
            return None;
        }
    }
    Some(attrs)
}

fn is_language_tag(tag: &str) -> bool {
    tag.is_empty()
        || tag.split('-').all(|subtag| {
            !subtag.is_empty()
                && subtag.len() <= 8
                && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

fn attr_tokens(src: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in src.chars() {
        if escaped {
            buf.push('\\');
            buf.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            buf.push(ch);
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            buf.push(ch);
            continue;
        }
        // NO SPLIT ON A SIGIL. `attribute_list` is
        // `attribute, {space+, attribute}` (PART 7): a separator is required,
        // so `{.a.b}`, `{#i.c}` and `{.a#i}` are not attribute blocks and stay
        // literal. This branch used to break a class or id token at the next
        // `#` or `.`, manufacturing two attributes where the source has one
        // malformed one - `.a.b` now stays one token and fails `is_identifier`,
        // which is what makes the whole block literal.
        if ch.is_whitespace() {
            if !buf.is_empty() {
                tokens.push(std::mem::take(&mut buf));
            }
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }
    tokens
}

fn parse_standalone_attrs(line: &str) -> Option<Attrs> {
    let trimmed = trim_ascii(line);
    if !trimmed.starts_with('{') {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let last_close_brace = bytes.iter().rposition(|&b| b == b'}');
    let mut pos = 0usize;
    let mut attrs: Option<Attrs> = None;
    while pos < bytes.len() {
        // The block-attribute LINE, so `whitespace` at all three of its slots.
        let (incoming, next) = read_attrs_at_with(bytes, pos, last_close_brace, false)?;
        merge_attrs(&mut attrs, incoming);
        pos = next;
        if pos < bytes.len() && bytes[pos] != b'{' {
            return None;
        }
    }
    attrs
}

/// A standalone block-attribute block, possibly spanning several contiguous
/// (non-blank) lines: it opens with `{` and closes with `}` on a later line
/// (`{#id` / ` .foo}`). Consumes the lines and returns the parsed attributes,
/// or leaves the cursor untouched if it is not a valid attribute block.
fn parse_standalone_attrs_block(cur: &mut LineCursor) -> Option<Attrs> {
    let first = cur.peek()?;
    if !trim_ascii_start(first).starts_with('{') {
        return None;
    }
    if let Some(attrs) = parse_standalone_attrs(first) {
        cur.consume();
        return Some(attrs);
    }
    // See `standalone_attrs_block_len` for the cursor-free twin of everything
    // below; the two must keep answering the same way.
    // A COMPLETE single line (already closes with `}`) that parse_standalone_attrs
    // rejected is not a valid attribute block -- do NOT rescue it via the
    // multi-line strip-outer path below, which would parse an interior `}{` as an
    // unquoted value (`{k=v}{+i+}` -> k="v}{+i+", swallowing the whole line). The
    // multi-line join is only for a block that genuinely continues onto later
    // lines (`{#id` then `.foo}`), i.e. whose first line does not itself close.
    // Matches carve-js, which keeps such a line literal.
    if trim_ascii_end(first).ends_with('}') {
        return None;
    }
    // Multi-line: join contiguous lines until one closes with `}`.
    let mut joined = String::new();
    let mut count = 0usize;
    let mut quote: Option<char> = None;
    while let Some(line) = cur.lines.get(cur.pos + count).copied() {
        if is_blank_line(line) {
            return None;
        }
        if !joined.is_empty() {
            // A QUOTED VALUE STOPS AT THE NEWLINE (PART 4, carve#888). A line
            // break inside the quotes is not content: it ends the production,
            // and the whole attribute block is unrecognized.
            //
            // This engine collapsed the break to a SPACE, which no production
            // in either normative file describes - all three engines accepted
            // the shape and none of them agreed on what it meant, which is what
            // an unstated rule looks like.
            //
            // `continuation` is where a newline IS admitted, and it sits
            // BETWEEN two tokens rather than inside one, so a block attribute
            // may still span lines: the space below is that continuation, and
            // it is only reached where no value is open across the break.
            if quote.is_some() {
                return None;
            }
            joined.push(' ');
        }
        joined.push_str(trim_ascii(line));
        quote = open_quote_at_end(trim_ascii(line));
        count += 1;
        if trim_ascii_end(line).ends_with('}') {
            let inner = trim_ascii(&joined);
            if inner.starts_with('{') && inner.ends_with('}') {
                if let Some(attrs) = parse_attrs_with(&inner[1..inner.len() - 1], false) {
                    for _ in 0..count {
                        cur.consume();
                    }
                    return Some(attrs);
                }
            }
            return None;
        }
    }
    None
}

/// How many LINES a standalone attribute block occupies when one starts at
/// `lines[0]`, or `None` when these lines do not spell one.
///
/// The cursor-free twin of `parse_standalone_attrs_block`, for the predicates
/// that must ask "does an attribute block sit here" without a cursor to consume
/// from. Both follow the same rules: a single complete line, or a run joined
/// until one closes with `}`, refused at a blank line and refused when a quoted
/// value is left open across a break (PART 4, markup-carve/carve#888).
///
/// FLUSH-LEFT ONLY, like every caller's own guard: an indented `{...}` is lazy
/// paragraph text under the strict column-0 rule (PART 9 section 24 C3).
fn standalone_attrs_block_len(lines: &[&str]) -> Option<usize> {
    let first = *lines.first()?;
    if first.starts_with([' ', '\t']) {
        return None;
    }
    // AN ATTRIBUTE BLOCK OPENS WITH A BRACE, and refusing everything else here
    // is what keeps this cheap. `interrupts_paragraph` asks this question of
    // EVERY continuation line, and without the guard a paragraph whose lines
    // never end in `}` sends the join loop below over the whole remainder once
    // per line - quadratic time and allocation on ordinary prose that has no
    // attributes in it at all.
    if !trim_ascii(first).starts_with('{') {
        return None;
    }
    if parse_standalone_attrs(first).is_some() {
        return Some(1);
    }
    // Same refusal as the cursor form: a line that already closes and was
    // rejected is not rescued by the multi-line join.
    if trim_ascii_end(first).ends_with('}') {
        return None;
    }
    let mut joined = String::new();
    let mut quote: Option<char> = None;
    for (count, line) in lines.iter().enumerate() {
        if is_blank_line(line) {
            return None;
        }
        if !joined.is_empty() {
            if quote.is_some() {
                return None;
            }
            joined.push(' ');
        }
        joined.push_str(trim_ascii(line));
        quote = open_quote_at_end(trim_ascii(line));
        if trim_ascii_end(line).ends_with('}') {
            let inner = trim_ascii(&joined);
            if inner.starts_with('{')
                && inner.ends_with('}')
                && parse_attrs_with(&inner[1..inner.len() - 1], false).is_some()
            {
                return Some(count + 1);
            }
            return None;
        }
    }
    None
}

/// What a quoted line says about a standalone attribute block starting on it.
enum QuotedAttrsBlock {
    /// One starts here and spans this many QUOTED lines.
    Block(usize),
    /// None starts here, and none can start on any of the next `0..n` lines
    /// either: the scan walked that far without meeting a line that could CLOSE
    /// one. See [`quoted_attrs_block_len`] for why that generalizes.
    NoneWithin(usize),
    /// None starts here. A later line may still open one.
    No,
}

/// Whether a standalone attribute block starts on the quoted line `stripped`,
/// and how many QUOTED lines it takes.
///
/// `stripped` is the current line with ONE quote marker taken off, and `rest`
/// is the raw document from the next line on. Both are walked down to their
/// INNERMOST quoted content, exactly as [`ParaOpen::resolve`] does: a lazy line
/// continues the innermost open paragraph, so what an attribute block ends is
/// the innermost paragraph too, and a block written at depth 2 has to answer
/// like one written at depth 1 (markup-carve/carve#506).
///
/// THE BRACE GUARD COMES FIRST AND IS TESTED ON THE UNWALKED LINE. Every quoted
/// line reaches this, and the walk costs one strip per remaining quote marker -
/// which is precisely the per-line cost `ParaOpen` was built to defer
/// (markup-carve/carve-rs#731). A line with no `{` anywhere on it cannot open an
/// attribute block at any depth, so a quote ladder pays one `memchr` and stops.
///
/// AND THE SCAN REPORTS WHAT IT PROVED, so a run of brace-shaped lines that
/// close nothing is walked ONCE rather than once per line. A block ends at the
/// first line with a trailing `}`; if the walk reaches a blank line or the end
/// of the quoted run without meeting one, no line it passed can open a block
/// either, and `NoneWithin` hands the caller that window to skip. Without it a
/// quoted `{a` repeated n times pays an O(n) scan per line - a second quadratic
/// on top of the one the inner paragraph parse already has on that shape.
fn quoted_attrs_block_len<'a>(stripped: &'a str, rest: &[&'a str]) -> QuotedAttrsBlock {
    if !stripped.contains('{') {
        return QuotedAttrsBlock::No;
    }
    fn innermost(mut line: &str) -> &str {
        while let Some(next) = strip_blockquote_prefix(line) {
            line = next;
        }
        line
    }
    let first = innermost(stripped);
    // The single-line spelling is already answered by
    // `interrupts_paragraph_with_rest`, which `ParaOpen` consults. Only the
    // WRAPPED one needs the lookahead, and refusing a complete line here keeps
    // this from being a second answer to a question that already has one.
    if trim_ascii_end(first).ends_with('}') {
        return QuotedAttrsBlock::No;
    }
    let mut block: Vec<&str> = vec![first];
    let mut met_a_closer = false;
    for line in rest {
        // The block lives INSIDE the quote: an unprefixed line is the lazy
        // continuation the caller is deciding about, not part of the block.
        let Some(next) = strip_blockquote_prefix(line) else {
            break;
        };
        let inner = innermost(next);
        block.push(inner);
        if is_blank_line(inner) {
            break;
        }
        if trim_ascii_end(inner).ends_with('}') {
            met_a_closer = true;
            break;
        }
    }
    match standalone_attrs_block_len(&block) {
        Some(len) => QuotedAttrsBlock::Block(len),
        // A closer was there and the join was still refused - a later line may
        // open a block that ends on that same closer, so nothing is proved.
        None if met_a_closer => QuotedAttrsBlock::No,
        None => QuotedAttrsBlock::NoneWithin(block.len()),
    }
}

/// The quote a run leaves OPEN at its end.
///
/// A backslash escapes the next character, so `\"` neither opens a value nor
/// closes one. Used to decide whether a line break falls inside a
/// `quoted_value`, which PART 4 forbids in both of its alternatives
/// (carve#888).
///
/// The scan starts from NO open quote every time, and deliberately takes no
/// incoming state: a line that ends inside a value refuses the block at the
/// very next break, so no later line is ever scanned from inside one. A
/// carried-in state would be a parameter that cannot take a second value -
/// dead by construction and untestable, which is the shape this repository has
/// had to remove repeatedly.
fn open_quote_at_end(s: &str) -> Option<char> {
    let mut quote = None;
    let mut escaped = false;
    for ch in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(q) => {
                if ch == '\\' {
                    escaped = true;
                } else if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '\\' => escaped = true,
                '"' | '\'' => quote = Some(ch),
                _ => {}
            },
        }
    }
    quote
}

/// Split a TRAILING standalone block-attribute block off a collected chunk,
/// returning its attributes and shortening the chunk to the lines before it.
///
/// WHY A LIST ITEM NEEDS THIS AND NOTHING ELSE DOES. `parse_blocks` owns the
/// only pending-attribute slot, and it attaches a `{…}` line to the block that
/// follows it in the SAME stream. Inside a list item that stream is a chunk:
/// `collect_item_continuation_block_mapped` stops at a marker sitting at the
/// item's content column, so `parse_list` can own the sub-list and its
/// looseness bookkeeping. That stop puts the attribute line at the END of one
/// chunk and the nested list at the start of a different one, each parsed with
/// its own slot - so the attributes had nothing to attach to and were dropped
/// where `parse_blocks` returns (markup-carve/carve-rs#1007).
///
/// A paragraph, block quote or code fence in that position is not a marker, so
/// the collector never breaks and both lines land in one chunk, which is why
/// only a NESTED LIST lost them. Handing the split-off attributes to the
/// sub-list branch reunites the two halves without teaching `parse_blocks` to
/// return leftovers, which would thread a new value through every one of its
/// callers.
///
/// The block may span lines (`{#id` / `.foo}`), so the whole trailing run is
/// validated by `parse_standalone_attrs_block` - the same reader the top-level
/// path uses - and the split only happens when that reader consumes the run
/// exactly.
fn split_trailing_attrs(nested: &mut MappedSource) -> Option<(Attrs, Option<Pos>)> {
    let lines: Vec<&str> = nested.source.lines().collect();
    if lines.is_empty() {
        return None;
    }
    // A trailing attribute block ends on the last line, so that line must close
    // one. Anything else is ordinary content and there is nothing to split.
    if !trim_ascii_end(lines[lines.len() - 1]).ends_with('}') {
        return None;
    }
    // Walk back to the line that OPENS the run. A blank ends the search: a
    // block-attribute block is contiguous.
    let mut start = lines.len() - 1;
    loop {
        let line = lines[start];
        if is_blank_line(line) {
            return None;
        }
        // FLUSH LEFT, the same guard `parse_blocks` puts on this call
        // (`line_flush`). The chunk is already dedented by the item's content
        // column, so flush here means AT that column - and a line PAST it is
        // ordinary text, which is what corpus 87-compact-list-blocks-10 pins:
        // `- a` / blank / three spaces / `{.c}` is a second paragraph reading
        // `{.c}`, not an attribute block. Trimming the indent away instead
        // deleted that paragraph and re-tightened the item.
        if line.starts_with('{') {
            break;
        }
        if line.starts_with([' ', '\t']) {
            return None;
        }
        if start == 0 {
            return None;
        }
        start -= 1;
    }
    let tail: Vec<&str> = lines[start..].to_vec();
    let mut probe = LineCursor::new_with_cols(&tail, None, None);
    let attrs = parse_standalone_attrs_block(&mut probe)?;
    // Only a run the reader consumed WHOLE is the chunk's trailing attribute
    // block. A partial consume means the lines after it are content, and
    // splitting there would silently delete them.
    if !probe.eof() {
        return None;
    }
    // WHERE THE RUN WAS WRITTEN, for §15 A4's diagnostic - taken BEFORE the
    // maps are truncated below, since after that the split-off lines have no
    // entries left to read. A set lifted off a chunk and never placed is
    // dropped when the list ends or when a sibling marker opens
    // (markup-carve/carve#1281), and the drop is a finding wherever it happens.
    let pos = mapped_span_of(nested, start, lines.len(), &lines);
    let line_count = lines.len();
    let kept: Vec<&str> = lines[..start].to_vec();
    let source = kept.join("\n");
    nested.source = source;
    // Both maps are read by LINE INDEX, and the kept lines keep the indices
    // they had, so trailing entries are simply never reached - trimming them is
    // tidiness, not correctness. It is only safe on a map that is index-aligned
    // with the chunk: `push_newline_at` skips a LEADING run of unmapped lines
    // rather than storing `None` for them, so a shorter `line_map` is offset
    // from the lines and cutting it at `start` would drop an entry belonging to
    // a line being KEPT. Leave such a map alone; its extra entries cost nothing.
    if nested.col_map.len() == line_count {
        nested.col_map.truncate(start);
    }
    if nested.line_map.len() == line_count {
        nested.line_map.truncate(start);
    }
    Some((attrs, pos))
}

/// The document span of `lines[start..end]` inside a collected chunk.
///
/// [`span_of`] answers this for a [`LineCursor`], which carries the same two
/// maps under different names; a chunk that has already been split off has no
/// cursor over it, so the same arithmetic is done here. `None` when the chunk
/// carries no line map - positions are opt-in, and a span nobody asked for is
/// not worth inventing.
fn mapped_span_of(nested: &MappedSource, start: usize, end: usize, lines: &[&str]) -> Option<Pos> {
    let last = end.saturating_sub(1).max(start);
    let start_line = *nested.line_map.get(start)?;
    let start_line = start_line?;
    let end_line = nested
        .line_map
        .get(last)
        .copied()
        .flatten()
        .unwrap_or(start_line);
    let stripped = nested.col_map.get(start).copied().flatten().unwrap_or(0);
    let end_stripped = nested
        .col_map
        .get(last)
        .copied()
        .flatten()
        .unwrap_or(stripped);
    let indent = lines
        .get(start)
        .map(|l| l.chars().count() - trim_ascii_start(l).chars().count())
        .unwrap_or(0);
    let width = lines.get(last).map(|l| l.chars().count()).unwrap_or(0);
    Some(Pos {
        start_line,
        end_line,
        start_column: (stripped.max(0) as usize) + indent + 1,
        end_column: (end_stripped.max(0) as usize) + width + 1,
        start_offset: 0,
        end_offset: 0,
    })
}

fn merge_attrs(target: &mut Option<Attrs>, incoming: Attrs) {
    if target.is_none() {
        *target = Some(incoming);
        return;
    }
    let target = target.as_mut().unwrap();
    if incoming.id.is_some() {
        target.id = incoming.id;
    }
    target.classes.extend(incoming.classes);
    target.key_values.extend(incoming.key_values);
    // Merge the render order too: a later id/key overrides the value but keeps
    // its original slot position, so consecutive attribute lines emit in
    // first-appearance order (`{#a}` / `{k=v}` / `{.c}` -> id, then k, then
    // class). Without this only the last line's slots were rendered.
    for slot in incoming.order {
        if !target.order.contains(&slot) {
            target.order.push(slot);
        }
    }
}

/// Merge a leading block-attribute line onto a node that may already carry
/// its own opener attributes. Leading attrs are earlier in source, so their
/// classes precede the opener's and the opener wins on id/key conflict (§15).
fn merge_leading_attrs(target: &mut Option<Attrs>, leading: Attrs) {
    match target.take() {
        None => *target = Some(leading),
        Some(own) => {
            *target = Some(leading);
            merge_attrs(target, own);
        }
    }
}

/// Add a `data-source-line` attribute to a block node, preserving any existing
/// attributes. No-op for blocks that carry no attributes (raw block, comment,
/// abbreviation definition).
fn stamp_source_line(node: &mut BlockNode, line: usize) {
    let slot: Option<&mut Option<Attrs>> = match node {
        // §15 A2b puts a definition's attributes on its own line, not from a
        // preceding block-attribute line, so there is no slot to fill here.
        BlockNode::LinkReferenceDefinition(_) => None,
        BlockNode::Heading(n) => Some(&mut n.attrs),
        BlockNode::Paragraph(n) => Some(&mut n.attrs),
        BlockNode::ThematicBreak(n) => Some(&mut n.attrs),
        BlockNode::CodeBlock(n) => Some(&mut n.attrs),
        BlockNode::List(n) => Some(&mut n.attrs),
        BlockNode::BlockQuote(n) => Some(&mut n.attrs),
        BlockNode::Table(n) => Some(&mut n.attrs),
        BlockNode::Admonition(n) => Some(&mut n.attrs),
        BlockNode::Div(n) => Some(&mut n.attrs),
        BlockNode::LineBlock(n) => Some(&mut n.attrs),
        BlockNode::DefinitionList(n) => Some(&mut n.attrs),
        BlockNode::Figure(n) => Some(&mut n.attrs),
        BlockNode::FigureGroup(n) => Some(&mut n.attrs),
        BlockNode::Extension(n) => Some(&mut n.attrs),
        BlockNode::BlockImage(n) => Some(&mut n.attrs),
        // A citation definition's attributes are the metadata block on its own
        // line, not a preceding block-attribute line - same as the link
        // reference definition above.
        BlockNode::AbbreviationDef(_)
        | BlockNode::CitationDefinition(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_) => None,
    };
    let Some(opt) = slot else {
        return;
    };
    let attrs = opt.get_or_insert_with(Attrs::default);
    stamp_source_line_attr(attrs, line);
}

fn source_line_attrs(
    mut attrs: Option<Attrs>,
    line: Option<usize>,
    options: &Options<'_>,
) -> Option<Attrs> {
    if options.source_lines {
        if let Some(line) = line {
            let attrs = attrs.get_or_insert_with(Attrs::default);
            stamp_source_line_attr(attrs, line);
        }
    }
    attrs
}

fn stamp_source_line_attr(attrs: &mut Attrs, line: usize) {
    let key = "data-source-line";
    if !attrs.key_values.contains_key(key) {
        attrs.key_values.insert(key.to_string(), line.to_string());
        attrs.order.push(AttrSlot::Key(key.to_string()));
    }
}

fn apply_attrs_to_block(node: &mut BlockNode, attrs: Attrs) {
    match node {
        BlockNode::Heading(n) => n.attrs = Some(attrs),
        BlockNode::Paragraph(n) => n.attrs = Some(attrs),
        BlockNode::ThematicBreak(n) => n.attrs = Some(attrs),
        BlockNode::CodeBlock(n) => n.attrs = Some(attrs),
        BlockNode::List(n) => n.attrs = Some(attrs),
        BlockNode::BlockQuote(n) => n.attrs = Some(attrs),
        BlockNode::Table(n) => {
            n.columns = table_columns_from_attrs(&attrs);
            n.attrs = Some(attrs);
        }
        // A typed colon-fence opener may already carry its own attribute
        // block (`::: note {.x}`); a leading block-attribute line is earlier
        // in source, so its classes come first and the opener's win on
        // id/key conflict (§15) -- merge instead of clobbering.
        BlockNode::Admonition(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::Div(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::LineBlock(n) => merge_leading_attrs(&mut n.attrs, attrs),
        BlockNode::DefinitionList(n) => n.attrs = Some(attrs),
        BlockNode::Figure(n) => n.attrs = Some(attrs),
        // The bare opener carries nothing of its own (§4c), so the preceding
        // block-attribute line is the group's only attribute source.
        BlockNode::FigureGroup(n) => n.attrs = Some(attrs),
        BlockNode::Extension(n) => n.attrs = Some(attrs),
        // A direct block image (`{#id}\n![…](…)`) carries the leading attrs on
        // the `<img>` itself; the image's own inline attrs win on conflict (§15).
        BlockNode::BlockImage(img) => merge_leading_attrs(&mut img.attrs, attrs),
        _ => {}
    }
}

fn table_columns_from_attrs(attrs: &Attrs) -> Vec<TableColumn> {
    let split = |key: &str| {
        attrs
            .key_values
            .get(key)
            .map(|v| v.split(',').collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let aligns = split("aligns");
    let valigns = split("valigns");
    let widths = split("widths");
    let len = aligns.len().max(valigns.len()).max(widths.len());
    (0..len)
        .map(|i| TableColumn {
            align: aligns.get(i).and_then(|v| match *v {
                "left" => Some(TableAlign::Left),
                "right" => Some(TableAlign::Right),
                "center" => Some(TableAlign::Center),
                _ => None,
            }),
            valign: valigns.get(i).and_then(|v| match *v {
                "top" => Some(TableVerticalAlign::Top),
                "middle" => Some(TableVerticalAlign::Middle),
                "bottom" => Some(TableVerticalAlign::Bottom),
                _ => None,
            }),
            width: widths
                .get(i)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| *v > 0.0 && *v <= 100.0)
                .map(|v| v / 100.0),
        })
        .collect()
}

fn apply_attrs_to_inline(node: &mut InlineNode, attrs: Attrs) {
    match node {
        InlineNode::Emphasis(n) => n.attrs = Some(attrs),
        InlineNode::Link(n) => n.attrs = Some(attrs),
        InlineNode::Image(n) => n.attrs = Some(attrs),
        InlineNode::Span(n) => n.attrs = Some(attrs),
        InlineNode::Math(n) => n.attrs = Some(attrs),
        InlineNode::AutoLink(n) => n.attrs = Some(attrs),
        InlineNode::Extension(n) => n.attrs = Some(attrs),
        _ => {}
    }
}

/// Merge an attribute block onto an inline node, accumulating classes (§15)
/// instead of overwriting -- used for chained blocks (`[x]{.a}{.b}`).
fn merge_attrs_into_inline(node: &mut InlineNode, attrs: Attrs) {
    match node {
        InlineNode::Emphasis(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Link(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Image(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Span(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Math(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::AutoLink(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Extension(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Code(n) => merge_attrs(&mut n.attrs, attrs),
        // A trailing standalone block chains onto an inline literal, promoting a
        // bare literal to a `<span>` (`` !`x`{.a}{.b} `` -> class="a b"). Matches
        // carve-js, whose merge attaches to any non-text node.
        InlineNode::LiteralInline(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::Footnote(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::CriticInsert(n) => merge_attrs(&mut n.attrs, attrs),
        InlineNode::CriticDelete(n) => merge_attrs(&mut n.attrs, attrs),
        _ => {}
    }
}

/// Whether an inline node can carry an attribute block (so a following `{...}`
/// attaches rather than staying literal). Text/raw nodes cannot.
fn inline_is_attributable(node: &InlineNode) -> bool {
    matches!(
        node,
        InlineNode::Emphasis(_)
            | InlineNode::Link(_)
            | InlineNode::Image(_)
            | InlineNode::Span(_)
            | InlineNode::Math(_)
            | InlineNode::AutoLink(_)
            | InlineNode::Extension(_)
            | InlineNode::Code(_)
            | InlineNode::LiteralInline(_)
            | InlineNode::Footnote(_)
            | InlineNode::CriticInsert(_)
            | InlineNode::CriticDelete(_)
    )
}

fn try_extension_block(cur: &mut LineCursor, options: &Options<'_>) -> Option<BlockNode> {
    if options.extensions.is_empty() {
        return None;
    }
    let ctx = MatcherContext::new(options);
    for ext in &options.extensions {
        if let Some(BlockMatch {
            node,
            lines_consumed,
        }) = ext.match_block(cur.lines, cur.pos, &ctx)
        {
            if lines_consumed == 0 || cur.pos + lines_consumed > cur.lines.len() {
                continue;
            }
            cur.pos += lines_consumed;
            return Some(node);
        }
    }
    None
}

// ============================================================================
// Inline parsing
// ============================================================================

/// Number of distinct `X}` pair closers tracked in `InlineBounds::delim_brace`.
/// Covers the critic markers (`+ - ~ #`) and the forced-emphasis delimiters
/// (`/ * _ ^ , = ~`); `~` is shared between critic substitution/strike.
const DELIM_BRACE_SLOTS: usize = 10;

/// Slot in `InlineBounds::delim_brace` for the leading byte of an `X}` pair, or
/// `None` for a byte that never opens a tracked pair.
#[inline]
fn delim_brace_slot(b: u8) -> Option<usize> {
    Some(match b {
        b'+' => 0,
        b'-' => 1,
        b'~' => 2,
        b'#' => 3,
        b'/' => 4,
        b'*' => 5,
        b'_' => 6,
        b'^' => 7,
        b',' => 8,
        b'=' => 9,
        _ => return None,
    })
}

/// Precomputed, per-inline-text closer positions used to short-circuit the
/// per-position construct scanners in `parse_inline_context`. Each scanner needs
/// a specific closing delimiter somewhere ahead; if the last occurrence of that
/// closer lies before the candidate opener, the scan could only walk to
/// end-of-text and fail, so it is skipped in O(1). Without these bounds, a run
/// of unclosed openers (`{+`×n, `[^`×n, `[x]{`×n, `<`×n, …) forces every opener
/// to re-scan to EOF -- classic O(n^2). Skipping only ever elides a scan that
/// would have returned `None`, so output stays byte-identical.
struct InlineBounds<'a> {
    /// Matching `]` index for every `[` (see `compute_bracket_matches`); empty
    /// when the text contains no bracket construct trigger.
    matches: &'a [usize],
    /// Index of the last `)` (inline link/image destination closer).
    last_close_paren: Option<usize>,
    /// Index of the last `}` (attribute-block closer).
    last_close_brace: Option<usize>,
    /// Index of the last `]` (footnote-ref / inline-footnote / extension closer).
    last_close_bracket: Option<usize>,
    /// Index of the last `>` (crossref / autolink closer).
    last_gt: Option<usize>,
    /// For each tracked `X}` pair, the index of the leading `X` of its LAST
    /// occurrence (see `delim_brace_slot`). Used by critic markup and forced
    /// emphasis, whose closers are two-byte `X}` pairs.
    delim_brace: [Option<usize>; DELIM_BRACE_SLOTS],
}

pub(crate) struct InlineAnchor<'a> {
    lines: &'a [Option<(usize, isize)>],
    /// Byte offsets in the text at which a new anchor SEGMENT begins without a
    /// newline being there to mark it.
    ///
    /// A newline in the text is the ordinary way one line ends and the next
    /// begins, and it needs no list. A TABLE CELL rebuilt across a `+`
    /// continuation has no newline in it - the fragments are joined by a
    /// manufactured space - and yet each fragment sits on a different source
    /// line. Without this the whole cell reads as one line and every position
    /// past the first fragment lands on the row line at a column that holds
    /// something else.
    ///
    /// Each break opens a segment, so `lines` is indexed by segment rather than
    /// by newline count.
    ///
    /// A BREAK IS A GAP, which a newline is not. The source holds characters
    /// between two segments that the text does not - a closing pipe, a line
    /// break, a `+` and an opening pipe - so a span reaching across one would
    /// not select its own text. Any node that crosses a break is therefore left
    /// unplaced: absent beats wrong (PART 12 section 4). A newline needs no such
    /// rule because it is IN the text, so a span across it still selects itself.
    ///
    /// Offsets are ASCENDING and each one sits IN FRONT OF A CHARACTER, never
    /// at end of text - a segment with nothing in it would have no position to
    /// give.
    breaks: &'a [usize],
}

impl<'a> InlineAnchor<'a> {
    fn lines(lines: &'a [Option<(usize, isize)>]) -> Self {
        Self { lines, breaks: &[] }
    }
}

struct InlinePositionMap<'a> {
    lines: &'a [Option<(usize, isize)>],
    byte_line: Vec<usize>,
    byte_column: Vec<usize>,
    /// Whether the segments are separated by GAPS rather than by newlines -
    /// see `InlineAnchor::breaks`. A span across a gap carries no position.
    gapped: bool,
}

impl<'a> InlinePositionMap<'a> {
    fn new(text: &str, anchor: InlineAnchor<'a>) -> Self {
        let mut byte_line = vec![0usize; text.len() + 1];
        let mut byte_column = vec![0usize; text.len() + 1];
        let mut line = 0usize;
        let mut column = 0usize;
        let mut breaks = anchor.breaks.iter().copied().peekable();
        for (byte, ch) in text.char_indices() {
            // A break OPENS a segment, so it is applied BEFORE the character it
            // sits in front of rather than after the one behind it.
            while breaks.peek() == Some(&byte) {
                breaks.next();
                line += 1;
                column = 0;
            }
            byte_line[byte] = line;
            byte_column[byte] = column;
            for idx in byte + 1..byte + ch.len_utf8() {
                byte_line[idx] = line;
                byte_column[idx] = column;
            }
            if ch == '\n' {
                line += 1;
                column = 0;
            } else {
                column += 1;
            }
        }
        byte_line[text.len()] = line;
        byte_column[text.len()] = column;
        Self {
            lines: anchor.lines,
            byte_line,
            byte_column,
            gapped: !anchor.breaks.is_empty(),
        }
    }

    fn pos(&self, start: usize, end: usize) -> Option<Pos> {
        if start > end || end > self.byte_line.len().saturating_sub(1) {
            return None;
        }
        let start_line_idx = *self.byte_line.get(start)?;
        let end_line_idx = *self.byte_line.get(end)?;
        if self.gapped && start_line_idx != end_line_idx {
            return None;
        }
        for idx in start_line_idx..=end_line_idx {
            self.lines.get(idx).copied().flatten()?;
        }
        let (start_line, start_stripped) = self.lines.get(start_line_idx).copied().flatten()?;
        let (end_line, end_stripped) = self.lines.get(end_line_idx).copied().flatten()?;
        Some(Pos {
            start_line,
            end_line,
            start_column: document_column(start_stripped, self.byte_column[start]),
            end_column: document_column(end_stripped, self.byte_column[end]),
            start_offset: 0,
            end_offset: 0,
        })
    }
}

fn inline_pos(map: Option<&InlinePositionMap<'_>>, start: usize, end: usize) -> Option<Pos> {
    map.and_then(|m| m.pos(start, end))
}

fn set_inline_node_pos(node: &mut InlineNode, pos: Option<Pos>) {
    match node {
        InlineNode::Text(n) => n.pos = pos,
        InlineNode::EscapedText(n) => n.pos = pos,
        InlineNode::SmartPunctuation(n) => n.pos = pos,
        InlineNode::Emphasis(n) => n.pos = pos,
        InlineNode::Code(n) => n.pos = pos,
        InlineNode::Link(n) => n.pos = pos,
        InlineNode::Image(n) => n.pos = pos,
        InlineNode::Span(n) => n.pos = pos,
        InlineNode::Math(n) => n.pos = pos,
        InlineNode::RawInline(n) => n.pos = pos,
        InlineNode::LiteralInline(n) => n.pos = pos,
        InlineNode::Symbol(n) => n.pos = pos,
        InlineNode::AutoLink(n) => n.pos = pos,
        InlineNode::CrossRef(n) => n.pos = pos,
        InlineNode::CaptionNumber(n) => n.pos = pos,
        InlineNode::Mention(n) => n.pos = pos,
        InlineNode::Tag(n) => n.pos = pos,
        InlineNode::CitationGroup(n) => n.pos = pos,
        InlineNode::Extension(n) => n.pos = pos,
        InlineNode::Abbreviation(n) => n.pos = pos,
        InlineNode::Footnote(n) => n.pos = pos,
        InlineNode::SoftBreak(n) | InlineNode::HardBreak(n) => n.pos = pos,
        InlineNode::CriticInsert(n) => n.pos = pos,
        InlineNode::CriticDelete(n) => n.pos = pos,
        InlineNode::CriticSubstitute(n) => n.pos = pos,
        InlineNode::Comment(n) => n.pos = pos,
        InlineNode::CriticComment(n) => n.pos = pos,
    }
}

impl InlineBounds<'_> {
    /// True when a `]` occurs at or after `pos`.
    #[inline]
    fn has_bracket_from(&self, pos: usize) -> bool {
        self.last_close_bracket.is_some_and(|p| p >= pos)
    }

    /// True when a `>` occurs at or after `pos`.
    #[inline]
    fn has_gt_from(&self, pos: usize) -> bool {
        self.last_gt.is_some_and(|p| p >= pos)
    }

    /// True when an `X}` pair with leading byte `delim` occurs at or after
    /// `pos`.
    #[inline]
    fn has_delim_brace_from(&self, delim: u8, pos: usize) -> bool {
        delim_brace_slot(delim).is_some_and(|s| self.delim_brace[s].is_some_and(|p| p >= pos))
    }
}

pub(crate) fn parse_inline_with_options(text: &str, options: &Options<'_>) -> Vec<InlineNode> {
    parse_inline_context(text, options, false, false, None, 0)
}

fn parse_inline_with_anchor(
    text: &str,
    options: &Options<'_>,
    anchor: InlineAnchor<'_>,
) -> Vec<InlineNode> {
    if !options.positions {
        return parse_inline_with_options(text, options);
    }
    let map = InlinePositionMap::new(text, anchor);
    parse_inline_context(text, options, false, false, Some(&map), 0)
}

fn parse_caption_inline_with_options(
    text: &str,
    options: &Options<'_>,
    caption_context: bool,
) -> Vec<InlineNode> {
    parse_inline_context(text, options, caption_context, false, None, 0)
}

/// A caption's inline content, anchored to the source lines it was folded from.
///
/// Separate from `parse_inline_with_anchor` only because a caption may contain
/// a caption NUMBER placeholder and ordinary inline content may not, so the
/// two cannot share the flag.
fn parse_caption_inline_with_anchor(
    text: &str,
    options: &Options<'_>,
    lines: Vec<Option<(usize, isize)>>,
    caption_context: bool,
) -> Vec<InlineNode> {
    if !options.positions {
        return parse_caption_inline_with_options(text, options, caption_context);
    }
    let map = InlinePositionMap::new(text, InlineAnchor::lines(&lines));
    parse_inline_context(text, options, true, false, Some(&map), 0)
}

fn parse_inline_context(
    text: &str,
    options: &Options<'_>,
    mut caption_number_allowed: bool,
    in_footnote: bool,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Vec<InlineNode> {
    // Recursion cap (see MAX_NESTING_DEPTH). Nested links/spans/emphasis recurse
    // through here one frame per level; over the cap, keep the remaining text
    // literal rather than recursing further (prevents a stack-overflow abort on
    // input like `[[[[[…x]]]]]`). Shares the depth counter with block parsing.
    let Some(_depth) = DepthGuard::enter() else {
        return vec![InlineNode::text(text.to_string())];
    };
    let bytes = text.as_bytes();
    // A `[` only opens an inline link, reference link, or span when a `](`,
    // `][`, or `]{` follows (there is no bare shortcut-reference form). If none
    // occur, those attempts -- each an O(n) bracket scan -- can be skipped, so a
    // deeply nested run like `[[[[x]]]]` stays O(n) instead of O(n^2). Footnotes
    // (`[^...]`) are handled separately and cheaply gated on `[^`.
    let has_link_trigger = text.contains("](") || text.contains("][") || text.contains("]{");
    // Precompute every `[`-to-`]` match once (O(n)) so the per-`[` link /
    // reference / span / image parsers locate their closing bracket in O(1)
    // instead of re-scanning O(n) each. Without this, deeply nested balanced
    // links (`[[[...x]()]()...]`) are O(n^2). Only needed when bracket
    // constructs can actually fire.
    let has_brackets = has_link_trigger || text.contains("![");
    let bracket_matches = if has_brackets {
        compute_bracket_matches(bytes)
    } else {
        Vec::new()
    };
    // Last-occurrence positions of each mandatory closer, precomputed once so the
    // per-position scanners short-circuit in O(1) when their closer cannot lie
    // ahead (see InlineBounds). Each is gated on a cheap presence check; a
    // `rposition`/pair scan runs only when that byte actually appears. This is
    // what keeps runs of unclosed openers linear instead of O(n^2).
    let last_close_paren = if has_brackets {
        bytes.iter().rposition(|&b| b == b')')
    } else {
        None
    };
    let has_close_brace = text.contains('}');
    let last_close_brace = if has_close_brace {
        bytes.iter().rposition(|&b| b == b'}')
    } else {
        None
    };
    let last_close_bracket = if text.contains(']') {
        bytes.iter().rposition(|&b| b == b']')
    } else {
        None
    };
    let last_gt = if text.contains('>') {
        bytes.iter().rposition(|&b| b == b'>')
    } else {
        None
    };
    let last_delimited_comment_close = bytes.windows(2).rposition(|w| w == b"%}");
    // For each tracked `X}` pair (`+} -} ~} #}` for critic, plus the forced-
    // emphasis delimiters), record the leading byte's LAST position. Built only
    // when a `}` exists at all, since every such pair ends in `}`.
    let mut delim_brace: [Option<usize>; DELIM_BRACE_SLOTS] = [None; DELIM_BRACE_SLOTS];
    if has_close_brace {
        for p in 0..bytes.len().saturating_sub(1) {
            if bytes[p + 1] == b'}' {
                if let Some(slot) = delim_brace_slot(bytes[p]) {
                    delim_brace[slot] = Some(p);
                }
            }
        }
    }
    let bounds = InlineBounds {
        matches: &bracket_matches,
        last_close_paren,
        last_close_brace,
        last_close_bracket,
        last_gt,
        delim_brace,
    };
    // Per-delimiter memo of the earliest opener position from which the emphasis
    // closer scan already failed. Once an opener of a given delimiter finds no
    // valid closer to EOF, every later opener of that delimiter also fails, so
    // the scan is skipped in O(1). Keeps `_a](`×n / `*a](`×n linear. See
    // cached_find_emphasis_close.
    let mut emphasis_no_close: [Option<usize>; EMPHASIS_DELIM_SLOTS] = [None; EMPHASIS_DELIM_SLOTS];
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut buf_start: Option<usize> = None;
    let mut buf_placeable = true;
    let mut buf_src_delta: isize = 0;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];

        // Backslash escapes
        // A backslash at the very end of the content (no following byte) is a
        // hard break, mirroring the `\`-before-newline rule at end of input
        // (`para\` at EOF -> `<br>`), matching djot and the cheatsheet.
        if c == b'\\' && i + 1 >= bytes.len() {
            flush_text(
                &mut out,
                &mut buf,
                positions,
                base,
                &mut buf_start,
                &mut buf_placeable,
                &mut buf_src_delta,
            );
            out.push(InlineNode::HardBreak(Break {
                pos: inline_pos(positions, base + i, base + i + 1),
            }));
            i += 1;
            continue;
        }
        if c == b'\\' && i + 1 < bytes.len() {
            let nxt = bytes[i + 1];
            if nxt == b' ' {
                if buf_start.is_none() {
                    buf_start = Some(i);
                }
                // Two source bytes become one placeholder character, so the
                // buffer no longer measures the source. Record the difference
                // rather than refusing a position: the span covers exactly the
                // source this run came from, which is what the reference
                // publishes too. Only the VALUE differs from the slice, and a
                // slice holding a backslash is already exempt from that
                // comparison for this reason.
                buf_src_delta += 2 - crate::NBSP_PLACEHOLDER.len_utf8() as isize;
                buf.push(crate::NBSP_PLACEHOLDER);
                i += 2;
                continue;
            }
            if is_escapable(nxt) {
                // The escape is its own node: the backslash carries intent the
                // literal character does not (carve issue 350). The caret keeps
                // its placeholder inside the node's value, so the checks that
                // stop `\^` being read as a caption marker still see it.
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                out.push(InlineNode::EscapedText(EscapedText {
                    // The CHARACTER, not a marker standing in for it. PART 12
                    // section 1 requires mapping internals on the way out, and
                    // the node type already carries the only thing the marker
                    // was distinguishing: an `escaped_text` node IS an escape,
                    // so the writer emits a backslash plus this value without
                    // needing the value to say so again (carve-rs#408).
                    value: (nxt as char).to_string(),
                    pos: inline_pos(positions, base + i, base + i + 2),
                }));
                i += 2;
                continue;
            }
        }

        // Explicitly delimited inline comment. The first closer wins; without
        // one the opener remains ordinary visible text. Backslash escapes have
        // already been consumed, and code/raw spans consume their whole run.
        if c == b'{'
            && bytes.get(i + 1) == Some(&b'%')
            && last_delimited_comment_close.is_some_and(|close| close >= i + 2)
        {
            if let Some(close_rel) = bytes[i + 2..].windows(2).position(|w| w == b"%}") {
                let close = i + 2 + close_rel;
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let raw = String::from_utf8_lossy(&bytes[i + 2..close]);
                let without_leading = raw.strip_prefix(' ').unwrap_or(&raw);
                let content = without_leading
                    .strip_suffix(' ')
                    .unwrap_or(without_leading)
                    .to_string();
                out.push(InlineNode::Comment(Comment {
                    block: false,
                    delimited: true,
                    content,
                    pos: inline_pos(positions, base + i, base + close + 2),
                }));
                i = close + 2;
                continue;
            }
        }

        // Trailing line comment: `%%` at start of line or after whitespace runs
        // to end of line and is dropped (`text %% comment`).
        if c == b'%'
            && bytes.get(i + 1) == Some(&b'%')
            && (i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t' || bytes[i - 1] == b'\n')
        {
            // Popping from the END keeps the buffer equal to the source it
            // started at - `flush_text` measures the span as
            // `start .. start + buf.len()`, so a shorter buffer is a shorter
            // span, not a wrong one. Clearing `buf_placeable` here refused a
            // position for every line ending in a `%%` comment.
            while buf.ends_with(' ') || buf.ends_with('\t') {
                buf.pop();
            }
            let comment_start = i;
            match bytes[i..].iter().position(|&b| b == b'\n') {
                Some(p) => i += p,
                None => i = bytes.len(),
            }
            // The comment renders to nothing, but it is PUBLISHED: PART 12 has a
            // `comment` node and carve-js and carve-php both emit one here, so a
            // tree that records what the author wrote cannot drop it
            // (carve-rs#513). Flush the text before it so the node lands in
            // source order.
            flush_text(
                &mut out,
                &mut buf,
                positions,
                base,
                &mut buf_start,
                &mut buf_placeable,
                &mut buf_src_delta,
            );
            let content = String::from_utf8_lossy(&bytes[comment_start + 2..i])
                .trim_start()
                .to_string();
            out.push(InlineNode::Comment(Comment {
                block: false,
                delimited: false,
                content,
                pos: inline_pos(positions, base + comment_start, base + i),
            }));
            continue;
        }

        // Inline code spans
        if c == b'`' {
            if let Some((value, consumed)) = parse_inline_code(bytes, i) {
                if let Some((raw, raw_consumed)) =
                    parse_raw_inline_after_code(bytes, i, &value, consumed)
                {
                    flush_text(
                        &mut out,
                        &mut buf,
                        positions,
                        base,
                        &mut buf_start,
                        &mut buf_placeable,
                        &mut buf_src_delta,
                    );
                    let mut node = InlineNode::RawInline(raw);
                    set_inline_node_pos(
                        &mut node,
                        inline_pos(positions, base + i, base + i + raw_consumed),
                    );
                    out.push(node);
                    i += raw_consumed;
                    continue;
                }
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                // Push the bare code span. A trailing inline attribute block
                // (`` `code`{.cls} ``) is attached by the general attr-merge in
                // the main loop, which runs AFTER the forced-emphasis / critic
                // checks -- so `` `c`{_u_} `` is a code span + forced underline,
                // not a bogus `_u_` attribute. Matches carve-js / carve-php.
                out.push(InlineNode::Code(Code {
                    value,
                    attrs: None,
                    pos: inline_pos(positions, base + i, base + i + consumed),
                }));
                i += consumed;
                continue;
            }
        }

        if c == b'$' {
            if let Some((math, consumed)) = parse_math(bytes, i, &bounds) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::Math(math);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
        }

        if c == b'{' {
            if let Some((critic, consumed)) =
                parse_critic_markup(bytes, i, options, in_footnote, &bounds, positions, base)
            {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = critic;
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
            // Forced intraword emphasis `{X…X}` — tried before inline attribute
            // blocks, matching the reference scan order.
            if let Some((mut node, consumed)) =
                parse_forced_emphasis(bytes, i, options, in_footnote, &bounds, positions, base)
            {
                let mut consumed = consumed;
                // A trailing `{...}` attribute block attaches to the forced span,
                // exactly like a bare span (`{*x*}{.c}` -> <strong class="c">x</strong>).
                if bytes.get(i + consumed) == Some(&b'{') {
                    if let Some((attrs, next)) =
                        read_attrs_at(bytes, i + consumed, bounds.last_close_brace)
                    {
                        apply_attrs_to_inline(&mut node, attrs);
                        consumed = next - i;
                    }
                }
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
            // A standalone attribute block merges into the immediately preceding
            // inline node, so adjacent blocks chain (`[x]{.a}{.b}`,
            // `*x*{.a}{.b}` -> merged classes, §15). It must be GLUED: a
            // non-empty `buf` means text (e.g. a space) sits between the node
            // and the `{`, so the block stays literal. An empty/invalid `{...}`
            // also stays literal. Matches carve-php / carve-js.
            if buf.is_empty() && out.last().is_some_and(inline_is_attributable) {
                if let Some((attrs, next)) = read_attrs_at(bytes, i, bounds.last_close_brace) {
                    let last = out.last_mut().unwrap();
                    // A reference link carries `raw_ref` (its literal source) in
                    // case it stays unresolved. Merge the block into the link's
                    // attrs (used when it resolves) AND append the block's
                    // literal text to `raw_ref` (used when it reverts), so a
                    // resolved `[t][r]{.a}{.b}` gets class="a b" while an
                    // unresolved `[t][missing]{.a}{.b}` keeps both blocks literal.
                    if let InlineNode::Link(l) = last {
                        if let Some(raw) = l.raw_ref.as_mut() {
                            if let Ok(lit) = std::str::from_utf8(&bytes[i..next]) {
                                raw.push_str(lit);
                            }
                        }
                    }
                    merge_attrs_into_inline(last, attrs);
                    // An attached attribute block is owned by the construct it
                    // decorates, so extend the construct through the closing
                    // brace while retaining its original opening delimiter.
                    if let Some(attr_extent) = inline_pos(positions, base + i, base + next) {
                        if let Some(pos) = inline_pos_mut(last) {
                            pos.end_line = attr_extent.end_line;
                            pos.end_column = attr_extent.end_column;
                            pos.end_offset = attr_extent.end_offset;
                        }
                    }
                    i = next;
                    continue;
                }
            }
        }

        // Inline literal (§27): a `!` prefix on a verbatim code span, mirroring
        // the `$`-math prefix above. The span content is captured verbatim,
        // later HTML-escaped and emitted by every renderer with the `<code>`
        // wrapper dropped; a trailing `{…}` attaches below via the general
        // attr-merge as an ordinary inline attribute block (no special
        // first-token sigil). Like math it requires a CLOSED span — a bare `!`
        // before an unclosed run stays literal and the run becomes an ordinary
        // (unclosed) code span. Tried before the image case, which needs `[`.
        if c == b'!' && bytes.get(i + 1) == Some(&b'`') {
            if let Some((lit, consumed)) = parse_literal_inline(bytes, i) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::LiteralInline(lit);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
        }

        // Image: ![alt](src), then reference image ![alt][ref] / ![alt][].
        if c == b'!' && bytes.get(i + 1) == Some(&b'[') {
            if let Some((img, consumed)) = parse_image_at(bytes, i, &bounds) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::Image(img);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
            if let Some((img, consumed)) = parse_reference_image(bytes, i, &bounds) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::Image(img);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
        }

        // Inline link: [text](href)
        if c == b'[' {
            if !in_footnote {
                if let Some((footnote, consumed)) = parse_footnote_ref(bytes, i, &bounds) {
                    flush_text(
                        &mut out,
                        &mut buf,
                        positions,
                        base,
                        &mut buf_start,
                        &mut buf_placeable,
                        &mut buf_src_delta,
                    );
                    let mut node = InlineNode::Footnote(footnote);
                    set_inline_node_pos(
                        &mut node,
                        inline_pos(positions, base + i, base + i + consumed),
                    );
                    out.push(node);
                    i += consumed;
                    continue;
                }
            }
            if has_link_trigger {
                if let Some((link, consumed)) = parse_inline_link_with_options(
                    bytes,
                    i,
                    options,
                    in_footnote,
                    &bounds,
                    positions,
                    base,
                ) {
                    flush_text(
                        &mut out,
                        &mut buf,
                        positions,
                        base,
                        &mut buf_start,
                        &mut buf_placeable,
                        &mut buf_src_delta,
                    );
                    let mut node = InlineNode::Link(link);
                    set_inline_node_pos(
                        &mut node,
                        inline_pos(positions, base + i, base + i + consumed),
                    );
                    out.push(node);
                    i += consumed;
                    continue;
                }
                if let Some((link, consumed)) =
                    parse_reference_link(bytes, i, options, in_footnote, &bounds, positions, base)
                {
                    flush_text(
                        &mut out,
                        &mut buf,
                        positions,
                        base,
                        &mut buf_start,
                        &mut buf_placeable,
                        &mut buf_src_delta,
                    );
                    let mut node = InlineNode::Link(link);
                    set_inline_node_pos(
                        &mut node,
                        inline_pos(positions, base + i, base + i + consumed),
                    );
                    out.push(node);
                    i += consumed;
                    continue;
                }
                if let Some((span, consumed)) =
                    parse_span(bytes, i, options, in_footnote, &bounds, positions, base)
                {
                    flush_text(
                        &mut out,
                        &mut buf,
                        positions,
                        base,
                        &mut buf_start,
                        &mut buf_placeable,
                        &mut buf_src_delta,
                    );
                    let mut node = InlineNode::Span(span);
                    set_inline_node_pos(
                        &mut node,
                        inline_pos(positions, base + i, base + i + consumed),
                    );
                    out.push(node);
                    i += consumed;
                    continue;
                }
            }
        }

        if c == b'<' {
            if let Some((crossref, consumed)) = parse_crossref(text, i, &bounds) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::CrossRef(crossref);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
        }

        if c == b'@' {
            if let Some((mention, consumed)) = parse_mention(text, i) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::Mention(mention);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
        }

        if c == b'#' {
            if caption_number_allowed && !bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                out.push(InlineNode::CaptionNumber(CaptionNumber {
                    number: None,
                    pos: inline_pos(positions, base + i, base + i + 1),
                }));
                caption_number_allowed = false;
                i += 1;
                continue;
            }
            if let Some((tag, consumed)) = parse_tag(text, i) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::Tag(tag);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
        }

        if c == b'<' {
            if let Some((autolink, consumed)) = parse_autolink(text, i, &bounds) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::AutoLink(autolink);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
        }

        // Inline extension: :name[content]
        if c == b':' {
            if let Some((node, consumed)) =
                parse_inline_extension(bytes, i, options, in_footnote, &bounds, positions, base)
            {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::Extension(node);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
            if let Some((symbol, consumed)) = parse_symbol(text, i, &bounds) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::Symbol(symbol);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
        }

        // Smart typography (§8): parsed into AST nodes so renderers can choose
        // glyph output or source-preserving Carve output without rescanning.
        if let Some((nodes, consumed)) = parse_smart_punctuation_at(text, i, &buf, &out) {
            flush_text(
                &mut out,
                &mut buf,
                positions,
                base,
                &mut buf_start,
                &mut buf_placeable,
                &mut buf_src_delta,
            );
            let mut local = 0usize;
            for mut node in nodes {
                let width = smart_punctuation_source_width(&node);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i + local, base + i + local + width),
                );
                local += width;
                out.push(node);
            }
            i += consumed;
            continue;
        }

        // Inline footnote `^[content]`. A `^` anywhere else is literal text
        // (there is no bare superscript), so `^^[x]` is a literal `^` + a note.
        if !in_footnote && c == b'^' && bytes.get(i + 1) == Some(&b'[') {
            if let Some((footnote, consumed)) =
                parse_inline_footnote(bytes, i, options, &bounds, positions, base)
            {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                let mut node = InlineNode::Footnote(footnote);
                set_inline_node_pos(
                    &mut node,
                    inline_pos(positions, base + i, base + i + consumed),
                );
                out.push(node);
                i += consumed;
                continue;
            }
        }

        // Bold-italic, sub, highlight, then single-char emphasis
        if let Some((mut node, consumed)) = match_emphasis(
            bytes,
            i,
            options,
            in_footnote,
            &mut emphasis_no_close,
            positions,
            base,
        ) {
            let mut consumed = consumed;
            if bytes.get(i + consumed) == Some(&b'{') {
                if let Some((attrs, next)) =
                    read_attrs_at(bytes, i + consumed, bounds.last_close_brace)
                {
                    apply_attrs_to_inline(&mut node, attrs);
                    consumed = next - i;
                }
            }
            flush_text(
                &mut out,
                &mut buf,
                positions,
                base,
                &mut buf_start,
                &mut buf_placeable,
                &mut buf_src_delta,
            );
            set_inline_node_pos(
                &mut node,
                inline_pos(positions, base + i, base + i + consumed),
            );
            out.push(node);
            i += consumed;
            continue;
        }

        // Soft break
        if c == b'\n' {
            if buf.ends_with('\\') {
                buf.pop();
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                // `hard_break = '\', newline`, so the span covers BOTH
                // characters. Placing the newline alone left the backslash in
                // no node at all: a break reporting one character where the
                // construct is two, which nothing rendered differently and no
                // checker could see (carve#549).
                let backslash = if i > 0 && bytes[i - 1] == b'\\' {
                    i - 1
                } else {
                    i
                };
                out.push(InlineNode::HardBreak(Break {
                    pos: inline_pos(positions, base + backslash, base + i + 1),
                }));
                i += 1;
                continue;
            }
            flush_text(
                &mut out,
                &mut buf,
                positions,
                base,
                &mut buf_start,
                &mut buf_placeable,
                &mut buf_src_delta,
            );
            out.push(InlineNode::SoftBreak(Break {
                pos: inline_pos(positions, base + i, base + i + 1),
            }));
            i += 1;
            continue;
        }

        if let Some(InlineMatch { node, end }) = try_extension_inline(text, i, options) {
            // `end` must land on a char boundary or `text[i..]`/slicing panics;
            // a misbehaving extension matcher must not be able to crash the core.
            if end > i && end <= text.len() && text.is_char_boundary(end) {
                flush_text(
                    &mut out,
                    &mut buf,
                    positions,
                    base,
                    &mut buf_start,
                    &mut buf_placeable,
                    &mut buf_src_delta,
                );
                out.push(node);
                i = end;
                continue;
            }
        }

        let ch = text[i..].chars().next().unwrap();
        if buf_start.is_none() {
            buf_start = Some(i);
        }
        buf.push(ch);
        i += ch.len_utf8();
    }
    flush_text(
        &mut out,
        &mut buf,
        positions,
        base,
        &mut buf_start,
        &mut buf_placeable,
        &mut buf_src_delta,
    );
    out
}

fn parse_critic_markup(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(InlineNode, usize)> {
    // Match the two-byte opener on raw bytes -- validating `bytes[start..]` as
    // UTF-8 here would be O(n) at every `{`, i.e. O(n^2) over a run of critic
    // openers. Only the matched inner slice (up to the closing pair) is
    // validated, once a pair is located.
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let content_start = start + 2;
    // Each critic form closes on a two-byte `X}` pair; if that pair does not lie
    // ahead, the `find_seq` could only scan to end-of-text and fail, so bail in
    // O(1). Keeps `{+`×n / `{-`×n / `{~ }`×n (no closing pair) linear.
    match bytes.get(start + 1).copied()? {
        b'+' => {
            if !bounds.has_delim_brace_from(b'+', start) {
                return None;
            }
            let pair = find_seq(bytes, content_start, b"+}")?;
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            Some((
                InlineNode::CriticInsert(CriticInsert {
                    children: parse_inline_context(
                        inner,
                        options,
                        false,
                        in_footnote,
                        positions,
                        base + content_start,
                    ),
                    attrs: None,
                    pos: None,
                }),
                pair + 2 - start,
            ))
        }
        b'-' => {
            if !bounds.has_delim_brace_from(b'-', start) {
                return None;
            }
            let pair = find_seq(bytes, content_start, b"-}")?;
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            Some((
                InlineNode::CriticDelete(CriticDelete {
                    children: parse_inline_context(
                        inner,
                        options,
                        false,
                        in_footnote,
                        positions,
                        base + content_start,
                    ),
                    attrs: None,
                    pos: None,
                }),
                pair + 2 - start,
            ))
        }
        b'~' => {
            // A critic substitution is `{~old~>new~}`: the `~>` separator must
            // sit within this `{~ … ~}`. Without it (`{~view~}`), this is not
            // critic markup -- it falls through to forced strike emphasis.
            if !bounds.has_delim_brace_from(b'~', start) {
                return None;
            }
            let pair = find_seq(bytes, content_start, b"~}")?;
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            let sep = inner.find("~>")?;
            Some((
                InlineNode::CriticSubstitute(CriticSubstitute {
                    old_text: inner[..sep].to_string(),
                    new_text: inner[sep + 2..].to_string(),
                    pos: None,
                }),
                pair + 2 - start,
            ))
        }
        b'#' => {
            if !bounds.has_delim_brace_from(b'#', start) {
                return None;
            }
            let pair = find_seq(bytes, content_start, b"#}")?;
            let inner = std::str::from_utf8(&bytes[content_start..pair]).ok()?;
            Some((
                InlineNode::CriticComment(CriticComment {
                    text: inner.to_string(),
                    pos: None,
                }),
                pair + 2 - start,
            ))
        }
        _ => None,
    }
}

fn parse_footnote_ref(
    bytes: &[u8],
    start: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(Footnote, usize)> {
    if bytes.get(start) != Some(&b'[') || bytes.get(start + 1) != Some(&b'^') {
        return None;
    }
    // The id runs to the closing `]`; with no `]` ahead the scan could only walk
    // to end-of-text and fail, so bail in O(1) (keeps `[^`×n linear).
    if !bounds.has_bracket_from(start) {
        return None;
    }
    let mut i = start + 2;
    // A label is a physical-line identifier. Definition markers cannot cross
    // a newline, so such a reference could never bind to a valid definition.
    while i < bytes.len() && bytes[i] != b']' && bytes[i] != b'\n' && bytes[i] != b'\r' {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b']' {
        return None;
    }
    let id = std::str::from_utf8(&bytes[start + 2..i]).ok()?.to_string();
    // Same production, reference side: `[^]` is not a `reference_footnote`.
    // Carve has no shortcut reference either, so it is literal text - which is
    // what it already was here until a `[^]: …` line existed for it to bind to.
    if id.is_empty() {
        return None;
    }
    let mut attrs = None;
    let mut after = i + 1;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Footnote {
            attrs,
            id: Some(id),
            inline: None,
            number: None,
            ref_id: None,
            pos: None,
        },
        after - start,
    ))
}

fn parse_inline_footnote(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(Footnote, usize)> {
    if bytes.get(start) != Some(&b'^') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    // The body runs to a balancing `]`; with no `]` ahead the bracket scan could
    // only walk to end-of-text and fail, so bail in O(1) (keeps `^[`×n linear).
    if !bounds.has_bracket_from(start) {
        return None;
    }
    let (content, after_bracket) = read_bracketed(bytes, start + 1)?;
    if content.trim().is_empty() {
        return None;
    }
    let mut attrs = None;
    let mut after = after_bracket;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    let children =
        parse_inline_context(&content, options, false, true, positions, base + start + 2);
    Some((
        Footnote {
            attrs,
            id: None,
            inline: Some(children),
            number: None,
            ref_id: None,
            pos: None,
        },
        after - start,
    ))
}

fn parse_raw_inline_after_code(
    bytes: &[u8],
    start: usize,
    value: &str,
    code_consumed: usize,
) -> Option<(RawInline, usize)> {
    let attr_start = start + code_consumed;
    if bytes.get(attr_start) != Some(&b'{') || bytes.get(attr_start + 1) != Some(&b'=') {
        return None;
    }
    let mut i = attr_start + 2;
    let format_start = i;
    while i < bytes.len() && bytes[i] != b'}' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    if format_start == i {
        return None;
    }
    let format = std::str::from_utf8(&bytes[format_start..i]).ok()?;
    // The format must be a valid `format_name` (an identifier: letter/`_`
    // start, then letter/digit/`_`/`-`), per grammar §20. Anything else
    // (`{=h=}`, `{==h==}`, `{=text/html}`) is NOT a raw inline -- it falls back
    // to a plain code span plus forced-emphasis / literal text, matching
    // carve-js / carve-php. Without this rs greedily consumed the code span and
    // dropped its content for a bogus format.
    let mut fc = format.bytes();
    let valid = matches!(fc.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
        && fc.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !valid {
        return None;
    }
    Some((
        RawInline {
            format: format.to_string(),
            content: value.to_string(),
            injected: false,
            pos: None,
        },
        i + 1 - start,
    ))
}

/// Inline literal (`` !`…` ``, grammar PART 9 §27, `literal_inline = '!',
/// code_span`). A `!` PREFIX on a verbatim code span, mirroring `parse_math`'s
/// `$` prefix: the maximal backtick run captures the content verbatim, which is
/// later HTML-escaped and emitted by every renderer with the `<code>` wrapper
/// dropped. A CLOSED span is required — a `!` before an unclosed run returns
/// `None`, leaving the `!` literal and the run to become an ordinary (unclosed)
/// code span, exactly as `$` before an unclosed run behaves.
///
/// Returns a bare literal; a trailing `{…}` is the ORDINARY inline attribute
/// block and is attached by the general attr-merge in the scanner (same path a
/// bare code span uses), so `` !`x`{.ipa} `` and chained `` !`x`{.a}{.b} ``
/// both work without any special first-token handling here.
fn parse_literal_inline(bytes: &[u8], start: usize) -> Option<(LiteralInline, usize)> {
    if bytes.get(start) != Some(&b'!') || bytes.get(start + 1) != Some(&b'`') {
        return None;
    }
    let tick = start + 1;
    // Require a CLOSED span, like `$`-math in carve-js. `parse_inline_code`
    // itself accepts an unclosed opener (consuming to the end of the block), so
    // the closedness is checked explicitly here: a `!` before an unclosed run
    // stays literal and the run becomes an ordinary (unclosed) code span.
    if !inline_code_is_closed(bytes, tick) {
        return None;
    }
    let (content, code_consumed) = parse_inline_code(bytes, tick)?;
    Some((
        LiteralInline {
            content,
            attrs: None,
            pos: None,
        },
        tick + code_consumed - start,
    ))
}

/// True iff a verbatim code span opening at `start` (a backtick) has a matching
/// equal-length closing run — i.e. it is CLOSED rather than an opener that runs
/// unclosed to the end of the block. Used to gate the inline literal (§27) to
/// closed spans only, matching the `$`-math prefix.
fn inline_code_is_closed(bytes: &[u8], start: usize) -> bool {
    let mut i = start;
    while i < bytes.len() && bytes[i] == b'`' {
        i += 1;
    }
    let open_len = i - start;
    if open_len == 0 {
        return false;
    }
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let close_start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        if i - close_start == open_len {
            return true;
        }
    }
    false
}

fn parse_math(bytes: &[u8], start: usize, bounds: &InlineBounds<'_>) -> Option<(Math, usize)> {
    let display = bytes.get(start + 1) == Some(&b'$');
    let tick = if display { start + 2 } else { start + 1 };
    if bytes.get(tick) != Some(&b'`') {
        return None;
    }
    // Math reuses a code span for its verbatim body (grammar `math_inline =
    // '$', code_span`): a MAXIMAL backtick run opens and an equal-length run
    // closes, so `$``a``` and `$`a``b`` behave like the code span `` `a``b` ``.
    let (content, code_consumed) = parse_inline_code(bytes, tick)?;
    // Empty verbatim content is NOT math (`$``` / `$$```): the `$` stays literal
    // and the backtick pair is an empty code span. Matches carve-js / carve-php.
    if content.is_empty() {
        return None;
    }
    let end = tick + code_consumed;
    // A trailing attribute block attaches to the math span (math reuses the
    // code-span attribute slot), EXCEPT `{=format}`, the raw-inline form,
    // which is code-span-only and not inherited by math -- leave it literal.
    let mut attrs = None;
    let mut after = end;
    if bytes.get(end) == Some(&b'{') && bytes.get(end + 1) != Some(&b'=') {
        if let Some((parsed, next)) = read_attrs_at(bytes, end, bounds.last_close_brace) {
            attrs = Some(parsed);
            after = next;
        }
    }
    Some((
        Math {
            attrs,
            display,
            content,
            pos: None,
        },
        after - start,
    ))
}

fn parse_reference_link(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(Link, usize)> {
    let text_close = bracketed_close(bytes, start, bounds.matches)?;
    let after_text = text_close + 1;
    if bytes.get(after_text) != Some(&b'[') {
        return None;
    }
    let label_close = bracketed_close(bytes, after_text, bounds.matches)?;
    let after_label = label_close + 1;
    // Both brackets are present, so materializing their labels now costs O(1)
    // per accepted reference rather than per candidate `[`.
    let text = std::str::from_utf8(&bytes[start + 1..text_close])
        .ok()?
        .to_string();
    let label = std::str::from_utf8(&bytes[after_text + 1..label_close])
        .ok()?
        .to_string();
    let ref_label = if label.is_empty() {
        text.clone()
    } else {
        label
    };
    // A trailing attribute block attaches to the resolved link, the same
    // slot an inline link uses (`[t][x]{.c}`).
    let mut attrs = None;
    let mut after = after_label;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Link {
            attrs,
            href: String::new(),
            title: None,
            children: parse_inline_context(
                &text,
                options,
                false,
                in_footnote,
                positions,
                base + start + 1,
            ),
            ref_label: Some(ref_label),
            // `raw_ref` is the literal source emitted only when the
            // reference does not resolve; it must include the consumed
            // attribute block so an unresolved `[t][x]{.c}` stays fully
            // literal rather than silently dropping the `{.c}`. A resolved
            // reference ignores `raw_ref` and applies `attrs` instead.
            raw_ref: Some(std::str::from_utf8(&bytes[start..after]).ok()?.to_string()),
            from_crossref: false,
            from_heading_reference: false,
            pos: None,
        },
        after - start,
    ))
}

fn flush_text(
    out: &mut Vec<InlineNode>,
    buf: &mut String,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
    buf_start: &mut Option<usize>,
    buf_placeable: &mut bool,
    buf_src_delta: &mut isize,
) {
    if !buf.is_empty() {
        let start = buf_start.take().unwrap_or(0);
        // The span ends where the SOURCE run ends, which is not
        // `start + buf.len()` whenever a substitution made the buffer a
        // different length than the bytes it came from. `buf_src_delta` carries
        // that difference so the end stays anchored to the source; without it a
        // run holding one no-break-space escape reported a span one byte short.
        let end =
            (start as isize + buf.len() as isize + *buf_src_delta).max(start as isize) as usize;
        out.push(InlineNode::Text(Text {
            value: std::mem::take(buf),
            pos: (*buf_placeable)
                .then(|| inline_pos(positions, base + start, base + end))
                .flatten(),
        }));
    }
    *buf_start = None;
    *buf_placeable = true;
    *buf_src_delta = 0;
}

fn parse_smart_punctuation_at(
    text: &str,
    i: usize,
    buf: &str,
    out: &[InlineNode],
) -> Option<(Vec<InlineNode>, usize)> {
    let prev = if buf.is_empty() {
        last_emitted_glyph(out)
    } else {
        buf.chars().last().unwrap_or_default()
    };
    if text.as_bytes().get(i) == Some(&b'-') && text.as_bytes().get(i + 1) == Some(&b'-') {
        let n = text.as_bytes()[i..]
            .iter()
            .take_while(|&&b| b == b'-')
            .count();
        let glyphs = crate::render::allocate_dashes(n);
        let mut consumed = 0usize;
        let mut nodes = Vec::new();
        for glyph in glyphs.chars() {
            let (kind, width) = if glyph == '—' {
                ("em_dash", 3)
            } else {
                ("en_dash", 2)
            };
            nodes.push(InlineNode::SmartPunctuation(SmartPunctuation {
                kind: kind.to_string(),
                value: text[i + consumed..i + consumed + width].to_string(),
                glyph: None,
                pos: None,
            }));
            consumed += width;
        }
        return Some((nodes, n));
    }

    for (source, kind) in [
        ("<->", "left_right_arrow"),
        ("(tm)", "trademark"),
        ("...", "ellipsis"),
        ("->", "rightwards_arrow"),
        ("<-", "leftwards_arrow"),
        ("=>", "rightwards_double_arrow"),
        ("<=", "less_than_or_equal"),
        (">=", "greater_than_or_equal"),
        ("!=", "not_equal"),
        ("+-", "plus_minus"),
        ("(c)", "copyright"),
        ("(r)", "registered"),
    ] {
        if text[i..].starts_with(source) {
            return Some((
                vec![InlineNode::SmartPunctuation(SmartPunctuation {
                    kind: kind.to_string(),
                    value: source.to_string(),
                    glyph: None,
                    pos: None,
                })],
                source.len(),
            ));
        }
    }

    let c = text[i..].chars().next()?;
    if c == '"' {
        let open = quote_open_context(prev);
        let glyph = if open { "“" } else { "”" };
        let kind = if open {
            "left_double_quote"
        } else {
            "right_double_quote"
        };
        return Some((
            vec![InlineNode::SmartPunctuation(SmartPunctuation {
                kind: kind.to_string(),
                value: "\"".to_string(),
                glyph: Some(glyph.to_string()),
                pos: None,
            })],
            1,
        ));
    }
    if c == '\'' {
        let next_digit = text
            .as_bytes()
            .get(i + 1)
            .is_some_and(|b| b.is_ascii_digit());
        let prev_alnum = prev.is_alphanumeric();
        let apostrophe = prev_alnum || next_digit || !quote_open_context(prev);
        let glyph = if apostrophe { "’" } else { "‘" };
        let kind = if apostrophe {
            "right_single_quote"
        } else {
            "left_single_quote"
        };
        return Some((
            vec![InlineNode::SmartPunctuation(SmartPunctuation {
                kind: kind.to_string(),
                value: "'".to_string(),
                glyph: Some(glyph.to_string()),
                pos: None,
            })],
            1,
        ));
    }

    None
}

fn smart_punctuation_source_width(node: &InlineNode) -> usize {
    match node {
        InlineNode::SmartPunctuation(s) => s.value.len(),
        _ => 0,
    }
}

fn last_emitted_glyph(out: &[InlineNode]) -> char {
    match out.last() {
        Some(InlineNode::SmartPunctuation(node)) => {
            smart_punctuation_glyph(node).chars().last().unwrap_or('x')
        }
        // An escaped character is its own node but still the character before
        // the quote, and quote flanking reads that character: `\{"quoted"`
        // opens on the brace exactly as an unescaped `{` would (corpus 163).
        Some(InlineNode::EscapedText(t)) => t.value.chars().last().unwrap_or('x'),
        None => '\0',
        Some(_) => 'x',
    }
}

fn quote_open_context(prev: char) -> bool {
    prev == '\0'
        || prev.is_whitespace()
        || prev == crate::NBSP_PLACEHOLDER
        || matches!(
            prev,
            '(' | '[' | '{' | '=' | ':' | '-' | '/' | '–' | '—' | '“' | '‘'
        )
}

fn is_escapable(b: u8) -> bool {
    matches!(
        b,
        b'\\'
            | b'`'
            | b'*'
            | b'_'
            | b'{'
            | b'}'
            | b'['
            | b']'
            | b'('
            | b')'
            | b'"'
            | b'\''
            | b'#'
            | b'+'
            | b'-'
            | b'.'
            | b'!'
            | b'~'
            | b'^'
            | b'/'
            | b'<'
            | b'>'
            | b'@'
            | b'%'
            | b'|'
            | b'='
            | b','
            | b':'
            | b';'
            | b'$'
            | b'&'
            | b'?'
    )
}

// The closed-verbatim-span single-space strip: one leading and one trailing
// space are removed when the content BOTH begins and ends with a space -- but
// NOT when it consists entirely of spaces. The all-space guard matches the
// executable spec's `codeText()` and the CommonMark rule it derives from
// ("...but does not consist entirely of space characters"). Without the guard
// `` `  ` `` would strip to the empty string, and an empty verbatim span has no
// representable Carve source (a bare `` `` `` reparses as a two-backtick
// opener), so `carve fmt` could not round-trip it. Shared by the closed and
// unclosed verbatim paths so the two cannot drift apart.
fn strip_verbatim_padding(raw: &str) -> &str {
    if raw.starts_with(' ') && raw.ends_with(' ') && !raw.chars().all(|c| c == ' ') {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}

fn parse_inline_code(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    // Count opening backticks
    let mut i = start;
    while i < bytes.len() && bytes[i] == b'`' {
        i += 1;
    }
    let open_len = i - start;
    if open_len == 0 {
        return None;
    }
    let content_start = i;
    // Find closing run of exactly `open_len` backticks not followed by another backtick
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let close_start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        let close_len = i - close_start;
        if close_len == open_len {
            let raw = std::str::from_utf8(&bytes[content_start..close_start]).ok()?;
            return Some((strip_verbatim_padding(raw).to_string(), i - start));
        }
        // Different length closer — keep scanning past it
    }
    // No matching closer: an unclosed verbatim opener is opaque to the end of
    // the text (matches djot / carve-php / carve-js).
    let raw = std::str::from_utf8(&bytes[content_start..]).ok()?;
    Some((strip_verbatim_padding(raw).to_string(), bytes.len() - start))
}

fn parse_image_at(bytes: &[u8], start: usize, bounds: &InlineBounds<'_>) -> Option<(Image, usize)> {
    if bytes.get(start) != Some(&b'!') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    let alt_close = bracketed_close(bytes, start + 1, bounds.matches)?;
    let after_alt = alt_close + 1;
    if bytes.get(after_alt) != Some(&b'(') {
        return None;
    }
    let (src, title, after_paren) =
        read_link_target(bytes, after_alt + 1, bounds.last_close_paren)?;
    // Only a valid `(target)` reaches here, so the alt copy is deferred off the
    // failing-`![...]()` path that would otherwise be O(n) per position.
    let alt = std::str::from_utf8(&bytes[start + 2..alt_close])
        .ok()?
        .to_string();
    let mut attrs = None;
    let mut after = after_paren;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Image {
            attrs,
            src,
            alt,
            title,
            ref_label: None,
            raw_ref: None,
            pos: None,
        },
        after - start,
    ))
}

/// Parse a reference image `![alt][ref]` / collapsed `![alt][]` — the image
/// form of a reference link, mirroring `parse_reference_link`. `src` is empty
/// until `resolve_reference_links` fills it from the matching `[label]: url`
/// def; the full form allows an empty alt (label = ref), collapsed needs a
/// non-empty alt (label = alt).
fn parse_reference_image(
    bytes: &[u8],
    start: usize,
    bounds: &InlineBounds<'_>,
) -> Option<(Image, usize)> {
    if bytes.get(start) != Some(&b'!') || bytes.get(start + 1) != Some(&b'[') {
        return None;
    }
    let alt_close = bracketed_close(bytes, start + 1, bounds.matches)?;
    let after_alt = alt_close + 1;
    if bytes.get(after_alt) != Some(&b'[') {
        return None;
    }
    let label_close = bracketed_close(bytes, after_alt, bounds.matches)?;
    let after_label = label_close + 1;
    let alt = std::str::from_utf8(&bytes[start + 2..alt_close])
        .ok()?
        .to_string();
    let label = std::str::from_utf8(&bytes[after_alt + 1..label_close])
        .ok()?
        .to_string();
    // Collapsed `![alt][]` reuses the alt as the label, so it needs a non-empty
    // alt; the full `![alt][ref]` form accepts an empty alt (label = ref).
    if label.is_empty() && alt.is_empty() {
        return None;
    }
    let ref_label = if label.is_empty() { alt.clone() } else { label };
    let mut attrs = None;
    let mut after = after_label;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Image {
            attrs,
            src: String::new(),
            alt,
            title: None,
            ref_label: Some(ref_label),
            raw_ref: Some(std::str::from_utf8(&bytes[start..after]).ok()?.to_string()),
            pos: None,
        },
        after - start,
    ))
}

fn parse_inline_link_with_options(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(Link, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let text_close = bracketed_close(bytes, start, bounds.matches)?;
    let after_bracket = text_close + 1;
    if bytes.get(after_bracket) != Some(&b'(') {
        return None;
    }
    let (href, title, after_paren) =
        read_link_target(bytes, after_bracket + 1, bounds.last_close_paren)?;
    // The label is copied only once a valid `(target)` follows; the failing
    // `[...]()` path (empty target) returns above without allocating, which is
    // what keeps `[[[...x]()]()...]()` linear instead of quadratic.
    let text = std::str::from_utf8(&bytes[start + 1..text_close])
        .ok()?
        .to_string();
    let mut attrs = None;
    let mut after = after_paren;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        Link {
            attrs,
            href,
            title,
            children: parse_inline_context(
                &text,
                options,
                false,
                in_footnote,
                positions,
                base + start + 1,
            ),
            ref_label: None,
            raw_ref: None,
            from_crossref: false,
            from_heading_reference: false,
            pos: None,
        },
        after - start,
    ))
}

fn parse_inline_extension(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(InlineExtension, usize)> {
    if bytes.get(start) != Some(&b':') {
        return None;
    }
    // The content runs to the first `]`; with no `]` ahead the scan could only
    // walk to end-of-text and fail, so bail in O(1) (keeps `:a[`×n linear).
    if !bounds.has_bracket_from(start) {
        return None;
    }
    let mut i = start + 1;
    let name_start = i;
    // `extension_name = identifier`: must start with a letter or `_` -- a
    // digit-first name (`:1[x]`) is invalid and the whole construct stays
    // literal. (`:a1[x]` is fine; digits are allowed after the first char.)
    match bytes.get(i) {
        Some(b) if b.is_ascii_alphabetic() || *b == b'_' => {}
        _ => return None,
    }
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
    {
        i += 1;
    }
    if i == name_start || bytes.get(i) != Some(&b'[') {
        return None;
    }
    let name = std::str::from_utf8(&bytes[name_start..i]).ok()?.to_string();
    // `extension_content = {character - ']'}`: the content runs to the FIRST
    // unescaped `]` and does not balance nested brackets (`:foo[a [b] c]` ->
    // `<span class="ext-foo">a [b</span> c]`).
    let (content, after_bracket) = read_to_first_bracket(bytes, i)?;
    let mut attrs = None;
    let mut after = after_bracket;
    if bytes.get(after) == Some(&b'{') {
        if let Some((parsed_attrs, next)) = read_attrs_at(bytes, after, bounds.last_close_brace) {
            attrs = Some(parsed_attrs);
            after = next;
        }
    }
    Some((
        InlineExtension {
            attrs,
            name,
            children: parse_inline_context(
                &content,
                options,
                false,
                in_footnote,
                positions,
                base + i + 1,
            ),
            pos: None,
        },
        after - start,
    ))
}

fn parse_span(
    bytes: &[u8],
    start: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(Span, usize)> {
    let content_close = bracketed_close(bytes, start, bounds.matches)?;
    let after_bracket = content_close + 1;
    if bytes.get(after_bracket) != Some(&b'{') {
        return None;
    }
    let (attrs, after_attrs) = read_attrs_at(bytes, after_bracket, bounds.last_close_brace)
        .or_else(|| read_empty_attrs_at(bytes, after_bracket))?;
    // Absorb a CHAIN of adjacent attribute blocks (`[x]{.a}{.b}` ->
    // class="a b"), accumulating classes (§15). A non-attribute `{...}` (e.g.
    // an empty `{}`) reads as None and is left literal, so `[x]{}{}` keeps the
    // trailing `{}` -- matching carve-php / carve-js.
    let mut attrs = Some(attrs);
    let mut after_attrs = after_attrs;
    while bytes.get(after_attrs) == Some(&b'{') {
        match read_attrs_at(bytes, after_attrs, bounds.last_close_brace) {
            Some((more, next)) => {
                merge_attrs(&mut attrs, more);
                after_attrs = next;
            }
            None => break,
        }
    }
    // Only a valid `{attrs}` follow reaches here, so the content copy stays off
    // the failing `[...]` path (e.g. `[...]()` never gets past the `{` check).
    let content = std::str::from_utf8(&bytes[start + 1..content_close])
        .ok()?
        .to_string();
    Some((
        Span {
            attrs,
            injected: false,
            children: parse_inline_context(
                &content,
                options,
                false,
                in_footnote,
                positions,
                base + start + 1,
            ),
            pos: None,
        },
        after_attrs - start,
    ))
}

fn read_empty_attrs_at(bytes: &[u8], start: usize) -> Option<(Attrs, usize)> {
    if bytes.get(start) != Some(&b'{') {
        return None;
    }
    let mut i = start + 1;
    // ONLY SPACES between the braces. The BLESSED EMPTY BLOCK is a separate
    // position rather than a use of the separator, and it is the one most
    // likely to be missed: narrowing the separator alone leaves `[x]{<TAB>}` a
    // valid empty block, and the corpus document that pins it stays green
    // (PART 4 THE INLINE INTERIOR IS SPACE-ONLY, carve#906; the executable spec
    // needed two edits for exactly this reason).
    //
    // A newline was already excluded here, for the neighbouring reason: it is
    // not a single-line inline attribute block, so `[x]{` + newline + `}` stays
    // literal.
    while bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    if bytes.get(i) == Some(&b'}') {
        Some((Attrs::default(), i + 1))
    } else {
        None
    }
}

/// Length of a mention/tag name = name_word ('.' name_word)*, where
/// name_word = (letter | digit | '_' | '-')+ (grammar PART 9 §7). A `.` is
/// INTERIOR only -- it must sit between two name_words, so `a..b` yields `a`
/// (the run stops before the doubled dot) and `markus.` yields `markus`.
fn name_run_len(s: &str) -> usize {
    let b = s.as_bytes();
    let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    let mut i = 0;
    while i < b.len() && is_word(b[i]) {
        i += 1;
    }
    if i == 0 {
        return 0;
    }
    while b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(|&c| is_word(c)) {
        i += 1; // the interior dot
        while i < b.len() && is_word(b[i]) {
            i += 1;
        }
    }
    i
}

fn parse_mention(text: &str, pos: usize) -> Option<(Mention, usize)> {
    if pos > 0 {
        let prev = text.as_bytes()[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    let rest = text.get(pos + 1..)?;
    let len = name_run_len(rest);
    if len == 0 {
        return None;
    }
    Some((
        Mention {
            attrs: None,
            user: rest[..len].to_string(),
            pos: None,
        },
        len + 1,
    ))
}

fn parse_tag(text: &str, pos: usize) -> Option<(Tag, usize)> {
    if pos > 0 {
        let prev = text.as_bytes()[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    let rest = text.get(pos + 1..)?;
    let len = name_run_len(rest);
    if len == 0 {
        return None;
    }
    Some((
        Tag {
            attrs: None,
            name: rest[..len].to_string(),
            pos: None,
        },
        len + 1,
    ))
}

fn parse_symbol(text: &str, pos: usize, bounds: &InlineBounds<'_>) -> Option<(Symbol, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(pos) != Some(&b':') {
        return None;
    }
    if pos > 0 {
        let prev = bytes[pos - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    // The first name char is a letter, digit, `+` or `-` (so the reaction
    // shortcodes `:+1:` / `:-1:` parse), but never `_`: `:_x_:` would steal
    // from underline. Scanning the symbol at the opening `:` also gives it
    // precedence over smart typography, so `:+-:` is the symbol `+-`, not a
    // `±` between colons (grammar PART 9 §7).
    let first = *bytes.get(pos + 1)?;
    if !first.is_ascii_alphanumeric() && first != b'+' && first != b'-' {
        return None;
    }
    let mut len = 1;
    while let Some(&b) = bytes.get(pos + 1 + len) {
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-' {
            len += 1;
        } else {
            break;
        }
    }
    let close_pos = pos + 1 + len;
    if bytes.get(close_pos) != Some(&b':') {
        return None;
    }
    let (attrs, consumed) =
        if let Some((attrs, next)) = read_attrs_at(bytes, close_pos + 1, bounds.last_close_brace) {
            (Some(attrs), next - pos)
        } else {
            (None, len + 2)
        };
    Some((
        Symbol {
            name: text[pos + 1..close_pos].to_string(),
            attrs,
            pos: None,
        },
        consumed,
    ))
}

fn parse_autolink(text: &str, pos: usize, bounds: &InlineBounds<'_>) -> Option<(AutoLink, usize)> {
    // The target runs to the closing `>`; with no `>` ahead the scan could only
    // walk to end-of-text and fail, so bail in O(1) (keeps `<`×n linear).
    if !bounds.has_gt_from(pos) {
        return None;
    }
    let rest = text.get(pos..)?;
    let close = rest.find('>')?;
    let target = &rest[1..close];
    let mut attrs = None;
    let mut consumed = close + 1;
    let bytes = text.as_bytes();
    if bytes.get(pos + consumed) == Some(&b'{') {
        if let Some((parsed_attrs, next)) =
            read_attrs_at(bytes, pos + consumed, bounds.last_close_brace)
        {
            attrs = Some(parsed_attrs);
            consumed = next - pos;
        }
    }
    if is_url_autolink_target(target) {
        return Some((
            AutoLink {
                attrs,
                href: target.to_string(),
                text: target.to_string(),
                pos: None,
            },
            consumed,
        ));
    }
    if is_email_autolink_target(target) {
        return Some((
            AutoLink {
                attrs,
                href: format!("mailto:{target}"),
                text: target.to_string(),
                pos: None,
            },
            consumed,
        ));
    }
    None
}

/// `email_char = letter | digit | '.' | '-' | '_' | '+'` (grammar.ebnf).
/// Note `:` is deliberately NOT an email char.
fn is_email_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+')
}

/// `email_autolink = {email_char}+ '@' {email_char}+ '.' {letter}+`.
/// The local part and domain are both non-empty runs of `email_char`, and the
/// domain MUST end in `.` followed by a TLD of one or more ASCII letters. So
/// `<a@b>` (no TLD) and `<x@y:z>` (`:` is not an email_char) stay literal,
/// while `<a@b.com>` is a `mailto:` link.
fn is_email_autolink_target(target: &str) -> bool {
    let bytes = target.as_bytes();
    let Some(at) = bytes.iter().position(|&b| b == b'@') else {
        return false;
    };
    let local = &bytes[..at];
    let domain = &bytes[at + 1..];
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if !local.iter().all(|&b| is_email_char(b)) || !domain.iter().all(|&b| is_email_char(b)) {
        return false;
    }
    // A single `@` only: `@` is not an email_char, so any later `@` already
    // failed the `is_email_char` check above.
    // Domain must end in `.` + TLD ({letter}+).
    let Some(dot) = domain.iter().rposition(|&b| b == b'.') else {
        return false;
    };
    let tld = &domain[dot + 1..];
    if dot == 0 {
        // No host label before the final dot.
        return false;
    }
    !tld.is_empty() && tld.iter().all(|&b| b.is_ascii_alphabetic())
}

fn is_url_autolink_target(target: &str) -> bool {
    let Some((scheme, url)) = target.split_once(':') else {
        return false;
    };
    let Some(first) = scheme.bytes().next() else {
        return false;
    };
    if url.is_empty() || !first.is_ascii_alphabetic() {
        return false;
    }
    scheme
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
        && url.chars().all(is_url_char)
}

/// PART 3 `url_char`, the autolink body's character class.
///
/// Inside ASCII it is the enumerated set and nothing else, so `"`, `\`, `` ` ``,
/// `{`, `}`, `|`, `^`, `<` and `>` still break an autolink. Outside ASCII the
/// clause AN AUTOLINK BODY ADMITS NON-ASCII AND EXCLUDES FORMAT CHARACTERS
/// reads `unicode_url_char - format_char - control_char` (carve#844,
/// carve#860): an internationalized domain, an accented host and a non-ASCII
/// path are autolinks, because the same destination written
/// `[t](https://<IDN>/)` already links through `link_destination` and one
/// destination cannot answer differently on the character set depending on the
/// spelling.
///
/// The CONTROL term is not redundant with the ASCII enumeration, which is the
/// trap it is written around: `unicode_url_char` is "non-whitespace, non-ASCII",
/// so the C1 block U+0080-U+009F satisfies it - those are Cc, are not Cf, and
/// only U+0085 is whitespace. Without the term, fourteen control characters
/// would be `url_char`s while every C0 control is excluded.
///
/// `link_destination` is a DIFFERENT production and is unchanged: a format
/// character in an inline destination or a reference definition is still an
/// ordinary destination character.
fn is_url_char(c: char) -> bool {
    if c.is_ascii() {
        return is_url_autolink_char(c as u8);
    }
    // `char::is_whitespace` is the White_Space property and `char::is_control`
    // is General_Category=Cc - the properties themselves, not a host language's
    // `\s` shorthand. JavaScript's `\s` matches U+FEFF and misses U+0085, and
    // PCRE's Unicode `\s` matches U+180E, which Unicode 6.3.0 removed from
    // White_Space; spelling either half that way decides part of this rule by
    // accident (carve-php#957).
    !c.is_whitespace() && !c.is_control() && !is_format_char(c)
}

/// General_Category=Cf: 170 codepoints in 21 ranges (Unicode 15.0.0).
///
/// A format character is invisible by definition, so a host carrying one
/// renders as the host without it and links somewhere else. That is a spoofing
/// surface rather than an authoring convenience, which is why the class is
/// excluded rather than the individual zero-width characters anyone has thought
/// to name. The whole property is pinned by codepoint in
/// `tests/autolink_url_char_classes.rs`, since 170 corpus documents stating one
/// rule is not a corpus.
fn is_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
    )
}

fn is_url_autolink_char(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b':'
                | b'/'
                | b'?'
                | b'#'
                | b'['
                | b']'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b'%'
        )
}

fn parse_crossref(text: &str, pos: usize, bounds: &InlineBounds<'_>) -> Option<(CrossRef, usize)> {
    // The target runs to the closing `>`; with no `>` ahead the scan could only
    // walk to end-of-text and fail, so bail in O(1) (keeps `</#`×n linear).
    if !bounds.has_gt_from(pos) {
        return None;
    }
    let rest = text.get(pos..)?;
    let inner = rest.strip_prefix("</#")?;
    let close = inner.find('>')?;
    let target = &inner[..close];
    if target.is_empty() || target.bytes().any(|b| b.is_ascii_whitespace()) {
        return None;
    }
    Some((
        CrossRef {
            target: target.to_string(),
            // Filled by `fill_crossref_hrefs` once the whole document exists:
            // resolution needs the heading table, which is not built yet here.
            href: None,
            pos: None,
        },
        close + 4,
    ))
}

/// O(1) lookup of the matching `]` for the `[` at `start` using a precomputed
/// match table (see `compute_bracket_matches`). `start` must index a `[`.
/// Returns the byte index of the closing `]` (a borrow position, no allocation).
///
/// Callers materialize the bracket label only after the follow (target `(`,
/// reference `[`, span `{`) validates, so a `[` whose construct never completes
/// stays O(1) instead of eagerly copying its label at every position -- the
/// difference between linear and quadratic parsing on pathological input like
/// `[[[...x]()]()...]()`.
fn bracketed_close(bytes: &[u8], start: usize, matches: &[usize]) -> Option<usize> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let close = *matches.get(start)?;
    if close == NO_BRACKET_MATCH {
        return None;
    }
    Some(close)
}

/// Read `[...]` content for an inline extension: the content runs to the
/// FIRST `]` and does NOT balance nested brackets or honor escapes
/// (`extension_content = {character - ']'}`, carve-js regex `\[([^\]]*)\]`).
/// Returns the content and the index just past the closing `]`.
fn read_to_first_bracket(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let content_start = start + 1;
    let mut i = content_start;
    while i < bytes.len() {
        if bytes[i] == b']' {
            let text = std::str::from_utf8(&bytes[content_start..i])
                .ok()?
                .to_string();
            return Some((text, i + 1));
        }
        i += 1;
    }
    None
}

/// The body of the bracketed run `text` opens with, if the run closes inside it.
///
/// The canonical writer asks this before deciding whether a `^` it is about to
/// emit opens an inline note, so the question has to be answered by the READER
/// rather than by a second scan that promises to agree with it: the close is
/// bracket-balanced, escape-aware and opaque inside a verbatim span or an
/// editorial comment, and [`read_bracketed`] is where all of that already lives.
///
/// `None` when the run does not close in `text` - which for the writer means the
/// closer may still arrive from a later node, so it must not drop the escape.
pub(crate) fn bracketed_run_body(text: &str) -> Option<String> {
    read_bracketed(text.as_bytes(), 0).map(|(body, _)| body)
}

/// Does a RAW bracketed run re-read as itself when written between `[` and `]`?
///
/// The writer needs this because a raw run - an image's alt text - resolves no
/// escapes: whatever sits between the brackets IS the value, backslashes and
/// all. So the writer cannot neutralize a `]` by escaping it. It can only ask
/// whether the reader's own scan would close where it is about to put the `]`,
/// and write the run verbatim when it does.
///
/// It is the READER's scan rather than a second spelling of it: the same
/// [`read_bracketed`] the inline pass closes a link's text with, run over the
/// run wrapped in the brackets it will be written between. Balance,
/// escape-awareness and opacity inside a verbatim span or an editorial comment
/// therefore hold by construction.
pub(crate) fn raw_bracket_run_closes(text: &str) -> bool {
    let wrapped = format!("[{text}]");
    read_bracketed(wrapped.as_bytes(), 0).is_some_and(|(_, after)| after == wrapped.len())
}

fn read_bracketed(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut i = start + 1;
    let content_start = i;
    let mut depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'`' => {
                // An unclosed verbatim span is opaque to end of text, so no `]`
                // after it can close the bracket: the construct is not balanced.
                i = skip_code_span(bytes, i)?;
            }
            b'{' if skip_editorial_comment(bytes, i).is_some() => {
                i = skip_editorial_comment(bytes, i)?;
            }
            b'[' => {
                depth += 1;
                i += 1;
            }
            b']' if depth > 0 => {
                depth -= 1;
                i += 1;
            }
            b']' => {
                let text = std::str::from_utf8(&bytes[content_start..i])
                    .ok()?
                    .to_string();
                return Some((text, i + 1));
            }
            _ => i += 1,
        }
    }
    None
}

/// Skip an editorial comment opening at `start` (`{#`), returning the index just
/// past its `#}`.
///
/// Its content is LITERAL (PART 9 `editorial_comment`), so a `]` inside it is
/// text and cannot be the close of a link label - and no escape can say so
/// either, because `{# ... #}` resolves none. Returns None when there is no
/// closer, in which case it is not a comment and the scan continues normally
/// (carve#403).
fn skip_editorial_comment(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'{') || bytes.get(start + 1) != Some(&b'#') {
        return None;
    }
    let mut i = start + 2;
    while i + 1 < bytes.len() {
        if bytes[i] == b'#' && bytes[i + 1] == b'}' {
            return Some(i + 2);
        }
        i += 1;
    }
    None
}

/// Sentinel in a bracket-match table meaning "this `[` has no matching `]`".
const NO_BRACKET_MATCH: usize = usize::MAX;

/// Precompute, in a single O(n) pass, the matching `]` index for every `[` in
/// `bytes`, mirroring `read_bracketed`'s scan rules exactly (backslash escapes
/// skip two bytes; an unclosed inline-code span is opaque to end of text and
/// closes no bracket; a `[` increments depth, the first `]` at depth>0
/// decrements it, and the `]` at depth 0 matches the most recent unmatched
/// `[`). The returned table lets the per-`[` link/reference/span parsers find
/// their closing bracket in O(1) instead of re-scanning O(n) at every position,
/// which removes the O(n^2) blowup on deeply nested balanced links
/// (`[[[...x]()]()...]`). Output is unchanged: a lookup yields the same close
/// index `read_bracketed` would return by scanning.
///
/// Entry `i` is meaningful only when `bytes[i] == b'['`; it holds the matching
/// `]` index, or `NO_BRACKET_MATCH` when that `[` never closes.
fn compute_bracket_matches(bytes: &[u8]) -> Vec<usize> {
    let mut matches = vec![NO_BRACKET_MATCH; bytes.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'`' => match skip_code_span(bytes, i) {
                // An unclosed code span is opaque to end of text: no later `]`
                // can close a bracket, so every still-open `[` stays unmatched.
                Some(next) => i = next,
                None => break,
            },
            // Mirrors `read_bracketed`: an editorial comment's content is
            // literal, so brackets inside it are text.
            b'{' if skip_editorial_comment(bytes, i).is_some() => {
                i = skip_editorial_comment(bytes, i).unwrap_or(i + 1);
            }
            b'[' => {
                stack.push(i);
                i += 1;
            }
            b']' => {
                if let Some(open) = stack.pop() {
                    matches[open] = i;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    matches
}

/// Skip a verbatim (code) span opening at `start` (a backtick run). Returns the
/// index just past the equal-length closing run, or `None` when the span is
/// unclosed (opaque to end of text) — mirroring the reference bracket scanner.
fn skip_code_span(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() && bytes[i] == b'`' {
        i += 1;
    }
    let open_len = i - start;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let close_start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        if i - close_start == open_len {
            return Some(i);
        }
    }
    None
}

/// Read `href[ "title"])` starting at `start` (just past the opening `(`).
/// Returns (href, optional title, index just past the closing `)`).
/// Resolve backslash escapes in a link/image title: `\X` becomes `X` when X is
/// ASCII punctuation (so `\"` is a literal quote), otherwise the backslash is
/// kept. Mirrors carve-js's unescapeAttrValue.
fn unescape_title(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next.is_ascii_punctuation() {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// Walk a destination that contains a parenthesis or a backslash, balancing the
/// parentheses and resolving the three escapes. Returns the destination and the
/// index the scan stopped at.
fn scan_balanced_destination(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let mut href_bytes: Vec<u8> = Vec::new();
    let mut depth: usize = 0;
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' {
            if let Some(&next) = bytes.get(i + 1) {
                if matches!(next, b'(' | b')' | b'\\') {
                    href_bytes.push(next);
                    i += 2;
                    continue;
                }
            }
        }
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            if depth == 0 {
                break;
            }
            depth -= 1;
        } else if matches!(b, b' ' | b'\t' | b'\n') {
            break;
        }
        href_bytes.push(b);
        i += 1;
    }
    let href = String::from_utf8(href_bytes).ok()?;
    // The byte loop above breaks on ASCII whitespace only. `unicode_url_char`
    // is "any non-whitespace, non-ASCII Unicode character" with no qualifier,
    // so a destination carrying a narrow no-break space is not a destination -
    // exactly as on the plain path.
    //
    // The plain path got this check and this one did not, which made the rule
    // depend on whether the URL happened to contain a PARENTHESIS: only a
    // destination with one reached here. `[x](<NBSP>https://e.com)` was
    // rejected while `[x](<NBSP>https://e.com/a(b))` linked with the invisible
    // character in the href, and `javascript:alert(1)` - parenthesised - slipped
    // through too, which is what made this look like a scheme-specific
    // divergence rather than a hole (carve#404, carve#407).
    if href.chars().any(char::is_whitespace) {
        return None;
    }
    Some((href, i))
}

fn read_link_target(
    bytes: &[u8],
    start: usize,
    last_close_paren: Option<usize>,
) -> Option<(String, Option<String>, usize)> {
    // A valid inline target MUST end with a `)` (checked below). If no `)`
    // occurs at or after `start`, the destination scan can only walk to
    // end-of-text and then fail -- so short-circuit here in O(1). Without this,
    // a run like `[a](`×n (no `)` anywhere) makes every `[` re-scan to EOF,
    // which is O(n^2). `last_close_paren` is the index of the last `)` in the
    // whole text, precomputed once by the caller; skipping only ever elides a
    // call that would have returned `None`, keeping output byte-identical.
    if last_close_paren.map_or(true, |p| start > p) {
        return None;
    }
    // Per the grammar, a destination's parentheses BALANCE: the scan ends at
    // whitespace, which begins a title, or at the first `)` with no opener left
    // to pair with. So a URL carrying a parenthesis -- Wikipedia and MDN
    // produce them constantly -- is written plainly. Djot and CommonMark both
    // balance the same way. The only escapes are an escaped parenthesis and an
    // escaped backslash, for the unbalanced case; a backslash before anything
    // else is an ordinary character, so URLs full of backslashes need no
    // doubling.
    //
    // Almost every destination holds none of those three characters, and that
    // run is a plain slice of the input. Finding it first keeps the common case
    // copy-free; only a run that actually contains one pays for the balancing
    // scan, which has to build its string byte by byte to drop the escapes.
    let mut plain_end = start;
    while plain_end < bytes.len()
        && !matches!(bytes[plain_end], b' ' | b'\t' | b'\n' | b'(' | b')' | b'\\')
    {
        plain_end += 1;
    }
    let (href, mut i) =
        if plain_end == bytes.len() || matches!(bytes[plain_end], b' ' | b'\t' | b'\n' | b')') {
            let plain = std::str::from_utf8(&bytes[start..plain_end]).ok()?;
            // The byte scan above stops at ASCII whitespace only, and
            // `unicode_url_char` is "any non-whitespace, non-ASCII Unicode
            // character" without a qualifier - so a destination carrying a
            // narrow no-break space is not a destination at all, and the link
            // does not form (carve#404).
            if plain.chars().any(char::is_whitespace) {
                return None;
            }
            (plain.to_string(), plain_end)
        } else {
            scan_balanced_destination(bytes, start)?
        };
    if href.is_empty() {
        return None;
    }
    // THE TITLE'S PADDING RUN IS SPACES. `link_title` is spelled `space` in the
    // grammar, and `image_title = link_title`, so this one run serves both
    // callers. The slot is PADDING rather than a marker separator - a link is
    // already a link once its destination has been read - but PART 7's MARKER
    // SEPARATORS AND PADDING SLOTS decides the terminal by POSITION, and an
    // inline destination is about as far from a leading indentation run as a
    // slot gets. The executable grammar spells it `destTitle = titleSp+ (quoted
    // | squoted)` with `titleSp = " "` (carve#901, carve-rs#726).
    //
    // The run is tested rather than its first character: a check on the first
    // character rejects `[t](/u<TAB>"T")` and passes `[t](/u<SP><TAB>"T")`, and
    // the rule is about the run.
    //
    // A run holding a tab therefore leaves `i` short of the `)` this production
    // requires below, and the whole construct falls back to literal text -
    // which is already what a U+00A0 in the same slot does, since the
    // destination scan rejects a non-ASCII space outright.
    //
    // THE SLOT IS EXACTLY ONE SPACE. `link_title = space, ('"' ...)` spells it
    // as one character, and carve#912 ruled that the production is right and
    // the lax readers narrow: a padding slot sits between two tokens on a line
    // whose construct is already fixed, so its width means nothing and a run
    // means nothing twice. `image_title = link_title`, so this one test serves
    // both callers - the two have disagreed before (carve#888).
    //
    // A wider run does not lose the TITLE, it loses the LINK: the slot does not
    // match, the quoted run is left unconsumed, the `)` test below fails and
    // every character survives as text. That is the failure PART 7 already
    // names, not a new one.
    //
    // Only the title's own padding narrows. A run before the `)` with NO title
    // after it is not this slot - nothing is being padded - and stays tolerated,
    // which is why the one-space test is conditioned on a quote following it.
    let title_at = if bytes.get(i) == Some(&b' ') {
        i + 1
    } else {
        i
    };
    let title_follows = matches!(bytes.get(title_at), Some(&b'"') | Some(&b'\''));
    if title_follows {
        i = title_at;
    } else {
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
    }
    let mut title: Option<String> = None;
    if title_follows {
        let quote = bytes[i];
        i += 1;
        let title_start = i;
        // A backslash escapes the next byte, so `\"` is a literal quote inside
        // the title rather than its terminator (matches carve-php / carve-js).
        while i < bytes.len() && bytes[i] != quote {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                i += 2;
            } else {
                i += 1;
            }
        }
        if i >= bytes.len() {
            return None;
        }
        title = Some(unescape_title(
            std::str::from_utf8(&bytes[title_start..i]).ok()?,
        ));
        i += 1;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
    }
    if bytes.get(i) != Some(&b')') {
        return None;
    }
    Some((href, title, i + 1))
}

fn match_emphasis(
    bytes: &[u8],
    i: usize,
    options: &Options<'_>,
    in_footnote: bool,
    no_close: &mut [Option<usize>; EMPHASIS_DELIM_SLOTS],
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(InlineNode, usize)> {
    let c = bytes[i];

    // /*bold italic*/ -- a combined Strong>Emphasis span. The grammar
    // (`boldItalic = "/*" ~spaceOrEnd biInner+ "*/"`) requires the content to
    // start with a non-space char and be non-empty; carve-php additionally
    // rejects a closer whose content ends in whitespace, scanning on to a later
    // `*/`. Empty / space-bounded content is NOT bold-italic and falls through
    // to ordinary `/` emphasis below.
    if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
        let start = i + 2;
        // Opener guard: the first content byte must exist and not be whitespace.
        if bytes.get(start).is_some_and(|b| !b.is_ascii_whitespace()) {
            let mut search = start;
            while let Some(close) = find_seq(bytes, search, b"*/") {
                // Reject empty content or content ending in whitespace; keep
                // scanning for a later closer, matching carve-php.
                if close > start && !bytes[close - 1].is_ascii_whitespace() {
                    let inner = std::str::from_utf8(&bytes[start..close]).ok()?;
                    return Some((
                        InlineNode::Emphasis(Emphasis {
                            attrs: None,
                            kind: EmphasisKind::BoldItalic,
                            children: parse_inline_context(
                                inner,
                                options,
                                false,
                                in_footnote,
                                positions,
                                base + start,
                            ),
                            pos: None,
                        }),
                        close + 2 - i,
                    ));
                }
                search = close + 1;
            }
        }
    }
    // Single-char delimiters. Highlight `=` is single-char like the rest; a
    // doubled `==` is therefore literal by same-delimiter adjacency (checked
    // below), exactly like `**x**`. There is NO bare `^`/`,` delimiter:
    // superscript and subscript exist only in the braced forms `{^x^}`/`{,x,}`
    // (grammar PART 9 §9 rationale note) -- a bare caret or comma is literal.
    let kind = match c {
        b'/' => EmphasisKind::Italic,
        b'*' => EmphasisKind::Strong,
        b'_' => EmphasisKind::Underline,
        b'~' => EmphasisKind::Strike,
        b'=' => EmphasisKind::Highlight,
        _ => return None,
    };
    let delim = c;
    // Opener: next char must exist and not be space/newline/delim
    let after = bytes.get(i + 1).copied()?;
    if after == b' ' || after == b'\n' || after == delim {
        return None;
    }
    // A `=` that is part of a multi-char smart-typography operator is consumed
    // by that operator, not as a highlight opener (grammar PART 8 / §8): it
    // begins `=>` or trails `<=` / `>=` / `!=`. (The spaced forms like `x <= y`
    // already fail the opener test -- their `=` is followed by whitespace -- but
    // compact forms like `a <=b` would otherwise open a stray `<mark>`.)
    if delim == b'=' {
        if after == b'>' {
            return None;
        }
        if i > 0 && matches!(bytes[i - 1], b'<' | b'>' | b'!') {
            return None;
        }
    }
    if i > 0 {
        let prev = bytes[i - 1];
        // No same-type nesting: a delimiter adjacent to the same delimiter does
        // not open, so a doubled delimiter (`**x**`, `==x==`, `,,x,,`) is literal.
        if prev == delim {
            return None;
        }
        // Word-boundary opener (spec §9): no bare delimiter opens after an
        // alphanumeric or `_`, keeping paths/identifiers/numbers literal
        // (`a/b/c`, `foo*bar*baz`, `snake_case`, `x = 5`, `key=value`, `1,2,3`).
        // Use the forced `{X…X}` family for deliberate intraword emphasis.
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
        // Italic/underline additionally can't open after `/` (path protection,
        // e.g. `snake_/case/`).
        if (delim == b'/' || delim == b'_') && prev == b'/' {
            return None;
        }
    }
    let close = cached_find_emphasis_close(bytes, i + 1, delim, no_close)?;
    let inner = std::str::from_utf8(&bytes[i + 1..close]).ok()?;
    Some((
        InlineNode::Emphasis(Emphasis {
            attrs: None,
            kind,
            children: parse_inline_context(
                inner,
                options,
                false,
                in_footnote,
                positions,
                base + i + 1,
            ),
            pos: None,
        }),
        close + 1 - i,
    ))
}

fn try_extension_inline(text: &str, pos: usize, options: &Options<'_>) -> Option<InlineMatch> {
    if options.extensions.is_empty() {
        return None;
    }
    if !text.is_char_boundary(pos) {
        return None;
    }
    let ctx = MatcherContext::new(options);
    for ext in &options.extensions {
        if let Some(matched) = ext.match_inline(text, pos, &ctx) {
            return Some(matched);
        }
    }
    None
}

fn apply_abbreviations(doc: &mut Document) {
    let mut defs = BTreeMap::new();
    for child in &doc.children {
        if let BlockNode::AbbreviationDef(def) = child {
            defs.insert(def.abbr.clone(), def.expansion.clone());
        }
    }
    if defs.is_empty() {
        return;
    }
    // The definitions stay in the tree. HTML uses document children as the
    // abbreviation table, while the non-HTML renderers need the authored
    // definition line to survive as source.
    let index = abbreviation_index(&defs);
    for block in &mut doc.children {
        apply_abbreviations_block(block, &index);
    }
}

type AbbreviationIndex<'a> = BTreeMap<char, Vec<(&'a str, &'a str)>>;

fn abbreviation_index(defs: &BTreeMap<String, String>) -> AbbreviationIndex<'_> {
    let mut index: AbbreviationIndex<'_> = BTreeMap::new();
    for (abbr, expansion) in defs {
        if let Some(first) = abbr.chars().next() {
            index
                .entry(first)
                .or_default()
                .push((abbr.as_str(), expansion.as_str()));
        }
    }
    index
}

fn apply_abbreviations_block(block: &mut BlockNode, index: &AbbreviationIndex<'_>) {
    match block {
        BlockNode::Heading(h) => apply_abbreviations_inline(&mut h.children, index),
        BlockNode::Paragraph(p) => apply_abbreviations_inline(&mut p.children, index),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    apply_abbreviations_block(child, index);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    apply_abbreviations_inline(&mut cell.children, index);
                }
            }
        }
        BlockNode::Admonition(a) => {
            for child in &mut a.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::FigureGroup(g) => {
            for child in &mut g.children {
                apply_abbreviations_block(child, index);
            }
            if let Some(caption) = &mut g.caption {
                apply_abbreviations_inline(caption, index);
            }
        }
        // A `:::` div and a block extension were missing, so an abbreviation
        // never expanded inside one -- even with the definition at the top
        // level, where collection was never in doubt. carve-js expands it.
        BlockNode::Div(d) => {
            for child in &mut d.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::Extension(e) => {
            for child in &mut e.children {
                apply_abbreviations_block(child, index);
            }
        }
        BlockNode::DefinitionList(dl) => {
            for item in &mut dl.items {
                for term in &mut item.terms {
                    apply_abbreviations_inline(&mut term.children, index);
                }
                for def in &mut item.definitions {
                    for child in &mut def.children {
                        apply_abbreviations_block(child, index);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            apply_abbreviations_inline(&mut f.caption, index);
            match &mut *f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        apply_abbreviations_block(child, index);
                    }
                }
                FigureTarget::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            apply_abbreviations_inline(&mut cell.children, index);
                        }
                    }
                }
                FigureTarget::Image(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Paragraph(_) => {}
            }
        }
        _ => {}
    }
}

fn apply_abbreviations_inline(nodes: &mut Vec<InlineNode>, index: &AbbreviationIndex<'_>) {
    let mut out = Vec::new();
    for node in std::mem::take(nodes) {
        match node {
            InlineNode::Text(text) => {
                let mut parts = replace_abbreviations_in_text(&text.value, index, text.pos);
                out.append(&mut parts);
            }
            InlineNode::Emphasis(mut e) => {
                apply_abbreviations_inline(&mut e.children, index);
                out.push(InlineNode::Emphasis(e));
            }
            InlineNode::Link(mut l) => {
                apply_abbreviations_inline(&mut l.children, index);
                out.push(InlineNode::Link(l));
            }
            InlineNode::Extension(mut e) => {
                apply_abbreviations_inline(&mut e.children, index);
                out.push(InlineNode::Extension(e));
            }
            // PART 9R R3 matches a term in RENDERED TEXT at word boundaries, and
            // the container the text sits in does not change that. A span fell
            // to the catch-all arm below and its children were never walked, so
            // `[HTML]{.x}` and `[HTML]{kbd}` silently dropped the expansion that
            // `*HTML*` and `[HTML](/u)` got - and PART 9 §10 made the second
            // spelling a documented feature, so the loss sits inside a construct
            // the docs teach (carve#1151).
            InlineNode::Span(mut sp) => {
                apply_abbreviations_inline(&mut sp.children, index);
                out.push(InlineNode::Span(sp));
            }
            InlineNode::CitationGroup(mut g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        apply_abbreviations_inline(prefix, index);
                    }
                    if let Some(locator) = &mut item.locator {
                        apply_abbreviations_inline(locator, index);
                    }
                }
                out.push(InlineNode::CitationGroup(g));
            }
            other => out.push(other),
        }
    }
    *nodes = out;
}

/// Split a text node around the abbreviations in it.
///
/// The pieces are CONTIGUOUS SLICES of the node being split, so when that node
/// carried a span every piece's span follows from its offset within it - no
/// re-scanning of the document, and no invention.
///
/// The two guards are what keep that true. A text node whose span is a
/// different length than its value is not a verbatim slice of the source (the
/// no-break-space sentinel is one character standing in for two), and a node
/// spanning more than one line has no single column to count from. Either way
/// the pieces get no position rather than a derived-from-wrong one.
fn replace_abbreviations_in_text(
    text: &str,
    index: &AbbreviationIndex<'_>,
    pos: Option<Pos>,
) -> Vec<InlineNode> {
    let anchor = pos.filter(|p| {
        p.start_line == p.end_line
            && p.end_offset.saturating_sub(p.start_offset) == text.chars().count()
    });
    // Chars consumed so far, which is the offset of the NEXT piece.
    let mut chars_done = 0usize;
    let span_from = |start: usize, len: usize| -> Option<Pos> {
        let p = anchor?;
        Some(Pos {
            start_line: p.start_line,
            end_line: p.start_line,
            start_column: p.start_column + start,
            end_column: p.start_column + start + len,
            start_offset: p.start_offset + start,
            end_offset: p.start_offset + start + len,
        })
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let mut matched: Option<(&str, &str)> = None;
        let ch = text[i..].chars().next().unwrap();
        if let Some(candidates) = index.get(&ch) {
            for (abbr, expansion) in candidates {
                if text[i..].starts_with(abbr)
                    && is_word_boundary(text, i)
                    && is_word_boundary(text, i + abbr.len())
                {
                    matched = Some((*abbr, *expansion));
                    break;
                }
            }
        }
        if let Some((abbr, expansion)) = matched {
            let len = abbr.chars().count();
            out.push(InlineNode::Abbreviation(Abbreviation {
                abbr: abbr.to_string(),
                expansion: expansion.to_string(),
                pos: span_from(chars_done, len),
            }));
            chars_done += len;
            i += abbr.len();
            continue;
        }
        match out.last_mut() {
            Some(InlineNode::Text(existing)) => {
                existing.value.push(ch);
                if let Some(p) = existing.pos.as_mut() {
                    p.end_column += 1;
                    p.end_offset += 1;
                }
            }
            _ => out.push(InlineNode::Text(Text {
                value: ch.to_string(),
                pos: span_from(chars_done, 1),
            })),
        }
        chars_done += 1;
        i += ch.len_utf8();
    }
    out
}

fn is_word_boundary(text: &str, pos: usize) -> bool {
    if pos == 0 || pos >= text.len() {
        return true;
    }
    !text.as_bytes()[pos - 1].is_ascii_alphanumeric()
        || !text.as_bytes()[pos].is_ascii_alphanumeric()
}

/// Assign `caption_number.number` per label, in document order.
///
/// Called by the parse and by `ast_json::from_json`, which is why it is visible
/// to the crate: a published number has to describe THIS document, and an
/// ingested tree may have had a captioned element removed since it was written
/// (carve#758). Counters are built fresh on every call, so re-running it on an
/// unedited tree reproduces the same numbering.
pub(crate) fn number_crossref_captions(doc: &mut Document) {
    let mut caption_counts = BTreeMap::new();
    let mut titles = BTreeMap::new();
    number_captioned_blocks(&mut doc.children, &mut caption_counts, &mut titles);
    for blocks in doc.footnote_defs.values_mut() {
        number_captioned_blocks(blocks, &mut caption_counts, &mut titles);
    }
}

pub(crate) fn crossref_index_for_document(doc: &Document, lowercase_ids: bool) -> CrossrefIndex {
    let mut counts = BTreeMap::new();
    let mut titles = BTreeMap::new();
    let mut labels = BTreeMap::new();
    let mut explicit_ids = std::collections::BTreeSet::new();
    collect_explicit_ids(&doc.children, &mut explicit_ids);
    for blocks in doc.footnote_defs.values() {
        collect_explicit_ids(blocks, &mut explicit_ids);
    }
    // This index serves `</#id>` crossrefs only, which DO resolve into a
    // blockquote, so the quoted set it fills is discarded.
    let mut quoted = std::collections::BTreeSet::new();
    // This index serves `</#id>` only, which looks up by id, so the text index
    // it fills is discarded with the quoted set above.
    let mut by_text = BTreeMap::new();
    collect_heading_titles(
        &doc.children,
        &mut HeadingScan {
            counts: &mut counts,
            titles: &mut titles,
            labels: &mut labels,
            quoted: &mut quoted,
            by_text: &mut by_text,
        },
        lowercase_ids,
        &explicit_ids,
        false,
    );
    for blocks in doc.footnote_defs.values() {
        collect_heading_titles(
            blocks,
            &mut HeadingScan {
                counts: &mut counts,
                titles: &mut titles,
                labels: &mut labels,
                quoted: &mut quoted,
                by_text: &mut by_text,
            },
            lowercase_ids,
            &explicit_ids,
            false,
        );
    }
    collect_caption_titles(&doc.children, &mut titles);
    for blocks in doc.footnote_defs.values() {
        collect_caption_titles(blocks, &mut titles);
    }
    crossref_index(titles, labels)
}

fn heading_index(
    children: &[BlockNode],
    footnote_defs: &BTreeMap<String, Vec<BlockNode>>,
    lowercase_ids: bool,
) -> CrossrefIndex {
    let mut counts = BTreeMap::new();
    let mut titles = BTreeMap::new();
    let mut labels = BTreeMap::new();
    let mut explicit_ids = std::collections::BTreeSet::new();
    collect_explicit_ids(children, &mut explicit_ids);
    for blocks in footnote_defs.values() {
        collect_explicit_ids(blocks, &mut explicit_ids);
    }
    let mut quoted = std::collections::BTreeSet::new();
    let mut by_text = BTreeMap::new();
    collect_heading_titles(
        children,
        &mut HeadingScan {
            counts: &mut counts,
            titles: &mut titles,
            labels: &mut labels,
            quoted: &mut quoted,
            by_text: &mut by_text,
        },
        lowercase_ids,
        &explicit_ids,
        false,
    );
    for blocks in footnote_defs.values() {
        collect_heading_titles(
            blocks,
            &mut HeadingScan {
                counts: &mut counts,
                titles: &mut titles,
                labels: &mut labels,
                quoted: &mut quoted,
                by_text: &mut by_text,
            },
            lowercase_ids,
            &explicit_ids,
            false,
        );
    }
    let mut index = crossref_index(titles, labels);
    index.quoted = quoted;
    index.by_text = by_text;
    index
}

/// A cloned crossref label, shared by every reference that resolves to it.
///
/// `Rc` rather than a per-reference clone: a document may reference one heading
/// any number of times, and the label is immutable once captured.
pub(crate) type CrossrefLabel = std::rc::Rc<Vec<InlineNode>>;

/// Capture a heading's label for the crossref index: a CLONE OF ITS INLINE
/// NODES, not of its rendered text (PART 9R R4, carve#915).
///
/// The difference is the whole point of the clause. A node carries the SOURCE
/// RUN the author typed; a string carries only the glyphs some renderer chose.
/// Flattening here would destroy the run before any renderer is invoked, so
/// smart typography's SOURCE mode (§8) could not recover it on ANY target and
/// no renderer change would reach the loss -- the label would have been
/// materialized in the wrong subsystem. The same argument covers every other
/// run a renderer may want back: a code span, an escape, an inline literal.
///
/// Two transformations are applied to the clone, both of them rules that hold
/// wherever this label is about to be placed rather than presentation choices:
///
/// - A NESTED cross-reference becomes empty text. Resolution is ONE LEVEL, so a
///   cloned label is never re-expanded; doing it here (rather than at render
///   time) also makes a crossref cycle structurally impossible to follow.
/// - Links never nest, so the clone is cleaned as if it already sat inside an
///   anchor -- which it does on every target that emits one.
///
/// A footnote reference is dropped, which is what the previous flattening did
/// too: the label renders inside the referring paragraph, and a second copy of
/// a `fnref` anchor would publish a duplicate id.
fn crossref_label_nodes(children: &[InlineNode]) -> CrossrefLabel {
    std::rc::Rc::new(crossref_label_clone(children))
}

/// The core crossref's label: [`derive_display_nodes`] in a link context, since
/// a resolved reference always renders inside the anchor it produces.
pub(crate) fn crossref_label_clone(children: &[InlineNode]) -> Vec<InlineNode> {
    derive_display_nodes(children, true)
}

/// The one derivation every consumer of a heading's display text goes through
/// (PART 9R R4, DERIVED DISPLAY TEXT CLONES THE SAME NODES,
/// markup-carve/carve#957).
///
/// R4 binds every such consumer, not the core crossref alone, and names three: a
/// numbered cross-reference label, an index term's display, a table-of-contents
/// entry. Each answering the follow-on questions on its own is how one rule
/// acquires four readings, so they all call this.
///
/// `inside_link` is the CALLER's context rather than a fact about `children`: a
/// crossref label and a TOC entry are placed inside an `<a>` and pass `true`; an
/// index list item is not an anchor (only the backrefs after the display are) and
/// passes `false`, so an authored link in the term survives.
pub(crate) fn derive_display_nodes(children: &[InlineNode], inside_link: bool) -> Vec<InlineNode> {
    let mut nodes = children.to_vec();
    strip_non_authored(&mut nodes);
    flatten_nested_crossrefs(&mut nodes);
    enforce_no_nesting_inline(nodes, inside_link)
}

/// Reduce a cloned run to what the AUTHOR wrote.
///
/// THE LABEL IS TAKEN BEFORE ANY RENDER-STAGE INJECTION (PART 9R R4). A heading's
/// cloned nodes are its AUTHORED inline content, so whatever a later stage added
/// to the heading is not part of the label. That half of the clause is aimed at
/// THIS engine: it builds the crossref index at RENDER time, after every
/// `before_render` hook has already run, so the injections are in the heading by
/// the time the label is taken and the pristine reading has to be recovered here
/// rather than obtained by ordering.
///
/// Five kinds come out, and each is what the flatten this replaces already
/// produced, so no construct moves byte-wise by being dropped:
///
/// - A `section-number` SPAN, injected by `headingNumbers` (§9). R4 names this
///   one explicitly.
/// - A PERMALINK ANCHOR, injected by `headingPermalinks`. R4 names this one too.
///   Left in, a resolved `</#id>` rendered an `<a>` INSIDE its own `<a>`.
/// - A FOOTNOTE REFERENCE, which is a pointer into the endnotes rather than
///   display text: a second copy publishes a duplicate `fnref` id and points the
///   backlink at whichever rendered last.
/// - An `:index[term]` MARKER, invisible by §8.1 - it emits no visible text, so
///   it is not display text anywhere it is derived, and its `idx-…` anchor id is
///   published exactly once. What comes out is the COUNTED CARRIER the extension
///   rewrites a body marker to, and only that. The AUTHORED `index` extension
///   stays: with the extension off it degrades to the visible generic fallback
///   `<span class="ext-index">term</span>` (§8.3, "the marker cannot hide without
///   its handler"), so dropping it would make a derived label disagree with the
///   heading it was derived from and lose authored text (raised by codex review).
///   With the extension ON, an authored marker that reaches here unrewritten
///   renders inert and invisible, which is the heading's own answer too.
/// - A CITATION GROUP, the other resolution result a heading can carry. It
///   renders as an anchor into the references list, and with a bibliography pool
///   active it also carries a per-use `cite-…` id - so a second copy nests an
///   anchor inside the derived label's own anchor AND publishes a duplicate DOM
///   id, the same two failures the footnote reference has. The author's raw
///   `[@key]` run goes back in its place, which is what the flatten produced.
/// - An ABBREVIATION, an R3 resolution result. The author wrote the short form;
///   cloning the resolved node republishes the whole `<abbr title="…">` once per
///   derived site, an amplification the body renderer bounds with a budget this
///   path cannot reach. Taking the author's `abbr` back out is both the bounded
///   answer and the correct one.
fn strip_non_authored(nodes: &mut Vec<InlineNode>) {
    nodes.retain(|node| match node {
        InlineNode::Footnote(_) => false,
        InlineNode::RawInline(r) => !r.injected,
        InlineNode::Span(s) => !s.injected,
        InlineNode::Extension(e) => e.name != INDEX_MARKER_CARRIER,
        _ => true,
    });
    for node in nodes.iter_mut() {
        match node {
            InlineNode::Abbreviation(a) => {
                *node = InlineNode::Text(Text {
                    value: std::mem::take(&mut a.abbr),
                    pos: a.pos,
                });
                continue;
            }
            InlineNode::CitationGroup(g) => {
                *node = InlineNode::Text(Text {
                    value: std::mem::take(&mut g.raw),
                    pos: g.pos,
                });
                continue;
            }
            _ => {}
        }
        match node {
            InlineNode::Emphasis(e) => strip_non_authored(&mut e.children),
            InlineNode::Span(s) => strip_non_authored(&mut s.children),
            InlineNode::Link(l) => strip_non_authored(&mut l.children),
            InlineNode::Extension(e) => strip_non_authored(&mut e.children),
            InlineNode::CriticInsert(c) => strip_non_authored(&mut c.children),
            InlineNode::CriticDelete(c) => strip_non_authored(&mut c.children),
            _ => {}
        }
    }
}

/// Class on the `section-number` span `headingNumbers` injects into a heading.
/// The strip does NOT key on it (see [`Span::injected`]); it is named once, and
/// the extension reads it from here so the class and its documentation cannot
/// drift apart.
pub(crate) const SECTION_NUMBER_CLASS: &str = "section-number";
/// The counted carrier `index` rewrites a body marker to in `before_render`.
pub(crate) const INDEX_MARKER_CARRIER: &str = "carve-index-marker";

/// Replace every `</#id>` inside a cloned label with empty text. Resolution is
/// ONE LEVEL, so a cloned label is never re-expanded; doing it here rather than
/// at render time also makes a crossref cycle structurally impossible to follow.
fn flatten_nested_crossrefs(nodes: &mut [InlineNode]) {
    for node in nodes.iter_mut() {
        match node {
            InlineNode::CrossRef(_) => {
                *node = InlineNode::Text(Text {
                    value: String::new(),
                    pos: None,
                })
            }
            InlineNode::Emphasis(e) => flatten_nested_crossrefs(&mut e.children),
            InlineNode::Span(s) => flatten_nested_crossrefs(&mut s.children),
            InlineNode::Link(l) => flatten_nested_crossrefs(&mut l.children),
            InlineNode::Extension(e) => flatten_nested_crossrefs(&mut e.children),
            InlineNode::CriticInsert(c) => flatten_nested_crossrefs(&mut c.children),
            InlineNode::CriticDelete(c) => flatten_nested_crossrefs(&mut c.children),
            _ => {}
        }
    }
}

fn crossref_index(
    titles: BTreeMap<String, String>,
    labels: BTreeMap<String, CrossrefLabel>,
) -> CrossrefIndex {
    // Case-folded index of known ids -> actual (case-preserved) id. First
    // occurrence wins, so a duplicate that only differs in case does not shadow
    // the earlier heading. Used as a fallback when an exact match fails, so a
    // lowercase reference resolves to a `Getting-Started` heading and the
    // emitted href uses the ACTUAL id.
    let mut folded: BTreeMap<String, String> = BTreeMap::new();
    for id in titles.keys() {
        folded.entry(case_fold(id)).or_insert_with(|| id.clone());
    }
    CrossrefIndex {
        titles,
        labels,
        folded,
        by_text: BTreeMap::new(),
        quoted: std::collections::BTreeSet::new(),
    }
}

/// Heading-id lookup table for `</#id>` cross-references: exact id -> title,
/// plus a case-folded fallback (folded id -> actual case-preserved id) so a
/// lowercase reference resolves to a case-preserved heading.
#[derive(Default)]
pub(crate) struct CrossrefIndex {
    titles: BTreeMap<String, String>,
    /// Id -> the label a resolved reference RENDERS: the target heading's own
    /// inline nodes, cloned (PART 9R R4). `titles` is the same label flattened
    /// to text, which is all a slug or a text lookup needs; a renderer needs
    /// the nodes, because only they still carry what the author typed.
    ///
    /// A CAPTION id has a title but no entry here: its label is LABEL + NUMBER
    /// ("Figure 1"), text that no node of the document ever held.
    labels: BTreeMap<String, CrossrefLabel>,
    folded: BTreeMap<String, String>,
    /// Normalized heading TEXT -> that heading's id, in document order (first
    /// occurrence wins). PART 11 R1 keys the implicit `[label][]` index by "the
    /// document's headings keyed by their rendered plain text"; looking the
    /// label up among the IDS agrees with that only while the id is the slug of
    /// the text, and stops the moment an author sets one explicitly
    /// (carve-rs#477).
    by_text: BTreeMap<String, String>,
    /// Ids belonging to a heading with a blockquote ancestor. They resolve as
    /// `</#id>` crossrefs like any other, and are DECLINED as implicit
    /// `[label][]` reference targets (PART 11 R1).
    quoted: std::collections::BTreeSet<String>,
}

impl CrossrefIndex {
    /// Resolve a cross-reference target to its `(actual_id, title)`. Tries an
    /// exact match first, then a case-folded fallback (first-occurrence wins).
    /// Resolve for an IMPLICIT `[label][]` reference, which declines a heading
    /// with a blockquote ancestor (PART 11 R1). Quoted text names the quoted
    /// document's headings, not this one's, and a quotation is the one
    /// container whose wording the author does not control. Declining does not
    /// make the heading unreachable: `</#id>` still addresses it, by id rather
    /// than by wording.
    pub(crate) fn resolve_ref(&self, target: &str) -> Option<(&str, &str)> {
        // An all-excluded label (for example a symbol-only heading reference)
        // has no prose key. The heading still receives the fallback id `s`,
        // but an empty invisible key must not reach that id through slugging.
        if normalize_heading_label(target).is_empty() {
            return None;
        }
        // By TEXT first (R1's index), then the id lookup the `</#id>` path uses.
        // The fallback keeps every document whose id IS the slug of its heading
        // text resolving exactly as before.
        if let Some(id) = self.by_text.get(&normalize_heading_label(target)) {
            if !self.quoted.contains(id) {
                if let Some((id, title)) = self.titles.get_key_value(id) {
                    return Some((id.as_str(), title.as_str()));
                }
            }
        }
        // Fallback: the slug of the label against the id index, which is what
        // this did before the text index existed. It still answers every
        // document whose heading id IS the slug of its text.
        let (id, title) = self.resolve(&slugify_parse(target, false))?;
        if self.quoted.contains(id) {
            return None;
        }
        Some((id, title))
    }

    /// Whether R1'S TEXT INDEX answers for `target` - the first of the two keys
    /// `resolve_ref` offers, without its slug fallback.
    ///
    /// A collapsed reference has to publish WHICH key answered (PART 12 §3a,
    /// markup-carve/carve#962), and `resolve_ref` cannot say: it returns the
    /// heading either way. The two conditions are the same ones step one of
    /// `resolve_ref` applies, and are kept beside it for that reason.
    pub(crate) fn answers_by_text(&self, target: &str) -> bool {
        self.by_text
            .get(&normalize_heading_label(target))
            .is_some_and(|id| !self.quoted.contains(id) && self.titles.contains_key(id))
    }

    /// The cloned inline nodes a resolved reference to `id` renders, when the
    /// target is a HEADING. `None` for a caption id, whose label is
    /// LABEL + NUMBER rather than any node of the document; the caller falls
    /// back to `resolve`'s title for that.
    pub(crate) fn label(&self, id: &str) -> Option<CrossrefLabel> {
        self.labels.get(id).cloned()
    }

    pub(crate) fn resolve(&self, target: &str) -> Option<(&str, &str)> {
        if let Some((id, title)) = self.titles.get_key_value(target) {
            return Some((id.as_str(), title.as_str()));
        }
        let id = self.folded.get(&case_fold(target))?;
        let title = self.titles.get(id)?;
        Some((id.as_str(), title.as_str()))
    }
}

/// The comparison PART 11 R1 specifies for the implicit heading fallback: the
/// label and the heading text are "both trimmed, their internal whitespace runs
/// collapsed to one space, and then compared case-INSENSITIVELY".
///
/// Deliberately looser than the exact, case-sensitive matching `linkDefs` uses:
/// a linkDefs label is an identifier the author wrote twice, while a heading ref
/// is prose quoted from elsewhere in the document.
/// The key R1's heading index is built on and looked up by: whitespace runs
/// collapsed, NFC-normalized, then case-folded.
///
/// NFC is here because heading IDS are already NFC (section 25), so without it a
/// document publishes a precomposed id and then declines a reference spelling
/// that exact string. This engine LOOKED right before the fold was added, but
/// only through `resolve_ref`'s slug fallback - which answers a cross-spelling
/// reference solely when the heading's id IS the slug of its text. Give the
/// heading an id of its own and the accident stops working:
///
/// ```text
/// {#custom}
/// # Cafe + U+0301
///
/// see [precomposed Cafe][]
/// ```
///
/// resolved in the executable spec, carve-js and carve-php and stayed literal
/// here (carve#725). The fold belongs on the TEXT index, which is the index R1
/// describes.
///
/// NFC and not NFKC: a ligature heading must not be reached by its ASCII
/// spelling - compatibility folding changes which text the author is quoting,
/// not how it is spelled.
fn normalize_heading_label(s: &str) -> String {
    case_fold(&crate::unicode_nfc::nfc(
        &s.split_whitespace().collect::<Vec<_>>().join(" "),
    ))
}

/// Per-code-point lowercase fold, used for case-insensitive `</#id>` lookup.
/// Matches the `lowercase` transform in `slugify_parse` (no context mappings).
fn case_fold(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        for lc in ch.to_lowercase() {
            out.push(lc);
        }
    }
    out
}

fn resolve_reference_links(
    doc: &mut Document,
    defs: &BTreeMap<String, LinkDef>,
    heading_index: &CrossrefIndex,
) {
    for block in &mut doc.children {
        resolve_reference_links_block(block, defs, heading_index);
    }
    for blocks in doc.footnote_defs.values_mut() {
        for block in blocks {
            resolve_reference_links_block(block, defs, heading_index);
        }
    }
}

fn resolve_reference_links_block(
    block: &mut BlockNode,
    defs: &BTreeMap<String, LinkDef>,
    heading_index: &CrossrefIndex,
) {
    match block {
        BlockNode::Heading(h) => {
            resolve_reference_links_inline(&mut h.children, defs, heading_index)
        }
        BlockNode::Paragraph(p) => {
            resolve_reference_links_inline(&mut p.children, defs, heading_index)
        }
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    resolve_reference_links_block(child, defs, heading_index);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                resolve_reference_links_block(child, defs, heading_index);
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                resolve_reference_links_inline(caption, defs, heading_index);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    resolve_reference_links_inline(&mut cell.children, defs, heading_index);
                }
            }
        }
        BlockNode::Admonition(a) => {
            for child in &mut a.children {
                resolve_reference_links_block(child, defs, heading_index);
            }
        }
        BlockNode::FigureGroup(g) => {
            for child in &mut g.children {
                resolve_reference_links_block(child, defs, heading_index);
            }
            if let Some(caption) = &mut g.caption {
                resolve_reference_links_inline(caption, defs, heading_index);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                resolve_reference_links_block(child, defs, heading_index);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    resolve_reference_links_inline(term, defs, heading_index);
                }
                for definition in &mut item.definitions {
                    for child in definition {
                        resolve_reference_links_block(child, defs, heading_index);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            resolve_reference_links_inline(&mut f.caption, defs, heading_index);
            match &mut *f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        resolve_reference_links_block(child, defs, heading_index);
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        resolve_reference_links_inline(caption, defs, heading_index);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            resolve_reference_links_inline(&mut cell.children, defs, heading_index);
                        }
                    }
                }
                FigureTarget::Image(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Paragraph(_) => {}
            }
        }
        _ => {}
    }
}

fn resolve_reference_links_inline(
    nodes: &mut Vec<InlineNode>,
    defs: &BTreeMap<String, LinkDef>,
    heading_index: &CrossrefIndex,
) {
    let mut out = Vec::new();
    for mut node in std::mem::take(nodes) {
        match &mut node {
            InlineNode::Link(l) => {
                if let Some(label) = &l.ref_label {
                    // Every branch below KEEPS the node: an unresolved reference
                    // is still a link (PART 12 §3a), so the only question is
                    // whether a destination gets filled in. Reverting it to text
                    // is what §3a forbids - it discarded the fact that the
                    // author wrote a reference, and it did so only on the HTML
                    // path, so one document had two shapes (carve#486).
                    if let Some(def) = defs.get(label) {
                        // PART 12 §3a, A RESOLVED REFERENCE KEEPS ITS
                        // DESTINATION: `ref` and `raw_ref` stay BESIDE `href`,
                        // the same way §5 has footnote numbering added
                        // alongside rather than in place of the reference.
                        // Clearing them made `[a][]` and `[a](/url)` the same
                        // tree - the distinction the clause protects
                        // (carve#597). Every renderer already asks whether the
                        // DESTINATION is empty, so nothing downstream reads the
                        // label as "unresolved".
                        l.href = def.href.clone();
                        // PART 9R R1: the definition's attributes transfer to
                        // the link, and the link's own override per key. "Per
                        // key" is §15 A3's merge - the one stacked attribute
                        // lists already use - so a repeated id or key takes the
                        // LAST value (the link's) and classes ACCUMULATE across
                        // the two. Definition first, link second (carve#604).
                        if let Some(def_attrs) = &def.attrs {
                            let own = l.attrs.take();
                            let mut merged = Some(def_attrs.clone());
                            if let Some(own) = own {
                                merge_attrs(&mut merged, own);
                            }
                            l.attrs = merged;
                        }
                        l.title = def.title.clone();
                    } else if is_collapsed_reference(l) {
                        // Implicit heading reference. The LABEL goes in, not a
                        // slug of it: PART 11 R1 keys this index by the heading's
                        // rendered text, and `resolve_ref` slugifies only for its
                        // fallback. Slugifying here first hid every heading whose
                        // id is not the slug of its text (carve-rs#477).
                        //
                        // `resolve_ref` declines a heading with a blockquote
                        // ancestor; the plain `resolve` used by `</#id>` does
                        // not. Sharing one index for both lookups is what made
                        // this engine resolve into quoted material (#410).
                        //
                        // R1 OFFERS THE INDEX TWO KEYS, IN ORDER: the label AS
                        // WRITTEN, then its rendered plain text. They are the
                        // same string for a label carrying no markup, no escape
                        // and no smart-punctuation trigger, which is why the
                        // second was never separated out. Where they differ,
                        // `ref` publishes THE ONE THAT ANSWERED (PART 12 §3a,
                        // markup-carve/carve#962): the field is defined as the
                        // label the reference RESOLVES BY, and the authored
                        // spelling is already kept in `raw_ref`, so publishing
                        // it in both left the resolution key nowhere in the
                        // tree.
                        //
                        // The derived form is `plain_inlines_parse` over the
                        // link's OWN CHILDREN, which are the parsed label - the
                        // same function the heading index derives its key from,
                        // so the resolver and the index cannot disagree about
                        // what a label renders as.
                        //
                        // DERIVED, NOT NORMALIZED. Trimming, whitespace
                        // collapse, NFC and case folding belong to MATCHING and
                        // stay inside `normalize_heading_label`;
                        // `[Getting Started][]` under `# getting started` has
                        // always published `Getting Started`, and publishing the
                        // folded key would rewrite every plain label in every
                        // document to make one markup-bearing one right.
                        //
                        // NOT A BLANKET STRIP. The definition branch above is
                        // the case a blanket strip inverts: `defs` keys on the
                        // label AS WRITTEN, case-sensitively, and never reaches
                        // here. Five corpus documents carry a collapsed
                        // reference with a markup-bearing label and only TWO
                        // resolve through this branch; a strip would have moved
                        // all five.
                        let derived = plain_inlines_parse(&l.children);
                        let key = if heading_index.answers_by_text(label) {
                            None
                        } else if derived != *label && heading_index.answers_by_text(&derived) {
                            Some(derived)
                        } else {
                            // The slug fallback answered, or nothing did. The
                            // slug is not one of R1's two keys, so there is no
                            // derived key to publish and the authored spelling
                            // stands.
                            None
                        };
                        let lookup = key.as_deref().unwrap_or(label);
                        if let Some((actual_id, _)) = heading_index.resolve_ref(lookup) {
                            let actual_id = actual_id.to_string();
                            l.href = format!("#{actual_id}");
                            l.title = None;
                            l.from_heading_reference = true;
                            if let Some(key) = key {
                                l.ref_label = Some(key);
                            }
                            // KEEP `ref_label` / `raw_ref`. The href is what the
                            // HTML needs; the reference is what the AUTHOR wrote,
                            // and the canonical writer has to reproduce it. Clearing
                            // them turned `[getting started][]` into
                            // `[getting started](#Getting-Started)` on every fmt
                            // pass, baking a generated id into the source and
                            // disagreeing with carve-js, which keeps the reference
                            // form (carve#478).
                            //
                            // An EXPLICIT definition above still clears them: there
                            // both engines write the resolved link, so the authored
                            // `[x][]` plus its `[x]: url` line is not reproducible
                            // from the tree either way.
                        }
                    }
                    // A reference tail FRAMES this link's text; it does not
                    // seal it. The text is ordinary inline content, so a
                    // reference written inside it resolves like any other
                    // (corpus 313, markup-carve/carve#1196) - `[t[x][r2]][r]`
                    // renders `x` as a link, the same as the inline-destination
                    // spelling `[t[x][r2]](/u)` already did here.
                    //
                    // AFTER this node's own tail, not before: the heading-index
                    // fallback above derives its lookup key from the children's
                    // plain text, and that key is the text the AUTHOR wrote.
                    resolve_reference_links_inline(&mut l.children, defs, heading_index);
                    out.push(node);
                } else {
                    resolve_reference_links_inline(&mut l.children, defs, heading_index);
                    out.push(node);
                }
            }
            InlineNode::Emphasis(e) => {
                resolve_reference_links_inline(&mut e.children, defs, heading_index);
                out.push(node);
            }
            InlineNode::Span(s) => {
                resolve_reference_links_inline(&mut s.children, defs, heading_index);
                out.push(node);
            }
            InlineNode::Extension(e) => {
                resolve_reference_links_inline(&mut e.children, defs, heading_index);
                out.push(node);
            }
            // AN INLINE NOTE'S CONTENT IS ORDINARY INLINE CONTENT
            // (markup-carve/carve#1203). PART 9 §16 disables FOOTNOTE
            // recognition inside a note and says nothing about references, so a
            // reference written there resolves like any other. This walk had no
            // arm for it, so `^[see [t][r]]` reached the reader as literal text
            // while `*[t][r]*` one node over resolved.
            //
            // The crossref pass a few hundred lines up already descends here,
            // which is why `^[see </#h>]` worked and this did not: one rule,
            // two walks, and only one of them complete.
            InlineNode::Footnote(f) => {
                if let Some(inline) = &mut f.inline {
                    resolve_reference_links_inline(inline, defs, heading_index);
                }
                out.push(node);
            }
            // The same gap, measured rather than assumed: both critic ranges
            // hold inline children and both left a reference inside them
            // unresolved. `CriticSubstitute` and `CriticComment` are NOT here
            // because they hold strings rather than children - there is nothing
            // to descend into, and an arm for them could not fail.
            InlineNode::CriticInsert(c) => {
                resolve_reference_links_inline(&mut c.children, defs, heading_index);
                out.push(node);
            }
            InlineNode::CriticDelete(c) => {
                resolve_reference_links_inline(&mut c.children, defs, heading_index);
                out.push(node);
            }
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    if let Some(prefix) = &mut item.prefix {
                        resolve_reference_links_inline(prefix, defs, heading_index);
                    }
                    if let Some(locator) = &mut item.locator {
                        resolve_reference_links_inline(locator, defs, heading_index);
                    }
                }
                out.push(node);
            }
            InlineNode::Image(img) => {
                if let Some(label) = &img.ref_label {
                    if let Some(def) = defs.get(label) {
                        // PART 12 §3a - see the note on the link branch above.
                        img.src = def.href.clone();
                        img.title = def.title.clone();
                        // AN IMAGE REFERENCE RESOLVES THE SAME ENTRY -
                        // NORMATIVE. It looks the label up in the same table and
                        // takes the same three fields, so a definition's
                        // attributes reach the image exactly as they reach a
                        // link: `[ex]: /i.png {.wide}` gives `class="wide"`.
                        // This branch took `href` and `title` and stopped, which
                        // is not a rule, it is where the implementation stopped -
                        // and the clause says so by name (carve#697).
                        //
                        // Same §15 A3 merge as the link branch above: definition
                        // first, use site second, so a repeated key takes the
                        // LAST value and classes ACCUMULATE in source order.
                        if let Some(def_attrs) = &def.attrs {
                            let own = img.attrs.take();
                            let mut merged = Some(def_attrs.clone());
                            if let Some(own) = own {
                                merge_attrs(&mut merged, own);
                            }
                            img.attrs = merged;
                        }
                        out.push(node);
                    } else {
                        // Unresolved image references stay as Image nodes. They
                        // render from `raw_ref`, preserving the fact that the
                        // author wrote a reference at all (PART 12 §3a).
                        out.push(node);
                    }
                } else {
                    out.push(node);
                }
            }
            _ => out.push(node),
        }
    }
    *nodes = out;
}

/// Promote a paragraph whose sole child is a direct or resolved image to a
/// block-level image, matching the standalone inline-image rule
/// (`detect_block_image`) and carve-php. Recurses into container blocks.
/// Length (in bytes) of a leading `^` + one-or-more whitespace caption marker
/// (`RE_CAPTION = /^\^\s+/`), or `None` when the text does not open a caption.
/// A caption line mirrors a heading's first line (`detect_heading`): `^` +
/// one-or-more literal spaces (the grammar delimiter is a space, not a tab) +
/// non-empty content. Returns the caption text with leading spaces skipped and
/// trailing whitespace trimmed. None when there is no space after `^`, the
/// delimiter is a tab, or the content is empty (`^ ` alone).
fn caption_content(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b'^') || bytes.get(1) != Some(&b' ') {
        return None;
    }
    let mut start = 1;
    while start < bytes.len() && bytes[start] == b' ' {
        start += 1;
    }
    // Verbatim content (see detect_heading): a caption folds continuation lines
    // like a paragraph, so first-line trailing is interior; only the final
    // assembled caption is trailing-stripped (§756). The gate still tests a
    // trailing-stripped view so `^ ` / `^  ` / `^ \t` are not captions.
    let text = &line[start..];
    if trim_ascii_end(text).is_empty() {
        return None;
    }
    Some(text)
}

/// Byte length of a caption marker (`^` + one-or-more spaces) at the START of an
/// inline Text node, used when splitting a reference-image figure caption off
/// its leading text. Mirrors `caption_content`'s delimiter: a space, not a tab.
/// Content-emptiness is decided separately (`caption_first_line_has_content`),
/// because the caption's content may live in a following inline node (`^ *b*`,
/// where the marker node is just `"^ "` and `*b*` is an Emphasis sibling).
fn caption_marker_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'^') || bytes.get(1) != Some(&b' ') {
        return None;
    }
    let mut n = 1;
    while n < bytes.len() && bytes[n] == b' ' {
        n += 1;
    }
    Some(n)
}

/// Whether a string carries caption content: at least one non-ASCII-whitespace
/// byte. A non-breaking space (U+00A0) and any other non-ASCII byte count as
/// content, matching the direct-caption path (`caption_content` trims only
/// ASCII whitespace) and carve-php's byte-mode `\S`. `str::trim` is
/// Unicode-aware and would wrongly drop NBSP, so test bytes directly.
fn has_caption_content(s: &str) -> bool {
    s.bytes().any(|b| !b.is_ascii_whitespace())
}

/// Whether a `[Image, SoftBreak, "^ …", …]` paragraph's caption carries any
/// content on its FIRST line: text after the `^ ` marker on the marker node, or
/// any following inline node before the first soft break. Rejects an empty
/// first-line caption (`^ ` with content only on later folded lines, or none).
fn caption_first_line_has_content(children: &[InlineNode]) -> bool {
    if let InlineNode::Text(t) = &children[2] {
        if let Some(n) = caption_marker_len(&t.value) {
            if has_caption_content(&t.value[n..]) {
                return true;
            }
        }
    }
    for child in &children[3..] {
        match child {
            InlineNode::SoftBreak(_) => break,
            InlineNode::Text(t) if !has_caption_content(&t.value) => continue,
            _ => return true,
        }
    }
    false
}

fn promote_block_images(blocks: &mut [BlockNode], figures_only: bool) {
    for block in blocks.iter_mut() {
        // The sole-image -> block-image promotion is skipped in `figures_only`
        // mode (the formatter): a paragraph and a bare block image serialize
        // identically, so the only effect there would be dropping a leading
        // block-attribute line (`{#id}`) that the paragraph carries but a bare
        // block image cannot. The formatter keeps it a paragraph so those attrs
        // survive.
        //
        // Only a REAL image (direct or resolved reference) promotes. An
        // unresolved reference image keeps its `ref_label` and renders as
        // literal source inside the paragraph; promoting it would drop that
        // required `<p>` wrapper.
        let single_image = !figures_only
            && matches!(
                block,
                BlockNode::Paragraph(p)
                    if p.children.len() == 1
                        && matches!(&p.children[0], InlineNode::Image(img) if !is_unresolved_image(img))
            );
        if single_image {
            // Take the children out first so the paragraph borrow ends before
            // `block` is reassigned. A leading block-attribute line (`{#id}`)
            // landed on the paragraph; carry it onto the promoted block image
            // (its own inline attrs win on conflict, §15), matching a direct
            // block image -- otherwise the id would be lost with the wrapper.
            let (mut children, para_attrs) = match block {
                BlockNode::Paragraph(p) => (std::mem::take(&mut p.children), p.attrs.take()),
                _ => unreachable!(),
            };
            if let InlineNode::Image(mut img) = children.remove(0) {
                if let Some(attrs) = para_attrs {
                    merge_leading_attrs(&mut img.attrs, attrs);
                }
                *block = BlockNode::BlockImage(img);
            }
            continue;
        }
        // A resolved reference image on its own line followed by a `^ ` caption
        // becomes a Figure, matching a direct-image figure and carve-php. A
        // reference image arrives here as `Paragraph[Image, SoftBreak,
        // "^ caption…"]` (the syntactic block-image/caption pass only knows the
        // inline `![…](…)` form); an unresolved ref keeps `ref_label` and stays
        // literal. The caption inlines are already parsed
        // (paragraph interruption already stopped the caption at a block opener,
        // so a multi-line caption keeps its interior soft breaks); strip the
        // `^ ` marker from the leading Text.
        // Strict column-0 (docs/divergence-from-djot.md §11): the image must have
        // sat at its container's content column. An INDENTED image + caption is
        // literal paragraph text (a `<p>` with an inline image and a literal
        // `^ caption` line), matching carve-php / carve-js -- so gate on
        // `at_content_column`. A flush-left DIRECT image + caption never reaches
        // here (it became a Figure at parse time); this path serves resolved
        // REFERENCE images, which likewise promote only when flush-left.
        let ref_figure = matches!(
            block,
            BlockNode::Paragraph(p)
                if p.at_content_column
                    && p.children.len() >= 3
                    && matches!(&p.children[0], InlineNode::Image(img) if !is_unresolved_image(img))
                    && matches!(p.children[1], InlineNode::SoftBreak(_))
                    && matches!(&p.children[2], InlineNode::Text(t) if caption_marker_len(&t.value).is_some())
                    && caption_first_line_has_content(&p.children)
        );
        if ref_figure {
            // Carry a leading block-attribute line (`{#id}` etc.) from the
            // paragraph onto the figure, matching a direct-image figure (which
            // takes the attrs at parse time) and carve-php -- otherwise
            // `carve fmt` would drop it.
            // The paragraph's own span IS the figure's: it opened at the
            // image and ran to the end of the caption, which is exactly the
            // construct the author wrote. Taken here, before the paragraph is
            // dismantled, because nothing downstream can reconstruct it -- the
            // image and the caption inlines are placed, but the figure's own
            // extent only exists on the node being replaced (carve-rs#737).
            let (mut children, attrs, para_pos) = match block {
                BlockNode::Paragraph(p) => (
                    std::mem::take(&mut p.children),
                    p.attrs.take(),
                    p.pos.take(),
                ),
                _ => unreachable!(),
            };
            let InlineNode::Image(img) = children.remove(0) else {
                unreachable!()
            };
            children.remove(0); // the soft break
            if let InlineNode::Text(t) = &mut children[0] {
                let n = caption_marker_len(&t.value).unwrap();
                let rest = t.value[n..].to_string();
                if rest.is_empty() {
                    children.remove(0);
                } else {
                    t.value = rest;
                    // The SPAN moves with the value. Stripping the marker from
                    // the text and leaving the position covering it left a node
                    // whose span did not slice back to its own content - span
                    // 9..14 reading `^ cap` for the value `cap` (carve-rs#620,
                    // corpus 207). The direct-image path never had this: it
                    // parses the caption from the text after the marker, so its
                    // anchor is right to begin with, and only this post-parse
                    // promotion edits a node the parser already positioned.
                    //
                    // The marker is `^` plus spaces, so bytes and codepoints
                    // advance together and one `n` serves both the offset and
                    // the column.
                    if let Some(pos) = &mut t.pos {
                        pos.start_offset += n;
                        pos.start_column += n;
                    }
                }
            }
            *block = BlockNode::Figure(Figure {
                attrs,
                target: Box::new(FigureTarget::Image(img)),
                caption: children,
                short_caption: None,
                // PART 12 §4 exempts a REASSEMBLED node, and this one is not:
                // its lines are contiguous and the direct-image path publishes
                // exactly this span for the same construct. markup-carve/carve#913
                // rules `pos` markup-inclusive with a parent's span containing
                // every child's, which the paragraph's span already satisfies.
                pos: para_pos,
            });
            continue;
        }
        match block {
            BlockNode::BlockQuote(b) => promote_block_images(&mut b.children, figures_only),
            BlockNode::Admonition(a) => promote_block_images(&mut a.children, figures_only),
            // Descend into the group so an image-with-caption paragraph built
            // from a resolved reference image still becomes a panel (§4c).
            BlockNode::FigureGroup(g) => promote_block_images(&mut g.children, figures_only),
            BlockNode::Div(d) => promote_block_images(&mut d.children, figures_only),
            BlockNode::List(l) => {
                for item in &mut l.items {
                    promote_block_images(&mut item.children, figures_only);
                }
            }
            BlockNode::DefinitionList(d) => {
                for item in &mut d.items {
                    for def in &mut item.definitions {
                        promote_block_images(def, figures_only);
                    }
                }
            }
            _ => {}
        }
    }
}

/// An image the document never resolved: it carries a reference and no source.
///
/// PART 12 §3a keeps `ref` and `raw_ref` on a RESOLVED reference as well, so the
/// presence of a label stopped answering this on its own - a resolved reference
/// image stopped promoting to a figure (carve#597).
fn is_unresolved_image(image: &Image) -> bool {
    image.ref_label.is_some() && image.src.is_empty()
}

fn is_collapsed_reference(link: &Link) -> bool {
    let Some(raw) = &link.raw_ref else {
        return false;
    };
    let bytes = raw.as_bytes();
    let Some((_, after_text)) = read_bracketed(bytes, 0) else {
        return false;
    };
    let Some((label, _)) = read_bracketed(bytes, after_text) else {
        return false;
    };
    label.is_empty()
}

/// Every explicit `{#id}` in these blocks, for the auto-slug skip below.
///
/// Mirrors `document_ids`'s pass A: an auto slug must not land on an id an
/// explicit one already claims, and deciding that needs the whole document
/// first, since the explicit id may appear after the heading that would
/// collide with it (#335).
fn collect_explicit_ids(blocks: &[BlockNode], out: &mut std::collections::BTreeSet<String>) {
    for block in blocks {
        match block {
            BlockNode::Heading(h) => {
                if let Some(id) = h.attrs.as_ref().and_then(|a| a.id.as_ref()) {
                    out.insert(id.clone());
                }
            }
            BlockNode::Paragraph(p) => {
                if let Some(id) = p.attrs.as_ref().and_then(|a| a.id.as_ref()) {
                    out.insert(id.clone());
                }
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_explicit_ids(&item.children, out);
                }
            }
            BlockNode::BlockQuote(b) => collect_explicit_ids(&b.children, out),
            BlockNode::Admonition(a) => collect_explicit_ids(&a.children, out),
            BlockNode::FigureGroup(g) => collect_explicit_ids(&g.children, out),
            BlockNode::Div(d) => collect_explicit_ids(&d.children, out),
            BlockNode::DefinitionList(d) => {
                for item in &d.items {
                    for definition in &item.definitions {
                        collect_explicit_ids(definition, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// `in_blockquote`: the heading still gets an id and stays a `</#id>` target,
/// but PART 11 R1 declines it from the implicit `[label][]` reference index -
/// quoted text names the QUOTED document's headings, not this one's. Recorded
/// rather than skipped, because the two lookups share one walk.
/// The four accumulators `collect_heading_titles` threads through its walk,
/// grouped so the recursion carries one `&mut` rather than a widening argument
/// list.
struct HeadingScan<'a> {
    counts: &'a mut BTreeMap<String, usize>,
    titles: &'a mut BTreeMap<String, String>,
    /// Id -> the heading's own inline NODES, which is what a resolved
    /// cross-reference clones (PART 9R R4). See `CrossrefIndex::labels`.
    labels: &'a mut BTreeMap<String, CrossrefLabel>,
    quoted: &'a mut std::collections::BTreeSet<String>,
    /// Normalized heading text -> id, first occurrence wins (PART 11 R1).
    by_text: &'a mut BTreeMap<String, String>,
}

fn collect_heading_titles(
    blocks: &[BlockNode],
    scan: &mut HeadingScan<'_>,
    lowercase_ids: bool,
    explicit_ids: &std::collections::BTreeSet<String>,
    in_blockquote: bool,
) {
    for block in blocks {
        match block {
            BlockNode::Heading(h) => {
                let title = plain_inlines_parse(&h.children);
                let base = h
                    .attrs
                    .as_ref()
                    .and_then(|attrs| attrs.id.clone())
                    .unwrap_or_else(|| slugify_parse(&title, lowercase_ids));
                // Same numbering the renderer uses, INCLUDING the skip past an
                // id an explicit `{#id}` already claims. Without it this index
                // assigned `API-2` to a heading the renderer calls `API-3`, so a
                // cross-reference resolved to the wrong heading - or, once the
                // renderer was fixed, to none at all (#335).
                let has_explicit = h.attrs.as_ref().is_some_and(|a| a.id.is_some());
                let mut count = scan.counts.get(&base).copied().unwrap_or(0);
                let id = loop {
                    count += 1;
                    let candidate = if count == 1 {
                        base.clone()
                    } else {
                        format!("{base}-{count}")
                    };
                    if has_explicit || !explicit_ids.contains(&candidate) {
                        break candidate;
                    }
                };
                scan.counts.insert(base, count);
                if in_blockquote {
                    scan.quoted.insert(id.clone());
                } else {
                    // First occurrence wins - R1 resolves to "the FIRST heading
                    // with that text". This walk is in document order, so an
                    // `or_insert` is that rule. A scan.quoted heading is not indexed
                    // at all, so a later unquoted one with the same text still
                    // wins rather than being shadowed by a declined entry.
                    let text_key = normalize_heading_label(&title);
                    if !text_key.is_empty() {
                        scan.by_text.entry(text_key).or_insert_with(|| id.clone());
                    }
                }
                scan.labels
                    .insert(id.clone(), crossref_label_nodes(&h.children));
                scan.titles.insert(id, title);
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_heading_titles(
                        &item.children,
                        scan,
                        lowercase_ids,
                        explicit_ids,
                        in_blockquote,
                    );
                }
            }
            BlockNode::BlockQuote(b) => {
                collect_heading_titles(&b.children, scan, lowercase_ids, explicit_ids, true)
            }
            BlockNode::Admonition(a) => collect_heading_titles(
                &a.children,
                scan,
                lowercase_ids,
                explicit_ids,
                in_blockquote,
            ),
            BlockNode::FigureGroup(g) => collect_heading_titles(
                &g.children,
                scan,
                lowercase_ids,
                explicit_ids,
                in_blockquote,
            ),
            BlockNode::Div(d) => collect_heading_titles(
                &d.children,
                scan,
                lowercase_ids,
                explicit_ids,
                in_blockquote,
            ),
            BlockNode::DefinitionList(d) => {
                for item in &d.items {
                    for definition in &item.definitions {
                        collect_heading_titles(
                            definition,
                            scan,
                            lowercase_ids,
                            explicit_ids,
                            in_blockquote,
                        );
                    }
                }
            }
            BlockNode::Figure(f) => match &*f.target {
                FigureTarget::BlockQuote(b) => {
                    collect_heading_titles(&b.children, scan, lowercase_ids, explicit_ids, true)
                }
                FigureTarget::Table(_)
                | FigureTarget::Image(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Paragraph(_) => {}
            },
            _ => {}
        }
    }
}

fn number_captioned_blocks(
    blocks: &mut [BlockNode],
    counts: &mut BTreeMap<String, usize>,
    titles: &mut BTreeMap<String, String>,
) {
    for block in blocks {
        match block {
            BlockNode::Table(t) => number_table_caption(t, counts, titles),
            BlockNode::Figure(f) => {
                number_caption(&mut f.caption, f.attrs.as_ref(), counts, titles);
                match &mut *f.target {
                    FigureTarget::BlockQuote(b) => {
                        number_captioned_blocks(&mut b.children, counts, titles);
                    }
                    FigureTarget::Table(t) => number_table_caption(t, counts, titles),
                    FigureTarget::Image(_)
                    | FigureTarget::CodeBlock(_)
                    | FigureTarget::Paragraph(_) => {}
                }
            }
            BlockNode::FigureGroup(group) => {
                // THE GROUP IS ONE NUMBERING UNIT (§4c). Its caption draws
                // first - before anything inside the body, matching the
                // oracle - and that one draw is also what the panel ids
                // register under, with a letter by panel order.
                let drew = group.caption.as_mut().and_then(|caption| {
                    number_caption(caption, group.attrs.as_ref(), counts, titles)
                });
                if let Some((label, number)) = &drew {
                    register_panel_titles(&group.children, label, *number, titles);
                }
                // PANELS ARE NOT SEQUENCE UNITS: a panel's own caption draws
                // nothing (a `#` there stays literal, §4c), but content
                // inside a quote panel and every non-panel child numbers
                // normally.
                for child in &mut group.children {
                    match child {
                        BlockNode::Figure(f) => {
                            if let FigureTarget::BlockQuote(b) = &mut *f.target {
                                number_captioned_blocks(&mut b.children, counts, titles);
                            }
                        }
                        BlockNode::Table(_) => {}
                        other => {
                            number_captioned_blocks(std::slice::from_mut(other), counts, titles)
                        }
                    }
                }
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    number_captioned_blocks(&mut item.children, counts, titles);
                }
            }
            BlockNode::BlockQuote(b) => number_captioned_blocks(&mut b.children, counts, titles),
            BlockNode::Admonition(a) => number_captioned_blocks(&mut a.children, counts, titles),
            BlockNode::Div(d) => number_captioned_blocks(&mut d.children, counts, titles),
            BlockNode::DefinitionList(d) => {
                for item in &mut d.items {
                    for definition in &mut item.definitions {
                        number_captioned_blocks(definition, counts, titles);
                    }
                }
            }
            _ => {}
        }
    }
}

fn number_table_caption(
    table: &mut Table,
    counts: &mut BTreeMap<String, usize>,
    titles: &mut BTreeMap<String, String>,
) {
    if let Some(caption) = &mut table.caption {
        number_caption(caption, table.attrs.as_ref(), counts, titles);
    }
}

/// Returns the label and number the caption drew, when it held a `#`
/// placeholder - the figure group's arm derives its panels' crossref text
/// from that draw (§4c).
fn number_caption(
    caption: &mut [InlineNode],
    attrs: Option<&Attrs>,
    counts: &mut BTreeMap<String, usize>,
    titles: &mut BTreeMap<String, String>,
) -> Option<(String, usize)> {
    let idx = caption
        .iter()
        .position(|node| matches!(node, InlineNode::CaptionNumber(_)))?;
    let label = plain_inlines_parse(&caption[..idx])
        .trim_end_matches(char::is_whitespace)
        .to_string();
    let next = counts.entry(label.clone()).or_insert(0);
    *next += 1;
    let number = *next;
    if let InlineNode::CaptionNumber(caption_number) = &mut caption[idx] {
        caption_number.number = Some(number);
    }
    if let Some(id) = attrs.and_then(|attrs| attrs.id.as_ref()) {
        titles
            .entry(id.clone())
            .or_insert_with(|| format!("{label} {number}"));
    }
    Some((label, number))
}

/// A panel's crossref letter by its order among the group's panels: a..z,
/// then aa, ab, ... (PART 9 §4c; the letters exist in crossref text only).
fn panel_letter(index: usize) -> String {
    let mut out = Vec::new();
    let mut n = index + 1;
    while n > 0 {
        n -= 1;
        out.push(b'a' + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).expect("ascii letters")
}

/// Register a numbered group's panel ids as "Label N" plus a letter by panel
/// order (§4c). Panels are the `Figure` and `Table` nodes among the group's
/// direct children; an unnumbered group's panels stay plain anchors, exactly
/// as an id on an uncaptioned figure does.
fn register_panel_titles(
    children: &[BlockNode],
    label: &str,
    number: usize,
    titles: &mut BTreeMap<String, String>,
) {
    let mut panel_index = 0usize;
    for child in children {
        let id = match child {
            BlockNode::Figure(f) => f.attrs.as_ref().and_then(|attrs| attrs.id.clone()),
            BlockNode::Table(t) => t.attrs.as_ref().and_then(|attrs| attrs.id.clone()),
            _ => continue,
        };
        if let Some(id) = id {
            titles
                .entry(id)
                .or_insert_with(|| format!("{label} {number}{}", panel_letter(panel_index)));
        }
        panel_index += 1;
    }
}

fn collect_caption_titles(blocks: &[BlockNode], titles: &mut BTreeMap<String, String>) {
    for block in blocks {
        match block {
            BlockNode::Table(t) => collect_table_caption_title(t, titles),
            BlockNode::Figure(f) => {
                collect_caption_title(&f.caption, f.attrs.as_ref(), titles);
                match &*f.target {
                    FigureTarget::BlockQuote(b) => collect_caption_titles(&b.children, titles),
                    FigureTarget::Table(t) => collect_table_caption_title(t, titles),
                    FigureTarget::Image(_)
                    | FigureTarget::CodeBlock(_)
                    | FigureTarget::Paragraph(_) => {}
                }
            }
            BlockNode::FigureGroup(g) => {
                // The ingest twin of the group arm in `number_captioned_blocks`:
                // read the number the group's caption already carries, register
                // the group id and the panel letters from that one draw, and
                // skip the panel captions exactly as the numbering pass does.
                if let Some(caption) = &g.caption {
                    collect_caption_title(caption, g.attrs.as_ref(), titles);
                    if let Some((label, number)) = numbered_caption_draw(caption) {
                        register_panel_titles(&g.children, &label, number, titles);
                    }
                }
                for child in &g.children {
                    match child {
                        BlockNode::Figure(f) => {
                            if let FigureTarget::BlockQuote(b) = &*f.target {
                                collect_caption_titles(&b.children, titles);
                            }
                        }
                        BlockNode::Table(_) => {}
                        other => collect_caption_titles(std::slice::from_ref(other), titles),
                    }
                }
            }
            BlockNode::List(l) => {
                for item in &l.items {
                    collect_caption_titles(&item.children, titles);
                }
            }
            BlockNode::BlockQuote(b) => collect_caption_titles(&b.children, titles),
            BlockNode::Admonition(a) => collect_caption_titles(&a.children, titles),
            BlockNode::Div(d) => collect_caption_titles(&d.children, titles),
            BlockNode::DefinitionList(d) => {
                for item in &d.items {
                    for definition in &item.definitions {
                        collect_caption_titles(definition, titles);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The label and number an ALREADY-NUMBERED caption carries, read without
/// assigning anything - the ingest-side twin of `number_caption`'s return.
fn numbered_caption_draw(caption: &[InlineNode]) -> Option<(String, usize)> {
    let idx = caption
        .iter()
        .position(|node| matches!(node, InlineNode::CaptionNumber(_)))?;
    let number = match &caption[idx] {
        InlineNode::CaptionNumber(n) => n.number?,
        _ => return None,
    };
    let label = plain_inlines_parse(&caption[..idx])
        .trim_end_matches(char::is_whitespace)
        .to_string();
    Some((label, number))
}

fn collect_table_caption_title(table: &Table, titles: &mut BTreeMap<String, String>) {
    if let Some(caption) = &table.caption {
        collect_caption_title(caption, table.attrs.as_ref(), titles);
    }
}

fn collect_caption_title(
    caption: &[InlineNode],
    attrs: Option<&Attrs>,
    titles: &mut BTreeMap<String, String>,
) {
    let Some(idx) = caption
        .iter()
        .position(|node| matches!(node, InlineNode::CaptionNumber(_)))
    else {
        return;
    };
    let Some(number) = caption.get(idx).and_then(|node| match node {
        InlineNode::CaptionNumber(n) => n.number,
        _ => None,
    }) else {
        return;
    };
    if let Some(id) = attrs.and_then(|attrs| attrs.id.as_ref()) {
        let label = plain_inlines_parse(&caption[..idx])
            .trim_end_matches(char::is_whitespace)
            .to_string();
        titles
            .entry(id.clone())
            .or_insert_with(|| format!("{label} {number}"));
    }
}

/// Enforce "links never nest" (CommonMark: a link may not contain another
/// link). This is a single post-resolution pass: it runs AFTER reference-link
/// resolution because reference links turn into `Link` nodes at that stage. A
/// link found inside another link is unwrapped to its (recursively cleaned)
/// text, so only the outermost destination applies; an autolink inside a link becomes plain text
/// (the display form the renderer would emit, with a leading `mailto:` scheme
/// stripped). A footnote body renders in the endnotes section, outside any
/// anchor, so its links are not nested -- the walk re-enters a footnote body
/// with `inside_link = false`.
/// PART 12 §1a: no node's children hold two adjacent `text` nodes.
///
/// The parser splits a run wherever it had to make a decision -- a reference
/// that never resolved and stayed as its reference node, an autolink unwrapped
/// because links do not nest, a table cell rebuilt from several lines. Those
/// splits are bookkeeping, not the document: publishing them lets two engines
/// put out 1 node and 4 for the same characters, both valid against the schema,
/// which is what §1's "read another's output" exists to rule out.
///
/// This runs on the TREE rather than on the way out, because §6 requires
/// `parse(x)` serialized and deserialized to equal `parse(x)`. Merging only at
/// the encoder would satisfy §1a and break §6 on the same document.
///
/// Runs merge only where they are CONTIGUOUS in the source. A span covering
/// text the node does not contain -- the `<`/`>` of an unwrapped autolink, the
/// delimiter between two halves of a wrapped table cell -- would not select its
/// own value, which §4 rates worse than no span at all.
fn coalesce_text_runs(doc: &mut Document) {
    for block in &mut doc.children {
        coalesce_block(block);
    }
    for body in doc.footnote_defs.values_mut() {
        for block in body {
            coalesce_block(block);
        }
    }
}

fn coalesce_block(block: &mut BlockNode) {
    match block {
        BlockNode::Heading(h) => coalesce_inlines(&mut h.children),
        BlockNode::Paragraph(p) => coalesce_inlines(&mut p.children),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    coalesce_block(child);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                coalesce_block(child);
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                coalesce_inlines(title);
            }
            for child in &mut a.children {
                coalesce_block(child);
            }
        }
        BlockNode::FigureGroup(g) => {
            if let Some(caption) = &mut g.caption {
                coalesce_inlines(caption);
            }
            for child in &mut g.children {
                coalesce_block(child);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                coalesce_block(child);
            }
        }
        BlockNode::LineBlock(l) => {
            for child in &mut l.children {
                coalesce_block(child);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    coalesce_inlines(&mut term.children);
                }
                for definition in &mut item.definitions {
                    for child in &mut definition.children {
                        coalesce_block(child);
                    }
                }
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                coalesce_inlines(caption);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    coalesce_inlines(&mut cell.children);
                }
            }
        }
        BlockNode::Extension(e) => {
            // Inline content does not only live in `children`: a `before_render`
            // rewrite stashes a parsed title in `summary`, and an extension can
            // wrap already-parsed blocks. A walk that stops at the carrier node
            // leaves both uncoalesced.
            if let Some(summary) = &mut e.summary {
                coalesce_inlines(summary);
            }
            for child in &mut e.children {
                coalesce_block(child);
            }
        }
        BlockNode::Figure(f) => {
            coalesce_inlines(&mut f.caption);
            match &mut *f.target {
                FigureTarget::BlockQuote(b) => {
                    for child in &mut b.children {
                        coalesce_block(child);
                    }
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        coalesce_inlines(caption);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            coalesce_inlines(&mut cell.children);
                        }
                    }
                }
                FigureTarget::Paragraph(p) => coalesce_inlines(&mut p.children),
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
        _ => {}
    }
}

fn coalesce_inlines(nodes: &mut Vec<InlineNode>) {
    for node in nodes.iter_mut() {
        match node {
            InlineNode::Emphasis(n) => coalesce_inlines(&mut n.children),
            InlineNode::Link(n) => coalesce_inlines(&mut n.children),
            InlineNode::Span(n) => coalesce_inlines(&mut n.children),
            InlineNode::Extension(n) => coalesce_inlines(&mut n.children),
            InlineNode::CriticInsert(n) => coalesce_inlines(&mut n.children),
            InlineNode::CriticDelete(n) => coalesce_inlines(&mut n.children),
            InlineNode::Footnote(n) => {
                if let Some(inline) = &mut n.inline {
                    coalesce_inlines(inline);
                }
            }
            // A citation item carries THREE inline arrays beside `children`:
            // `prefix`, `locator` and `suffix`. `[see [missing][nope] @a]`
            // publishes a prefix of two adjacent text nodes without this.
            InlineNode::CitationGroup(group) => {
                for item in &mut group.items {
                    for field in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        coalesce_inlines(field);
                    }
                }
            }
            _ => {}
        }
    }

    if nodes.len() < 2 {
        return;
    }
    let taken = std::mem::take(nodes);
    let mut out: Vec<InlineNode> = Vec::with_capacity(taken.len());
    for node in taken {
        match node {
            InlineNode::Text(text) => {
                if let Some(InlineNode::Text(previous)) = out.last_mut() {
                    previous.pos = merged_text_pos(previous.pos, text.pos);
                    previous.value.push_str(&text.value);
                    continue;
                }
                out.push(InlineNode::Text(text));
            }
            other => out.push(other),
        }
    }
    *nodes = out;
}

/// The span of two merged text runs, or None when it would not be truthful.
fn merged_text_pos(left: Option<Pos>, right: Option<Pos>) -> Option<Pos> {
    let (left, right) = (left?, right?);
    if left.end_offset != right.start_offset {
        return None;
    }
    Some(Pos {
        end_line: right.end_line,
        end_column: right.end_column,
        end_offset: right.end_offset,
        ..left
    })
}

/// The span of the text a nested autolink unwraps to.
///
/// A link cannot contain a link, so `[pre <http://h> post](/u)` keeps only the
/// autolink's DISPLAY text - and that text is a sub-slice of what the autolink
/// occupied, not the whole of it. Handing over the autolink's own span would
/// cover the `<` and `>` too, so the span would not select the text it belongs
/// to, which is worse than leaving it unplaced.
///
/// The narrowing is only applied when the arithmetic is unambiguous: the source
/// is either exactly the display text (a bare URL) or the display text inside
/// one delimiter on each side (`<...>`). Anything else - a `mailto:` the author
/// wrote out, an unusual spelling - yields None rather than a guess.
fn unwrapped_autolink_pos(link: &AutoLink, display: &str) -> Option<Pos> {
    let pos = link.pos?;
    let width = pos.end_column.checked_sub(pos.start_column)?;
    let shown = display.chars().count();

    if width == shown {
        return Some(pos);
    }
    if width == shown + 2 && pos.start_line == pos.end_line {
        return Some(Pos {
            start_column: pos.start_column + 1,
            end_column: pos.end_column - 1,
            start_offset: pos.start_offset + 1,
            end_offset: pos.end_offset - 1,
            ..pos
        });
    }

    None
}

/// "Links never nest" is a RENDERING rule that binds the renderer and not the
/// encoder (PART 12 section 3a, markup-carve/carve#817), so the tree keeps the
/// link or autolink the author wrote inside a label and every renderer unwraps
/// it here.
pub(crate) fn unwrap_nested_anchors(children: &[InlineNode]) -> std::borrow::Cow<'_, [InlineNode]> {
    if holds_nested_anchor(children) {
        std::borrow::Cow::Owned(enforce_no_nesting_inline(children.to_vec(), true))
    } else {
        std::borrow::Cow::Borrowed(children)
    }
}

/// The ONE spelling of "this link never resolved". Its readers shadow each
/// other: the fast path below and the fold itself short-circuit in sequence, so
/// with the predicate written twice, flipping EITHER copy alone changes no
/// output and a mutation on either comes back green while the pair is
/// load-bearing. The footnote numbering pass reads it for the same reason - it
/// has to reach the same answer `render_link` does about the same node, and two
/// spellings of that are two things to keep in step (PART 9R R2).
pub(crate) fn is_unresolved_reference(link: &Link) -> bool {
    link.ref_label.is_some() && link.href.is_empty()
}

fn holds_nested_anchor(nodes: &[InlineNode]) -> bool {
    nodes.iter().any(|node| match node {
        InlineNode::AutoLink(_) => true,
        InlineNode::Link(link) => {
            !is_unresolved_reference(link) || holds_nested_anchor(&link.children)
        }
        InlineNode::Emphasis(emphasis) => holds_nested_anchor(&emphasis.children),
        InlineNode::Span(span) => holds_nested_anchor(&span.children),
        InlineNode::Extension(ext) => holds_nested_anchor(&ext.children),
        InlineNode::CriticInsert(critic) => holds_nested_anchor(&critic.children),
        InlineNode::CriticDelete(critic) => holds_nested_anchor(&critic.children),
        InlineNode::Text(_)
        | InlineNode::EscapedText(_)
        | InlineNode::SmartPunctuation(_)
        | InlineNode::Code(_)
        | InlineNode::Image(_)
        | InlineNode::Math(_)
        | InlineNode::RawInline(_)
        | InlineNode::LiteralInline(_)
        | InlineNode::Symbol(_)
        | InlineNode::CrossRef(_)
        | InlineNode::CaptionNumber(_)
        | InlineNode::Mention(_)
        | InlineNode::Tag(_)
        | InlineNode::CitationGroup(_)
        | InlineNode::Abbreviation(_)
        | InlineNode::Footnote(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticSubstitute(_)
        | InlineNode::CriticComment(_)
        | InlineNode::Comment(_) => false,
    })
}

fn enforce_no_nesting_inline(nodes: Vec<InlineNode>, inside_link: bool) -> Vec<InlineNode> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            InlineNode::Link(mut link) => {
                let unresolved = is_unresolved_reference(&link);
                let children = enforce_no_nesting_inline(link.children, true);
                if inside_link && !unresolved {
                    // A nested link is dropped; only its (cleaned) text remains
                    // because the outermost destination already applies.
                    out.extend(children);
                } else {
                    link.children = children;
                    out.push(InlineNode::Link(link));
                }
            }
            InlineNode::AutoLink(a) => {
                if inside_link {
                    let display = a
                        .href
                        .strip_prefix("mailto:")
                        .unwrap_or(&a.href)
                        .to_string();
                    out.push(InlineNode::Text(Text {
                        pos: unwrapped_autolink_pos(&a, &display),
                        value: display,
                    }));
                } else {
                    out.push(InlineNode::AutoLink(a));
                }
            }
            InlineNode::Footnote(mut f) => {
                // A footnote body renders outside the anchor, so its links are
                // not nested: re-enter with inside_link = false.
                if let Some(inline) = f.inline.take() {
                    f.inline = Some(enforce_no_nesting_inline(inline, false));
                }
                out.push(InlineNode::Footnote(f));
            }
            InlineNode::Emphasis(mut e) => {
                e.children = enforce_no_nesting_inline(e.children, inside_link);
                out.push(InlineNode::Emphasis(e));
            }
            InlineNode::Span(mut s) => {
                s.children = enforce_no_nesting_inline(s.children, inside_link);
                out.push(InlineNode::Span(s));
            }
            InlineNode::Extension(mut ext) => {
                ext.children = enforce_no_nesting_inline(ext.children, inside_link);
                out.push(InlineNode::Extension(ext));
            }
            InlineNode::CriticInsert(mut c) => {
                c.children = enforce_no_nesting_inline(c.children, inside_link);
                out.push(InlineNode::CriticInsert(c));
            }
            InlineNode::CriticDelete(mut c) => {
                c.children = enforce_no_nesting_inline(c.children, inside_link);
                out.push(InlineNode::CriticDelete(c));
            }
            other => out.push(other),
        }
    }
    out
}

fn plain_inlines_parse(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(s) => out.push_str(&s.value),
            // An escaped character is VISIBLE prose - `\*` renders as `*` - so it
            // contributes to every text derived from this run, exactly as the
            // text around it does. Without this arm the `_ => {}` below swallowed
            // it, and a heading the author escaped got a title, an id and a
            // PART 9R R1 `by_text` key with the escaped characters missing
            // (carve-rs#800). carve-js `inlineText` and carve-php
            // `inlineTextLeaf` both carry the same arm.
            InlineNode::EscapedText(e) => out.push_str(&e.value),
            InlineNode::SmartPunctuation(s) => out.push_str(smart_punctuation_glyph(s)),
            InlineNode::Emphasis(e) => out.push_str(&plain_inlines_parse(&e.children)),
            InlineNode::Code(s) => out.push_str(&s.value),
            // An inline literal renders as visible prose (§27), so it feeds the
            // parse-time cross-reference slug like a code span does.
            InlineNode::LiteralInline(l) => out.push_str(&l.content),
            // Math is verbatim text the reader sees, so it feeds the parse-time
            // slug exactly as a code span does. MEASURED: this arm feeds PART 9R
            // R1's `by_text` index, and `plain_inlines_typography` is the one the
            // rendered id derives through. Without this one a heading published
            // `id="a-x-b"` while `[a $`x` b][]` still resolved to `a-b`, linking
            // to an anchor no element carried (carve#1283).
            InlineNode::Math(m) => out.push_str(&m.content),
            InlineNode::Link(l) => out.push_str(&plain_inlines_parse(&l.children)),
            InlineNode::AutoLink(a) => out.push_str(&a.text),
            InlineNode::Image(i) => out.push_str(&i.alt),
            InlineNode::Extension(e) => out.push_str(&plain_inlines_parse(&e.children)),
            InlineNode::CitationGroup(g) => out.push_str(&g.raw),
            InlineNode::Abbreviation(a) => out.push_str(&a.abbr),
            InlineNode::Mention(m) => {
                out.push('@');
                out.push_str(&m.user);
            }
            InlineNode::Tag(t) => {
                out.push('#');
                out.push_str(&t.name);
            }
            InlineNode::CaptionNumber(n) => {
                if let Some(number) = n.number {
                    out.push_str(&number.to_string());
                }
            }
            // A soft/hard break is a word separator, so parse-time
            // cross-reference slugs match the rendered heading id. (A heading
            // cannot hold one from a parse any more -- headings end at the
            // newline -- but an ingested AST can carry any inline, PART 12.)
            InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) => out.push(' '),
            _ => {}
        }
    }
    out
}

/// Carve "Automatic Identifiers" slug (spec #73). The single canonical
/// implementation, shared by the HTML and Markdown renderers so all id
/// derivation in carve-rs stays byte-identical (and identical to carve-js /
/// carve-php).
/// Reverse smart-typography substitutions to their ASCII source, so a heading
/// id never depends on presentational typography. The inverse of the parser's
/// smart tokens plus smart quotes and dashes; the recovered ASCII punctuation
/// then collapses in the slug run. Kept byte-identical to carve-js / carve-php.
fn de_typography(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '↔' => out.push_str("<->"),
            '™' => out.push_str("(tm)"),
            '…' => out.push_str("..."),
            '→' => out.push_str("->"),
            '←' => out.push_str("<-"),
            '⇒' => out.push_str("=>"),
            '≤' => out.push_str("<="),
            '≥' => out.push_str(">="),
            '≠' => out.push_str("!="),
            '±' => out.push_str("+-"),
            '©' => out.push_str("(c)"),
            '®' => out.push_str("(r)"),
            '–' | '—' => out.push('-'),
            '‘' | '’' => out.push('\''),
            '“' | '”' => out.push('"'),
            other => out.push(other),
        }
    }
    out
}

/// Code points removed from a heading-id source before slugging: the
/// bidi-override / isolate controls (also stripped from rendered text, see
/// `escape::is_bidi_control`) plus the zero-width characters that are NOT
/// stripped from text but must never leak into an `id="..."`.
fn is_id_strippable(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}'   // bidi LRE/RLE/PDF/LRO/RLO
        | '\u{2066}'..='\u{2069}' // bidi isolates LRI/RLI/FSI/PDI
        | '\u{200B}'              // zero-width space
        | '\u{200C}'              // zero-width non-joiner
        | '\u{200D}'              // zero-width joiner
        | '\u{2060}'              // word joiner
        | '\u{FEFF}'              // zero-width no-break space / BOM
        | '\u{00AD}'              // soft hyphen
    )
}

/// NFC-normalize, then drop the invisible/dangerous controls (see
/// `is_id_strippable`). The pre-slug transform that makes a generated id
/// deterministic and Trojan-Source-safe (corpus 117). Parity with carve-js
/// Give every heading the id the renderer will assign it, where the source
/// wrote none.
///
/// The id takes no `order` slot: `order` is the source-appearance order of the
/// slots in a `{#id .class key=value}` block, and a slugged id never appeared in
/// one. `render_carve` reads that back - an id with no slot, that a fresh parse
/// would re-derive, is not written into the source it produces.
fn stamp_generated_heading_ids(doc: &mut Document, lowercase: bool) {
    let ids = crate::document_ids::assigned_heading_ids(doc, lowercase);
    if ids.is_empty() {
        return;
    }
    let mut next = ids.into_iter();
    stamp_heading_ids_in(&mut doc.children, &mut next);
    let keys: Vec<String> = doc.footnote_defs.keys().cloned().collect();
    for key in keys {
        if let Some(blocks) = doc.footnote_defs.get_mut(&key) {
            stamp_heading_ids_in(blocks, &mut next);
        }
    }
}

fn stamp_heading_ids_in(blocks: &mut [BlockNode], next: &mut impl Iterator<Item = String>) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Heading(h) => {
                let Some(id) = next.next() else {
                    return;
                };
                if h.attrs.as_ref().and_then(|a| a.id.as_ref()).is_none() {
                    h.attrs.get_or_insert_with(Attrs::default).id = Some(id);
                }
            }
            BlockNode::BlockQuote(b) => stamp_heading_ids_in(&mut b.children, next),
            BlockNode::Div(d) => stamp_heading_ids_in(&mut d.children, next),
            BlockNode::Admonition(a) => stamp_heading_ids_in(&mut a.children, next),
            BlockNode::FigureGroup(g) => stamp_heading_ids_in(&mut g.children, next),
            BlockNode::List(l) => {
                for item in l.items.iter_mut() {
                    stamp_heading_ids_in(&mut item.children, next);
                }
            }
            BlockNode::Figure(f) => {
                if let FigureTarget::BlockQuote(b) = &mut *f.target {
                    stamp_heading_ids_in(&mut b.children, next);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for entry in dl.items.iter_mut() {
                    for definition in entry.definitions.iter_mut() {
                        stamp_heading_ids_in(&mut definition.children, next);
                    }
                }
            }
            _ => {}
        }
    }
}

/// `sanitizeIdSource`.
fn sanitize_id_source(text: &str) -> String {
    crate::unicode_nfc::nfc(text)
        .chars()
        .filter(|c| !is_id_strippable(*c))
        .collect()
}

pub(crate) fn slugify_parse(text: &str, lowercase: bool) -> String {
    // Carve "Automatic Identifiers" (spec #73), kept byte-identical to
    // carve-js / carve-php:
    //   - keep ASCII alphanumerics AND every non-ASCII code point (>= U+0080)
    //     verbatim; replace each maximal run of ASCII non-alphanumerics with a
    //     single '-' and trim. (Do NOT filter by Unicode is_alphanumeric: the
    //     spec keeps non-ASCII symbols, marks, and punctuation, e.g. a CJK
    //     comma or a bullet, just like the `[^0-9A-Za-z\x80-\x10FFFF]+` rule.)
    //   - smart-typography output is first reversed to its ASCII source (see
    //     de_typography) so an id never depends on presentational typography.
    //   - the DEFAULT is CASE-PRESERVING: kept characters are emitted verbatim
    //     (`# Getting Started` -> `Getting-Started`, `# Über uns` -> `Über-uns`).
    //   - when `lowercase` is set, fold kept characters per code point
    //     (`char::to_lowercase`). Per-code-point folding avoids context mappings
    //     (Greek final-sigma) so the result is portable and matches the other
    //     impls regardless of stdlib whole-string casing behavior. carve-rs has
    //     no ASCII transliterator, so ascii-folding is intentionally not offered
    //     here -- `lowercase` is the only transform.
    // Trojan-Source hardening for generated ids (corpus 117), applied BEFORE
    // the slug run so the remaining text slugs as usual:
    //   - NFC normalization, so a precomposed `é` (U+00E9) and a decomposed
    //     `e`+U+0301 produce the SAME id.
    //   - strip bidi-override / isolate controls (U+202A..U+202E, U+2066..U+2069)
    //     and zero-width characters (U+200B/C/D, U+2060, U+FEFF, U+00AD) so none
    //     of these can ever appear inside an `id="..."`.
    // Matches carve-js `sanitizeIdSource` (heading-ids.ts).
    let sanitized = sanitize_id_source(text);
    let detyped = de_typography(&sanitized);
    let mut out = String::new();
    let mut last_dash = false;
    for ch in detyped.chars() {
        if ch.is_ascii_alphanumeric() || ch as u32 >= 0x80 {
            if lowercase {
                for lc in ch.to_lowercase() {
                    out.push(lc);
                }
            } else {
                out.push(ch);
            }
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    // A leading Unicode number (\p{N}: Nd/Nl/No) is a valid HTML id but not a
    // bare CSS selector, so prefix 's-'. Empty -> 's'. Matches carve-js/php.
    if out.chars().next().is_some_and(char::is_numeric) {
        out = format!("s-{out}");
    }
    if out.is_empty() {
        "s".to_string()
    } else {
        out
    }
}

/// Forced intraword emphasis `{X…X}` (spec §22): emits the same node as the bare
/// delimiter X, but with no word-boundary condition. X is one of `/ * _ ^ , ~ =`.
/// The closing `X}` is the first one after at least one content byte, mirroring
/// the non-greedy `^\{(X)([\s\S]+?)\1\}` match. `{=html}` (no trailing `=`) does
/// not match, so raw-format attribute blocks are unaffected.
fn parse_forced_emphasis(
    bytes: &[u8],
    i: usize,
    options: &Options<'_>,
    in_footnote: bool,
    bounds: &InlineBounds<'_>,
    positions: Option<&InlinePositionMap<'_>>,
    base: usize,
) -> Option<(InlineNode, usize)> {
    let delim = bytes.get(i + 1).copied()?;
    let kind = match delim {
        b'/' => EmphasisKind::Italic,
        b'*' => EmphasisKind::Strong,
        b'_' => EmphasisKind::Underline,
        b'^' => EmphasisKind::Super,
        b',' => EmphasisKind::Sub,
        b'~' => EmphasisKind::Strike,
        b'=' => EmphasisKind::Highlight,
        _ => return None,
    };
    // The span closes on a `delim}` pair; with no such pair ahead the scan could
    // only walk to end-of-text and fail, so bail in O(1) (keeps `{/`×n linear).
    if !bounds.has_delim_brace_from(delim, i) {
        return None;
    }
    let content_start = i + 2;
    let mut j = content_start;
    while j + 1 < bytes.len() {
        if bytes[j] == delim && bytes[j + 1] == b'}' {
            if j == content_start {
                return None; // empty content: `+?` requires at least one byte
            }
            let inner = std::str::from_utf8(&bytes[content_start..j]).ok()?;
            return Some((
                InlineNode::Emphasis(Emphasis {
                    attrs: None,
                    kind,
                    children: parse_inline_context(
                        inner,
                        options,
                        false,
                        in_footnote,
                        positions,
                        base + content_start,
                    ),
                    pos: None,
                }),
                j + 2 - i,
            ));
        }
        j += 1;
    }
    None
}

fn find_seq(bytes: &[u8], from: usize, marker: &[u8]) -> Option<usize> {
    if marker.is_empty() || from + marker.len() > bytes.len() {
        return None;
    }
    let mut j = from;
    while j + marker.len() <= bytes.len() {
        if &bytes[j..j + marker.len()] == marker {
            return Some(j);
        }
        j += 1;
    }
    None
}

/// Number of distinct single-char emphasis delimiters (`/ * _ ~ =`), plus one
/// catch-all slot. Sizes the per-`parse_inline_context` no-close memo.
const EMPHASIS_DELIM_SLOTS: usize = 6;

#[inline]
fn emphasis_delim_index(delim: u8) -> usize {
    match delim {
        b'/' => 0,
        b'*' => 1,
        b'_' => 2,
        b'~' => 3,
        b'=' => 4,
        _ => 5,
    }
}

/// `find_emphasis_close` with a per-delimiter failure memo. Once an opener of a
/// given delimiter finds no valid closer scanning to end-of-text, every later
/// opener of that delimiter (a larger `from`) also fails: the main loop only
/// calls `match_emphasis` at positions outside code spans / escapes -- the same
/// positions `find_emphasis_close` treats as "clean" -- so a suffix scan from a
/// larger `from` can never expose a closer that the earlier, wider scan missed.
/// This bounds `_a](`×n / `*a](`×n at O(n) instead of O(n^2) while keeping
/// output byte-identical (skipping only ever elides a call that would fail).
fn cached_find_emphasis_close(
    bytes: &[u8],
    from: usize,
    delim: u8,
    no_close: &mut [Option<usize>; EMPHASIS_DELIM_SLOTS],
) -> Option<usize> {
    let idx = emphasis_delim_index(delim);
    if let Some(first) = no_close[idx] {
        if from >= first {
            return None;
        }
    }
    let close = find_emphasis_close(bytes, from, delim);
    if close.is_none() {
        no_close[idx] = Some(match no_close[idx] {
            Some(f) => f.min(from),
            None => from,
        });
    }
    close
}

fn find_emphasis_close(bytes: &[u8], from: usize, delim: u8) -> Option<usize> {
    let mut j = from;
    while j < bytes.len() {
        let ch = bytes[j];
        if ch == b'\\' && j + 1 < bytes.len() {
            j += 2;
            continue;
        }
        if ch == b'`' {
            // Skip a verbatim span: opener run of N backticks closes on a run
            // of exactly N. An unclosed run is opaque to end of text, so no
            // emphasis closer can follow it.
            let open_start = j;
            while j < bytes.len() && bytes[j] == b'`' {
                j += 1;
            }
            let open_len = j - open_start;
            let mut found = false;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    let close_start = j;
                    while j < bytes.len() && bytes[j] == b'`' {
                        j += 1;
                    }
                    if j - close_start == open_len {
                        found = true;
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            if !found {
                return None;
            }
            continue;
        }
        if ch == delim {
            let prev = bytes.get(j.wrapping_sub(1)).copied().unwrap_or(b' ');
            if prev == b' ' || prev == b'\n' {
                j += 1;
                continue;
            }
            // Word-boundary closer (spec §9): no bare delimiter closes when
            // followed by an alphanumeric. Applies to every delimiter.
            // NOTE: a `=` is NOT excluded here when it abuts a smart operator
            // (`=b=>`): both reference impls (carve-js, carve-php) let the
            // highlight close there, so rs matches them rather than being the
            // lone grammar-pedantic outlier on this unpinned corner. The
            // operator exclusion applies only to the OPENER (the corpus-pinned
            // `a => b` case).
            if let Some(&next) = bytes.get(j + 1) {
                if next.is_ascii_alphanumeric() {
                    j += 1;
                    continue;
                }
            }
            return Some(j);
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod container_comment_dedent_steps {
    //! COUNTED guard on the container-comment dedent walk
    //! (markup-carve/carve-rs#1047).
    //!
    //! Counted rather than timed, for the reason the module below it records at
    //! length: a call count is a property of the algorithm and reproduces
    //! byte-identically across runs and loads, while this repo's timing tests
    //! had to be serialized against each other to stop them flaking.
    //!
    //! The subject is the shape the first version of the fix had. One container
    //! can hold many comment openers, and answering "does this one close inside
    //! its container" by walking forward from each meant walking the whole
    //! container once per opener - O(m^3) work for an O(m^2) document, which is
    //! exactly the class `comment_fence_close_index` exists to avoid for
    //! column-0 openers.

    use super::CONTAINER_DEDENT_STEPS;

    /// Dedent-walk steps over one full parse-and-render of `src`, on its own
    /// thread so the thread-local tally cannot pick up another test's.
    fn steps_for(src: String) -> u64 {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                CONTAINER_DEDENT_STEPS.with(|c| c.set(0));
                let _ = crate::to_html(&src);
                CONTAINER_DEDENT_STEPS.with(|c| c.get())
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// `m` comment openers of distinct widths at one item's content column,
    /// above `m * m` filler lines, with every matching closer only past the
    /// dedent. Every opener therefore passes the document-wide "is there a
    /// closer of this width later" test and none of them closes inside the
    /// item, which is the worst case for the container bound.
    ///
    /// The work is the line count: a walk that visits the container once has to
    /// grow with it, and one that visits it per opener cannot stay flat.
    fn openers_over_one_container(m: usize) -> (String, u64) {
        let mut out = String::from("- x\n");
        for i in 0..m {
            out.push_str("  ");
            out.push_str(&"%".repeat(3 + i));
            out.push_str(" o\n");
        }
        for _ in 0..m * m {
            out.push_str("  filler line here\n");
        }
        for i in 0..m {
            out.push_str(&"%".repeat(3 + i));
            out.push('\n');
        }
        out.push_str("\n[r][]\n");
        let lines = out.lines().count() as u64;
        (out, lines)
    }

    /// The walk must cost steps in proportion to the DOCUMENT, not to openers
    /// times container length.
    ///
    /// Three claims, in the shape `quote_prefix_calls` uses:
    ///
    /// 1. A floor. The first opener at a column has to walk to the dedent to
    ///    learn where it is, so a zero count is a dead counter rather than a
    ///    faster parser, and the two claims below would both pass on one.
    /// 2. A ceiling, wide enough for honest drift: the two definition
    ///    pre-passes each hold their own memo, so one walk of the container
    ///    apiece is already two steps per line.
    /// 3. The shape, which is the load-bearing half. A per-opener walk makes the
    ///    per-line cost climb with `m` - measured at about 118 steps per line at
    ///    m=60 against 238 at m=120 - while one walk per column per pre-pass
    ///    holds it flat at about 2.
    #[test]
    fn many_openers_over_one_container_walk_it_once_apiece() {
        let (small_src, small_lines) = openers_over_one_container(60);
        let (large_src, large_lines) = openers_over_one_container(120);
        let small_steps = steps_for(small_src);
        let large_steps = steps_for(large_src);

        assert!(
            small_steps > 0 && large_steps > 0,
            "the dedent-step counter is dead: {small_steps} and {large_steps} steps",
        );
        assert!(
            large_steps <= 8 * large_lines,
            "{large_steps} steps over {large_lines} lines ({:.1} each): \
             the container is being re-walked per opener",
            large_steps as f64 / large_lines as f64,
        );
        assert!(
            large_steps * small_lines * 10 <= small_steps * large_lines * 11,
            "the per-line dedent cost climbs with size ({:.1} at {small_lines} lines, \
             {:.1} at {large_lines}): the container is being re-walked per opener",
            small_steps as f64 / small_lines as f64,
            large_steps as f64 / large_lines as f64,
        );
    }

    /// The same worst case one prefix over: `m` quoted openers of distinct
    /// widths in ONE quote, above `m * m` quoted filler lines, with every
    /// matching closer only past the blank that ends the quote.
    ///
    /// The quote bound (markup-carve/carve#1341) is a second forward walk, and a
    /// new walk with no counted guard is a walk nobody would notice going
    /// quadratic. Every opener passes the "is there a closer of this width and
    /// depth later" test and none of them closes inside its quote, which is the
    /// shape that makes a per-opener walk visible.
    fn quoted_openers_over_one_quote(m: usize) -> (String, u64) {
        let mut out = String::new();
        for i in 0..m {
            out.push_str("> ");
            out.push_str(&"%".repeat(3 + i));
            out.push_str(" o\n");
        }
        for _ in 0..m * m {
            out.push_str("> filler line here\n");
        }
        out.push('\n');
        for i in 0..m {
            out.push_str("> ");
            out.push_str(&"%".repeat(3 + i));
            out.push('\n');
        }
        out.push_str("\n[r][]\n");
        let lines = out.lines().count() as u64;
        (out, lines)
    }

    /// The three claims `many_openers_over_one_container_walk_it_once_apiece`
    /// makes, for the blank-line bound: a floor so a dead counter cannot pass,
    /// a ceiling, and the flat per-line shape that a per-opener walk cannot hold.
    #[test]
    fn many_quoted_openers_over_one_quote_walk_it_once_apiece() {
        let (small_src, small_lines) = quoted_openers_over_one_quote(60);
        let (large_src, large_lines) = quoted_openers_over_one_quote(120);
        let small_steps = steps_for(small_src);
        let large_steps = steps_for(large_src);

        assert!(
            small_steps > 0 && large_steps > 0,
            "the blank-walk counter is dead: {small_steps} and {large_steps} steps",
        );
        assert!(
            large_steps <= 8 * large_lines,
            "{large_steps} steps over {large_lines} lines ({:.1} each): \
             the quote is being re-walked per opener",
            large_steps as f64 / large_lines as f64,
        );
        assert!(
            large_steps * small_lines * 10 <= small_steps * large_lines * 11,
            "the per-line blank-walk cost climbs with size ({:.1} at {small_lines} lines, \
             {:.1} at {large_lines}): the quote is being re-walked per opener",
            small_steps as f64 / small_lines as f64,
            large_steps as f64 / large_lines as f64,
        );
    }
}

#[cfg(test)]
mod quote_prefix_calls {
    //! COUNTED guards on the blockquote prefix (markup-carve/carve-rs#731).
    //!
    //! They live here rather than in `tests/` because the counter they read is
    //! `#[cfg(test)]`, so a release build carries none of it. `cargo test` runs
    //! this binary, which is what CI runs.
    //!
    //! Counted, not timed, deliberately. This repo already records why a clock
    //! cannot express these bounds: `tests/perf_regressions.rs` had to
    //! serialize its 34 timing tests against each other because they flaked
    //! competing for cores, and carve-js's `writer-deep-list-perf.test.ts`
    //! carries "No ratio guard here on purpose... would also fail on the
    //! healthy build". A call count is a property of the algorithm, not of the
    //! machine: every figure quoted below reproduces byte-identically across
    //! runs and loads.

    use super::QUOTE_PREFIX_CALLS;

    /// Strips counted over one full parse-and-render of `src`.
    ///
    /// On its own thread for two reasons: the counter is thread-local, so a
    /// fresh thread cannot pick up another test's tally, and a 200-deep quote
    /// recurses past the default 2 MiB test stack in a debug build.
    fn calls_for(src: String) -> u64 {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                QUOTE_PREFIX_CALLS.with(|c| c.set(0));
                let _ = crate::to_html(&src);
                QUOTE_PREFIX_CALLS.with(|c| c.get())
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// One measurement: the work the document contains, and the strips it cost.
    struct Measured {
        work: u64,
        calls: u64,
    }

    impl Measured {
        fn per_unit(&self) -> f64 {
            self.calls as f64 / self.work as f64
        }
    }

    /// Assert that a document's strip count stays PROPORTIONAL to the work it
    /// contains, across a doubling of that work.
    ///
    /// Three separate claims, each of which has been shown to fail on its own:
    ///
    /// 1. A floor. Every quote marker has to be stripped at least once to be
    ///    recognized, so a count below the work is not a faster parser - it is a
    ///    counter that stopped counting. Without this the rest would pass on a
    ///    dead counter reporting zero.
    /// 2. A ceiling, generous enough to survive honest constant-factor drift.
    /// 3. The shape, which is the load-bearing half: a cost proportional to the
    ///    work holds its per-unit rate flat or falling as the document grows,
    ///    while anything superlinear in the work must make it climb. This one
    ///    is a statement about the curve rather than its constant, so it still
    ///    fires when a ceiling has been raised past the point of usefulness.
    ///
    /// The comparison is cross-multiplied integers so no float rounding enters
    /// the verdict; a tenth is allowed for drift. Floats appear in the failure
    /// messages only.
    fn assert_proportional(what: &str, small: Measured, large: Measured, ceiling: u64) {
        assert!(
            small.calls >= small.work && large.calls >= large.work,
            "{what}: the strip counter is not counting - {} strips for {} units of work, \
             {} for {}",
            small.calls,
            small.work,
            large.calls,
            large.work,
        );
        assert!(
            large.calls <= ceiling * large.work,
            "{what}: {} strips for {} units of work ({:.1} each, ceiling {ceiling}) - \
             the quote markers are being re-walked",
            large.calls,
            large.work,
            large.per_unit(),
        );
        assert!(
            large.calls * small.work * 10 <= small.calls * large.work * 11,
            "{what}: the per-unit strip cost climbs with size ({:.1} at the smaller size, \
             {:.1} at the larger): the quote markers are being re-walked",
            small.per_unit(),
            large.per_unit(),
        );
    }

    /// A blockquote depth ladder: line `i` carries `i` quote markers, so the
    /// document holds `depth * (depth + 1) / 2` markers in total - which is the
    /// work, and is exactly what carve-js spends on it.
    fn ladder(depth: usize) -> (String, u64) {
        ladder_of(depth, "a")
    }

    /// The same ladder with a chosen line body, so a guard can say what the
    /// quoted text is made of.
    fn ladder_of(depth: usize, body: &str) -> (String, u64) {
        let mut out = String::new();
        for i in 1..=depth {
            for _ in 0..i {
                out.push_str("> ");
            }
            out.push_str(body);
            out.push('\n');
        }
        (out, (depth * (depth + 1) / 2) as u64)
    }

    /// One deep quoted paragraph followed by `depth` lazy continuation lines,
    /// each of which has to ask whether that paragraph is still open. The work
    /// is one pass over every line at every level.
    fn lazy_tail(depth: usize) -> (String, u64) {
        let mut out = "> ".repeat(depth);
        out.push_str("a\n");
        for _ in 0..depth {
            out.push_str("lazy\n");
        }
        (out, ((depth + 1) * depth) as u64)
    }

    /// A ladder must not cost strips per nesting level per line.
    ///
    /// Before the fix the quote's paragraph-open state was decided eagerly on
    /// every quoted line, and deciding it walked the line down to its innermost
    /// content - a walk each enclosing level repeated over the same markers:
    ///
    /// | depth | markers | strips, eager | strips, deferred |
    /// | ----- | ------- | ------------- | ---------------- |
    /// | 25    | 325     | 6,495         | 3,245            |
    /// | 50    | 1,275   | 35,495        | 12,120           |
    /// | 100   | 5,050   | 223,495       | 46,745           |
    /// | 200   | 20,100  | 1,556,994     | 183,494          |
    ///
    /// carve-rs still spends about 9 strips per marker where carve-js spends 1.
    /// That residue is the prefix re-scan every implementation shares
    /// (markup-carve/carve#752), it is flat in depth, and it is not this
    /// guard's subject.
    #[test]
    fn a_depth_ladder_costs_strips_in_proportion_to_its_markers() {
        let (small_src, small_work) = ladder(100);
        let (large_src, large_work) = ladder(200);
        assert_proportional(
            "depth ladder",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            16,
        );
    }

    /// Ordinary quoted PROSE holding a colon must stay on the deferred path.
    ///
    /// The ladder above is made of `a`, so it says nothing about the pre-test
    /// that decides when §12's absorption flag has to be resolved
    /// (markup-carve/carve-rs#738). That pre-test has to be conservative, and
    /// the conservative reading is a COLON RUN: widening it to any `:` puts the
    /// cubic walk back for every `Note:`, `12:30` and `https://` in a quoted
    /// document - 1,556,994 strips at depth 200 rather than 183,494 - while the
    /// colon-free ladder above stays green and sees none of it.
    ///
    /// A line that really does carry `:::` is not covered, and cannot be:
    /// deciding the absorption on such a line is what #738 asks for, and that
    /// answer is only reachable by walking to the innermost content.
    #[test]
    fn quoted_prose_holding_a_colon_still_defers() {
        let (small_src, small_work) = ladder_of(100, "Note: at 12:30, see https://example.com");
        let (large_src, large_work) = ladder_of(200, "Note: at 12:30, see https://example.com");
        assert_proportional(
            "a depth ladder of colon-bearing prose",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            16,
        );
    }

    /// A run of lazy continuation lines must resolve the open paragraph once,
    /// not once per line.
    ///
    /// The ladder above never reads the paragraph-open state at all, so it
    /// cannot see this: deferring the walk without caching its answer passes
    /// the ladder guard and the whole corpus byte for byte, while costing
    /// 4,143,610 strips here against 103,910 - and climbing, 28 to 53 to 103
    /// per unit across depths 50, 100 and 200, where the cached answer falls,
    /// 2.86 to 2.68 to 2.58.
    #[test]
    fn lazy_continuation_under_a_deep_quote_resolves_the_paragraph_once() {
        let (small_src, small_work) = lazy_tail(100);
        let (large_src, large_work) = lazy_tail(200);
        assert_proportional(
            "lazy continuation under a deep quote",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            8,
        );
    }
}

/// A footnote body's continuation floor is a COLUMN count.
///
/// `footnote_body_floor` is compared against `indent_columns` at both of its
/// call sites, so it has to speak the same unit. It was computed with
/// `leading_ws`, a CHARACTER count, and a tab in the definition line's own
/// indentation made the two sides disagree.
///
/// These assertions are on the function rather than on the engine's output, and
/// deliberately so: no input reaches the site today (see the function's own
/// note), so a test written against rendered HTML or a published AST would be a
/// check that cannot fail - the class catalogued in markup-carve/carve#755. The
/// ticket asked for exactly this: fix the unit, and prove it with a case where
/// the two spellings differ on a tabbed input.
#[cfg(test)]
mod footnote_body_floor_unit {
    use super::{footnote_body_floor, indent_columns, leading_ws};

    #[test]
    fn a_tab_makes_the_two_spellings_disagree() {
        // THE PROOF. One tab indents the definition line. As characters it is
        // 1, so the old spelling asked continuations to reach column 3; as
        // columns it is 4, so the floor is 6. A continuation at column 4 or 5
        // would have been accepted under the old spelling and must not be.
        let def = "\t[^a]: note";
        assert_eq!(leading_ws(def), 1, "the character count");
        assert_eq!(indent_columns(def), 4, "the column count");
        assert_ne!(
            leading_ws(def) + 2,
            footnote_body_floor(def),
            "the two spellings must differ on a tabbed definition line, or this \
             test is pinning nothing"
        );
        assert_eq!(footnote_body_floor(def), 6);
    }

    #[test]
    fn a_mixed_run_disagrees_too() {
        // A space then a tab: the tab advances to the next 4-stop from column
        // 1, so the indentation is still 4 columns while being 2 characters.
        // Included because a fix that special-cased a LEADING tab would pass
        // the case above and fail here.
        let def = " \t[^a]: note";
        assert_eq!(leading_ws(def), 2);
        assert_eq!(indent_columns(def), 4);
        assert_eq!(footnote_body_floor(def), 6);
    }

    #[test]
    fn control_space_only_indentation_is_unchanged() {
        // CONTROL, and the reason the bug was latent: for space-only
        // indentation the two counts are equal, so every reachable input today
        // gets the same answer either way. This is what must NOT move.
        for def in ["[^a]: note", "  [^a]: note", "      [^a]: note"] {
            assert_eq!(leading_ws(def), indent_columns(def), "{def}");
            assert_eq!(footnote_body_floor(def), leading_ws(def) + 2, "{def}");
        }
    }

    #[test]
    fn control_the_floor_is_the_indentation_plus_two() {
        // CONTROL for the other half of the expression. §16 asks for ">= 2"
        // relative to the definition line; a mutation changing the addend
        // passes every unit assertion above that only compares the two
        // spellings, so the constant is pinned on its own.
        assert_eq!(footnote_body_floor("[^a]: x"), 2);
        assert_eq!(footnote_body_floor("    [^a]: x"), 6);
    }
}
