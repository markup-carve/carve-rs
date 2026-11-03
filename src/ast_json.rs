//! JSON encoding/decoding for the public Carve AST exchange shape.
//!
//! Reading and writing generic JSON is serde_json's job. What is local is the
//! mapping between that and the AST, and the depth budget the reader enforces
//! before serde recurses.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Map;

use crate::ast::*;

mod error;
pub use error::{AstJsonError, AstJsonErrorKind};

pub(crate) type Json = serde_json::Value;

/// Refuse a document nested past the budget WITHOUT recursing, so the check
/// cannot itself overflow the stack on the input it exists to reject. Only
/// after this passes is serde's own recursion limit lifted.
fn refuse_deeper_than_budget(input: &str) -> Result<(), AstJsonError> {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for &byte in input.as_bytes() {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                if depth > MAX_JSON_DEPTH {
                    return Err(AstJsonError::depth(
                        "JSON nests deeper than the reader's depth budget",
                    ));
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

/// PART 12 section 21: a NUL arriving as `\u0000` is replaced rather than
/// carried into the tree. A RAW NUL byte stays a syntax error, which is
/// serde_json's answer for any C0 control inside a string.
///
/// This used to live in the hand-rolled string scanner, where it read as part
/// of the JSON grammar. It is not: it is a Carve ingest rule, so it runs as its
/// own pass. Iterative, because the tree it walks may be nested to the reader's
/// full depth budget.
fn replace_nul(root: &mut Json) {
    let mut stack = vec![root];
    while let Some(value) = stack.pop() {
        match value {
            Json::String(text) if text.contains('\0') => *text = text.replace('\0', "\u{fffd}"),
            Json::Array(values) => stack.extend(values.iter_mut()),
            Json::Object(entries) => {
                if entries.keys().any(|key| key.contains('\0')) {
                    *entries = std::mem::take(entries)
                        .into_iter()
                        .map(|(key, value)| (key.replace('\0', "\u{fffd}"), value))
                        .collect();
                }
                stack.extend(entries.values_mut());
            }
            _ => {}
        }
    }
}

pub(crate) fn parse_value(input: &str) -> Result<Json, AstJsonError> {
    refuse_deeper_than_budget(input)?;
    let mut reader = serde_json::Deserializer::from_str(input);
    reader.disable_recursion_limit();
    let mut value = Json::deserialize(&mut reader).map_err(AstJsonError::from_serde)?;
    reader.end().map_err(AstJsonError::from_serde)?;
    // A raw NUL is a syntax error, so the escape is the only way one reaches a
    // decoded string. Documents without it skip the walk over every node.
    if input.contains("\\u0000") {
        replace_nul(&mut value);
    }
    Ok(value)
}

pub(crate) fn value_to_json(value: &Json) -> String {
    serde_json::to_string(value).expect("a serde_json Value always serializes")
}
thread_local! {
    static ENCODE_DEPTH: Cell<usize> = const { Cell::new(0) };
    static ENCODE_REFUSED: Cell<bool> = const { Cell::new(false) };
}

/// Serialize an AST, refusing a programmatically constructed tree beyond the
/// same depth budget used by the JSON reader.
pub fn try_to_json(doc: &Document) -> Result<String, AstJsonError> {
    validate_interchange_shape(doc)?;
    ENCODE_DEPTH.with(|depth| depth.set(0));
    ENCODE_REFUSED.with(|refused| refused.set(false));
    let mut out = String::new();
    write_document(&mut out, doc);
    if ENCODE_REFUSED.with(Cell::get) {
        Err(AstJsonError::depth(
            "JSON nests deeper than the encoder's depth budget",
        ))
    } else {
        Ok(out)
    }
}

pub(crate) fn validate_interchange_shape(doc: &Document) -> Result<(), AstJsonError> {
    let mut pending: Vec<&BlockNode> = doc.children.iter().collect();
    for body in doc.footnote_defs.values() {
        pending.extend(body);
    }
    while let Some(block) = pending.pop() {
        match block {
            BlockNode::Section(section) => {
                if section.level.is_some_and(|level| !(1..=6).contains(&level)) {
                    return Err(AstJsonError::new("section.level must be between 1 and 6"));
                }
                pending.extend(&section.children);
            }
            BlockNode::Table(table) => validate_table_cells(table, &mut pending)?,
            BlockNode::Figure(figure) => match figure.target.as_ref() {
                FigureTarget::Table(table) => validate_table_cells(table, &mut pending)?,
                FigureTarget::BlockQuote(quote) => pending.extend(&quote.children),
                FigureTarget::Paragraph(_)
                | FigureTarget::CodeBlock(_)
                | FigureTarget::Image(_) => {}
            },
            BlockNode::List(list) => {
                for item in &list.items {
                    pending.extend(&item.children);
                }
            }
            BlockNode::DefinitionList(list) => {
                for item in &list.items {
                    for definition in &item.definitions {
                        pending.extend(&definition.children);
                    }
                }
            }
            BlockNode::BlockQuote(node) => pending.extend(&node.children),
            BlockNode::Admonition(node) => pending.extend(&node.children),
            BlockNode::Directive(node) => pending.extend(&node.children),
            BlockNode::Div(node) => pending.extend(&node.children),
            BlockNode::LineBlock(node) => pending.extend(&node.children),
            BlockNode::FigureGroup(node) => pending.extend(&node.children),
            BlockNode::BlockExtension(node) => pending.push(&node.fallback),
            BlockNode::ExtensionCarrier(node) => pending.extend(&node.children),
            _ => {}
        }
    }
    Ok(())
}

fn validate_table_cells<'a>(
    table: &'a Table,
    pending: &mut Vec<&'a BlockNode>,
) -> Result<(), AstJsonError> {
    for row in &table.rows {
        for cell in &row.cells {
            if let Some(blocks) = &cell.blocks {
                if !cell.children.is_empty() {
                    return Err(AstJsonError::new(
                        "table_cell cannot carry both children and blocks",
                    ));
                }
                pending.extend(blocks);
            }
        }
    }
    Ok(())
}

/// Serialize an AST. Prefer [`try_to_json`] for trees not produced by the
/// parser; this compatibility entry point panics on an over-depth API tree.
pub fn to_json(doc: &Document) -> String {
    try_to_json(doc).expect("AST JSON encoding failed")
}

pub(crate) fn source_layout_positions(doc: &Document) -> Vec<(String, usize, usize)> {
    fn walk(value: &Json, path: &str, out: &mut Vec<(String, usize, usize)>) {
        match value {
            Json::Object(object) => {
                if let Some(Json::Object(pos)) = object.get("pos") {
                    if let (Some(start), Some(end)) = (
                        pos.get("startOffset").and_then(Json::as_u64),
                        pos.get("endOffset").and_then(Json::as_u64),
                    ) {
                        out.push((path.to_owned(), start as usize, end as usize));
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
    let json = parse_value(&to_json(doc)).expect("encoder always writes valid JSON");
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
fn is_legacy_definition_entry(obj: &Map<String, Json>) -> bool {
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
fn field_path(parent: &str, field: &str) -> String {
    if parent.is_empty() {
        field.to_owned()
    } else {
        format!("{parent}.{field}")
    }
}

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
                        return Err(AstJsonError::unknown_field(format!(
                            "the legacy definition entry at {path} carries {key:?}, which the schema does not name (PART 12 §11)"
                        ), field_path(path, key)));
                    }
                }
            }
            if let Some(Json::String(ty)) = obj.get("type") {
                if let Some(known) = named_fields(crate::wire_fields::WIRE_FIELDS, ty) {
                    for key in obj.keys() {
                        if !known.contains(&key.as_str()) {
                            return Err(AstJsonError::unknown_field(format!(
                                "{ty} at {path} carries {key:?}, which the schema does not name (PART 12 §11)"
                            ), field_path(path, key)));
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
                                return Err(AstJsonError::unknown_field(format!(
                                    "{helper} at {path}.{helper} carries {key:?}, which the schema does not name (PART 12 §11)"
                                ), field_path(&field_path(path, helper), key)));
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
                                    return Err(AstJsonError::unknown_field(format!(
                                        "{position} at {path}.{property}[{index}] carries {key:?}, which the schema does not name (PART 12 §11)"
                                    ), field_path(&format!("{}[{index}]", field_path(path, property)), key)));
                                }
                            }
                        }
                    }
                }
            }
            // A block extension's payload is the extension's OWN data, opaque to
            // this schema and to every core renderer. The helper loop above
            // already closed the payload object itself at `format` / `value`;
            // descending into `value` applied the node rules to something that
            // is not a node, so an ordinary diagram spec carrying a `type` of
            // its own - `{"type":"swimlane","lanes":3}` - was refused as a node
            // with unnamed fields. carve-php hit the same trap in its #2324.
            let opaque_payload =
                matches!(obj.get("type"), Some(Json::String(ty)) if ty == "block_extension");
            for (key, value) in obj {
                if opaque_payload && key == "payload" {
                    continue;
                }
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

/// A short, non-leaking rendering of a value, for a section 12(d) error.
///
/// Primitives are shown as the JSON they are, so a caller reading the message
/// can tell `true` from `"true"` - which is the whole distinction a `const`
/// turns on. A container is named rather than dumped: the offending value is a
/// scalar in every case the schema pins, and a tree in the message would bury
/// it.
fn describe_json(value: &Json) -> String {
    match value {
        Json::Null => "null".to_string(),
        Json::Bool(flag) => flag.to_string(),
        Json::Number(number) => number.to_string(),
        Json::String(text) => format!("{text:?}"),
        Json::Array(_) => "an array".to_string(),
        Json::Object(_) => "an object".to_string(),
    }
}

/// PART 12 section 12(d), for a property the schema pins with `const`.
fn refuse_const_violations(node: &Json, path: &str) -> Result<(), AstJsonError> {
    match node {
        Json::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                refuse_const_violations(item, &format!("{path}[{index}]"))?;
            }
        }
        Json::Object(obj) => {
            if matches!(obj.get("type"), Some(Json::String(ty)) if ty == "comment") {
                if let Some(form) = obj.get("form") {
                    let valid = match form.as_str() {
                        Some("block") => {
                            obj.get("block") == Some(&Json::Bool(true))
                                && matches!(obj.get("delimited"), None | Some(Json::Bool(false)))
                        }
                        Some("line") => {
                            obj.get("block") == Some(&Json::Bool(false))
                                && matches!(obj.get("delimited"), None | Some(Json::Bool(false)))
                        }
                        Some("braced") => {
                            obj.get("block") == Some(&Json::Bool(false))
                                && obj.get("delimited") == Some(&Json::Bool(true))
                        }
                        _ => false,
                    };
                    if !valid {
                        return Err(AstJsonError::invalid_at(
                            format!(
                                "comment at {path} has form that disagrees with block or delimited"
                            ),
                            field_path(path, "form"),
                        ));
                    }
                }
            }
            if let Some(Json::String(ty)) = obj.get("type") {
                if let Some((_, pinned)) = crate::wire_fields::WIRE_CONST_FIELDS
                    .iter()
                    .find(|(name, _)| *name == ty.as_str())
                {
                    for (field, expected) in *pinned {
                        let Some(found) = obj.get(*field) else {
                            // ABSENT IS LEGAL, and that is the point of a
                            // `const`: the field is optional with one admitted
                            // value, so requiring it would refuse nearly every
                            // tree this engine's own encoder writes - which
                            // section 9(a) forbids.
                            continue;
                        };
                        let wanted = parse_value(expected)?;
                        if *found != wanted {
                            return Err(AstJsonError::new(format!(
                                "{ty} at {path} gives {field:?} {}, where the schema requires {} (PART 12 §12(d))",
                                describe_json(found),
                                describe_json(&wanted),
                            )));
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
                refuse_const_violations(value, &child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Decode and validate a document in the public AST exchange format.
pub fn from_json(input: &str) -> Result<Document, AstJsonError> {
    let json = parse_value(input)?;
    let root = json.expect_object("document root")?;
    if let Some(version) = root.get("astVersion") {
        let version = version.expect_string("document.astVersion")?;
        crate::ast_envelope::check_contract_version(version).map_err(|error| match error {
            crate::ast_envelope::AstEnvelopeError::Version { .. } => {
                AstJsonError::version(format!(
                    "AST root announces contract version {version}; this build implements {}",
                    crate::ast_envelope::AST_CONTRACT_VERSION
                ))
            }
            _ => AstJsonError::new(error.to_string()),
        })?;
    }
    refuse_unknown_fields(&json, "")?;
    refuse_const_violations(&json, "")?;
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
        let obj = child.expect_object("document.children[]")?;
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
    promote_ingested_block_images(&mut doc);

    Ok(doc)
}

/// Mark empty references resolved when an importer supplies definitions.
pub(crate) fn hydrate_resolved_empty_references(doc: &mut Document) {
    use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};
    use std::collections::HashSet;

    let mut definitions = HashSet::new();
    for block in &doc.children {
        if let BlockNode::LinkReferenceDefinition(def) = block {
            if def.href.is_empty()
                && crate::parse::reference_label_has_definition_spelling(&def.label)
            {
                definitions.insert(crate::parse::label_key(&def.label));
            }
        }
    }
    if definitions.is_empty() {
        return;
    }
    struct Hydrate<'a>(&'a HashSet<String>);
    impl SubtreeVisitor for Hydrate<'_> {
        fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
            for block in blocks {
                visit_block_children(block, self);
            }
        }
        fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
            for node in nodes {
                if let InlineNode::Link(link) = node {
                    link.resolved_empty_reference = link.href.is_empty()
                        && link
                            .ref_label
                            .as_ref()
                            .is_some_and(|label| self.0.contains(&crate::parse::label_key(label)));
                }
                visit_inline_children(node, self);
            }
        }
        fn image(&mut self, image: &mut Image) {
            image.resolved_empty_reference = image.src.is_empty()
                && image
                    .ref_label
                    .as_ref()
                    .is_some_and(|label| self.0.contains(&crate::parse::label_key(label)));
        }
    }
    let mut hydrate = Hydrate(&definitions);
    hydrate.blocks(&mut doc.children);
    for blocks in doc.footnote_defs.values_mut() {
        hydrate.blocks(blocks);
    }
}

/// Preserve a published result; compute it only for legacy payloads that omit it.
/// `refuse_const_violations` rejects `false`, so decoded `false` means absence.
fn promote_ingested_block_images(doc: &mut Document) {
    let mut worklist: Vec<&mut [BlockNode]> = vec![doc.children.as_mut_slice()];
    for blocks in doc.footnote_defs.values_mut() {
        worklist.push(blocks.as_mut_slice());
    }
    while let Some(level) = worklist.pop() {
        for block in level {
            if let BlockNode::Paragraph(p) = block {
                if !p.block_image {
                    p.block_image = crate::parse::lone_rendered_image(&p.children).is_some();
                }
            }
            match block {
                BlockNode::BlockQuote(b) => worklist.push(b.children.as_mut_slice()),
                BlockNode::Admonition(a) => worklist.push(a.children.as_mut_slice()),
                BlockNode::FigureGroup(g) => worklist.push(g.children.as_mut_slice()),
                BlockNode::Directive(d) => worklist.push(d.children.as_mut_slice()),
                BlockNode::Div(d) => worklist.push(d.children.as_mut_slice()),
                BlockNode::Section(d) => worklist.push(d.children.as_mut_slice()),
                BlockNode::List(l) => {
                    for item in &mut l.items {
                        worklist.push(item.children.as_mut_slice());
                    }
                }
                BlockNode::DefinitionList(d) => {
                    for item in &mut d.items {
                        for def in &mut item.definitions {
                            worklist.push(def.as_mut_slice());
                        }
                    }
                }
                _ => {}
            }
        }
    }
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
fn clear_unbacked_footnote_numbers(doc: &mut Document) {
    let _ = crate::render::collect_footnotes(doc, false);
}

mod encode;
use encode::*;
pub(crate) use encode::{
    block_pos, emphasis_type, footnote_defs_in_source_order, ordered_document_entries, DocEntry,
};
mod decode;
use decode::*;

/// Deepest JSON nesting the reader will follow.
const LONGEST_FIELD_CHAIN: usize = 6;
const MAX_JSON_DEPTH: usize = crate::parse::MAX_NESTING_DEPTH * LONGEST_FIELD_CHAIN + 16;

#[cfg(test)]
mod depth_budget_tests {
    use super::*;

    #[test]
    fn reader_refuses_json_beyond_its_budget_before_decoding() {
        let at_limit = format!(
            "{{\"nested\":{}0{}}}",
            "[".repeat(MAX_JSON_DEPTH - 1),
            "]".repeat(MAX_JSON_DEPTH - 1)
        );
        assert!(refuse_deeper_than_budget(&at_limit).is_ok());

        let source = format!(
            "{{\"type\":\"document\",\"srcByteLength\":0,\"children\":[],\"nested\":{}0{}}}",
            "[".repeat(MAX_JSON_DEPTH),
            "]".repeat(MAX_JSON_DEPTH)
        );
        let message = "JSON nests deeper than the reader's depth budget";
        assert_eq!(
            refuse_deeper_than_budget(&source).unwrap_err().to_string(),
            message
        );
        assert_eq!(from_json(&source).unwrap_err().to_string(), message);
        assert_eq!(
            crate::from_prosemirror(&source).unwrap_err().to_string(),
            message
        );
        assert_eq!(
            crate::ast_patch_from_json(&source).unwrap_err().to_string(),
            message
        );
    }
}
