//! JSON encoding/decoding for the public Carve AST exchange shape.
//!
//! This module intentionally has no serde dependency. It contains the small
//! JSON writer and parser needed for the schema-backed AST interchange format.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ast::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstJsonError {
    message: String,
}

impl AstJsonError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AstJsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AstJsonError {}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(i64),
    Float(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

pub(crate) fn parse_value(input: &str) -> Result<Json, AstJsonError> {
    Parser::new(input).parse()
}

pub(crate) fn value_to_json(value: &Json) -> String {
    fn write(out: &mut String, value: &Json, depth: usize) {
        assert!(
            depth <= MAX_JSON_DEPTH,
            "JSON value exceeds the encoder's depth budget"
        );
        match value {
            Json::Null => out.push_str("null"),
            Json::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Json::Number(value) => out.push_str(&value.to_string()),
            Json::Float(value) => out.push_str(&value.to_string()),
            Json::String(value) => write_string(out, value),
            Json::Array(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    write(out, value, depth + 1);
                }
                out.push(']');
            }
            Json::Object(values) => {
                out.push('{');
                for (index, (key, value)) in values.iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    write_string(out, key);
                    out.push(':');
                    write(out, value, depth + 1);
                }
                out.push('}');
            }
        }
    }
    let mut out = String::new();
    write(&mut out, value, 0);
    out
}

thread_local! {
    static ENCODE_DEPTH: Cell<usize> = const { Cell::new(0) };
    static ENCODE_REFUSED: Cell<bool> = const { Cell::new(false) };
}

struct EncodeDepthGuard;

impl EncodeDepthGuard {
    fn enter() -> Option<Self> {
        ENCODE_DEPTH.with(|depth| {
            let current = depth.get();
            // Every recursive AST node costs at least its object and the
            // containing `children` array on the wire. Refuse at the cheapest
            // possible conversion so anything accepted here remains readable
            // by this module's structural-depth guard.
            if current >= MAX_JSON_DEPTH / 2 {
                ENCODE_REFUSED.with(|refused| refused.set(true));
                None
            } else {
                depth.set(current + 1);
                Some(Self)
            }
        })
    }
}

impl Drop for EncodeDepthGuard {
    fn drop(&mut self) {
        ENCODE_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

/// Serialize an AST, refusing a programmatically constructed tree beyond the
/// same depth budget used by the JSON reader.
pub fn try_to_json(doc: &Document) -> Result<String, AstJsonError> {
    ENCODE_DEPTH.with(|depth| depth.set(0));
    ENCODE_REFUSED.with(|refused| refused.set(false));
    let mut out = String::new();
    write_document(&mut out, doc);
    if ENCODE_REFUSED.with(Cell::get) {
        Err(AstJsonError::new(
            "JSON nests deeper than the encoder's depth budget",
        ))
    } else {
        Ok(out)
    }
}

/// Serialize an AST. Prefer [`try_to_json`] for trees not produced by the
/// parser; this compatibility entry point panics on an over-depth API tree.
pub fn to_json(doc: &Document) -> String {
    try_to_json(doc).expect("AST JSON encoder depth budget exceeded")
}

pub(crate) fn source_layout_positions(doc: &Document) -> Vec<(String, usize, usize)> {
    fn walk(value: &Json, path: &str, out: &mut Vec<(String, usize, usize)>) {
        match value {
            Json::Object(object) => {
                if let Some(Json::Object(pos)) = object.get("pos") {
                    if let (Some(Json::Number(start)), Some(Json::Number(end))) =
                        (pos.get("startOffset"), pos.get("endOffset"))
                    {
                        if *start >= 0 && *end >= 0 {
                            out.push((path.to_owned(), *start as usize, *end as usize));
                        }
                    }
                }
                for (key, child) in object {
                    if key == "pos" {
                        continue;
                    }
                    let escaped = key.replace('~', "~0").replace('/', "~1");
                    walk(child, &format!("{path}/{escaped}"), out);
                }
            }
            Json::Array(values) => {
                for (index, child) in values.iter().enumerate() {
                    walk(child, &format!("{path}/{index}"), out);
                }
            }
            _ => {}
        }
    }
    let json = Parser::new(&to_json(doc))
        .parse()
        .expect("encoder always writes valid JSON");
    let mut out = Vec::new();
    walk(&json, "", &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The fields a LEGACY definition-list entry may carry.
///
/// The schema gives `definition_list.items` typed `definition_term` and
/// `definition_description` nodes, and this decoder also reads the OLD grouping
/// record - `{terms, definitions}`, an object with no `type` at all, which is
/// what the engines published before §4 settled the flat form. Trees in that
/// shape are stored, so the form stays readable; but "the schema names no
/// `type` here" is not "the schema names no fields here", and being invisible to
/// the type-keyed check below left every field on the record accepted.
///
/// The set is the one carve-js closed it to (carve-js#913), including the two
/// position arrays its own runtime record carries: the legacy publisher was that
/// runtime, and a narrower set here would refuse a stored payload carve-js
/// accepts, which is the interchange break §11 exists to prevent rather than to
/// cause. This engine's legacy path drops position data for the whole record
/// anyway - it sets `pos: None` on every term and description it rebuilds -
/// so the two names cost nothing beyond being spellable.
const LEGACY_DEFINITION_ENTRY_FIELDS: &[&str] =
    &["definitionLines", "definitionSpans", "definitions", "terms"];

/// Is this the untyped legacy definition entry, rather than some other untyped
/// object?
///
/// Array-valued, like carve-js's test, and NOT merely "the key is present":
/// `attrs.keyValues` is an open map of strings, so a document with an attribute
/// literally named `terms` would otherwise be read as a legacy entry and have
/// its other attributes refused.
fn is_legacy_definition_entry(obj: &BTreeMap<String, Json>) -> bool {
    if obj.contains_key("type") {
        return false;
    }
    matches!(obj.get("terms"), Some(Json::Array(_)))
        || matches!(obj.get("definitions"), Some(Json::Array(_)))
}

fn named_fields(
    table: &'static [(&'static str, &'static [&'static str])],
    key: &str,
) -> Option<&'static [&'static str]> {
    table
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, fields)| *fields)
}

/// PART 12 §11: refuse a property the schema does not name.
///
/// This engine's codec reads field by field, so an unknown property was simply
/// not carried - conformant on OUTPUT, and silent on INPUT. The clause rules
/// that out for the reason §9(b) gives about depth: a caller told the tree was
/// accepted learns nothing about what went missing (carve-rs#691).
fn refuse_unknown_fields(node: &Json, path: &str) -> Result<(), AstJsonError> {
    match node {
        Json::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                refuse_unknown_fields(item, &format!("{path}[{index}]"))?;
            }
        }
        Json::Object(obj) => {
            // A node kind the schema does not name at all is NOT this check's
            // business: the decoder turns an unusable kind away on its own
            // terms, and naming a field on a type nobody knows would send the
            // caller after the wrong thing.
            // The legacy definition entry is closed too. It has no `type` - the
            // schema gives it none, which is what exempts it from the node-type
            // rule - and it was thereby exempt from the FIELD rule as well, with
            // nothing else reaching it. The exemption is about the missing
            // `type`, not about everything else on the record (carve-rs#820,
            // carve-js#913).
            if is_legacy_definition_entry(obj) {
                for key in obj.keys() {
                    if !LEGACY_DEFINITION_ENTRY_FIELDS.contains(&key.as_str()) {
                        return Err(AstJsonError::new(format!(
                            "the legacy definition entry at {path} carries {key:?}, which the schema does not name (PART 12 §11)"
                        )));
                    }
                }
            }
            if let Some(Json::String(ty)) = obj.get("type") {
                if let Some(known) = named_fields(crate::wire_fields::WIRE_FIELDS, ty) {
                    for key in obj.keys() {
                        if !known.contains(&key.as_str()) {
                            return Err(AstJsonError::new(format!(
                                "{ty} at {path} carries {key:?}, which the schema does not name (PART 12 §11)"
                            )));
                        }
                    }
                    // The objects that hang off a node without a `type` of
                    // their own. They are closed in the schema too, and every
                    // node kind can carry them - which makes them the easiest
                    // place to slip a key past a type-keyed check.
                    for (helper, allowed) in crate::wire_fields::WIRE_HELPER_FIELDS {
                        let Some(Json::Object(value)) = obj.get(*helper) else {
                            continue;
                        };
                        for key in value.keys() {
                            if !allowed.contains(&key.as_str()) {
                                return Err(AstJsonError::new(format!(
                                    "{helper} at {path}.{helper} carries {key:?}, which the schema does not name (PART 12 §11)"
                                )));
                            }
                        }
                    }
                    // The records the schema closes but gives no `type`, reached
                    // through an ARRAY property of a typed node -
                    // `citation_group.items`, whose entries are `citation`
                    // objects, and `table.rowGroups.bodies`. The type-keyed
                    // check above cannot see them and the helper loop cannot
                    // either, so a citation carrying any extra field rode
                    // straight in.
                    for (position, allowed) in crate::wire_fields::WIRE_UNTYPED_ARRAY_FIELDS {
                        let Some((owner, property)) = position.split_once('.') else {
                            continue;
                        };
                        if *ty != *owner {
                            continue;
                        }
                        // The property may sit under an untyped object of its
                        // own - `table.rowGroups.bodies` - so the path is
                        // WALKED rather than read as one key. Without this a
                        // body group was the one record on the wire nothing
                        // closed.
                        let mut holder = Some(obj);
                        let mut steps: Vec<&str> = property.split('.').collect();
                        let leaf = steps.pop().unwrap_or(property);
                        for step in steps {
                            holder = match holder.and_then(|o| o.get(step)) {
                                Some(Json::Object(inner)) => Some(inner),
                                _ => None,
                            };
                        }
                        let Some(Json::Array(items)) = holder.and_then(|o| o.get(leaf)) else {
                            continue;
                        };
                        for (index, item) in items.iter().enumerate() {
                            let Json::Object(record) = item else {
                                continue;
                            };
                            for key in record.keys() {
                                if !allowed.contains(&key.as_str()) {
                                    return Err(AstJsonError::new(format!(
                                        "{position} at {path}.{property}[{index}] carries {key:?}, which the schema does not name (PART 12 §11)"
                                    )));
                                }
                            }
                        }
                    }
                }
            }
            for (key, value) in obj {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                refuse_unknown_fields(value, &child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn from_json(input: &str) -> Result<Document, AstJsonError> {
    let json = Parser::new(input).parse()?;
    refuse_unknown_fields(&json, "")?;
    let root = json.as_object("document root")?;
    let root_type = required_string(root, "document", "type")?;
    if root_type != "document" {
        return Err(AstJsonError::new(format!(
            "document.type must be \"document\", got {root_type:?}"
        )));
    }
    let src_byte_length = required_usize(root, "document", "srcByteLength")?;
    let mut children = Vec::new();
    let mut frontmatter_raw = None;
    let mut footnote_defs = BTreeMap::new();
    let mut footnote_def_pos = BTreeMap::new();
    for child in required_array(root, "document", "children")? {
        let obj = child.as_object("document.children[]")?;
        match required_string(obj, "block node", "type")? {
            "frontmatter" => {
                if frontmatter_raw.is_some() {
                    return Err(AstJsonError::new("frontmatter appears more than once"));
                }
                frontmatter_raw = Some(Frontmatter {
                    format: required_string(obj, "frontmatter", "format")?.to_string(),
                    content: required_string(obj, "frontmatter", "content")?.to_string(),
                    pos: optional_pos(obj, "frontmatter")?,
                });
            }
            "footnote" => {
                // `label` only. `id` was what carve-js and carve-php published
                // before PART 12 §7 settled the spelling, and reading it here
                // was a second spelling of a field name on the wire - the thing
                // §3's "field names are spec surface" exists to prevent, and a
                // document that decoded in two engines and failed in the third
                // (carve-rs#820, spec 743).
                let label = required_string(obj, "footnote", "label")?.to_string();
                let blocks = decode_blocks(required_array(obj, "footnote", "children")?)?;
                // A definition whose body places NOTHING published the
                // definition line as its extent, and nothing in the body records
                // it - so an ingest that dropped this re-derived from an empty
                // body and published no position at all, exactly the gap the
                // parse path had (markup-carve/carve#1023).
                //
                // Guarded by the same condition the parse path prunes on, so the
                // two build the same map: where the body DOES place something,
                // the span on the wire is that body's extent and belongs to no
                // definition line.
                if let Some(pos) = optional_pos(obj, "footnote")? {
                    footnote_def_pos.insert(label.clone(), pos);
                }
                footnote_defs.insert(label, blocks);
            }
            _ => children.push(decode_block(child)?),
        }
    }
    let mut doc = Document {
        // Rebuilt from the raw block, with the same function the parser uses.
        // The wire carries the raw text only, so leaving this empty would make
        // a decoded document differ from the parsed one in a field neither the
        // format nor the consumer asked about.
        frontmatter: frontmatter_raw
            .as_ref()
            .map(|raw: &Frontmatter| crate::parse::frontmatter_map(&raw.format, &raw.content))
            .unwrap_or_default(),
        frontmatter_raw,
        footnote_defs,
        footnote_def_pos,
        children,
        source_len: src_byte_length,
        // What the sender actually had to send. Exact rather than estimated:
        // this function is handed the payload, so the number does not have to
        // be guessed at. It bounds the expansion budgets and the profile's
        // `max_length`; see `Document::expansion_budget_len` and
        // `Document::untrusted_input_len`.
        ingest_payload_len: input.len(),
    };

    clear_unbacked_footnote_numbers(&mut doc);
    renumber_captions(&mut doc);

    Ok(doc)
}

/// Re-derive `caption_number.number` for an ingested tree.
///
/// The other half of PART 12 §5's "resolution results ARE serialized", and the
/// worse half: a stale footnote number contradicted the renderer, a stale
/// caption number is what the renderer PRINTS. Delete the first of two numbered
/// figures from a published tree and the survivor still rendered
/// `Figure 2: two`, where a fresh parse of the same document gives `Figure 1`
/// (carve#758).
///
/// The same pass the parse runs, for the same reason as the footnote one above:
/// numbering happens during `parse` in this engine, so an ingested tree numbered
/// the same way agrees with a parsed one and §6's round trip holds. It builds
/// its counters fresh, so on an unedited tree it reproduces what was there.
fn renumber_captions(doc: &mut Document) {
    crate::parse::number_crossref_captions(doc);
}

/// Drop `number` from any footnote REFERENCE whose definition is not in the tree.
///
/// PART 12 §5 serializes footnote numbering, and on a parsed document the number
/// always describes the document it came from. On an INGESTED one it need not:
/// delete a definition from a published tree - what an editor does when a user
/// removes one - and the reference no longer resolves, so every renderer emits
/// the literal `[^a]`, while the number copied off the payload still claimed a
/// footnote that is not there (carve#758). carve-php already drops it.
///
/// CLEARS, NEVER ASSIGNS. Numbering an ingested tree outright would break §6:
/// that round trip is `parse(x)` serialized and deserialized, and parsing alone
/// does no numbering, so a tree that legitimately carries none would come back
/// carrying them.
///
/// An INLINE footnote carries its own body and cannot be orphaned by a missing
/// definition; the pass leaves those numbered.
///
/// THE SAME PASS `parse` RUNS, not a second implementation. It assigns as well as
/// clears, which is right here and would be wrong in carve-js: `parse` numbers
/// footnotes in this engine, so an ingested tree numbered the same way agrees
/// with the parsed one and §6's round trip holds. carve-js does its numbering in
/// resolution instead, so there the pass has to clear without assigning.
///
/// `assign_ref_ids` stays false: `ref_id` is a rendering anchor and carve#762
/// removed it from the schema entirely (carve-rs#648).
fn clear_unbacked_footnote_numbers(doc: &mut Document) {
    let _ = crate::render::collect_footnotes(doc, false);
}

struct Writer<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> Writer<'a> {
    fn new(out: &'a mut String) -> Self {
        out.push('{');
        Self { out, first: true }
    }

    fn field(&mut self, name: &str, write: impl FnOnce(&mut String)) {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        write_string(self.out, name);
        self.out.push(':');
        write(self.out);
    }

    fn finish(self) {
        self.out.push('}');
    }
}

fn write_document(out: &mut String, doc: &Document) {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, "document"));
    w.field("children", |out| {
        out.push('[');
        let mut first = true;
        if let Some(raw) = &doc.frontmatter_raw {
            write_comma(out, &mut first);
            write_frontmatter(out, raw);
        }
        // Definitions come AFTER the content and in source order (PART 12 §7).
        //
        // `footnote_defs_in_source_order` rather than a second copy of that
        // sort: this function had its own, and the two then had to be kept
        // agreeing by hand - which is how the `carve` writer's copy came to
        // order definitions differently from the encoder's (carve-rs#685).
        let footnote_defs = footnote_defs_in_source_order(doc);
        for entry in ordered_document_entries(doc, &footnote_defs) {
            write_comma(out, &mut first);
            match entry {
                DocEntry::Block(child) => write_block(out, child),
                DocEntry::FootnoteDef(label, children, pos) => {
                    write_footnote_def(out, label, children, pos)
                }
            }
        }
        out.push(']');
    });
    w.field("srcByteLength", |out| write_usize(out, doc.source_len));
    w.finish();
}

/// One direct child of the serialized document.
///
/// A footnote definition is a map on the runtime document and only becomes a
/// sibling on the wire, so the two kinds have to be lined up before either is
/// written.
pub(crate) enum DocEntry<'a> {
    Block(&'a BlockNode),
    /// Label, body, and where the definition LINE was written - which the body
    /// does not record and an empty body cannot stand in for.
    FootnoteDef(&'a String, &'a Vec<BlockNode>, Option<&'a Pos>),
}

/// PART 12 §7: "Definitions appear in DOCUMENT ORDER by source position."
///
/// The two COLLECTED definition kinds - `link_reference_definition` and
/// `footnote` - are moved to the document, and §4 keeps the `pos` each was
/// written at, so their published order has to follow that `pos`. Writing
/// `doc.children` and then the footnote map put every link definition ahead of
/// every footnote whatever the author wrote, and `pos` then ran backwards
/// between two adjacent siblings (carve#746).
///
/// Only the collected kinds move. An `abbreviation_def` is not collected out of
/// the document - §7 refuses that specifically, since hoisting it would empty
/// the line rather than relocate visible output - so it already sits at its
/// source position and keeps its index here.
///
/// The reordering is confined to the slots the collected definitions already
/// occupy, so no other child moves, and the sort is stable, so two definitions
/// reporting the same offset keep the order they arrived in (which for
/// footnotes is the label tie-break applied by the caller).
/// A document's footnote definitions in SOURCE ORDER.
///
/// `Document::footnote_defs` is a `BTreeMap`, so iterating it yields label
/// order - and §7 orders collected definitions by source position. Every target
/// that prints the definitions needs this, so it lives here rather than being
/// re-derived per renderer: the `carve` writer had its own copy
/// (carve-rs#685) and markdown, plain and ansi each walked the map directly
/// (carve-rs#686).
///
/// A definition with no recorded span - positions are opt-in (§4) - sorts to the
/// end and keeps label order among its peers, which is the only order available
/// there.
pub(crate) fn footnote_defs_in_source_order(doc: &Document) -> Vec<(&String, &Vec<BlockNode>)> {
    let mut defs: Vec<(&String, &Vec<BlockNode>)> = doc.footnote_defs.iter().collect();
    defs.sort_by_key(|(label, children)| {
        (
            footnote_def_start(doc, label, children).unwrap_or(usize::MAX),
            label.as_str(),
        )
    });

    defs
}

/// Where a footnote definition starts, for §7's document order.
///
/// The DEFINITION LINE first, the body only as a fallback. Ordering by the body
/// alone put a definition with no blocks last whatever the author wrote, because
/// there was no block to read a start from - so `[^a]: {empty}` written above
/// `[^b]: x` was published below it, and §7's "document order by source
/// position" was decided by which definition happened to have content
/// (markup-carve/carve#1023).
///
/// The two never disagree: `footnote_def_pos` holds a line only for a
/// definition whose body places nothing, so exactly one of the two is available
/// per definition and the order is read from whichever that is.
fn footnote_def_start(doc: &Document, label: &str, children: &[BlockNode]) -> Option<usize> {
    doc.footnote_def_pos
        .get(label)
        .or_else(|| first_block_pos(children))
        .map(|pos| pos.start_offset)
}

pub(crate) fn ordered_document_entries<'a>(
    doc: &'a Document,
    footnote_defs: &[(&'a String, &'a Vec<BlockNode>)],
) -> Vec<DocEntry<'a>> {
    let mut entries: Vec<DocEntry<'a>> = doc.children.iter().map(DocEntry::Block).collect();
    entries.extend(footnote_defs.iter().map(|(label, body)| {
        DocEntry::FootnoteDef(label, body, doc.footnote_def_pos.get(label.as_str()))
    }));

    let slots: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| collected_definition_offset(entry).is_some())
        .map(|(i, _)| i)
        .collect();
    if slots.len() < 2 {
        return entries;
    }

    let mut moved: Vec<DocEntry<'a>> = Vec::with_capacity(slots.len());
    for &i in slots.iter().rev() {
        moved.push(entries.remove(i));
    }
    moved.reverse();
    moved.sort_by_key(|entry| collected_definition_offset(entry).unwrap_or(usize::MAX));
    for (&i, entry) in slots.iter().zip(moved) {
        entries.insert(i, entry);
    }
    entries
}

/// The published start offset of a COLLECTED definition, or `None` for anything
/// §7 does not collect. A collected definition with no placed position sorts
/// last rather than being given an invented one.
fn collected_definition_offset(entry: &DocEntry<'_>) -> Option<usize> {
    match entry {
        DocEntry::Block(BlockNode::LinkReferenceDefinition(n)) => {
            Some(n.pos.as_ref().map_or(usize::MAX, |pos| pos.start_offset))
        }
        DocEntry::FootnoteDef(_, body, pos) => Some(
            pos.or_else(|| first_block_pos(body))
                .map_or(usize::MAX, |pos| pos.start_offset),
        ),
        DocEntry::Block(_) => None,
    }
}

pub(crate) fn first_block_pos(children: &[BlockNode]) -> Option<&Pos> {
    children.iter().find_map(block_pos)
}

pub(crate) fn block_pos(node: &BlockNode) -> Option<&Pos> {
    match node {
        BlockNode::LinkReferenceDefinition(n) => n.pos.as_ref(),
        BlockNode::CitationDefinition(n) => n.pos.as_ref(),
        BlockNode::Heading(n) => n.pos.as_ref(),
        BlockNode::Paragraph(n) => n.pos.as_ref(),
        BlockNode::CodeBlock(n) => n.pos.as_ref(),
        BlockNode::List(n) => n.pos.as_ref(),
        BlockNode::BlockQuote(n) => n.pos.as_ref(),
        BlockNode::Table(n) => n.pos.as_ref(),
        BlockNode::Admonition(n) => n.pos.as_ref(),
        BlockNode::Div(n) => n.pos.as_ref(),
        BlockNode::LineBlock(n) => n.pos.as_ref(),
        BlockNode::DefinitionList(n) => n.pos.as_ref(),
        BlockNode::Figure(n) => n.pos.as_ref(),
        BlockNode::FigureGroup(n) => n.pos.as_ref(),
        BlockNode::AbbreviationDef(n) => n.pos.as_ref(),
        BlockNode::RawBlock(n) => n.pos.as_ref(),
        BlockNode::Comment(n) => n.pos.as_ref(),
        BlockNode::Extension(n) => n.pos.as_ref(),
        BlockNode::BlockImage(n) => n.pos.as_ref(),
        BlockNode::ThematicBreak(n) => n.pos.as_ref(),
    }
}

fn write_frontmatter(out: &mut String, raw: &Frontmatter) {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, "frontmatter"));
    w.field("format", |out| write_string(out, &raw.format));
    w.field("content", |out| write_string(out, &raw.content));
    write_pos_field(&mut w, &raw.pos);
    w.finish();
}

fn write_footnote_def(
    out: &mut String,
    label: &str,
    children: &[BlockNode],
    def_pos: Option<&Pos>,
) {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, "footnote"));
    w.field("label", |out| write_string(out, label));
    w.field("children", |out| write_blocks(out, children));
    // FIRST block's start through the LAST placed block's end. Taking the
    // first block's span alone left every later block - a `+` continuation, an
    // indented second paragraph - outside its own footnote (carve#565).
    //
    // A definition with NO PLACED BLOCK has nothing to derive from, and that is
    // not the same thing as being unplaceable: `[^f]: {empty}` is written on a
    // line of its own, so §4's "omit rather than invent" does not apply and the
    // definition line is the honest extent - the one the reference publishes.
    // Deriving from the body alone left this node the only one in the corpus
    // with no `pos` (markup-carve/carve#1023).
    //
    // The fallback is only reached when the body places nothing, so a
    // definition that HAS content keeps the extent it has always had.
    let pos = match def_pos
        .copied()
        .or_else(|| first_block_pos(children).copied())
    {
        Some(mut pos) => {
            if let Some(last) = children.iter().rev().find_map(block_pos) {
                if last.end_offset > pos.end_offset {
                    pos.end_offset = last.end_offset;
                    pos.end_line = last.end_line;
                    pos.end_column = last.end_column;
                }
            }
            Some(pos)
        }
        None => None,
    };
    write_pos_field(&mut w, &pos);
    w.finish();
}

fn write_block(out: &mut String, node: &BlockNode) {
    let Some(_depth) = EncodeDepthGuard::enter() else {
        return;
    };
    match node {
        BlockNode::Heading(n) => {
            let mut w = typed(out, "heading");
            w.field("level", |out| write_usize(out, n.level as usize));
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Paragraph(n) => write_paragraph(out, n),
        BlockNode::CodeBlock(n) => write_code_block(out, n),
        BlockNode::List(n) => {
            let mut w = typed(out, "list");
            w.field("ordered", |out| write_bool(out, n.ordered));
            w.field("tight", |out| write_bool(out, n.tight));
            w.field("items", |out| write_array(out, &n.items, write_list_item));
            if let Some(start) = n.start {
                w.field("start", |out| write_usize(out, start));
            }
            if let Some(ol_type) = n.ol_type {
                w.field("olType", |out| write_string(out, ol_type_json(ol_type)));
            }
            if let Some(delim) = n.delim {
                w.field("delim", |out| write_string(out, &delim.to_string()));
            }
            if let Some(bullet) = n.bullet_char {
                w.field("bulletChar", |out| write_string(out, &bullet.to_string()));
            }
            // Author choice like `delim` and `bulletChar`, so it rides the wire
            // beside them (PART 12 §3, carve#480). Absent at the default, which
            // is why this is `true`-only rather than a boolean field: without it
            // `. a` decoded as `1. a` and no engine could do better, because
            // none of them had anywhere to put the distinction.
            if n.bare_marker {
                w.field("bareMarker", |out| out.push_str("true"));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::BlockQuote(n) => write_block_quote(out, n),
        BlockNode::Table(n) => write_table(out, n),
        BlockNode::Admonition(n) => {
            let mut w = typed(out, "admonition");
            w.field("kind", |out| write_string(out, &n.kind));
            if let Some(title) = &n.title {
                w.field("title", |out| write_inlines(out, title));
            }
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            w.field("children", |out| write_blocks(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Div(n) => {
            let mut w = typed(out, "div");
            w.field("children", |out| write_blocks(out, &n.children));
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::LineBlock(n) => {
            let mut w = typed(out, "line_block");
            w.field("children", |out| write_blocks(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::DefinitionList(n) => {
            let mut w = typed(out, "definition_list");
            w.field("items", |out| write_definition_entries(out, &n.items));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Figure(n) => {
            let mut w = typed(out, "figure");
            w.field("target", |out| write_figure_target(out, &n.target));
            w.field("caption", |out| write_inlines(out, &n.caption));
            if let Some(short_caption) = &n.short_caption {
                w.field("shortCaption", |out| write_inlines(out, short_caption));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::FigureGroup(n) => {
            // PART 12 §16: `children` in source order (a consumer derives the
            // panel list by type, the way the renderer does - there is no
            // second `panels` key to disagree with them), `caption` only when
            // the closer hosted one - absent means uncaptioned, never an
            // empty placeholder.
            let mut w = typed(out, "figure_group");
            w.field("children", |out| write_blocks(out, &n.children));
            if let Some(caption) = &n.caption {
                w.field("caption", |out| write_inlines(out, caption));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::LinkReferenceDefinition(n) => {
            // PART 12 §10: `label` and `href` are required, `title` and `attrs`
            // ride along when the definition line carried them.
            let mut w = typed(out, "link_reference_definition");
            w.field("label", |out| write_string(out, &n.label));
            w.field("href", |out| write_string(out, &n.href));
            if let Some(title) = &n.title {
                w.field("title", |out| write_string(out, title));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::CitationDefinition(n) => {
            // PART 12 section 18: `key` and `children` are required, `attrs`
            // rides along when the definition line carried a metadata block.
            // Shaped after section 10's link reference definition, so the entry
            // is INLINE content - a footnote body holds blocks and this does
            // not.
            let mut w = typed(out, "citation_definition");
            w.field("key", |out| write_string(out, &n.key));
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::AbbreviationDef(n) => {
            let mut w = typed(out, "abbreviation_def");
            w.field("abbr", |out| write_string(out, &n.abbr));
            w.field("expansion", |out| write_string(out, &n.expansion));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::RawBlock(n) => {
            let mut w = typed(out, "raw_block");
            w.field("format", |out| write_string(out, &n.format));
            w.field("content", |out| write_string(out, &n.content));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Comment(n) => {
            let mut w = typed(out, "comment");
            w.field("block", |out| write_bool(out, n.block));
            if n.delimited {
                w.field("delimited", |out| write_bool(out, true));
            }
            w.field("content", |out| write_string(out, &n.content));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::Extension(n) => {
            let mut w = typed(out, "block_extension");
            w.field("name", |out| write_string(out, &n.name));
            w.field("children", |out| write_blocks(out, &n.children));
            if let Some(summary) = &n.summary {
                w.field("summary", |out| write_inlines(out, summary));
            }
            if let Some(label) = &n.label {
                w.field("label", |out| write_string(out, label));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        BlockNode::BlockImage(n) => write_image(out, n),
        BlockNode::ThematicBreak(n) => {
            let mut w = typed(out, "thematic_break");
            if let Some(marker @ ('*' | '_')) = n.marker {
                w.field("marker", |out| write_string(out, &marker.to_string()));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
    }
}

fn write_paragraph(out: &mut String, n: &Paragraph) {
    let mut w = typed(out, "paragraph");
    w.field("children", |out| write_inlines(out, &n.children));
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_code_block(out: &mut String, n: &CodeBlock) {
    let mut w = typed(out, "code_block");
    w.field("content", |out| write_string(out, &n.content));
    if let Some(lang) = &n.lang {
        w.field("lang", |out| write_string(out, lang));
    }
    if let Some(title) = &n.title {
        w.field("header", |out| write_string(out, title));
    }
    if let Some(label) = &n.label {
        w.field("label", |out| write_string(out, label));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_block_quote(out: &mut String, n: &BlockQuote) {
    let mut w = typed(out, "block_quote");
    w.field("children", |out| write_blocks(out, &n.children));
    // The ordinary attribute slot, like every other node's (PART 12 §3). This
    // cited a "PART 9 §4a" for the source of the quotation: PART 9 has 4b and 4c
    // and no 4a, and what this line writes is the attrs slot rather than an
    // attribution of any kind.
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_list_item(out: &mut String, n: &ListItem) {
    let mut w = typed(out, "list_item");
    w.field("children", |out| write_blocks(out, &n.children));
    if let Some(checked) = n.checked {
        w.field("checked", |out| write_bool(out, checked));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_table(out: &mut String, n: &Table) {
    let mut w = typed(out, "table");
    w.field("rows", |out| write_array(out, &n.rows, write_table_row));
    let derived;
    let columns = if n.columns.is_empty() {
        derived = columns_from_table_attrs(n.attrs.as_ref());
        &derived
    } else {
        &n.columns
    };
    if !columns.is_empty() {
        w.field("columns", |out| {
            write_array(out, columns, write_table_column)
        });
    }
    if let Some(caption) = &n.caption {
        w.field("caption", |out| write_inlines(out, caption));
    }
    if let Some(short_caption) = &n.short_caption {
        w.field("shortCaption", |out| write_inlines(out, short_caption));
    }
    if let Some(groups) = &n.row_groups {
        w.field("rowGroups", |out| write_row_groups(out, groups));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn columns_from_table_attrs(attrs: Option<&Attrs>) -> Vec<TableColumn> {
    let values = |key| {
        attrs
            .and_then(|a| a.key_values.get(key))
            .map(|v| v.split(',').collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let aligns = values("aligns");
    let valigns = values("valigns");
    let widths = values("widths");
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

fn write_table_column(out: &mut String, n: &TableColumn) {
    let mut w = Writer::new(out);
    if let Some(align) = n.align {
        w.field("align", |out| write_string(out, align_json(align)));
    }
    if let Some(valign) = n.valign {
        w.field("valign", |out| write_string(out, valign_json(valign)));
    }
    if let Some(width) = n.width {
        w.field("width", |out| out.push_str(&width.to_string()));
    }
    w.finish();
}

fn write_row_groups(out: &mut String, n: &TableRowGroups) {
    let mut w = Writer::new(out);
    w.field("headRows", |out| write_usize(out, n.head_rows));
    w.field("bodies", |out| {
        write_array(out, &n.bodies, write_body_group)
    });
    w.field("footRows", |out| write_usize(out, n.foot_rows));
    w.finish();
}

fn write_body_group(out: &mut String, n: &TableBodyGroup) {
    let mut w = Writer::new(out);
    w.field("headRows", |out| write_usize(out, n.head_rows));
    w.field("bodyRows", |out| write_usize(out, n.body_rows));
    if let Some(columns) = n.row_head_columns {
        w.field("rowHeadColumns", |out| write_usize(out, columns));
    }
    write_attrs_field(&mut w, &n.attrs);
    w.finish();
}

fn write_table_row(out: &mut String, n: &TableRow) {
    let mut w = typed(out, "table_row");
    w.field("cells", |out| write_array(out, &n.cells, write_table_cell));
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_table_cell(out: &mut String, n: &TableCell) {
    let mut w = typed(out, "table_cell");
    w.field("header", |out| write_bool(out, n.header));
    w.field("children", |out| write_inlines(out, &n.children));
    if let Some(span) = n.span {
        w.field("span", |out| {
            write_string(
                out,
                match span {
                    TableCellSpan::Rowspan => "rowspan",
                    TableCellSpan::Colspan => "colspan",
                },
            )
        });
    }
    if let Some(align) = n.align {
        w.field("align", |out| write_string(out, align_json(align)));
    }
    if let Some(valign) = n.valign {
        w.field("valign", |out| write_string(out, valign_json(valign)));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

/// A definition list's entries, FLATTENED into the `<dt>` / `<dd>` sequence the
/// wire carries (PART 12).
///
/// This engine groups terms with their definitions, which is convenient in
/// memory and not something to publish: `definition_term` and
/// `definition_description` are in the normative block vocabulary, so a profile
/// can name them, and a plain `{terms, definitions}` object can carry no `pos` -
/// leaving a term the only content in a serialized document an editor cannot
/// navigate to.
///
/// The grouping was also not AGREED. Given `:: a` / `:: b` / `:  x` / `:  y`
/// this engine produced three entries and carve-js produced one, while both
/// rendered the same `<dl>`. A structure two producers disagree about, which no
/// output depends on, is an internal.
fn write_definition_entries(out: &mut String, items: &[DefinitionItem]) {
    out.push('[');
    let mut first = true;
    for item in items {
        for term in &item.terms {
            write_comma(out, &mut first);
            let mut w = typed(out, "definition_term");
            w.field("children", |out| write_inlines(out, &term.children));
            write_attrs_field(&mut w, &term.attrs);
            write_pos_field(&mut w, &term.pos);
            w.finish();
        }
        for definition in &item.definitions {
            write_comma(out, &mut first);
            let mut w = typed(out, "definition_description");
            w.field("children", |out| write_blocks(out, &definition.children));
            write_attrs_field(&mut w, &definition.attrs);
            write_pos_field(&mut w, &definition.pos);
            w.finish();
        }
    }
    out.push(']');
}

fn write_figure_target(out: &mut String, target: &FigureTarget) {
    match target {
        FigureTarget::Image(n) => write_image(out, n),
        FigureTarget::BlockQuote(n) => write_block_quote(out, n),
        FigureTarget::Table(n) => write_table(out, n),
        FigureTarget::CodeBlock(n) => write_code_block(out, n),
        FigureTarget::Paragraph(n) => write_paragraph(out, n),
    }
}

fn write_inline(out: &mut String, node: &InlineNode) {
    let Some(_depth) = EncodeDepthGuard::enter() else {
        return;
    };
    match node {
        InlineNode::Text(n) => {
            let mut w = typed(out, "text");
            w.field("value", |out| write_string(out, &n.value));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::EscapedText(n) => {
            let mut w = typed(out, "escaped_text");
            w.field("value", |out| write_string(out, &n.value));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::SmartPunctuation(n) => {
            let mut w = typed(out, "smart_punctuation");
            w.field("kind", |out| write_string(out, &n.kind));
            w.field("value", |out| write_string(out, &n.value));
            if let Some(glyph) = &n.glyph {
                w.field("glyph", |out| write_string(out, glyph));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Emphasis(n) => {
            let mut w = typed(out, emphasis_type(n.kind));
            if n.kind == EmphasisKind::BoldItalic {
                // PART 11 §6: `/*x*/` and `*/x/*` BOTH yield a `strong`
                // wrapping an `emphasis`. `boldItalic` records which spelling
                // the author used; it does not replace the nesting.
                //
                // This engine holds the combined form as ONE node, which is
                // fine internally and wrong on the wire: the published tree had
                // `strong` > `text`, so carve-js decoding it produced a strong
                // with no emphasis at all and the italic was silently lost
                // (#513). The nesting is materialised here, at the boundary,
                // rather than changing the internal representation.
                // The materialised node needs its own span, or it is the one
                // node in the tree without one (PART 12 §4). The combined form
                // is `/*` CONTENT `*/`, both delimiters two ASCII characters,
                // so the inner span is this node's with two trimmed off each
                // end - lines are unchanged, because the delimiters sit on the
                // first and last line of the run. Checked against carve-js on
                // the single-run, nested-strong, mid-paragraph and multi-line
                // shapes.
                //
                // NOT when the node carries attributes. This engine's span for
                // an attributed inline covers the attribute block too, so
                // `/*x*/{#id}` ends at the `}` and trimming two lands inside
                // the attributes rather than at the content boundary. Omitting
                // is what §4 allows for a node with no honest span, and
                // inventing one that selects `x*/{#` is worse than none. The
                // outer span is the thing that is actually wrong there, and it
                // is not specific to this form - carve-rs#521.
                let inner_pos = n.pos.as_ref().filter(|_| n.attrs.is_none()).map(|p| Pos {
                    start_line: p.start_line,
                    end_line: p.end_line,
                    start_column: p.start_column + 2,
                    end_column: p.end_column.saturating_sub(2),
                    start_offset: p.start_offset + 2,
                    end_offset: p.end_offset.saturating_sub(2),
                });
                w.field("children", |out| {
                    out.push('[');
                    let mut inner = typed(out, "emphasis");
                    inner.field("children", |out| write_inlines(out, &n.children));
                    write_pos_field(&mut inner, &inner_pos);
                    inner.finish();
                    out.push(']');
                });
                w.field("boldItalic", |out| write_bool(out, true));
            } else {
                w.field("children", |out| write_inlines(out, &n.children));
            }
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Code(n) => {
            let mut w = typed(out, "code");
            w.field("value", |out| write_string(out, &n.value));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Link(n) => write_link(out, n),
        InlineNode::Image(n) => write_image(out, n),
        InlineNode::Span(n) => {
            let mut w = typed(out, "span");
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Math(n) => {
            let mut w = typed(out, "math");
            w.field("display", |out| write_bool(out, n.display));
            w.field("content", |out| write_string(out, &n.content));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::RawInline(n) => {
            let mut w = typed(out, "raw_inline");
            w.field("format", |out| write_string(out, &n.format));
            w.field("content", |out| write_string(out, &n.content));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::LiteralInline(n) => {
            let mut w = typed(out, "literal_inline");
            w.field("content", |out| write_string(out, &n.content));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Symbol(n) => {
            let mut w = typed(out, "symbol");
            w.field("name", |out| write_string(out, &n.name));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::AutoLink(n) => {
            let mut w = typed(out, "autolink");
            w.field("href", |out| write_string(out, &n.href));
            w.field("text", |out| write_string(out, &n.text));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CrossRef(n) => {
            let mut w = typed(out, "heading_ref");
            // The authored construct and its resolution, side by side
            // (PART 12 section 3a). `href` is absent where the crossref
            // resolved against nothing, which is what says so.
            w.field("target", |out| write_string(out, &n.target));
            if let Some(href) = &n.href {
                w.field("href", |out| write_string(out, href));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CaptionNumber(n) => {
            let mut w = typed(out, "caption_number");
            if let Some(number) = n.number {
                w.field("n", |out| write_usize(out, number));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Mention(n) => {
            let mut w = typed(out, "mention");
            write_attrs_field(&mut w, &n.attrs);
            w.field("user", |out| write_string(out, &n.user));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Tag(n) => {
            let mut w = typed(out, "tag");
            write_attrs_field(&mut w, &n.attrs);
            w.field("name", |out| write_string(out, &n.name));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CitationGroup(n) => {
            let mut w = typed(out, "citation_group");
            w.field("items", |out| write_array(out, &n.items, write_citation));
            w.field("raw", |out| write_string(out, &n.raw));
            if n.integral {
                w.field("mode", |out| write_string(out, "integral"));
            }
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Extension(n) => {
            let mut w = typed(out, "inline_extension");
            w.field("name", |out| write_string(out, &n.name));
            w.field("content", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Abbreviation(n) => {
            let mut w = typed(out, "abbreviation");
            w.field("abbr", |out| write_string(out, &n.abbr));
            w.field("expansion", |out| write_string(out, &n.expansion));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Footnote(n) => {
            if let Some(inline) = &n.inline {
                let mut w = typed(out, "inline_footnote");
                w.field("inline", |out| write_inlines(out, inline));
                if let Some(number) = n.number {
                    w.field("number", |out| write_usize(out, number));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            } else {
                let mut w = typed(out, "footnote_ref");
                if let Some(id) = &n.id {
                    w.field("id", |out| write_string(out, id));
                }
                if let Some(number) = n.number {
                    w.field("number", |out| write_usize(out, number));
                }
                write_attrs_field(&mut w, &n.attrs);
                write_pos_field(&mut w, &n.pos);
                w.finish();
            }
        }
        InlineNode::SoftBreak(n) => {
            let mut w = typed(out, "soft_break");
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::HardBreak(n) => {
            let mut w = typed(out, "hard_break");
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticInsert(n) => {
            let mut w = typed(out, "insert");
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticDelete(n) => {
            let mut w = typed(out, "delete");
            w.field("children", |out| write_inlines(out, &n.children));
            write_attrs_field(&mut w, &n.attrs);
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticSubstitute(n) => {
            let mut w = typed(out, "substitution");
            w.field("oldText", |out| write_string(out, &n.old_text));
            w.field("newText", |out| write_string(out, &n.new_text));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::Comment(n) => {
            let mut w = typed(out, "comment");
            w.field("block", |out| write_bool(out, false));
            if n.delimited {
                w.field("delimited", |out| write_bool(out, true));
            }
            w.field("content", |out| write_string(out, &n.content));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
        InlineNode::CriticComment(n) => {
            let mut w = typed(out, "critic_comment");
            w.field("text", |out| write_string(out, &n.text));
            write_pos_field(&mut w, &n.pos);
            w.finish();
        }
    }
}

fn write_link(out: &mut String, n: &Link) {
    let mut w = typed(out, "link");
    w.field("href", |out| write_string(out, &n.href));
    w.field("children", |out| write_inlines(out, &n.children));
    if let Some(title) = &n.title {
        w.field("title", |out| write_string(out, title));
    }
    if let Some(ref_label) = &n.ref_label {
        w.field("ref", |out| write_string(out, ref_label));
    }
    if let Some(raw_ref) = &n.raw_ref {
        w.field("rawRef", |out| write_string(out, raw_ref));
    }
    // `from_crossref` is deliberately NOT written: it is a render-time fact
    // about how this link was produced, the schema does not name it, and PART 12
    // §11 ingest refuses any property the schema does not name - so emitting it
    // made this engine's own `--json` output unreadable by this engine
    // (carve-rs#776). The flag lives on in memory for the renderers.
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_image(out: &mut String, n: &Image) {
    let mut w = typed(out, "image");
    w.field("src", |out| write_string(out, &n.src));
    w.field("alt", |out| write_string(out, &n.alt));
    if let Some(title) = &n.title {
        w.field("title", |out| write_string(out, title));
    }
    if let Some(ref_label) = &n.ref_label {
        w.field("ref", |out| write_string(out, ref_label));
    }
    if let Some(raw_ref) = &n.raw_ref {
        w.field("rawRef", |out| write_string(out, raw_ref));
    }
    write_attrs_field(&mut w, &n.attrs);
    write_pos_field(&mut w, &n.pos);
    w.finish();
}

fn write_citation(out: &mut String, n: &Citation) {
    let mut w = Writer::new(out);
    w.field("key", |out| write_string(out, &n.key));
    if let Some(prefix) = &n.prefix {
        w.field("prefix", |out| write_inlines(out, prefix));
    }
    if let Some(locator) = &n.locator {
        w.field("locator", |out| write_inlines(out, locator));
    }
    if let Some(locator_label) = &n.locator_label {
        w.field("locatorLabel", |out| write_string(out, locator_label));
    }
    if let Some(locator_value) = &n.locator_value {
        w.field("locatorValue", |out| write_string(out, locator_value));
    }
    if let Some(suffix) = &n.suffix {
        w.field("suffix", |out| write_inlines(out, suffix));
    }
    w.field("suppressAuthor", |out| write_bool(out, n.suppress_author));
    if let Some(number) = n.number {
        w.field("number", |out| write_usize(out, number));
    }
    if let Some(use_index) = n.use_index {
        w.field("useIndex", |out| write_usize(out, use_index));
    }
    w.finish();
}

fn typed<'a>(out: &'a mut String, ty: &str) -> Writer<'a> {
    let mut w = Writer::new(out);
    w.field("type", |out| write_string(out, ty));
    w
}

fn write_attrs_field(w: &mut Writer<'_>, attrs: &Option<Attrs>) {
    if let Some(attrs) = attrs {
        w.field("attrs", |out| write_attrs(out, attrs));
    }
}

/// `key_values` in the order the AUTHOR wrote them, which is what `attrs.order`
/// records.
///
/// The map behind it is a `BTreeMap`, so iterating it publishes an ALPHABETICAL
/// order that is this engine's storage choice and nothing the document said.
/// `[x]{b=1 a=2}` came out as `{"keyValues":{"a":"1","b":"2"},"order":["b","a"]}`,
/// which is one `attrs` object stating two different orders for the same three
/// characters, and the HTML renderer, which reads `order`, already agreed with
/// the second one. PART 12 §1 is explicit about which of the two is publishable:
/// an implementation whose internals differ MAPS on the way out, it does not
/// export its internals. `resources/ast-schema.json` calls `order`
/// "Source-appearance order of the slots", and PART 11 §6 makes the author's
/// attribute order a choice "the AST records".
///
/// A key `order` does not mention is still published, after the ones it does. An
/// `Attrs` built programmatically records no order at all (the schema says so),
/// and dropping its attributes to protect an ordering would lose the document to
/// save the bookkeeping.
fn ordered_key_values(attrs: &Attrs) -> Vec<(&str, &str)> {
    let mut out: Vec<(&str, &str)> = Vec::with_capacity(attrs.key_values.len());
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for slot in &attrs.order {
        let AttrSlot::Key(key) = slot else {
            continue;
        };
        let Some((key, value)) = attrs.key_values.get_key_value(key.as_str()) else {
            continue;
        };
        if seen.insert(key.as_str()) {
            out.push((key.as_str(), value.as_str()));
        }
    }
    for (key, value) in &attrs.key_values {
        if seen.insert(key.as_str()) {
            out.push((key.as_str(), value.as_str()));
        }
    }
    out
}

fn write_attrs(out: &mut String, attrs: &Attrs) {
    let mut w = Writer::new(out);
    if let Some(id) = &attrs.id {
        w.field("id", |out| write_string(out, id));
    }
    if !attrs.classes.is_empty() {
        w.field("classes", |out| write_string_array(out, &attrs.classes));
    }
    if !attrs.key_values.is_empty() {
        w.field("keyValues", |out| {
            let mut w = Writer::new(out);
            for (key, value) in ordered_key_values(attrs) {
                w.field(key, |out| write_string(out, value));
            }
            w.finish();
        });
    }
    if !attrs.order.is_empty() {
        w.field("order", |out| {
            out.push('[');
            let mut first = true;
            for slot in &attrs.order {
                write_comma(out, &mut first);
                match slot {
                    AttrSlot::Id => write_string(out, "#id"),
                    AttrSlot::Class => write_string(out, ".class"),
                    AttrSlot::Key(key) => write_string(out, key),
                }
            }
            out.push(']');
        });
    }
    w.finish();
}

fn write_pos_field(w: &mut Writer<'_>, pos: &Option<Pos>) {
    if let Some(pos) = pos {
        w.field("pos", |out| write_pos(out, pos));
    }
}

fn write_pos(out: &mut String, pos: &Pos) {
    let mut w = Writer::new(out);
    w.field("startLine", |out| write_usize(out, pos.start_line));
    w.field("endLine", |out| write_usize(out, pos.end_line));
    w.field("startColumn", |out| write_usize(out, pos.start_column));
    w.field("endColumn", |out| write_usize(out, pos.end_column));
    w.field("startOffset", |out| write_usize(out, pos.start_offset));
    w.field("endOffset", |out| write_usize(out, pos.end_offset));
    w.finish();
}

fn write_blocks(out: &mut String, blocks: &[BlockNode]) {
    write_array(out, blocks, write_block);
}

fn write_inlines(out: &mut String, inlines: &[InlineNode]) {
    write_array(out, inlines, write_inline);
}

fn write_string_array(out: &mut String, values: &[String]) {
    write_array(out, values, |out, s| write_string(out, s));
}

fn write_array<T>(out: &mut String, values: &[T], mut f: impl FnMut(&mut String, &T)) {
    out.push('[');
    let mut first = true;
    for value in values {
        write_comma(out, &mut first);
        f(out, value);
    }
    out.push(']');
}

fn write_comma(out: &mut String, first: &mut bool) {
    if *first {
        *first = false;
    } else {
        out.push(',');
    }
}

fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if c <= '\u{1f}' => {
                out.push_str("\\u");
                out.push(hex((c as u32 >> 12) & 0xf));
                out.push(hex((c as u32 >> 8) & 0xf));
                out.push(hex((c as u32 >> 4) & 0xf));
                out.push(hex(c as u32 & 0xf));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn hex(n: u32) -> char {
    char::from_digit(n, 16).unwrap()
}

fn write_bool(out: &mut String, value: bool) {
    out.push_str(if value { "true" } else { "false" });
}

fn write_usize(out: &mut String, value: usize) {
    out.push_str(&value.to_string());
}

fn ol_type_json(t: OrderedListType) -> &'static str {
    match t {
        OrderedListType::LowerAlpha => "a",
        OrderedListType::UpperAlpha => "A",
        OrderedListType::LowerRoman => "i",
        OrderedListType::UpperRoman => "I",
    }
}

fn align_json(t: TableAlign) -> &'static str {
    match t {
        TableAlign::Left => "left",
        TableAlign::Right => "right",
        TableAlign::Center => "center",
    }
}

fn valign_json(t: TableVerticalAlign) -> &'static str {
    match t {
        TableVerticalAlign::Top => "top",
        TableVerticalAlign::Middle => "middle",
        TableVerticalAlign::Bottom => "bottom",
    }
}

pub(crate) fn emphasis_type(t: EmphasisKind) -> &'static str {
    match t {
        EmphasisKind::Italic => "emphasis",
        EmphasisKind::Strong | EmphasisKind::BoldItalic => "strong",
        EmphasisKind::Underline => "underline",
        EmphasisKind::Strike => "strike",
        EmphasisKind::Super => "superscript",
        EmphasisKind::Sub => "subscript",
        EmphasisKind::Highlight => "highlight",
    }
}

fn decode_blocks(values: &[Json]) -> Result<Vec<BlockNode>, AstJsonError> {
    values.iter().map(decode_block).collect()
}

fn decode_inlines(values: &[Json]) -> Result<Vec<InlineNode>, AstJsonError> {
    values.iter().map(decode_inline).collect()
}

fn decode_block(value: &Json) -> Result<BlockNode, AstJsonError> {
    let obj = value.as_object("block node")?;
    let ty = required_string(obj, "block node", "type")?;
    match ty {
        "paragraph" => Ok(BlockNode::Paragraph(Paragraph {
            attrs: optional_attrs(obj)?,
            children: decode_inlines(required_array(obj, "paragraph", "children")?)?,
            // Parse-internal and NOT on the wire: it records whether the
            // paragraph's first line sat at its container's content column, and
            // the only reader is the image-figure promotion, which runs during
            // parsing and has already run by the time anything is serialized.
            // The default is the conservative answer - `true` would be a claim
            // about the source this tree no longer has, and the one thing it
            // could still do is promote a figure the author did not write.
            at_content_column: false,
            pos: optional_pos(obj, "paragraph")?,
        })),
        "heading" => Ok(BlockNode::Heading(Heading {
            attrs: optional_attrs(obj)?,
            level: required_usize(obj, "heading", "level")? as u8,
            children: decode_inlines(required_array(obj, "heading", "children")?)?,
            pos: optional_pos(obj, "heading")?,
        })),
        "block_quote" => Ok(BlockNode::BlockQuote(BlockQuote {
            attrs: optional_attrs(obj)?,
            children: decode_blocks(required_array(obj, "block_quote", "children")?)?,
            pos: optional_pos(obj, "block_quote")?,
        })),
        "list" => Ok(BlockNode::List(List {
            attrs: optional_attrs(obj)?,
            ordered: required_bool(obj, "list", "ordered")?,
            tight: required_bool(obj, "list", "tight")?,
            items: required_array(obj, "list", "items")?
                .iter()
                .map(decode_list_item)
                .collect::<Result<_, _>>()?,
            start: optional_usize(obj, "start")?,
            ol_type: optional_string(obj, "olType")?
                .map(decode_ol_type)
                .transpose()?,
            bare_marker: optional_bool(obj, "bareMarker")?.unwrap_or(false),
            delim: optional_marker_char(obj, "delim")?,
            bullet_char: optional_marker_char(obj, "bulletChar")?,
            pos: optional_pos(obj, "list")?,
        })),
        "code_block" => Ok(BlockNode::CodeBlock(CodeBlock {
            attrs: optional_attrs(obj)?,
            lang: optional_string(obj, "lang")?.map(str::to_string),
            // `header` only: `title` is not a name the schema gives this node,
            // so the unknown-field check refused the payload before this could
            // read it. A fallback that cannot fire is a check that cannot fail
            // (carve-rs#820).
            title: optional_string(obj, "header")?.map(str::to_string),
            label: optional_string(obj, "label")?.map(str::to_string),
            content: required_string(obj, "code_block", "content")?.to_string(),
            pos: optional_pos(obj, "code_block")?,
        })),
        "thematic_break" => Ok(BlockNode::ThematicBreak(ThematicBreak {
            marker: optional_thematic_break_marker(obj)?,
            attrs: optional_attrs(obj)?,
            pos: optional_pos(obj, "thematic_break")?,
        })),
        "table" => Ok(BlockNode::Table(decode_table(obj)?)),
        "table_row" => Err(AstJsonError::new(
            "table_row is only valid inside table.rows",
        )),
        "table_cell" => Err(AstJsonError::new(
            "table_cell is only valid inside table_row.cells",
        )),
        "admonition" => Ok(BlockNode::Admonition(Admonition {
            attrs: optional_attrs(obj)?,
            kind: required_string(obj, "admonition", "kind")?.to_string(),
            title: optional_inlines(obj, "title")?,
            label: optional_string(obj, "label")?.map(str::to_string),
            children: decode_blocks(required_array(obj, "admonition", "children")?)?,
            pos: optional_pos(obj, "admonition")?,
        })),
        "div" => Ok(BlockNode::Div(Div {
            attrs: optional_attrs(obj)?,
            label: optional_string(obj, "label")?.map(str::to_string),
            children: decode_blocks(required_array(obj, "div", "children")?)?,
            pos: optional_pos(obj, "div")?,
        })),
        "line_block" => Ok(BlockNode::LineBlock(LineBlock {
            attrs: optional_attrs(obj)?,
            children: decode_blocks(required_array(obj, "line_block", "children")?)?,
            pos: optional_pos(obj, "line_block")?,
        })),
        "definition_list" => Ok(BlockNode::DefinitionList(DefinitionList {
            attrs: optional_attrs(obj)?,
            items: decode_definition_entries(required_array(obj, "definition_list", "items")?)?,
            pos: optional_pos(obj, "definition_list")?,
        })),
        "figure" => Ok(BlockNode::Figure(Figure {
            attrs: optional_attrs(obj)?,
            target: Box::new(decode_figure_target(required_value(
                obj, "figure", "target",
            )?)?),
            rendered_target: None,
            caption: decode_inlines(required_array(obj, "figure", "caption")?)?,
            short_caption: optional_inlines(obj, "shortCaption")?,
            pos: optional_pos(obj, "figure")?,
        })),
        "figure_group" => Ok(BlockNode::FigureGroup(FigureGroup {
            attrs: optional_attrs(obj)?,
            children: decode_blocks(required_array(obj, "figure_group", "children")?)?,
            caption: optional_inlines(obj, "caption")?,
            pos: optional_pos(obj, "figure_group")?,
        })),
        "link_reference_definition" => Ok(BlockNode::LinkReferenceDefinition(
            LinkReferenceDefinition {
                label: required_string(obj, "link_reference_definition", "label")?.to_string(),
                href: required_string(obj, "link_reference_definition", "href")?.to_string(),
                title: optional_string(obj, "title")?.map(str::to_string),
                attrs: optional_attrs(obj)?,
                pos: optional_pos(obj, "link_reference_definition")?,
            },
        )),
        "citation_definition" => Ok(BlockNode::CitationDefinition(CitationDefinition {
            key: required_string(obj, "citation_definition", "key")?.to_string(),
            children: decode_inlines(required_array(obj, "citation_definition", "children")?)?,
            attrs: optional_attrs(obj)?,
            pos: optional_pos(obj, "citation_definition")?,
        })),
        "abbreviation_def" => Ok(BlockNode::AbbreviationDef(AbbreviationDef {
            abbr: required_string(obj, "abbreviation_def", "abbr")?.to_string(),
            expansion: required_string(obj, "abbreviation_def", "expansion")?.to_string(),
            pos: optional_pos(obj, "abbreviation_def")?,
        })),
        "raw_block" => Ok(BlockNode::RawBlock(RawBlock {
            format: required_string(obj, "raw_block", "format")?.to_string(),
            content: required_string(obj, "raw_block", "content")?.to_string(),
            pos: optional_pos(obj, "raw_block")?,
        })),
        "comment" => Ok(BlockNode::Comment(Comment {
            block: required_bool(obj, "comment", "block")?,
            delimited: optional_bool(obj, "delimited")?.unwrap_or(false),
            content: required_string(obj, "comment", "content")?.to_string(),
            pos: optional_pos(obj, "comment")?,
        })),
        "block_extension" => Ok(BlockNode::Extension(BlockExtension {
            attrs: optional_attrs(obj)?,
            name: required_string(obj, "block_extension", "name")?.to_string(),
            children: decode_blocks(required_array(obj, "block_extension", "children")?)?,
            summary: optional_inlines(obj, "summary")?,
            label: optional_string(obj, "label")?.map(str::to_string),
            pos: optional_pos(obj, "block_extension")?,
        })),
        "image" => Ok(BlockNode::BlockImage(decode_image(obj)?)),
        "frontmatter" => Err(AstJsonError::new(
            "frontmatter is only valid as a document child",
        )),
        "footnote" => Err(AstJsonError::new(
            "footnote is only valid as a document child",
        )),
        other => Err(AstJsonError::new(format!(
            "unknown block node type {other:?}"
        ))),
    }
}

fn decode_list_item(value: &Json) -> Result<ListItem, AstJsonError> {
    let obj = value.as_object("list_item")?;
    expect_type(obj, "list_item")?;
    Ok(ListItem {
        attrs: optional_attrs(obj)?,
        checked: optional_bool(obj, "checked")?,
        children: decode_blocks(required_array(obj, "list_item", "children")?)?,
        pos: optional_pos(obj, "list_item")?,
    })
}

fn decode_table(obj: &BTreeMap<String, Json>) -> Result<Table, AstJsonError> {
    let rows: Vec<TableRow> = required_array(obj, "table", "rows")?
        .iter()
        .map(decode_table_row)
        .collect::<Result<_, _>>()?;
    let row_groups = match obj.get("rowGroups") {
        Some(value) => Some(decode_row_groups(value, rows.len())?),
        None => None,
    };
    Ok(Table {
        attrs: optional_attrs(obj)?,
        caption: optional_inlines(obj, "caption")?,
        short_caption: optional_inlines(obj, "shortCaption")?,
        columns: match obj.get("columns") {
            Some(Json::Array(values)) => values
                .iter()
                .map(decode_table_column)
                .collect::<Result<_, _>>()?,
            Some(_) => {
                return Err(AstJsonError::new(
                    "table.columns must be an array".to_owned(),
                ))
            }
            None => Vec::new(),
        },
        rows,
        row_groups,
        pos: optional_pos(obj, "table")?,
    })
}

fn decode_table_column(value: &Json) -> Result<TableColumn, AstJsonError> {
    let obj = value.as_object("table.columns[]")?;
    let width = match obj.get("width") {
        Some(Json::Number(v)) if *v > 0 && *v <= 1 => Some(*v as f64),
        Some(Json::Float(v)) if *v > 0.0 && *v <= 1.0 => Some(*v),
        Some(_) => {
            return Err(AstJsonError::new(
                "table.columns[].width must be in (0, 1]".to_owned(),
            ))
        }
        None => None,
    };
    Ok(TableColumn {
        align: optional_string(obj, "align")?
            .map(decode_table_align)
            .transpose()?,
        valign: optional_string(obj, "valign")?
            .map(decode_table_valign)
            .transpose()?,
        width,
    })
}

/// PART 12 §15's partition MUST, checked HERE because here it can fail.
///
/// The counts have to account for every row exactly once. JSON Schema cannot
/// express a sum across fields, so the schema does not check it and a
/// non-summing partition validates; the importer cannot check it either, because
/// there the counts and the rows are built from the same list. A payload
/// arriving from elsewhere is the one place the two can disagree.
fn decode_row_groups(value: &Json, rows: usize) -> Result<TableRowGroups, AstJsonError> {
    let obj = value.as_object("table.rowGroups")?;
    let head_rows = required_usize(obj, "table.rowGroups", "headRows")?;
    let foot_rows = required_usize(obj, "table.rowGroups", "footRows")?;
    let bodies = required_array(obj, "table.rowGroups", "bodies")?
        .iter()
        .map(|body| {
            let body = body.as_object("table.rowGroups.bodies[]")?;
            Ok(TableBodyGroup {
                head_rows: required_usize(body, "table.rowGroups.bodies[]", "headRows")?,
                body_rows: required_usize(body, "table.rowGroups.bodies[]", "bodyRows")?,
                row_head_columns: optional_usize(body, "rowHeadColumns")?,
                attrs: optional_attrs(body)?,
            })
        })
        .collect::<Result<Vec<_>, AstJsonError>>()?;
    // CHECKED, because every one of these is a number off untrusted JSON and
    // `as_usize` bounds none of them. Two counts near `usize::MAX` wrap to a
    // small total in release and panic in debug, so the sum could both abort the
    // process and, wrapped, ACCEPT a partition that consumes nothing.
    let counted = bodies
        .iter()
        .try_fold(head_rows, |total, body| {
            total
                .checked_add(body.head_rows)?
                .checked_add(body.body_rows)
        })
        .and_then(|total| total.checked_add(foot_rows));
    let Some(counted) = counted else {
        return Err(AstJsonError::new(
            "table.rowGroups does not partition the table's rows: its counts do not add up to a number of rows (PART 12 §15)".to_owned(),
        ));
    };
    if counted != rows {
        return Err(AstJsonError::new(format!(
            "table.rowGroups does not partition the table's rows: the head, bodies and foot account for {counted} row{} of {rows} (PART 12 §15)",
            if counted == 1 { "" } else { "s" }
        )));
    }
    Ok(TableRowGroups {
        head_rows,
        bodies,
        foot_rows,
    })
}

fn decode_table_row(value: &Json) -> Result<TableRow, AstJsonError> {
    let obj = value.as_object("table_row")?;
    expect_type(obj, "table_row")?;
    Ok(TableRow {
        cells: required_array(obj, "table_row", "cells")?
            .iter()
            .map(decode_table_cell)
            .collect::<Result<_, _>>()?,
        attrs: optional_attrs(obj)?,
        pos: optional_pos(obj, "table_row")?,
    })
}

fn decode_table_cell(value: &Json) -> Result<TableCell, AstJsonError> {
    let obj = value.as_object("table_cell")?;
    expect_type(obj, "table_cell")?;
    Ok(TableCell {
        header: required_bool(obj, "table_cell", "header")?,
        span: optional_string(obj, "span")?
            .map(decode_cell_span)
            .transpose()?,
        align: optional_string(obj, "align")?
            .map(decode_table_align)
            .transpose()?,
        valign: optional_string(obj, "valign")?
            .map(decode_table_valign)
            .transpose()?,
        attrs: optional_attrs(obj)?,
        children: decode_inlines(required_array(obj, "table_cell", "children")?)?,
        pos: optional_pos(obj, "table_cell")?,
    })
}

/// The flat `<dt>` / `<dd>` sequence back to this engine's grouped entries.
///
/// The grouping rule is the renderer's, which is the only one all three engines
/// agree on: a run of terms opens an entry, the descriptions after it belong to
/// it, and the next term after a description starts the next entry.
///
/// A payload in the OLD `{terms, definitions}` form still decodes - trees in
/// that shape are stored, and this engine wrote them.
fn decode_definition_entries(values: &[Json]) -> Result<Vec<DefinitionItem>, AstJsonError> {
    let mut items: Vec<DefinitionItem> = Vec::new();

    for value in values {
        let obj = value.as_object("definition_list.items[]")?;
        if obj.contains_key("terms") || obj.contains_key("definitions") {
            items.push(decode_definition_item(value)?);
            continue;
        }

        let ty = required_string(obj, "definition_list.items[]", "type")?;
        match ty {
            "definition_term" => {
                let term = DefinitionTerm {
                    attrs: optional_attrs(obj)?,
                    children: decode_inlines(required_array(obj, "definition_term", "children")?)?,
                    pos: optional_pos(obj, "definition_term")?,
                };
                let start_new = items
                    .last()
                    .map(|item| !item.definitions.is_empty())
                    .unwrap_or(true);
                if start_new {
                    items.push(DefinitionItem {
                        terms: Vec::new(),
                        definitions: Vec::new(),
                        pos: None,
                    });
                }
                items
                    .last_mut()
                    .expect("an entry was just pushed")
                    .terms
                    .push(term);
            }
            "definition_description" => {
                let definition = DefinitionDef {
                    attrs: optional_attrs(obj)?,
                    children: decode_blocks(required_array(
                        obj,
                        "definition_description",
                        "children",
                    )?)?,
                    pos: optional_pos(obj, "definition_description")?,
                };
                if items.is_empty() {
                    // A description with no term before it: the parser cannot
                    // produce one, a hand-built payload can, and dropping it
                    // would lose content the caller handed us.
                    items.push(DefinitionItem {
                        terms: Vec::new(),
                        definitions: Vec::new(),
                        pos: None,
                    });
                }
                items
                    .last_mut()
                    .expect("an entry exists")
                    .definitions
                    .push(definition);
            }
            other => {
                return Err(AstJsonError::new(format!(
                "a definition list holds definition_term and definition_description, not {other}"
            )))
            }
        }
    }

    Ok(items)
}

fn decode_definition_item(value: &Json) -> Result<DefinitionItem, AstJsonError> {
    let obj = value.as_object("definition_item")?;
    let terms = required_array(obj, "definition_item", "terms")?
        .iter()
        .map(|value| {
            Ok(DefinitionTerm {
                attrs: None,
                children: decode_inlines(value.as_array("definition_item.terms[]")?)?,
                pos: None,
            })
        })
        .collect::<Result<_, AstJsonError>>()?;
    let definitions = required_array(obj, "definition_item", "definitions")?
        .iter()
        .map(|value| {
            Ok(DefinitionDef {
                attrs: None,
                children: decode_blocks(value.as_array("definition_item.definitions[]")?)?,
                pos: None,
            })
        })
        .collect::<Result<_, AstJsonError>>()?;
    Ok(DefinitionItem {
        terms,
        definitions,
        pos: None,
    })
}

fn decode_figure_target(value: &Json) -> Result<FigureTarget, AstJsonError> {
    let obj = value.as_object("figure.target")?;
    match required_string(obj, "figure.target", "type")? {
        "image" => Ok(FigureTarget::Image(decode_image(obj)?)),
        "block_quote" => match decode_block(value)? {
            BlockNode::BlockQuote(n) => Ok(FigureTarget::BlockQuote(n)),
            _ => unreachable!(),
        },
        "table" => Ok(FigureTarget::Table(decode_table(obj)?)),
        "code_block" => match decode_block(value)? {
            BlockNode::CodeBlock(n) => Ok(FigureTarget::CodeBlock(n)),
            _ => unreachable!(),
        },
        "paragraph" => match decode_block(value)? {
            BlockNode::Paragraph(n) => Ok(FigureTarget::Paragraph(n)),
            _ => unreachable!(),
        },
        other => Err(AstJsonError::new(format!(
            "unknown figure.target node type {other:?}"
        ))),
    }
}

fn decode_inline(value: &Json) -> Result<InlineNode, AstJsonError> {
    let obj = value.as_object("inline node")?;
    let ty = required_string(obj, "inline node", "type")?;
    match ty {
        "text" => Ok(InlineNode::Text(Text {
            value: required_string(obj, "text", "value")?.to_string(),
            pos: optional_pos(obj, "text")?,
        })),
        "escaped_text" => Ok(InlineNode::EscapedText(EscapedText {
            value: required_string(obj, "escaped_text", "value")?.to_string(),
            pos: optional_pos(obj, "escaped_text")?,
        })),
        "smart_punctuation" => Ok(InlineNode::SmartPunctuation(SmartPunctuation {
            kind: required_string(obj, "smart_punctuation", "kind")?.to_string(),
            value: required_string(obj, "smart_punctuation", "value")?.to_string(),
            glyph: optional_string(obj, "glyph")?.map(str::to_string),
            pos: optional_pos(obj, "smart_punctuation")?,
        })),
        "emphasis" | "strong" | "underline" | "strike" | "superscript" | "subscript"
        | "highlight" => {
            let kind = decode_emphasis_kind(ty, obj)?;
            let mut children = decode_inlines(required_array(obj, ty, "children")?)?;
            // The combined form is ONE node here and TWO on the wire: a
            // `strong` marked `boldItalic` wrapping an `emphasis` (PART 11 §6).
            // Unwrap that single emphasis child on the way in, or the kind and
            // the child both add italic and `/*x*/` round-trips to
            // `<strong><em><em>` (#513).
            if kind == EmphasisKind::BoldItalic && children.len() == 1 {
                if let InlineNode::Emphasis(inner) = &children[0] {
                    if inner.kind == EmphasisKind::Italic && inner.attrs.is_none() {
                        children = inner.children.clone();
                    }
                }
            }
            Ok(InlineNode::Emphasis(Emphasis {
                attrs: optional_attrs(obj)?,
                kind,
                children,
                pos: optional_pos(obj, ty)?,
            }))
        }
        "code" => Ok(InlineNode::Code(Code {
            value: required_string(obj, "code", "value")?.to_string(),
            attrs: optional_attrs(obj)?,
            pos: optional_pos(obj, "code")?,
        })),
        "link" => Ok(InlineNode::Link(Link {
            attrs: optional_attrs(obj)?,
            href: required_string(obj, "link", "href")?.to_string(),
            title: optional_string(obj, "title")?.map(str::to_string),
            children: decode_inlines(required_array(obj, "link", "children")?)?,
            ref_label: optional_string(obj, "ref")?.map(str::to_string),
            raw_ref: optional_string(obj, "rawRef")?.map(str::to_string),
            // Neither flag is on the wire: both are a writer's concern, and a
            // decoded document rebuilds them from the heading index the same way
            // a parse does. Reading `fromCrossref` here could never fire anyway -
            // `refuse_unknown_fields` turns the whole payload away first
            // (carve-rs#776).
            from_crossref: false,
            from_heading_reference: false,
            pos: optional_pos(obj, "link")?,
        })),
        "image" => Ok(InlineNode::Image(decode_image(obj)?)),
        "span" => Ok(InlineNode::Span(Span {
            // Ingested content is AUTHORED; `injected` is this crate's own
            // render-time bookkeeping and never a field of the payload.
            injected: false,
            attrs: optional_attrs(obj)?,
            children: decode_inlines(required_array(obj, "span", "children")?)?,
            pos: optional_pos(obj, "span")?,
        })),
        "math" => Ok(InlineNode::Math(Math {
            attrs: optional_attrs(obj)?,
            display: required_bool(obj, "math", "display")?,
            content: required_string(obj, "math", "content")?.to_string(),
            pos: optional_pos(obj, "math")?,
        })),
        "raw_inline" => Ok(InlineNode::RawInline(RawInline {
            format: required_string(obj, "raw_inline", "format")?.to_string(),
            content: required_string(obj, "raw_inline", "content")?.to_string(),
            // An ingested node is AUTHORED content: `injected` is a render-time
            // fact this crate sets, never a field of the payload (PART 12 §7).
            injected: false,
            pos: optional_pos(obj, "raw_inline")?,
        })),
        "literal_inline" => Ok(InlineNode::LiteralInline(LiteralInline {
            content: required_string(obj, "literal_inline", "content")?.to_string(),
            attrs: optional_attrs(obj)?,
            pos: optional_pos(obj, "literal_inline")?,
        })),
        "symbol" => Ok(InlineNode::Symbol(Symbol {
            name: required_string(obj, "symbol", "name")?.to_string(),
            attrs: optional_attrs(obj)?,
            pos: optional_pos(obj, "symbol")?,
        })),
        "autolink" => Ok(InlineNode::AutoLink(AutoLink {
            attrs: optional_attrs(obj)?,
            href: required_string(obj, "autolink", "href")?.to_string(),
            text: optional_string(obj, "text")?.unwrap_or("").to_string(),
            pos: optional_pos(obj, "autolink")?,
        })),
        "heading_ref" => Ok(InlineNode::CrossRef(CrossRef {
            target: required_string(obj, "heading_ref", "target")?.to_string(),
            href: optional_string(obj, "href")?.map(str::to_string),
            pos: optional_pos(obj, "heading_ref")?,
        })),
        "caption_number" => Ok(InlineNode::CaptionNumber(CaptionNumber {
            number: optional_usize(obj, "n")?,
            pos: optional_pos(obj, "caption_number")?,
        })),
        "mention" => Ok(InlineNode::Mention(Mention {
            attrs: optional_attrs(obj)?,
            user: required_string(obj, "mention", "user")?.to_string(),
            pos: optional_pos(obj, "mention")?,
        })),
        "tag" => Ok(InlineNode::Tag(Tag {
            attrs: optional_attrs(obj)?,
            name: required_string(obj, "tag", "name")?.to_string(),
            pos: optional_pos(obj, "tag")?,
        })),
        "citation_group" => Ok(InlineNode::CitationGroup(CitationGroup {
            items: required_array(obj, "citation_group", "items")?
                .iter()
                .map(decode_citation)
                .collect::<Result<_, _>>()?,
            raw: required_string(obj, "citation_group", "raw")?.to_string(),
            mode: None,
            integral: optional_string(obj, "mode")? == Some("integral"),
            pos: optional_pos(obj, "citation_group")?,
        })),
        "inline_extension" => Ok(InlineNode::Extension(InlineExtension {
            attrs: optional_attrs(obj)?,
            name: required_string(obj, "inline_extension", "name")?.to_string(),
            // `content` only, for the reason given at `code_block` above:
            // `children` is not a name the schema gives this node, so the
            // fallback could never be reached.
            children: decode_inlines(required_array(obj, "inline_extension", "content")?)?,
            pos: optional_pos(obj, "inline_extension")?,
        })),
        "abbreviation" => Ok(InlineNode::Abbreviation(Abbreviation {
            abbr: required_string(obj, "abbreviation", "abbr")?.to_string(),
            expansion: required_string(obj, "abbreviation", "expansion")?.to_string(),
            pos: optional_pos(obj, "abbreviation")?,
        })),
        "footnote_ref" => Ok(InlineNode::Footnote(Footnote {
            attrs: optional_attrs(obj)?,
            id: optional_string(obj, "id")?.map(str::to_string),
            inline: None,
            number: optional_usize(obj, "number")?,
            // NOT read from the wire. `refId` is a rendering convention -
            // `fnref1`, the anchor an endnotes section links back to - and
            // carve#762 removed it from the schema, so a tree carrying one is
            // invalid under `additionalProperties: false`. Reading it echoed a
            // payload's value straight back out, and an inherited anchor would
            // carry the previous document's numbering (carve-rs#648). The HTML
            // renderer assigns this itself while numbering.
            ref_id: None,
            pos: optional_pos(obj, "footnote_ref")?,
        })),
        "inline_footnote" => Ok(InlineNode::Footnote(Footnote {
            attrs: optional_attrs(obj)?,
            id: None,
            inline: Some(decode_inlines(required_array(
                obj,
                "inline_footnote",
                "inline",
            )?)?),
            number: optional_usize(obj, "number")?,
            // NOT read from the wire. `refId` is a rendering convention -
            // `fnref1`, the anchor an endnotes section links back to - and
            // carve#762 removed it from the schema, so a tree carrying one is
            // invalid under `additionalProperties: false`. Reading it echoed a
            // payload's value straight back out, and an inherited anchor would
            // carry the previous document's numbering (carve-rs#648). The HTML
            // renderer assigns this itself while numbering.
            ref_id: None,
            pos: optional_pos(obj, "inline_footnote")?,
        })),
        "soft_break" => Ok(InlineNode::SoftBreak(Break {
            pos: optional_pos(obj, "soft_break")?,
        })),
        "hard_break" => Ok(InlineNode::HardBreak(Break {
            pos: optional_pos(obj, "hard_break")?,
        })),
        "insert" => Ok(InlineNode::CriticInsert(CriticInsert {
            attrs: optional_attrs(obj)?,
            children: decode_inlines(required_array(obj, "insert", "children")?)?,
            pos: optional_pos(obj, "insert")?,
        })),
        "delete" => Ok(InlineNode::CriticDelete(CriticDelete {
            attrs: optional_attrs(obj)?,
            children: decode_inlines(required_array(obj, "delete", "children")?)?,
            pos: optional_pos(obj, "delete")?,
        })),
        "substitution" => Ok(InlineNode::CriticSubstitute(CriticSubstitute {
            old_text: required_string(obj, "substitution", "oldText")?.to_string(),
            new_text: required_string(obj, "substitution", "newText")?.to_string(),
            pos: optional_pos(obj, "substitution")?,
        })),
        "critic_comment" => Ok(InlineNode::CriticComment(CriticComment {
            text: required_string(obj, "critic_comment", "text")?.to_string(),
            pos: optional_pos(obj, "critic_comment")?,
        })),
        // The inline half of `comment`. The block half decodes in the block
        // table above; `block` says which one a payload names.
        "comment" => Ok(InlineNode::Comment(Comment {
            block: required_bool(obj, "comment", "block")?,
            delimited: optional_bool(obj, "delimited")?.unwrap_or(false),
            content: required_string(obj, "comment", "content")?.to_string(),
            pos: optional_pos(obj, "comment")?,
        })),
        other => Err(AstJsonError::new(format!(
            "unknown inline node type {other:?}"
        ))),
    }
}

fn decode_image(obj: &BTreeMap<String, Json>) -> Result<Image, AstJsonError> {
    Ok(Image {
        attrs: optional_attrs(obj)?,
        src: required_string(obj, "image", "src")?.to_string(),
        alt: required_string(obj, "image", "alt")?.to_string(),
        title: optional_string(obj, "title")?.map(str::to_string),
        ref_label: optional_string(obj, "ref")?.map(str::to_string),
        raw_ref: optional_string(obj, "rawRef")?.map(str::to_string),
        pos: optional_pos(obj, "image")?,
    })
}

fn decode_citation(value: &Json) -> Result<Citation, AstJsonError> {
    let obj = value.as_object("citation")?;
    Ok(Citation {
        key: required_string(obj, "citation", "key")?.to_string(),
        prefix: optional_inlines(obj, "prefix")?,
        locator: optional_inlines(obj, "locator")?,
        locator_label: optional_string(obj, "locatorLabel")?.map(str::to_string),
        locator_value: optional_string(obj, "locatorValue")?.map(str::to_string),
        suffix: optional_inlines(obj, "suffix")?,
        suppress_author: required_bool(obj, "citation", "suppressAuthor")?,
        number: optional_usize(obj, "number")?,
        label: None,
        use_index: optional_usize(obj, "useIndex")?,
    })
}

fn decode_emphasis_kind(
    ty: &str,
    obj: &BTreeMap<String, Json>,
) -> Result<EmphasisKind, AstJsonError> {
    Ok(match ty {
        "emphasis" => EmphasisKind::Italic,
        "strong" if optional_bool(obj, "boldItalic")?.unwrap_or(false) => EmphasisKind::BoldItalic,
        "strong" => EmphasisKind::Strong,
        "underline" => EmphasisKind::Underline,
        "strike" => EmphasisKind::Strike,
        "superscript" => EmphasisKind::Super,
        "subscript" => EmphasisKind::Sub,
        "highlight" => EmphasisKind::Highlight,
        _ => return Err(AstJsonError::new(format!("unknown emphasis type {ty:?}"))),
    })
}

fn decode_ol_type(value: &str) -> Result<OrderedListType, AstJsonError> {
    match value {
        "a" => Ok(OrderedListType::LowerAlpha),
        "A" => Ok(OrderedListType::UpperAlpha),
        "i" => Ok(OrderedListType::LowerRoman),
        "I" => Ok(OrderedListType::UpperRoman),
        other => Err(AstJsonError::new(format!(
            "list.olType has invalid value {other:?}"
        ))),
    }
}

fn decode_cell_span(value: &str) -> Result<TableCellSpan, AstJsonError> {
    match value {
        "rowspan" => Ok(TableCellSpan::Rowspan),
        "colspan" => Ok(TableCellSpan::Colspan),
        other => Err(AstJsonError::new(format!(
            "table_cell.span has invalid value {other:?}"
        ))),
    }
}

fn decode_table_align(value: &str) -> Result<TableAlign, AstJsonError> {
    match value {
        "left" => Ok(TableAlign::Left),
        "right" => Ok(TableAlign::Right),
        "center" => Ok(TableAlign::Center),
        other => Err(AstJsonError::new(format!(
            "table_cell.align has invalid value {other:?}"
        ))),
    }
}

fn decode_table_valign(value: &str) -> Result<TableVerticalAlign, AstJsonError> {
    match value {
        "top" => Ok(TableVerticalAlign::Top),
        "middle" => Ok(TableVerticalAlign::Middle),
        "bottom" => Ok(TableVerticalAlign::Bottom),
        other => Err(AstJsonError::new(format!(
            "table_cell.valign has invalid value {other:?}"
        ))),
    }
}

fn optional_marker_char(
    obj: &BTreeMap<String, Json>,
    field: &str,
) -> Result<Option<char>, AstJsonError> {
    optional_string(obj, field)?
        .map(|s| {
            let mut chars = s.chars();
            let Some(ch) = chars.next() else {
                return Err(AstJsonError::new(format!("list.{field} cannot be empty")));
            };
            if chars.next().is_some() {
                return Err(AstJsonError::new(format!(
                    "list.{field} must be one character"
                )));
            }
            Ok(ch)
        })
        .transpose()
}

fn optional_thematic_break_marker(
    obj: &BTreeMap<String, Json>,
) -> Result<Option<char>, AstJsonError> {
    let marker = optional_marker_char(obj, "marker")?;
    match marker {
        None | Some('-' | '*' | '_') => Ok(marker),
        Some(value) => Err(AstJsonError::new(format!(
            "thematic_break.marker must be one of `-`, `*`, or `_`, got `{value}`"
        ))),
    }
}

fn optional_attrs(obj: &BTreeMap<String, Json>) -> Result<Option<Attrs>, AstJsonError> {
    let Some(value) = obj.get("attrs") else {
        return Ok(None);
    };
    let attrs_obj = value.as_object("attrs")?;
    let mut key_values = BTreeMap::new();
    if let Some(kv) = attrs_obj.get("keyValues") {
        for (key, value) in kv.as_object("attrs.keyValues")? {
            key_values.insert(
                key.clone(),
                value.as_string("attrs.keyValues value")?.to_string(),
            );
        }
    }
    let classes = match attrs_obj.get("classes") {
        Some(value) => value
            .as_array("attrs.classes")?
            .iter()
            .map(|value| value.as_string("attrs.classes[]").map(str::to_string))
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };
    let order = match attrs_obj.get("order") {
        Some(value) => value
            .as_array("attrs.order")?
            .iter()
            .map(|value| {
                let slot = value.as_string("attrs.order[]")?;
                Ok(match slot {
                    "#id" => AttrSlot::Id,
                    ".class" => AttrSlot::Class,
                    key => AttrSlot::Key(key.to_string()),
                })
            })
            .collect::<Result<_, AstJsonError>>()?,
        None => Vec::new(),
    };
    Ok(Some(Attrs {
        id: optional_string(attrs_obj, "id")?.map(str::to_string),
        classes,
        key_values,
        order,
    }))
}

fn optional_pos(
    obj: &BTreeMap<String, Json>,
    node_type: &str,
) -> Result<Option<Pos>, AstJsonError> {
    let Some(value) = obj.get("pos") else {
        return Ok(None);
    };
    let pos = value.as_object("pos")?;
    Ok(Some(Pos {
        start_line: required_usize(pos, node_type, "startLine")?,
        end_line: required_usize(pos, node_type, "endLine")?,
        start_column: required_usize(pos, node_type, "startColumn")?,
        end_column: required_usize(pos, node_type, "endColumn")?,
        start_offset: required_usize(pos, node_type, "startOffset")?,
        end_offset: required_usize(pos, node_type, "endOffset")?,
    }))
}

fn optional_inlines(
    obj: &BTreeMap<String, Json>,
    field: &str,
) -> Result<Option<Vec<InlineNode>>, AstJsonError> {
    obj.get(field)
        .map(|value| decode_inlines(value.as_array(field)?))
        .transpose()
}

fn expect_type(obj: &BTreeMap<String, Json>, expected: &str) -> Result<(), AstJsonError> {
    let actual = required_string(obj, expected, "type")?;
    if actual == expected {
        Ok(())
    } else {
        Err(AstJsonError::new(format!(
            "{expected}.type must be {expected:?}, got {actual:?}"
        )))
    }
}

fn required_value<'a>(
    obj: &'a BTreeMap<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<&'a Json, AstJsonError> {
    obj.get(field)
        .ok_or_else(|| AstJsonError::new(format!("{node_type}.{field} is required")))
}

fn required_array<'a>(
    obj: &'a BTreeMap<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<&'a [Json], AstJsonError> {
    required_value(obj, node_type, field)?.as_array(&format!("{node_type}.{field}"))
}

fn required_string<'a>(
    obj: &'a BTreeMap<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<&'a str, AstJsonError> {
    required_value(obj, node_type, field)?.as_string(&format!("{node_type}.{field}"))
}

fn required_bool(
    obj: &BTreeMap<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<bool, AstJsonError> {
    required_value(obj, node_type, field)?.as_bool(&format!("{node_type}.{field}"))
}

fn required_usize(
    obj: &BTreeMap<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<usize, AstJsonError> {
    required_value(obj, node_type, field)?.as_usize(&format!("{node_type}.{field}"))
}

fn optional_string<'a>(
    obj: &'a BTreeMap<String, Json>,
    field: &str,
) -> Result<Option<&'a str>, AstJsonError> {
    obj.get(field)
        .map(|value| value.as_string(field))
        .transpose()
}

fn optional_bool(obj: &BTreeMap<String, Json>, field: &str) -> Result<Option<bool>, AstJsonError> {
    obj.get(field).map(|value| value.as_bool(field)).transpose()
}

fn optional_usize(
    obj: &BTreeMap<String, Json>,
    field: &str,
) -> Result<Option<usize>, AstJsonError> {
    obj.get(field)
        .map(|value| value.as_usize(field))
        .transpose()
}

impl Json {
    fn as_object(&self, context: &str) -> Result<&BTreeMap<String, Json>, AstJsonError> {
        match self {
            Json::Object(obj) => Ok(obj),
            _ => Err(AstJsonError::new(format!("{context} must be an object"))),
        }
    }

    fn as_array(&self, context: &str) -> Result<&[Json], AstJsonError> {
        match self {
            Json::Array(values) => Ok(values),
            _ => Err(AstJsonError::new(format!("{context} must be an array"))),
        }
    }

    fn as_string(&self, context: &str) -> Result<&str, AstJsonError> {
        match self {
            Json::String(value) => Ok(value),
            _ => Err(AstJsonError::new(format!("{context} must be a string"))),
        }
    }

    fn as_bool(&self, context: &str) -> Result<bool, AstJsonError> {
        match self {
            Json::Bool(value) => Ok(*value),
            _ => Err(AstJsonError::new(format!("{context} must be a boolean"))),
        }
    }

    fn as_usize(&self, context: &str) -> Result<usize, AstJsonError> {
        match self {
            Json::Number(value) if *value >= 0 => Ok(*value as usize),
            _ => Err(AstJsonError::new(format!(
                "{context} must be a non-negative integer"
            ))),
        }
    }
}

/// Deepest JSON nesting the reader will follow.
///
/// The reader is recursive-descent, so nesting depth is stack depth, and a
/// document is untrusted input: `[[[[…]]]]` 200000 deep overflowed the stack and
/// ABORTED the process rather than returning an error. The markup parser bounds
/// itself the same way (`MAX_NESTING_DEPTH` in parse.rs, 200, matching carve-js
/// and carve-php).
///
/// THE UNIT IS NOT THE PARSER'S. This bound counts JSON structural levels; the
/// parser's `MAX_NESTING_DEPTH` counts AST levels. The conversion between them
/// is not a property of the format - it is a property of whichever container
/// has the LONGEST FIELD CHAIN on the wire, so it changes whenever a node type
/// gains a field:
///
/// - a div is `object` + `children` array, 2 structural levels per AST level
/// - a list is `object` + `items` + `list_item` + `children`, about 4
/// - a table is 6, the deepest chain any container has
///
/// Measured at the parser's cap of 200: 405 structural levels for a div ladder,
/// 405 for blockquotes, 805 for a list ladder (the worst that SCALES, ~4.1 per
/// level), 402 for a table under a deep chain, where an innermost table adds a
/// constant rather than scaling.
///
/// Getting this wrong is how the reader came to reject ASTs its own encoder had
/// produced (carve-rs#389): the bound was once the parser's 200 read as if it
/// were structural, which fits only about 99 containers, and a first fix
/// generalised a 2:1 ratio off the div shape and still rejected a 200-deep
/// list.
///
/// The lesson is the RATIO, not the deriving. This stays a function of the
/// parser's cap - raise that and this rises with it, which is the point, since
/// the reader must accept whatever the parser can emit whatever that limit
/// becomes. What is derived from measurement is the multiplier: the longest
/// field chain, not a ratio read off one shape.
const LONGEST_FIELD_CHAIN: usize = 6;
const MAX_JSON_DEPTH: usize = crate::parse::MAX_NESTING_DEPTH * LONGEST_FIELD_CHAIN + 16;

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            depth: 0,
        }
    }

    /// Run `f` one level deeper, refusing past the cap.
    fn nested<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, AstJsonError>,
    ) -> Result<T, AstJsonError> {
        if self.depth >= MAX_JSON_DEPTH {
            return Err(self.err("JSON nests deeper than the reader's depth budget"));
        }
        self.depth += 1;
        let out = f(self);
        self.depth -= 1;
        out
    }

    fn parse(mut self) -> Result<Json, AstJsonError> {
        let value = self.parse_value()?;
        self.skip_ws();
        if self.pos != self.input.len() {
            return Err(self.err("trailing characters after JSON value"));
        }
        Ok(value)
    }

    fn parse_value(&mut self) -> Result<Json, AstJsonError> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null", Json::Null),
            Some(b't') => self.parse_literal(b"true", Json::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", Json::Bool(false)),
            Some(b'"') => self.parse_string().map(Json::String),
            Some(b'[') => self.nested(Self::parse_array),
            Some(b'{') => self.nested(Self::parse_object),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(_) => Err(self.err("unexpected character in JSON value")),
            None => Err(self.err("unexpected end of input")),
        }
    }

    fn parse_literal(&mut self, literal: &[u8], value: Json) -> Result<Json, AstJsonError> {
        if self.input.as_bytes()[self.pos..].starts_with(literal) {
            self.pos += literal.len();
            Ok(value)
        } else {
            Err(self.err("invalid JSON literal"))
        }
    }

    fn parse_array(&mut self) -> Result<Json, AstJsonError> {
        self.expect(b'[')?;
        let mut values = Vec::new();
        loop {
            self.skip_ws();
            if self.consume(b']') {
                break;
            }
            values.push(self.parse_value()?);
            self.skip_ws();
            if self.consume(b']') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Array(values))
    }

    fn parse_object(&mut self) -> Result<Json, AstJsonError> {
        self.expect(b'{')?;
        let mut values = BTreeMap::new();
        loop {
            self.skip_ws();
            if self.consume(b'}') {
                break;
            }
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            values.insert(key, value);
            self.skip_ws();
            if self.consume(b'}') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(Json::Object(values))
    }

    fn parse_string(&mut self) -> Result<String, AstJsonError> {
        self.expect(b'"')?;
        let mut out = String::new();
        while let Some(byte) = self.next() {
            match byte {
                b'"' => return Ok(out),
                b'\\' => self.parse_escape(&mut out)?,
                0x00..=0x1f => return Err(self.err("control character in JSON string")),
                _ => {
                    let start = self.pos - 1;
                    let ch = self.input[start..]
                        .chars()
                        .next()
                        .ok_or_else(|| self.err("invalid UTF-8 in JSON string"))?;
                    self.pos = start + ch.len_utf8();
                    out.push(ch);
                }
            }
        }
        Err(self.err("unterminated JSON string"))
    }

    fn parse_escape(&mut self, out: &mut String) -> Result<(), AstJsonError> {
        match self.next() {
            Some(b'"') => out.push('"'),
            Some(b'\\') => out.push('\\'),
            Some(b'/') => out.push('/'),
            Some(b'b') => out.push('\u{08}'),
            Some(b'f') => out.push('\u{0c}'),
            Some(b'n') => out.push('\n'),
            Some(b'r') => out.push('\r'),
            Some(b't') => out.push('\t'),
            Some(b'u') => {
                let code = self.parse_hex4()?;
                if (0xd800..=0xdbff).contains(&code) {
                    self.expect(b'\\')?;
                    self.expect(b'u')?;
                    let low = self.parse_hex4()?;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return Err(self.err("invalid JSON unicode surrogate pair"));
                    }
                    let scalar = 0x10000 + (((code - 0xd800) << 10) | (low - 0xdc00));
                    out.push(
                        char::from_u32(scalar)
                            .ok_or_else(|| self.err("invalid JSON unicode escape"))?,
                    );
                } else if (0xdc00..=0xdfff).contains(&code) {
                    return Err(self.err("unpaired JSON unicode surrogate"));
                } else {
                    out.push(
                        char::from_u32(code)
                            .ok_or_else(|| self.err("invalid JSON unicode escape"))?,
                    );
                }
            }
            Some(_) => return Err(self.err("invalid JSON string escape")),
            None => return Err(self.err("unterminated JSON string escape")),
        }
        Ok(())
    }

    fn parse_hex4(&mut self) -> Result<u32, AstJsonError> {
        let mut value = 0;
        for _ in 0..4 {
            let Some(byte) = self.next() else {
                return Err(self.err("unterminated JSON unicode escape"));
            };
            value = (value << 4)
                | match byte {
                    b'0'..=b'9' => (byte - b'0') as u32,
                    b'a'..=b'f' => (byte - b'a' + 10) as u32,
                    b'A'..=b'F' => (byte - b'A' + 10) as u32,
                    _ => return Err(self.err("invalid JSON unicode escape")),
                };
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<Json, AstJsonError> {
        let start = self.pos;
        self.consume(b'-');
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
            }
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(self.err("invalid JSON number")),
        }
        let mut is_float = false;
        if self.consume(b'.') {
            is_float = true;
            let digit_start = self.pos;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
            if self.pos == digit_start {
                return Err(self.err("invalid JSON number"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let digit_start = self.pos;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
            if self.pos == digit_start {
                return Err(self.err("invalid JSON number"));
            }
        }
        let raw = &self.input[start..self.pos];
        if is_float {
            raw.parse::<f64>()
                .map(Json::Float)
                .map_err(|_| self.err("JSON number is out of range"))
        } else {
            raw.parse::<i64>()
                .map(Json::Number)
                .map_err(|_| self.err("JSON number is out of range"))
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), AstJsonError> {
        if self.consume(byte) {
            Ok(())
        } else {
            Err(self.err(format!("expected '{}'", byte as char)))
        }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }

    fn err(&self, message: impl Into<String>) -> AstJsonError {
        AstJsonError::new(format!("{} at byte {}", message.into(), self.pos))
    }
}
