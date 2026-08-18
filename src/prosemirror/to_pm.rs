use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::ast_json::{value_to_json, Json};

use super::{schema_map, ProseMirrorDoc, SchemaMap};

type Object = BTreeMap<String, Json>;

pub fn to_prosemirror(doc: &Document) -> ProseMirrorDoc {
    let mut renderer = Renderer {
        map: schema_map(),
        dropped: BTreeMap::new(),
        degraded: BTreeMap::new(),
        redundant_heading_ids: crate::render_carve::redundant_heading_ids(doc),
    };
    let Some(document_name) = renderer.name("document") else {
        return ProseMirrorDoc {
            json: value_to_json(&Json::Object(Object::new())),
            dropped: renderer.dropped,
            degraded: renderer.degraded,
        };
    };
    let mut root = object(document_name);
    let mut content = Vec::new();
    if let Some(frontmatter) = &doc.frontmatter_raw {
        let mut attrs = Object::new();
        attrs.insert("content".into(), Json::String(frontmatter.content.clone()));
        attrs.insert("format".into(), Json::String(frontmatter.format.clone()));
        if let Some(name) = renderer.name("frontmatter") {
            content.push(node_with(name, attrs, Vec::new()));
        }
    }
    content.extend(renderer.blocks(&doc.children));
    for (label, blocks) in &doc.footnote_defs {
        let mut attrs = Object::new();
        attrs.insert("label".into(), Json::String(label.clone()));
        if let Some(name) = renderer.name("footnote") {
            let children = renderer.blocks(blocks);
            content.push(node_with(name, attrs, children));
        }
    }
    root.insert("content".into(), Json::Array(content));
    ProseMirrorDoc {
        json: value_to_json(&Json::Object(root)),
        dropped: renderer.dropped,
        degraded: renderer.degraded,
    }
}

struct Renderer {
    map: &'static SchemaMap,
    dropped: BTreeMap<String, String>,
    degraded: BTreeMap<String, String>,
    redundant_heading_ids: BTreeSet<String>,
}

impl Renderer {
    fn blocks(&mut self, nodes: &[BlockNode]) -> Vec<Json> {
        nodes.iter().filter_map(|node| self.block(node)).collect()
    }

    fn block(&mut self, node: &BlockNode) -> Option<Json> {
        let (name, attrs, content) = match node {
            BlockNode::Heading(n) => (
                self.name("heading")?,
                structural_attrs(
                    n.attrs
                        .as_ref()
                        .map(|a| {
                            if a.id
                                .as_ref()
                                .is_some_and(|id| self.redundant_heading_ids.contains(id))
                                && !a.order.iter().any(|slot| matches!(slot, AttrSlot::Id))
                            {
                                let mut authored = a.clone();
                                authored.id = None;
                                authored
                            } else {
                                a.clone()
                            }
                        })
                        .as_ref(),
                    [("level", Json::Number(i64::from(n.level)))],
                ),
                self.inlines(&n.children, &[]),
            ),
            BlockNode::Paragraph(n) => (
                self.name("paragraph")?,
                attrs(n.attrs.as_ref()),
                self.inlines(&n.children, &[]),
            ),
            BlockNode::CodeBlock(n) => {
                let mut a = attrs(n.attrs.as_ref());
                if let Some(lang) = &n.lang {
                    if !lang.is_empty() {
                        a.insert("language".into(), Json::String(lang.clone()));
                    }
                }
                if let Some(title) = &n.title {
                    a.insert("carveFenceTitle".into(), Json::String(title.clone()));
                }
                if let Some(label) = &n.label {
                    if !label.is_empty() {
                        a.insert("carveFenceLabel".into(), Json::String(label.clone()));
                    }
                }
                (
                    self.name("code_block")?,
                    a,
                    self.text_content(&n.content, &[]),
                )
            }
            BlockNode::List(n) => {
                let task = n.items.iter().any(|item| item.checked.is_some());
                let name = if task {
                    self.nth_name("list", 2)?
                } else if n.ordered {
                    self.nth_name("list", 1)?
                } else {
                    self.nth_name("list", 0)?
                };
                let mut a = attrs(n.attrs.as_ref());
                if n.ordered {
                    a.insert("start".into(), Json::Number(n.start.unwrap_or(1) as i64));
                    if n.start.is_some() {
                        a.insert("carveListStartExplicit".into(), Json::Bool(true));
                    }
                }
                if n.bare_marker {
                    a.insert("carveBareMarker".into(), Json::Bool(true));
                }
                if let Some(style) = n.ol_type {
                    a.insert(
                        "carveListStyle".into(),
                        Json::String(ol_style(style).into()),
                    );
                }
                if let Some(marker) = n.delim.or(n.bullet_char) {
                    if marker == ')' || marker == '*' {
                        a.insert("carveListMarker".into(), Json::String(marker.to_string()));
                    }
                }
                a.insert("tight".into(), Json::Bool(n.tight));
                let items = n
                    .items
                    .iter()
                    .filter_map(|item| {
                        let item_name = if item.checked.is_some() {
                            self.nth_name("list_item", 1)?
                        } else {
                            self.nth_name("list_item", 0)?
                        };
                        let mut ia = attrs(item.attrs.as_ref());
                        if let Some(checked) = item.checked {
                            ia.insert("checked".into(), Json::Bool(checked));
                        }
                        Some(node_with(item_name, ia, self.blocks(&item.children)))
                    })
                    .collect();
                (name, a, items)
            }
            BlockNode::BlockQuote(n) => (
                self.name("block_quote")?,
                attrs(n.attrs.as_ref()),
                self.blocks(&n.children),
            ),
            BlockNode::Table(n) => return self.table(n),
            BlockNode::Admonition(n) => {
                if n.title
                    .as_ref()
                    .is_some_and(|title| title.iter().any(|v| !matches!(v, InlineNode::Text(_))))
                {
                    self.degraded.insert(
                        "admonition".into(),
                        "title markup is flattened into a ProseMirror attribute".into(),
                    );
                }
                let mut a = attrs(n.attrs.as_ref());
                let mut classes = a
                    .remove("class")
                    .and_then(|v| {
                        if let Json::String(s) = v {
                            Some(s)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                if !classes.is_empty() {
                    classes.push(' ');
                }
                classes.push_str(&n.kind);
                a.insert("class".into(), Json::String(classes));
                // The class alone cannot say this was an admonition rather
                // than a div that happens to carry the word: `kind` is free
                // text, so the way back would have to guess from a list of
                // known kinds, and any kind outside that list comes back as a
                // plain div. That is how `::: footnotes` lost its placement -
                // it parses as an admonition whose kind is `footnotes`, which
                // no such list contains. The kind rides explicitly, the way
                // this bridge already carries the other engine state an editor
                // model has no place for.
                a.insert("carveAdmonitionKind".into(), Json::String(n.kind.clone()));
                // NOT `title`: an admonition may also carry an authored
                // `title` attribute, and writing the opener's title into the
                // same key made the two indistinguishable - one corpus
                // document has both, and the attribute won. The opener title
                // is structure, so it rides under its own name like the kind.
                if let Some(title) = &n.title {
                    a.insert(
                        "carveAdmonitionTitle".into(),
                        Json::String(plain_text(title)),
                    );
                }
                if let Some(label) = &n.label {
                    a.insert("label".into(), Json::String(label.clone()));
                }
                (self.name("admonition")?, a, self.blocks(&n.children))
            }
            BlockNode::Div(n) => {
                let mut a = attrs(n.attrs.as_ref());
                // The label is the div's visible heading, and nothing else
                // carries it: the corpus round trip caught `::: note [First]`
                // coming back as a bare div with the word gone and nothing
                // reported. carve-grammars spells the attribute `label`, so
                // this does too rather than inventing a fourth spelling.
                if let Some(label) = n.label.as_ref().filter(|l| !l.is_empty()) {
                    a.insert("label".into(), Json::String(label.clone()));
                }
                let classes = n
                    .attrs
                    .as_ref()
                    .map(|a| a.classes.as_slice())
                    .unwrap_or_default();
                let name = if classes.iter().any(|c| c == "tabs") {
                    self.nth_name("div", 1)?
                } else if classes.iter().any(|c| c == "tab") {
                    self.nth_name("div", 2)?
                } else {
                    self.nth_name("div", 0)?
                };
                (name, a, self.blocks(&n.children))
            }
            BlockNode::LineBlock(n) => (
                self.name("line_block")?,
                attrs(n.attrs.as_ref()),
                self.blocks(&n.children),
            ),
            BlockNode::DefinitionList(n) => {
                let mut children = Vec::new();
                for item in &n.items {
                    for term in &item.terms {
                        children.push(node_with(
                            self.name("definition_term")?,
                            attrs(term.attrs.as_ref()),
                            self.inlines(&term.children, &[]),
                        ));
                    }
                    for def in &item.definitions {
                        children.push(node_with(
                            self.name("definition_description")?,
                            attrs(def.attrs.as_ref()),
                            self.blocks(&def.children),
                        ));
                    }
                }
                (
                    self.name("definition_list")?,
                    attrs(n.attrs.as_ref()),
                    children,
                )
            }
            BlockNode::Figure(n) => {
                let mut children = vec![
                    self.figure_target(&n.target)?,
                    node_with(
                        self.name("caption")?,
                        Object::new(),
                        self.inlines(&n.caption, &[]),
                    ),
                ];
                if let Some(short) = &n.short_caption {
                    children.push(node_with(
                        self.name("caption")?,
                        BTreeMap::from([("short".into(), Json::Bool(true))]),
                        self.inlines(short, &[]),
                    ));
                }
                (self.name("figure")?, attrs(n.attrs.as_ref()), children)
            }
            BlockNode::FigureGroup(n) => {
                // The vendored carve-grammars map has no name for
                // `figure_group`: the editor schema predates PART 9 §4c, and
                // adding one HERE would fork the map this bridge exists to
                // read rather than repeat. So the group degrades to the
                // generic container the map does have - the same `carveDiv`
                // an admonition rides on - keeping every panel and the group
                // caption, and losing only the fact that they were one figure.
                self.degrade("figure_group");
                let mut children = self.blocks(&n.children);
                if let Some(caption) = &n.caption {
                    children.push(node_with(
                        self.name("caption")?,
                        Object::new(),
                        self.inlines(caption, &[]),
                    ));
                }
                (self.name("div")?, attrs(n.attrs.as_ref()), children)
            }
            BlockNode::AbbreviationDef(_) => {
                self.drop_type("abbreviation_def", None);
                return None;
            }
            // Compile-required arm for the PART 12 section 18 node
            // (markup-carve/carve#1276). Dropped like the abbreviation
            // definition above: a ProseMirror schema has no node for a
            // definition line, and the pair carries it in the sidecar.
            BlockNode::CitationDefinition(_) => {
                self.drop_type("citation_definition", None);
                return None;
            }
            BlockNode::LinkReferenceDefinition(n) => {
                let mut a = attrs(n.attrs.as_ref());
                a.insert("label".into(), Json::String(n.label.clone()));
                a.insert("href".into(), Json::String(n.href.clone()));
                if let Some(t) = &n.title {
                    set_structural_title(&mut a, t);
                }
                (self.name("link_reference_definition")?, a, Vec::new())
            }
            BlockNode::RawBlock(n) => (
                self.name("raw_block")?,
                BTreeMap::from([("format".into(), Json::String(n.format.clone()))]),
                self.text_content(&n.content, &[]),
            ),
            BlockNode::Comment(n) => (
                self.nth_name("comment", 0)?,
                BTreeMap::from([
                    ("block".into(), Json::Bool(n.block)),
                    // §21a: the delimiters are the node's identity, not
                    // decoration. Drop the flag and a `{% ... %}` comment
                    // returns spelled `%%`, which runs to end of line and eats
                    // whatever followed it on that line.
                    ("delimited".into(), Json::Bool(n.delimited)),
                ]),
                self.text_content(&n.content, &[]),
            ),
            BlockNode::Extension(_) => {
                self.drop_type(
                    "block_extension",
                    Some("mapped bridge support is unimplemented"),
                );
                return None;
            }
            BlockNode::BlockImage(n) => (
                self.name("paragraph")?,
                Object::new(),
                self.image(n, &[]).into_iter().collect(),
            ),
            BlockNode::ThematicBreak(n) => {
                let mut a = attrs(n.attrs.as_ref());
                if let Some(m) = n.marker {
                    a.insert("carveMarker".into(), Json::String(m.to_string()));
                }
                (self.name("thematic_break")?, a, Vec::new())
            }
        };
        Some(node_with(name, attrs, content))
    }

    fn figure_target(&mut self, target: &FigureTarget) -> Option<Json> {
        match target {
            FigureTarget::Image(n) => Some(node_with(
                self.name("paragraph")?,
                Object::new(),
                vec![self.image(n, &[])?],
            )),
            FigureTarget::BlockQuote(n) => self.block(&BlockNode::BlockQuote(n.clone())),
            FigureTarget::Table(n) => self.table(n),
            FigureTarget::CodeBlock(n) => self.block(&BlockNode::CodeBlock(n.clone())),
            FigureTarget::Paragraph(n) => self.block(&BlockNode::Paragraph(n.clone())),
        }
    }

    fn table(&mut self, table: &Table) -> Option<Json> {
        let mut rows = Vec::new();
        for row in &table.rows {
            let cells = row
                .cells
                .iter()
                .filter_map(|cell| {
                    let name = if cell.header {
                        self.nth_name("table_cell", 1)?
                    } else {
                        self.nth_name("table_cell", 0)?
                    };
                    let mut a = attrs(cell.attrs.as_ref());
                    a.insert("colspan".into(), Json::Number(1));
                    a.insert("rowspan".into(), Json::Number(1));
                    if let Some(align) = cell.align {
                        a.insert(
                            "alignment".into(),
                            Json::String(
                                match align {
                                    TableAlign::Left => "left",
                                    TableAlign::Right => "right",
                                    TableAlign::Center => "center",
                                }
                                .into(),
                            ),
                        );
                    }
                    if let Some(span) = cell.span {
                        a.insert(
                            "carveSpanMarker".into(),
                            Json::String(
                                match span {
                                    TableCellSpan::Rowspan => "^",
                                    TableCellSpan::Colspan => "<",
                                }
                                .into(),
                            ),
                        );
                    }
                    Some(node_with(name, a, self.inlines(&cell.children, &[])))
                })
                .collect();
            rows.push(node_with(
                self.name("table_row")?,
                attrs(row.attrs.as_ref()),
                cells,
            ));
        }
        let mut content = Vec::new();
        if let Some(caption) = &table.caption {
            content.push(node_with(
                self.name("caption")?,
                Object::new(),
                self.inlines(caption, &[]),
            ));
        }
        content.extend(rows);
        Some(node_with(
            self.name("table")?,
            attrs(table.attrs.as_ref()),
            content,
        ))
    }

    fn inlines(&mut self, nodes: &[InlineNode], marks: &[Json]) -> Vec<Json> {
        let mut out = Vec::new();
        for node in nodes {
            self.inline(node, marks, &mut out);
        }
        out
    }

    fn inline(&mut self, node: &InlineNode, marks: &[Json], out: &mut Vec<Json>) {
        match node {
            InlineNode::Text(n) => self.push_text(out, &n.value, marks),
            InlineNode::EscapedText(n) => {
                self.degrade("escaped_text");
                self.push_text(out, &n.value, marks);
            }
            InlineNode::SmartPunctuation(n) => {
                self.degrade("smart_punctuation");
                self.push_text(out, smart_punctuation_glyph(n), marks);
            }
            InlineNode::SoftBreak(_) => {
                self.degrade("soft_break");
                self.push_text(out, " ", marks);
            }
            InlineNode::HardBreak(_) => {
                let Some(name) = self.name("hard_break") else {
                    return;
                };
                out.push(node_marked(name, Object::new(), Vec::new(), marks));
            }
            InlineNode::Emphasis(n) => {
                let mut next = marks.to_vec();
                if n.kind == EmphasisKind::BoldItalic {
                    let Some(strong) = self.name("strong") else {
                        return;
                    };
                    let Some(emphasis) = self.name("emphasis") else {
                        return;
                    };
                    next.push(mark(strong, attrs(n.attrs.as_ref())));
                    next.push(mark(emphasis, attrs(n.attrs.as_ref())));
                } else {
                    let Some(name) = self.emphasis_name(n.kind) else {
                        return;
                    };
                    next.push(mark(name, attrs(n.attrs.as_ref())));
                }
                for child in &n.children {
                    self.inline(child, &next, out);
                }
            }
            InlineNode::Code(n) => {
                let mut next = marks.to_vec();
                let Some(name) = self.name("code") else {
                    return;
                };
                next.push(mark(name, attrs(n.attrs.as_ref())));
                self.push_text(out, &n.value, &next);
            }
            InlineNode::Link(n) => {
                let before = out.len();
                let mut a = attrs(n.attrs.as_ref());
                a.insert("href".into(), Json::String(n.href.clone()));
                if let Some(t) = &n.title {
                    set_structural_title(&mut a, t);
                }
                if n.from_heading_reference {
                    a.insert("carveHeadingRef".into(), Json::Bool(true));
                }
                if let Some(r) = &n.ref_label {
                    a.insert("carveRef".into(), Json::String(r.clone()));
                }
                if let Some(r) = &n.raw_ref {
                    a.insert("carveRawRef".into(), Json::String(r.clone()));
                }
                let mut next = marks.to_vec();
                let Some(name) = self.name("link") else {
                    return;
                };
                let carried = a.clone();
                next.push(mark(name, a));
                for c in &n.children {
                    self.inline(c, &next, out);
                }
                if out.len() == before {
                    self.empty_mark("link", carried, marks, out);
                }
            }
            InlineNode::AutoLink(n) => {
                let mut a = attrs(n.attrs.as_ref());
                a.insert("href".into(), Json::String(n.href.clone()));
                a.insert("carveAutolink".into(), Json::Bool(true));
                let mut next = marks.to_vec();
                let Some(name) = self.name("autolink") else {
                    return;
                };
                next.push(mark(name, a));
                self.push_text(out, &n.text, &next);
            }
            InlineNode::Image(n) => {
                if let Some(image) = self.image(n, marks) {
                    out.push(image);
                }
            }
            InlineNode::Span(n) => {
                let before = out.len();
                let mut next = marks.to_vec();
                let Some(name) = self.name("span") else {
                    return;
                };
                next.push(mark(name, attrs(n.attrs.as_ref())));
                for c in &n.children {
                    self.inline(c, &next, out);
                }
                if out.len() == before {
                    self.empty_mark("span", attrs(n.attrs.as_ref()), marks, out);
                }
            }
            InlineNode::Math(n) => {
                let mut a = attrs(n.attrs.as_ref());
                a.insert("src".into(), Json::String(n.content.clone()));
                a.insert("display".into(), Json::Bool(n.display));
                let Some(name) = self.name("math") else {
                    return;
                };
                out.push(node_marked(name, a, Vec::new(), marks));
            }
            InlineNode::RawInline(n) => {
                let Some(name) = self.name("raw_inline") else {
                    return;
                };
                out.push(node_marked(
                    name,
                    BTreeMap::from([
                        ("format".into(), Json::String(n.format.clone())),
                        ("content".into(), Json::String(n.content.clone())),
                    ]),
                    Vec::new(),
                    marks,
                ));
            }
            InlineNode::LiteralInline(n) => {
                let mut a = attrs(n.attrs.as_ref());
                a.insert("content".into(), Json::String(n.content.clone()));
                let Some(name) = self.name("literal_inline") else {
                    return;
                };
                out.push(node_marked(name, a, Vec::new(), marks));
            }
            InlineNode::Symbol(n) => {
                let mut a = attrs(n.attrs.as_ref());
                a.insert("name".into(), Json::String(n.name.clone()));
                let Some(name) = self.name("symbol") else {
                    return;
                };
                out.push(node_marked(name, a, Vec::new(), marks));
            }
            InlineNode::CrossRef(n) => {
                let Some(name) = self.name("heading_ref") else {
                    return;
                };
                out.push(node_marked(
                    name,
                    BTreeMap::from([("target".into(), Json::String(n.target.clone()))]),
                    Vec::new(),
                    marks,
                ));
            }
            InlineNode::CaptionNumber(_) => self.drop_type("caption_number", None),
            InlineNode::Mention(n) => {
                let mut a = attrs(n.attrs.as_ref());
                a.insert("id".into(), Json::String(n.user.clone()));
                let Some(name) = self.nth_name("mention", 0) else {
                    return;
                };
                out.push(node_marked(name, a, Vec::new(), marks));
            }
            InlineNode::Tag(n) => {
                let mut a = attrs(n.attrs.as_ref());
                a.insert("id".into(), Json::String(n.name.clone())); /* `tag` is a PART 12 type classified under mention by the profile, so the shared map deliberately has no tag key. */
                let Some(name) = self.nth_name("mention", 1) else {
                    return;
                };
                out.push(node_marked(name, a, Vec::new(), marks));
            }
            InlineNode::CitationGroup(n) => {
                let mut a = Object::new();
                a.insert("raw".into(), Json::String(n.raw.clone()));
                a.insert("integral".into(), Json::Bool(n.integral));
                let items = n
                    .items
                    .iter()
                    .map(|item| {
                        let mut i = Object::new();
                        i.insert("key".into(), Json::String(item.key.clone()));
                        i.insert("suppressAuthor".into(), Json::Bool(item.suppress_author));
                        for (key, value) in [
                            ("prefix", &item.prefix),
                            ("locator", &item.locator),
                            ("suffix", &item.suffix),
                        ] {
                            if let Some(v) = value {
                                i.insert(key.into(), Json::Array(self.inlines(v, &[])));
                            }
                        }
                        if let Some(v) = &item.locator_label {
                            i.insert("locatorLabel".into(), Json::String(v.clone()));
                        }
                        if let Some(v) = &item.locator_value {
                            i.insert("locatorValue".into(), Json::String(v.clone()));
                        }
                        Json::Object(i)
                    })
                    .collect();
                a.insert("items".into(), Json::Array(items));
                let Some(name) = self.name("citation_group") else {
                    return;
                };
                out.push(node_marked(name, a, Vec::new(), marks));
            }
            InlineNode::Extension(n) => {
                let mut a = attrs(n.attrs.as_ref());
                a.insert("carveSource".into(), Json::String(format!(":{}", n.name)));
                let Some(name) = self.name("inline_extension") else {
                    return;
                };
                let content = self.inlines(&n.children, &[]);
                out.push(node_marked(name, a, content, marks));
            }
            InlineNode::Abbreviation(n) => {
                let mut next = marks.to_vec();
                let Some(name) = self.name("abbreviation") else {
                    return;
                };
                next.push(mark(
                    name,
                    BTreeMap::from([("title".into(), Json::String(n.expansion.clone()))]),
                ));
                self.push_text(out, &n.abbr, &next);
            }
            InlineNode::Footnote(n) => {
                let ty = if n.inline.is_some() {
                    "inline_footnote"
                } else {
                    "footnote_ref"
                };
                let mut a = attrs(n.attrs.as_ref());
                if let Some(id) = &n.id {
                    a.insert("label".into(), Json::String(id.clone()));
                }
                let content = n
                    .inline
                    .as_ref()
                    .map(|v| self.inlines(v, &[]))
                    .unwrap_or_default();
                let Some(name) = self.name(ty) else {
                    return;
                };
                out.push(node_marked(name, a, content, marks));
            }
            InlineNode::CriticInsert(n) => {
                self.mark_children("insert", &n.children, n.attrs.as_ref(), marks, out)
            }
            InlineNode::CriticDelete(n) => {
                self.mark_children("delete", &n.children, n.attrs.as_ref(), marks, out)
            }
            InlineNode::CriticSubstitute(n) => {
                let Some(name) = self.name("substitution") else {
                    return;
                };
                out.push(node_marked(
                    name,
                    BTreeMap::from([
                        ("oldText".into(), Json::String(n.old_text.clone())),
                        ("newText".into(), Json::String(n.new_text.clone())),
                    ]),
                    Vec::new(),
                    marks,
                ));
            }
            InlineNode::CriticComment(n) => {
                let mut next = marks.to_vec();
                let Some(name) = self.name("critic_comment") else {
                    return;
                };
                next.push(mark(name, Object::new()));
                self.push_text(out, &n.text, &next);
            }
            InlineNode::Comment(n) => {
                let Some(name) = self.nth_name("comment", 1) else {
                    return;
                };
                out.push(node_marked(
                    name,
                    BTreeMap::from([
                        ("content".into(), Json::String(n.content.clone())),
                        ("delimited".into(), Json::Bool(n.delimited)),
                    ]),
                    Vec::new(),
                    marks,
                ));
            }
        }
    }

    fn mark_children(
        &mut self,
        ty: &str,
        children: &[InlineNode],
        a: Option<&Attrs>,
        marks: &[Json],
        out: &mut Vec<Json>,
    ) {
        let before = out.len();
        let mut next = marks.to_vec();
        let Some(name) = self.name(ty) else {
            return;
        };
        next.push(mark(name, attrs(a)));
        for c in children {
            self.inline(c, &next, out);
        }
        if out.len() == before {
            self.empty_mark(ty, attrs(a), marks, out);
        }
    }

    /// A mark with no content, as the atom `markCarrierNodes` declares for it.
    ///
    /// A ProseMirror mark cannot span zero characters, so walking the children
    /// of `[](/u)`, `[]{.a}`, `{++}` or `{--}` produces nothing at all and the
    /// construct leaves the document. Two of these at least reported
    /// themselves dropped; the critic pair reported nothing, so `{++}` was
    /// deleted from the source in silence.
    fn empty_mark(&mut self, ty: &str, mark_attrs: Object, marks: &[Json], out: &mut Vec<Json>) {
        let Some(mark_type) = self.name(ty) else {
            return;
        };
        let Some(carrier) = self.map.mark_carrier.as_deref() else {
            self.drop_type(ty, Some("the vendored map declares no empty-mark carrier"));
            return;
        };
        let mut a = Object::new();
        a.insert("markType".into(), Json::String(mark_type.into()));
        if !mark_attrs.is_empty() {
            a.insert("markAttrs".into(), Json::Object(mark_attrs));
        }
        out.push(node_marked(carrier, a, Vec::new(), marks));
    }
    fn image(&mut self, n: &Image, marks: &[Json]) -> Option<Json> {
        let mut a = attrs(n.attrs.as_ref());
        a.insert("src".into(), Json::String(n.src.clone()));
        a.insert("alt".into(), Json::String(n.alt.clone()));
        if let Some(t) = &n.title {
            set_structural_title(&mut a, t);
        }
        if let Some(r) = &n.ref_label {
            a.insert("carveRef".into(), Json::String(r.clone()));
        }
        if let Some(r) = &n.raw_ref {
            a.insert("carveRawRef".into(), Json::String(r.clone()));
        }
        Some(node_marked(self.name("image")?, a, Vec::new(), marks))
    }
    fn name(&mut self, ty: &str) -> Option<&'static str> {
        self.nth_name(ty, 0)
    }
    fn nth_name(&mut self, ty: &str, n: usize) -> Option<&'static str> {
        let name = self
            .map
            .names
            .get(ty)
            .and_then(|v| v.get(n))
            .map(String::as_str);
        if name.is_none() {
            self.drop_type(ty, Some("the vendored map has no name for this type"));
        }
        name
    }

    fn push_text(&mut self, out: &mut Vec<Json>, text: &str, marks: &[Json]) {
        if text.is_empty() {
            return;
        }
        let Some(name) = self.name("text") else {
            return;
        };
        let mut o = object(name);
        o.insert("text".into(), Json::String(text.into()));
        if !marks.is_empty() {
            o.insert("marks".into(), Json::Array(marks.to_vec()));
        }
        out.push(Json::Object(o));
    }

    fn text_content(&mut self, text: &str, marks: &[Json]) -> Vec<Json> {
        let mut out = Vec::new();
        self.push_text(&mut out, text, marks);
        out
    }

    fn emphasis_name(&mut self, kind: EmphasisKind) -> Option<&'static str> {
        match kind {
            EmphasisKind::Italic => self.name("emphasis"),
            EmphasisKind::Strong | EmphasisKind::BoldItalic => self.name("strong"),
            EmphasisKind::Underline => self.name("underline"),
            EmphasisKind::Strike => self.name("strike"),
            EmphasisKind::Super => self.name("superscript"),
            EmphasisKind::Sub => self.name("subscript"),
            EmphasisKind::Highlight => self.name("highlight"),
        }
    }
    fn degrade(&mut self, ty: &str) {
        let reason = self
            .map
            .unmapped
            .get(ty)
            .cloned()
            .unwrap_or_else(|| "degraded to text".into());
        self.degraded.insert(ty.into(), reason);
    }
    fn drop_type(&mut self, ty: &str, reason: Option<&str>) {
        let reason = reason
            .map(str::to_owned)
            .or_else(|| self.map.unmapped.get(ty).cloned())
            .unwrap_or_else(|| "mapped bridge support is unimplemented".into());
        self.dropped.insert(ty.into(), reason);
    }
}

fn object(name: &str) -> Object {
    BTreeMap::from([("type".into(), Json::String(name.into()))])
}
fn node_with(name: &str, attrs: Object, content: Vec<Json>) -> Json {
    let mut o = object(name);
    if !attrs.is_empty() {
        o.insert("attrs".into(), Json::Object(attrs));
    }
    if !content.is_empty() {
        o.insert("content".into(), Json::Array(content));
    }
    Json::Object(o)
}
fn node_marked(name: &str, attrs: Object, content: Vec<Json>, marks: &[Json]) -> Json {
    let Json::Object(mut o) = node_with(name, attrs, content) else {
        unreachable!()
    };
    if !marks.is_empty() {
        o.insert("marks".into(), Json::Array(marks.to_vec()));
    }
    Json::Object(o)
}
fn mark(name: &str, attrs: Object) -> Json {
    node_with(name, attrs, Vec::new())
}
fn attrs(a: Option<&Attrs>) -> Object {
    let mut out = Object::new();
    if let Some(a) = a {
        // A RECORDED run that does not name `#id` proves the id was not
        // authored: the only ids Carve synthesizes are heading ids, and the
        // parser writes `#id` into `order` for every id somebody typed. The
        // map's `id` is the authored id, so a generated one does not go in it -
        // it is a resolution artifact, regenerated when the document renders.
        //
        // Only where the run was recorded, though. A heading with no attribute
        // line at all carries its generated id and an EMPTY order, which says
        // nothing either way, and reading it as proof would drop an authored id
        // out of any AST that reached this bridge without an order.
        let generated_id =
            !a.order.is_empty() && !a.order.iter().any(|s| matches!(s, AttrSlot::Id));
        if let Some(id) = a.id.as_ref().filter(|_| !generated_id) {
            out.insert("id".into(), Json::String(id.clone()));
        }
        if !a.classes.is_empty() {
            out.insert("class".into(), Json::String(a.classes.join(" ")));
        }
        for (k, v) in &a.key_values {
            out.entry(k.clone())
                .or_insert_with(|| Json::String(v.clone()));
        }
        if !a.order.is_empty() {
            out.insert(
                "carveAttrOrder".into(),
                Json::Array(
                    a.order
                        .iter()
                        .map(|slot| {
                            Json::String(match slot {
                                AttrSlot::Id => "#id".into(),
                                AttrSlot::Class => ".class".into(),
                                AttrSlot::Key(k) => k.clone(),
                            })
                        })
                        .collect(),
                ),
            );
        }
    }
    out
}
/// Put a STRUCTURAL title into the wire's `title`.
///
/// One wire field carries two different Carve slots: a link, an image and a
/// reference definition each have a structural title - the quoted string in
/// `(url "T")` - and each can also carry an AUTHORED `{title=T}` attribute,
/// which arrives here through the generic attribute run. `carveAttrOrder` is
/// what records which one somebody typed, so where the structural slot wins the
/// field the run must stop naming `title`: a run that still names it describes a
/// value no longer on the wire, and the importer reads the structural title back
/// as an authored attribute (carve-rs#1105).
fn set_structural_title(a: &mut Object, title: &str) {
    a.insert("title".into(), Json::String(title.to_owned()));
    let Some(Json::Array(order)) = a.get_mut("carveAttrOrder") else {
        return;
    };
    order.retain(|slot| !matches!(slot, Json::String(s) if s == "title"));
    if order.is_empty() {
        a.remove("carveAttrOrder");
    }
}

fn structural_attrs<const N: usize>(a: Option<&Attrs>, values: [(&str, Json); N]) -> Object {
    let mut out = Object::new();
    for (k, v) in values {
        out.insert(k.into(), v);
    }
    for (k, v) in attrs(a) {
        out.entry(k).or_insert(v);
    }
    out
}
fn ol_style(t: OrderedListType) -> &'static str {
    match t {
        OrderedListType::LowerAlpha => "a",
        OrderedListType::UpperAlpha => "A",
        OrderedListType::LowerRoman => "i",
        OrderedListType::UpperRoman => "I",
    }
}
fn plain_text(nodes: &[InlineNode]) -> String {
    nodes
        .iter()
        .map(|n| match n {
            InlineNode::Text(t) => t.value.clone(),
            InlineNode::EscapedText(t) => t.value.clone(),
            InlineNode::SmartPunctuation(t) => smart_punctuation_glyph(t).into(),
            _ => String::new(),
        })
        .collect()
}
