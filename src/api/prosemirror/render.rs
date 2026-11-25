//! A tree to ProseMirror JSON. A node the editor content model cannot hold
//! becomes an atom that carries it whole, so the way back gives the same
//! tree for any document.

use std::sync::Arc;

use serde_json::{Map, Value};

use super::kit::{self, array, carve_attr, carve_of, clean, fields, str_of, type_of, typed};
use super::{Context, Mapping, MappingEntry, Position, ProseMirrorError, ProseMirrorOptions};
use crate::api::Tree;
use crate::options::UrlPolicy;

type Node = Map<String, Value>;
type Result<T> = std::result::Result<T, ProseMirrorError>;

/// The wire form of `tree`, resolved anew as `render` resolves its input.
pub(super) fn resolved_value(tree: &Tree) -> Result<Value> {
    let arena = crate::Arena::new();
    let mut doc = tree.document().in_arena(&arena);
    crate::resolve_tree(&mut doc, &crate::Options::default(), &arena);
    let json = crate::owned_json(&doc.owned());
    crate::api::deep_json(&json).map_err(|e| ProseMirrorError::Invalid(format!("tree JSON: {e}")))
}

pub(super) struct Renderer {
    id: i64,
    policy: UrlPolicy,
    blanked: Vec<String>,
    mapping: Arc<Mapping>,
}

impl Renderer {
    pub(super) fn new(options: &ProseMirrorOptions) -> Self {
        Self {
            id: 0,
            policy: options.url_policy.clone(),
            blanked: Vec::new(),
            mapping: Arc::new(options.mapping.clone()),
        }
    }

    pub(super) fn render(mut self, tree: &Value) -> Result<(Value, Vec<String>)> {
        let Some(tree) = tree.as_object().filter(|t| type_of(t) == "document") else {
            return Err(ProseMirrorError::Invalid(
                "ProseMirrorRenderer expects a Carve document tree".into(),
            ));
        };
        let children = list(tree.get("children"));
        let mut content = self.blocks(&children)?;
        if !children.is_empty() {
            for (index, node) in content.iter_mut().enumerate() {
                if let Some(host) = self.mapping.node(&type_of(node))
                    && !host.carve()
                {
                    continue;
                }
                let mut carve = carve_of(node);
                carve.insert("$index".into(), Value::from(index));
                let mut attrs = kit::attrs_of(node);
                attrs.insert("carve".into(), Value::Object(carve));
                node.insert("attrs".into(), Value::Object(attrs));
            }
        }
        let mut doc = typed("doc");
        doc.insert(
            "attrs".into(),
            attrs1("carve", carve_attr(fields(tree, &["children"]))),
        );
        doc.insert("content".into(), array(content));
        Ok((Value::Object(doc), self.blanked))
    }

    fn denied(&mut self, url: Option<&Value>) -> bool {
        let Some(Value::String(url)) = url else {
            return false;
        };
        if crate::html::url::allowed(url, &self.policy) {
            return false;
        }
        self.blanked.push(url.clone());
        true
    }

    /// Blocks; an empty list becomes one empty paragraph, which the way back
    /// drops.
    fn blocks(&mut self, children: &[Node]) -> Result<Vec<Node>> {
        let mut out = Vec::with_capacity(children.len());
        for child in children {
            out.push(self.block(child)?);
        }
        if out.is_empty() {
            out.push(typed("paragraph"));
        }
        Ok(out)
    }

    fn block(&mut self, node: &Node) -> Result<Node> {
        let ty = type_of(node);
        if node.contains_key("caption") && ty != "caption" {
            return self.captioned(node);
        }
        if let Some(mapped) = self.load_mapped(node, Position::Block)? {
            return Ok(mapped);
        }
        self.kit_block(node, &ty)
    }

    fn kit_block(&mut self, node: &Node, ty: &str) -> Result<Node> {
        if kit::by_tree(kit::TEXTBLOCKS, ty).is_some() {
            return self.textblock(node, Map::new());
        }
        if kit::by_tree(kit::CONTAINERS, ty).is_some() {
            return self.container(node);
        }
        if let Some((_, name)) = kit::CODE_BLOCKS.iter().find(|(t, _)| *t == ty) {
            return Ok(code_like(node, name));
        }
        Ok(match ty {
            "code_block" => code_block(node),
            "thematic_break" => {
                let mut pm = typed("horizontalRule");
                pm.insert(
                    "attrs".into(),
                    attrs1("carve", carve_attr(fields(node, &[]))),
                );
                pm
            }
            "list" => self.list_node(node)?,
            "definition_list" => self.mixed(node, "carveDefinitionList")?,
            "line_block" => self.mixed(node, "carveLineBlock")?,
            "table" => self.table(node)?,
            _ => block_atom(node),
        })
    }

    /// The host node a tree node loads as, or `None`.
    fn load_mapped(&mut self, node: &Node, position: Position) -> Result<Option<Node>> {
        let mapping = Arc::clone(&self.mapping);
        for (name, entry) in &mapping.nodes {
            match entry {
                MappingEntry::Callback { from_tree, .. } => {
                    let Some(from_tree) = from_tree else { continue };
                    let mut ctx = Ctx {
                        renderer: self,
                        position,
                    };
                    if let Some(Value::Object(pm)) =
                        from_tree(&Value::Object(node.clone()), &mut ctx)?
                    {
                        return Ok(Some(pm));
                    }
                }
                MappingEntry::Declared { content, .. } => {
                    let want = if position == Position::Block {
                        "div"
                    } else {
                        "span"
                    };
                    if !declared(entry, node, want, !content) {
                        continue;
                    }
                    let mut pm = typed(name);
                    let attrs = declared_attrs(node);
                    if !attrs.is_empty() {
                        pm.insert("attrs".into(), Value::Object(attrs));
                    }
                    let children = list(node.get("children"));
                    if !children.is_empty() {
                        let content = if position == Position::Block {
                            self.blocks(&children)?
                        } else {
                            self.inlines(&children)?
                        };
                        pm.insert("content".into(), array(content));
                    }
                    return Ok(Some(pm));
                }
            }
        }
        Ok(None)
    }

    /// The host mark an inline container loads as, or `None`.
    fn load_mapped_mark(&mut self, node: &Node) -> Result<Option<Node>> {
        let mapping = Arc::clone(&self.mapping);
        for (name, entry) in &mapping.marks {
            match entry {
                MappingEntry::Callback { from_tree, .. } => {
                    let Some(from_tree) = from_tree else { continue };
                    let mut ctx = Ctx {
                        renderer: self,
                        position: Position::Inline,
                    };
                    if let Some(Value::Object(mark)) =
                        from_tree(&Value::Object(node.clone()), &mut ctx)?
                    {
                        return Ok(Some(mark));
                    }
                }
                MappingEntry::Declared { .. } => {
                    if !declared(entry, node, "span", false) {
                        continue;
                    }
                    let mut mark = typed(name);
                    let attrs = declared_attrs(node);
                    if !attrs.is_empty() {
                        mark.insert("attrs".into(), Value::Object(attrs));
                    }
                    return Ok(Some(mark));
                }
            }
        }
        Ok(None)
    }

    /// A block with its caption: the tree keeps the caption as a field of
    /// the block.
    fn captioned(&mut self, node: &Node) -> Result<Node> {
        let caption = kit::map(node.get("caption"));
        let mut node = node.clone();
        node.remove("caption");
        let mut caption_node = self.container(&caption)?;
        if list(caption.get("children")).is_empty() {
            let mut carve = carve_of(&caption_node);
            carve.insert("$empty".into(), Value::Bool(true));
            let mut attrs = kit::attrs_of(&caption_node);
            attrs.insert("carve".into(), Value::Object(carve));
            caption_node.insert("attrs".into(), Value::Object(attrs));
        }
        let mut pm = typed("carveCaptioned");
        pm.insert(
            "content".into(),
            array(vec![self.block(&node)?, caption_node]),
        );
        Ok(pm)
    }

    fn textblock(&mut self, node: &Node, extra: Map<String, Value>) -> Result<Node> {
        let Some((name, owned)) = kit::by_tree(kit::TEXTBLOCKS, &type_of(node)) else {
            return Ok(block_atom(node));
        };
        let mut omit = vec!["children"];
        omit.extend_from_slice(owned);
        let mut carve = fields(node, &omit);
        for (k, v) in extra {
            kit::or_insert(&mut carve, &k, v);
        }
        let children = list(node.get("children"));
        // A loaded empty paragraph is content; only one the editor made is a placeholder.
        if children.is_empty() && carve.is_empty() {
            carve.insert("$empty".into(), Value::Bool(true));
        }
        let mut attrs = attrs1("carve", carve_attr(carve));
        for key in owned {
            if let Some(v) = node.get(*key) {
                put(&mut attrs, key, v.clone());
            }
        }
        let content = self.inlines(&children)?;
        let mut pm = typed(name);
        pm.insert("attrs".into(), attrs);
        if !content.is_empty() {
            pm.insert("content".into(), array(content));
        }
        Ok(pm)
    }

    fn container(&mut self, node: &Node) -> Result<Node> {
        let ty = type_of(node);
        let ty = if kit::by_tree(kit::CONTAINERS, &ty).is_some() {
            ty
        } else {
            "caption".into()
        };
        let Some((name, owned)) = kit::by_tree(kit::CONTAINERS, &ty) else {
            return Ok(block_atom(node));
        };
        let title = list(node.get("title"));
        let titled = kit::TITLED.contains(&ty.as_str()) && !title.is_empty();
        let mut omit = vec!["children"];
        omit.extend_from_slice(owned);
        if titled {
            omit.push("title");
        }
        let mut attrs = attrs1("carve", carve_attr(fields(node, &omit)));
        for key in owned {
            if let Some(v) = node.get(*key) {
                put(&mut attrs, key, v.clone());
            }
        }
        let mut content = self.blocks(&list(node.get("children")))?;
        if titled {
            let inlines = self.inlines(&title)?;
            let mut t = typed("carveTitle");
            if !inlines.is_empty() {
                t.insert("content".into(), array(inlines));
            }
            content.insert(0, t);
        }
        let mut pm = typed(name);
        pm.insert("attrs".into(), attrs);
        pm.insert("content".into(), array(content));
        Ok(pm)
    }

    fn list_node(&mut self, node: &Node) -> Result<Node> {
        let items = list(node.get("children"));
        if items.is_empty() {
            return Ok(block_atom(node));
        }
        let tasks = items.iter().all(|item| item.contains_key("task"));
        let marker = kit::map(node.get("marker"));
        let ty = if tasks {
            "taskList"
        } else if marker.get("kind").and_then(Value::as_str) == Some("ordered") {
            "orderedList"
        } else {
            "bulletList"
        };
        let mut attrs = attrs1("carve", carve_attr(fields(node, &["children"])));
        if ty == "orderedList" {
            put(
                &mut attrs,
                "start",
                marker
                    .get("start")
                    .filter(|v| !v.is_null())
                    .cloned()
                    .unwrap_or(Value::from(1)),
            );
        }
        let mut content = Vec::with_capacity(items.len());
        for item in &items {
            content.push(self.list_item(item, tasks)?);
        }
        let mut pm = typed(ty);
        pm.insert("attrs".into(), attrs);
        pm.insert("content".into(), array(content));
        Ok(pm)
    }

    fn list_item(&mut self, item: &Node, tasks: bool) -> Result<Node> {
        let mut item_fields = fields(item, &["children"]);
        let children = list(item.get("children"));
        if children.is_empty() {
            item_fields.insert("$empty".into(), Value::Bool(true));
        }
        let mut attrs = attrs1("carve", carve_attr(item_fields));
        if tasks {
            put(
                &mut attrs,
                "checked",
                Value::Bool(task_checked(item.get("task"))),
            );
        }
        // Tiptap items start with a paragraph; an empty one, which the way back drops, leads any other block.
        let mut blocks = self.blocks(&children)?;
        if blocks.first().is_none_or(|b| type_of(b) != "paragraph") {
            blocks.insert(0, typed("paragraph"));
        }
        let mut pm = typed(if tasks { "taskItem" } else { "listItem" });
        pm.insert("attrs".into(), attrs);
        pm.insert("content".into(), array(blocks));
        Ok(pm)
    }

    /// Definition lists and line blocks: their children are their own node
    /// types and comments.
    fn mixed(&mut self, node: &Node, name: &str) -> Result<Node> {
        let children = list(node.get("children"));
        if children.is_empty() {
            return Ok(block_atom(node));
        }
        let mut content = Vec::with_capacity(children.len());
        for child in &children {
            content.push(self.block(child)?);
        }
        let mut pm = typed(name);
        pm.insert(
            "attrs".into(),
            attrs1("carve", carve_attr(fields(node, &["children"]))),
        );
        pm.insert("content".into(), array(content));
        Ok(pm)
    }

    fn table(&mut self, node: &Node) -> Result<Node> {
        let mut sections = Vec::new();
        let mut rows: Vec<Node> = Vec::new();
        for (index, section) in list(node.get("children")).iter().enumerate() {
            if type_of(section) == "comment" {
                let mut entry = Map::new();
                entry.insert("$node".into(), clean(&Value::Object(section.clone())));
                sections.push(entry);
            } else {
                sections.push(self.table_section(section, index, &mut rows)?);
            }
        }
        if rows.is_empty() || rows.iter().any(|r| kit::nodes(r.get("content")).is_empty()) {
            return Ok(block_atom(node));
        }
        // ProseMirror tables are rectangular: short rows get marked padding cells, which the way back drops while empty.
        let grid: Vec<Vec<(i64, i64)>> = rows
            .iter()
            .map(|row| {
                kit::nodes(row.get("content"))
                    .iter()
                    .map(|cell| {
                        let attrs = kit::attrs_of(cell);
                        (
                            kit::span_count(attrs.get("colspan")),
                            kit::span_count(attrs.get("rowspan")),
                        )
                    })
                    .collect()
            })
            .collect();
        let widths = kit::grid_widths(&grid);
        let width = widths.iter().copied().max().unwrap_or(0);
        let mut pad = typed("tableCell");
        let mut pad_carve = Map::new();
        pad_carve.insert("$pad".into(), Value::Bool(true));
        pad.insert("attrs".into(), attrs1("carve", Value::Object(pad_carve)));
        pad.insert("content".into(), array(vec![typed("paragraph")]));
        for (row, row_width) in rows.iter_mut().zip(widths) {
            let mut content = kit::nodes(row.get("content"));
            for _ in row_width..width {
                content.push(pad.clone());
            }
            row.insert("content".into(), array(content));
        }
        let mut carve = fields(node, &["children"]);
        carve.insert("$sections".into(), array(sections));
        if bare_spans(node) {
            carve.insert("$bareSpans".into(), Value::Bool(true));
        }
        let mut pm = typed("table");
        pm.insert("attrs".into(), attrs1("carve", Value::Object(carve)));
        pm.insert("content".into(), array(rows));
        Ok(pm)
    }

    /// The `$sections` entry of a table section; its rows go to `rows`.
    fn table_section(
        &mut self,
        section: &Node,
        index: usize,
        rows: &mut Vec<Node>,
    ) -> Result<Node> {
        let mut entry = Map::new();
        entry.insert(
            "type".into(),
            section.get("type").cloned().unwrap_or(Value::Null),
        );
        for (k, v) in fields(section, &["children"]) {
            kit::or_insert(&mut entry, &k, v);
        }
        let mut before: Vec<Value> = Vec::new();
        let mut has_row = false;
        for row in list(section.get("children")) {
            if type_of(&row) == "comment" {
                before.push(clean(&Value::Object(row)));
                continue;
            }
            has_row = true;
            let mut carve = fields(&row, &["children"]);
            carve.insert("$section".into(), Value::from(index));
            if !before.is_empty() {
                carve.insert("$before".into(), Value::Array(std::mem::take(&mut before)));
            }
            // ProseMirror has no cell where a span mark stands: the covering cell's counts span it.
            let mut content = Vec::new();
            for cell in list(row.get("children")) {
                if !cell.contains_key("span") {
                    content.push(self.cell(&cell)?);
                }
            }
            let th = content.iter().all(|c| type_of(c) == "tableHeader");
            if section.get("type").and_then(Value::as_str) != Some("table_head")
                && !content.is_empty()
                && th
            {
                carve.insert("$th".into(), Value::Bool(true));
            }
            let mut pm = typed("tableRow");
            pm.insert("attrs".into(), attrs1("carve", Value::Object(carve)));
            pm.insert("content".into(), array(content));
            rows.push(pm);
        }
        if !before.is_empty() {
            entry.insert("$after".into(), Value::Array(before));
        }
        if !has_row {
            entry.insert("$empty".into(), Value::Bool(true));
        }
        Ok(entry)
    }

    fn cell(&mut self, cell: &Node) -> Result<Node> {
        let mut content = Vec::new();
        for child in list(cell.get("children")) {
            if type_of(&child) == "cell_paragraph" {
                let mut as_paragraph = child.clone();
                as_paragraph.insert("type".into(), Value::String("paragraph".into()));
                let mut extra = Map::new();
                extra.insert("$type".into(), Value::String("cell_paragraph".into()));
                content.push(self.textblock(&as_paragraph, extra)?);
            } else {
                content.push(self.block(&child)?);
            }
        }
        let mut carve = fields(cell, &["children", "header", "colspan", "rowspan"]);
        if content.is_empty() {
            carve.insert("$empty".into(), Value::Bool(true));
        }
        let mut attrs = attrs1("carve", carve_attr(carve));
        for key in ["colspan", "rowspan"] {
            if let Some(n) = cell
                .get(key)
                .filter(|v| v.is_i64() || v.is_u64())
                .and_then(Value::as_i64)
                && n > 1
            {
                put(&mut attrs, key, Value::from(n));
            }
        }
        let header = cell.get("header") == Some(&Value::Bool(true));
        let mut pm = typed(if header { "tableHeader" } else { "tableCell" });
        pm.insert("attrs".into(), attrs);
        if content.is_empty() {
            content.push(typed("paragraph"));
        }
        pm.insert("content".into(), array(content));
        Ok(pm)
    }

    fn inlines(&mut self, children: &[Node]) -> Result<Vec<Node>> {
        let mut out = Vec::new();
        self.push_inlines(children, &[], &[], &mut out)?;
        Ok(out)
    }

    fn push_inlines(
        &mut self,
        children: &[Node],
        marks: &[Node],
        open: &[String],
        out: &mut Vec<Node>,
    ) -> Result<()> {
        for child in children {
            let inner = list(child.get("children"));
            let host_mark = if inner.is_empty() {
                None
            } else {
                self.load_mapped_mark(child)?
            };
            if let Some(host_mark) = host_mark {
                let host_name = type_of(&host_mark);
                if !open.contains(&host_name) {
                    let mut next_marks = marks.to_vec();
                    next_marks.push(host_mark);
                    let mut next_open = open.to_vec();
                    next_open.push(host_name);
                    self.push_inlines(&inner, &next_marks, &next_open, out)?;
                    continue;
                }
            }
            if let Some(host_node) = self.load_mapped(child, Position::Inline)? {
                out.push(with_marks(host_node, marks));
                continue;
            }
            if let Some((mark, owned)) = kit::by_tree(kit::MARKS, &type_of(child))
                && !inner.is_empty()
                && !open.iter().any(|o| o == mark)
            {
                let m = self.kit_mark(child, mark, owned, marks.len());
                let mut next_marks = marks.to_vec();
                next_marks.push(m);
                let mut next_open = open.to_vec();
                next_open.push(mark.to_string());
                self.push_inlines(&inner, &next_marks, &next_open, out)?;
                continue;
            }
            out.push(self.leaf(child, marks));
        }
        Ok(())
    }

    /// The kit mark of an inline container at `depth`.
    fn kit_mark(&mut self, child: &Node, mark: &str, owned: &[&str], depth: usize) -> Node {
        let mut omit = vec!["children"];
        omit.extend_from_slice(owned);
        let mut carve = fields(child, &omit);
        self.id += 1;
        carve.insert("$id".into(), Value::from(self.id));
        carve.insert("$depth".into(), Value::from(depth));
        let mut attrs = Map::new();
        for key in owned {
            attrs.insert(
                (*key).to_string(),
                child.get(*key).cloned().unwrap_or(Value::Null),
            );
        }
        if mark == "link" && self.denied(attrs.get("href")) {
            carve.insert(
                "href".into(),
                attrs.get("href").cloned().unwrap_or(Value::Null),
            );
            attrs.insert("href".into(), Value::Null);
        }
        attrs.insert("carve".into(), Value::Object(carve));
        let mut m = typed(mark);
        m.insert("attrs".into(), Value::Object(attrs));
        m
    }

    fn leaf(&mut self, node: &Node, marks: &[Node]) -> Node {
        match type_of(node).as_str() {
            "text" => {
                let value = str_of(node.get("value"));
                if value.is_empty() {
                    return inline_atom(node, marks);
                }
                let mut pm = typed("text");
                pm.insert("text".into(), Value::String(value));
                with_marks(pm, marks)
            }
            "code" => self.code(node, marks),
            "hard_break" => {
                let mut pm = typed("hardBreak");
                pm.insert(
                    "attrs".into(),
                    attrs1("carve", carve_attr(fields(node, &[]))),
                );
                with_marks(pm, marks)
            }
            "soft_break" => {
                let mut pm = typed("carveSoftBreak");
                pm.insert(
                    "attrs".into(),
                    attrs1("carve", carve_attr(fields(node, &[]))),
                );
                with_marks(pm, marks)
            }
            "image" => {
                let pm = self.image(node);
                with_marks(pm, marks)
            }
            "math" => {
                let mut attrs = Map::new();
                attrs.insert(
                    "value".into(),
                    node.get("value").cloned().unwrap_or(Value::Null),
                );
                attrs.insert(
                    "display".into(),
                    Value::Bool(node.get("display") == Some(&Value::Bool(true))),
                );
                attrs.insert(
                    "carve".into(),
                    carve_attr(fields(node, &["value", "display"])),
                );
                let mut pm = typed("carveMath");
                pm.insert("attrs".into(), Value::Object(attrs));
                with_marks(pm, marks)
            }
            "footnote_ref" => {
                let mut attrs = Map::new();
                attrs.insert(
                    "key".into(),
                    node.get("key").cloned().unwrap_or(Value::Null),
                );
                attrs.insert(
                    "carve".into(),
                    carve_attr(fields(node, &["key", "namespace"])),
                );
                let mut pm = typed("carveFootnoteRef");
                pm.insert("attrs".into(), Value::Object(attrs));
                with_marks(pm, marks)
            }
            _ => inline_atom(node, marks),
        }
    }

    fn code(&mut self, node: &Node, marks: &[Node]) -> Node {
        let value = str_of(node.get("value"));
        if value.is_empty() {
            return inline_atom(node, marks);
        }
        let mut carve = fields(node, &["value"]);
        self.id += 1;
        carve.insert("$id".into(), Value::from(self.id));
        carve.insert("$depth".into(), Value::from(marks.len()));
        let mut code = typed("code");
        code.insert("attrs".into(), attrs1("carve", Value::Object(carve)));
        let mut all = marks.to_vec();
        all.push(code);
        let mut pm = typed("text");
        pm.insert("text".into(), Value::String(value));
        pm.insert("marks".into(), array(all));
        pm
    }

    fn image(&mut self, node: &Node) -> Node {
        let denied = self.denied(node.get("src"));
        let mut attrs = Map::new();
        attrs.insert(
            "src".into(),
            if denied {
                Value::Null
            } else {
                node.get("src").cloned().unwrap_or(Value::Null)
            },
        );
        attrs.insert(
            "alt".into(),
            node.get("alt").cloned().unwrap_or(Value::Null),
        );
        attrs.insert(
            "title".into(),
            node.get("tooltip").cloned().unwrap_or(Value::Null),
        );
        let omit: &[&str] = if denied {
            &["alt", "tooltip"]
        } else {
            &["src", "alt", "tooltip"]
        };
        attrs.insert("carve".into(), carve_attr(fields(node, omit)));
        let mut pm = typed("image");
        pm.insert("attrs".into(), Value::Object(attrs));
        pm
    }
}

/// The callback context of the renderer: `blocks` and `inlines` load tree
/// children.
struct Ctx<'r> {
    renderer: &'r mut Renderer,
    position: Position,
}

impl Context for Ctx<'_> {
    fn position(&self) -> Position {
        self.position
    }

    fn blocks(&mut self, children: &Value) -> Result<Vec<Value>> {
        let nodes = list(Some(children));
        Ok(self
            .renderer
            .blocks(&nodes)?
            .into_iter()
            .map(Value::Object)
            .collect())
    }

    fn inlines(&mut self, children: &Value) -> Result<Vec<Value>> {
        let nodes = list(Some(children));
        Ok(self
            .renderer
            .inlines(&nodes)?
            .into_iter()
            .map(Value::Object)
            .collect())
    }
}

/// The objects of a JSON array; PHP's `is_array` list check.
fn list(value: Option<&Value>) -> Vec<Node> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| v.as_object().cloned().unwrap_or_default())
            .collect(),
        _ => Vec::new(),
    }
}

fn attrs1(key: &str, value: Value) -> Value {
    let mut m = Map::new();
    m.insert(key.into(), value);
    Value::Object(m)
}

fn put(attrs: &mut Value, key: &str, value: Value) {
    if let Value::Object(m) = attrs {
        m.insert(key.into(), value);
    }
}

fn block_atom(node: &Node) -> Node {
    let mut pm = typed("carveBlock");
    pm.insert(
        "attrs".into(),
        attrs1("carve", clean(&Value::Object(node.clone()))),
    );
    pm
}

fn inline_atom(node: &Node, marks: &[Node]) -> Node {
    let mut pm = typed("carveInline");
    pm.insert(
        "attrs".into(),
        attrs1("carve", clean(&Value::Object(node.clone()))),
    );
    with_marks(pm, marks)
}

fn with_marks(mut pm: Node, marks: &[Node]) -> Node {
    if !marks.is_empty() {
        pm.insert("marks".into(), array(marks.to_vec()));
    }
    pm
}

fn code_like(node: &Node, name: &str) -> Node {
    let value = str_of(node.get("value"));
    let mut pm = typed(name);
    pm.insert(
        "attrs".into(),
        attrs1("carve", carve_attr(fields(node, &["value"]))),
    );
    if !value.is_empty() {
        let mut text = typed("text");
        text.insert("text".into(), Value::String(value));
        pm.insert("content".into(), array(vec![text]));
    }
    pm
}

fn code_block(node: &Node) -> Node {
    let value = str_of(node.get("value"));
    let newline = value.ends_with('\n');
    let text = if newline && !value.ends_with("\n\n") {
        value.get(..value.len() - 1).unwrap_or_default().to_string()
    } else {
        value
    };
    let mut carve = fields(node, &["value", "syntax"]);
    if !newline {
        carve.insert("$newline".into(), Value::Bool(false));
    }
    let mut attrs = Map::new();
    attrs.insert(
        "language".into(),
        node.get("syntax").cloned().unwrap_or(Value::Null),
    );
    attrs.insert("carve".into(), carve_attr(carve));
    let mut pm = typed("codeBlock");
    pm.insert("attrs".into(), Value::Object(attrs));
    if !text.is_empty() {
        let mut t = typed("text");
        t.insert("text".into(), Value::String(text));
        pm.insert("content".into(), array(vec![t]));
    }
    pm
}

fn task_checked(task: Option<&Value>) -> bool {
    matches!(
        task.and_then(|t| t.get("mark")).and_then(Value::as_str),
        Some("x" | "X")
    )
}

/// Whether the table has counts and no mark cells: an imported span
/// (C2-A-093), which saves back without marks.
fn bare_spans(node: &Node) -> bool {
    let mut counted = false;
    for section in list(node.get("children")) {
        for row in list(section.get("children")) {
            for cell in list(row.get("children")) {
                if cell.contains_key("span") {
                    return false;
                }
                let above_one = |key: &str| {
                    cell.get(key)
                        .filter(|v| !v.is_null())
                        .and_then(Value::as_f64)
                        .is_some_and(|n| n > 1.0)
                };
                counted = counted || above_one("colspan") || above_one("rowspan");
            }
        }
    }
    counted
}

/// Whether `node` is exactly what a declared entry saves, so loading it as
/// the host type loses nothing.
pub(super) fn declared(entry: &MappingEntry, node: &Node, ty: &str, atom: bool) -> bool {
    let MappingEntry::Declared {
        class, attributes, ..
    } = entry
    else {
        return false;
    };
    if entry.declared_as() != Some(ty)
        || node.get("type").and_then(Value::as_str) != Some(ty)
        || !fields(node, &["children", "attrs"]).is_empty()
    {
        return false;
    }
    if atom && !kit::nodes(node.get("children")).is_empty() {
        return false;
    }
    let attrs = kit::map(node.get("attrs"));
    if attrs.keys().any(|k| k != "classes" && k != "keyValues")
        || attrs.get("classes") != Some(&Value::Array(vec![Value::String(class.clone())]))
    {
        return false;
    }
    declared_pairs(attributes, attrs.get("keyValues"))
}

/// Whether `pairs` are key/value pairs of distinct keys the entry declares.
fn declared_pairs(attributes: &[String], pairs: Option<&Value>) -> bool {
    let pairs = match pairs {
        None | Some(Value::Null) => return true,
        Some(Value::Array(pairs)) => pairs,
        Some(_) => return false,
    };
    let mut seen: Vec<&str> = Vec::new();
    for pair in pairs {
        let Some(key) = pair.get(0).and_then(Value::as_str) else {
            return false;
        };
        if !pair.get(1).is_some_and(Value::is_string)
            || seen.contains(&key)
            || !attributes.iter().any(|a| a == key)
        {
            return false;
        }
        seen.push(key);
    }
    true
}

/// The ProseMirror attributes of a declared node: its key/value pairs.
fn declared_attrs(node: &Node) -> Map<String, Value> {
    let mut out = Map::new();
    let attrs = kit::map(node.get("attrs"));
    if let Some(Value::Array(pairs)) = attrs.get("keyValues") {
        for pair in pairs {
            if let (Some(k), Some(v)) = (
                pair.get(0).and_then(Value::as_str),
                pair.get(1).and_then(Value::as_str),
            ) {
                out.insert(k.into(), Value::String(v.into()));
            }
        }
    }
    out
}
