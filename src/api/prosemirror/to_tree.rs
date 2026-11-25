//! ProseMirror JSON to a tree. A node or mark with no Carve node in the kit
//! or the mapping is an error, not a skip.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Map, Value};

use super::kit::{
    self, array, attrs_of, carve_of, clean, str_of, tree_fields, type_of, typed, with,
};
use super::{
    Context, IgnoredAttribute, Mapping, MappingEntry, Position, ProseMirrorError,
    ProseMirrorOptions,
};

type Node = Map<String, Value>;
type Result<T> = std::result::Result<T, ProseMirrorError>;

/// The attributes each node and mark reads; any other attribute with a
/// value is reported as ignored.
const READ: &[(&str, &[&str])] = &[
    ("heading", &["level"]),
    ("codeBlock", &["language"]),
    ("orderedList", &["start"]),
    ("taskItem", &["checked"]),
    ("tableCell", &["colspan", "rowspan"]),
    ("tableHeader", &["colspan", "rowspan"]),
    ("image", &["src", "alt", "title"]),
    ("link", &["href"]),
    ("carveAdmonition", &["kind"]),
    ("carveMath", &["value", "display"]),
    ("carveFootnoteRef", &["key"]),
];

/// Values the kit's own extensions give every node or mark of a type, which
/// carry nothing to lose.
const STOCK: &[(&str, &str, &str)] = &[
    ("link", "target", "_blank"),
    ("link", "rel", "noopener noreferrer nofollow"),
];

const POINTER: &str = "$pointer";

pub(super) struct Saved {
    pub(super) tree: Value,
    pub(super) origins: Vec<Option<i64>>,
    pub(super) pointers: BTreeMap<String, String>,
    pub(super) ignored: Vec<IgnoredAttribute>,
}

pub(super) struct Saver {
    mapping: Arc<Mapping>,
    pointers: BTreeMap<String, String>,
    ignored: Vec<IgnoredAttribute>,
}

fn invalid(message: String) -> ProseMirrorError {
    ProseMirrorError::Invalid(message)
}

impl Saver {
    pub(super) fn new(options: &ProseMirrorOptions) -> Self {
        Self {
            mapping: Arc::new(options.mapping.clone()),
            pointers: BTreeMap::new(),
            ignored: Vec::new(),
        }
    }

    pub(super) fn save(mut self, doc: &Value) -> Result<Saved> {
        let Some(doc) = doc.as_object().filter(|d| type_of(d) == "doc") else {
            return Err(invalid(
                "ProseMirrorToCarve expects a ProseMirror doc node".into(),
            ));
        };
        let mut children = Vec::new();
        let mut origins = Vec::new();
        for (at, pm) in content(doc).iter().enumerate() {
            if is_placeholder(pm) {
                continue;
            }
            let Some(node) = self.block(pm, &format!("/content/{at}"))? else {
                continue;
            };
            children.push(node);
            origins.push(
                carve_of(pm)
                    .get("$index")
                    .filter(|v| v.is_i64() || v.is_u64())
                    .and_then(Value::as_i64),
            );
        }
        let mut tree = typed("document");
        tree.insert("astVersion".into(), Value::String("2.0".into()));
        for (k, v) in tree_fields(&carve_of(doc)) {
            tree.insert(k, v);
        }
        tree.insert("children".into(), array(children));
        tree.insert(POINTER.into(), Value::String(String::new()));
        self.pointers.clear();
        let mut tree = Value::Object(tree);
        self.strip(&mut tree, "");
        self.ignored.clear();
        self.collect_ignored(doc, "");
        Ok(Saved {
            tree,
            origins,
            pointers: self.pointers,
            ignored: self.ignored,
        })
    }

    /// Moves each node's `$pointer` into the pointers under its tree path.
    fn strip(&mut self, node: &mut Value, path: &str) {
        let Value::Object(map) = node else { return };
        if let Some(pointer) = map.remove(POINTER) {
            self.pointers
                .insert(path.to_string(), str_of(Some(&pointer)));
        }
        for (key, value) in map.iter_mut() {
            match value {
                Value::Object(inner) => {
                    if inner.get("type").is_some_and(Value::is_string) {
                        self.strip(value, &format!("{path}/{key}"));
                    }
                }
                Value::Array(items) => {
                    for (index, item) in items.iter_mut().enumerate() {
                        if item.get("type").is_some_and(Value::is_string) {
                            self.strip(item, &format!("{path}/{key}/{index}"));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_ignored(&mut self, pm: &Node, path: &str) {
        let ty = type_of(pm);
        let attrs = attrs_of(pm);
        let read = self.reads(&ty, &attrs, path);
        for (attribute, value) in &attrs {
            if attribute == "carve" || value.is_null() || read.contains(attribute) {
                continue;
            }
            if STOCK
                .iter()
                .any(|(t, a, v)| *t == ty && a == attribute && value.as_str() == Some(*v))
            {
                continue;
            }
            self.ignored.push(IgnoredAttribute {
                path: path.to_string(),
                type_: ty.clone(),
                attribute: attribute.clone(),
                value: value.clone(),
            });
        }
        // A `^` below a cell of more than one column joins nothing (C2-B-172), so such a cell keeps only its columns.
        if (ty == "tableCell" || ty == "tableHeader")
            && kit::span_count(attrs.get("colspan")) > 1
            && kit::span_count(attrs.get("rowspan")) > 1
        {
            self.ignored.push(IgnoredAttribute {
                path: path.to_string(),
                type_: ty.clone(),
                attribute: "rowspan".into(),
                value: attrs.get("rowspan").cloned().unwrap_or(Value::Null),
            });
        }
        for (index, mark) in kit::nodes(pm.get("marks")).iter().enumerate() {
            self.collect_ignored(mark, &format!("{path}/marks/{index}"));
        }
        for (index, child) in content(pm).iter().enumerate() {
            self.collect_ignored(child, &format!("{path}/content/{index}"));
        }
    }

    /// The attributes a node or mark reads: a declared mapping its own, a
    /// callback all of them.
    fn reads(&self, ty: &str, attrs: &Node, path: &str) -> Vec<String> {
        let is_mark = path
            .rsplit_once("/marks/")
            .is_some_and(|(_, tail)| !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()));
        let host = if is_mark {
            self.mapping.mark(ty)
        } else {
            self.mapping.node(ty)
        };
        match host {
            Some(MappingEntry::Declared { attributes, .. }) => attributes.clone(),
            Some(MappingEntry::Callback { .. }) => attrs.keys().cloned().collect(),
            None => READ
                .iter()
                .find(|(t, _)| *t == ty)
                .map(|(_, r)| r.iter().map(|s| (*s).to_string()).collect())
                .unwrap_or_default(),
        }
    }

    fn blocks(&mut self, content: &[Node], base: &str, offset: usize) -> Result<Vec<Node>> {
        let mut out = Vec::new();
        for (index, pm) in content.iter().enumerate() {
            if is_placeholder(pm) {
                continue;
            }
            if let Some(node) = self.block(pm, &format!("{base}/content/{}", index + offset))? {
                out.push(node);
            }
        }
        Ok(out)
    }

    fn block(&mut self, pm: &Node, pointer: &str) -> Result<Option<Node>> {
        let mapping = Arc::clone(&self.mapping);
        let node = match mapping.node(&type_of(pm)) {
            Some(host) => Some(self.save_mapped(pm, host, Position::Block, pointer)?),
            None => self.block_of(pm, pointer)?,
        };
        Ok(node.map(|n| tag(n, pointer)))
    }

    fn block_of(&mut self, pm: &Node, pointer: &str) -> Result<Option<Node>> {
        let ty = type_of(pm);
        let carve = carve_of(pm);
        if kit::by_name(kit::TEXTBLOCKS, &ty).is_some() {
            return self.textblock(pm, pointer).map(Some);
        }
        if kit::by_name(kit::CONTAINERS, &ty).is_some() {
            return self.container(pm, pointer).map(Some);
        }
        if let Some((code, _)) = kit::CODE_BLOCKS.iter().find(|(_, n)| *n == ty) {
            let mut node = with(typed(code), tree_fields(&carve));
            kit::or_insert(&mut node, "value", Value::String(text_of(pm)));
            return Ok(Some(node));
        }
        self.other_block(pm, &ty, carve, pointer)
    }

    fn other_block(
        &mut self,
        pm: &Node,
        ty: &str,
        carve: Node,
        pointer: &str,
    ) -> Result<Option<Node>> {
        Ok(Some(match ty {
            "carveCaptioned" => return self.captioned(pm, pointer),
            "carveBlock" => {
                let mut carve = carve;
                carve.remove("$index");
                kit::map(Some(&clean(&Value::Object(carve))))
            }
            "codeBlock" => code_block(pm),
            "horizontalRule" => with(typed("thematic_break"), tree_fields(&carve)),
            "bulletList" | "orderedList" | "taskList" => return self.list_node(pm, pointer),
            "carveDefinitionList" => self.mixed(pm, "definition_list", pointer)?,
            "carveLineBlock" => self.mixed(pm, "line_block", pointer)?,
            "table" => self.table(pm, pointer)?,
            // A block image, as Tiptap's Image extension makes by default, is an image paragraph (C2-A-136, C2-H-020).
            "image" => {
                let mut node = typed("paragraph");
                node.insert("role".into(), Value::String("image".into()));
                node.insert("children".into(), array(vec![tag(image(pm), pointer)]));
                node
            }
            _ => {
                return Err(invalid(format!(
                    "ProseMirrorToCarve: no Carve node for the ProseMirror node \"{ty}\""
                )));
            }
        }))
    }

    /// The tree node of a host node.
    fn save_mapped(
        &mut self,
        pm: &Node,
        entry: &MappingEntry,
        position: Position,
        pointer: &str,
    ) -> Result<Node> {
        let ty = type_of(pm);
        let content = content(pm);
        let (as_, attributes, has_content) = match entry {
            MappingEntry::Callback { to_tree, .. } => {
                let Some(to_tree) = to_tree else {
                    return Err(invalid(format!(
                        "ProseMirrorToCarve: the mapping of the ProseMirror node \"{ty}\" has no toTree"
                    )));
                };
                let mut ctx = Ctx {
                    saver: self,
                    position,
                    pointer: pointer.to_string(),
                };
                return Ok(kit::map(Some(&to_tree(
                    &Value::Object(pm.clone()),
                    &mut ctx,
                )?)));
            }
            MappingEntry::Declared {
                as_,
                attributes,
                content,
                ..
            } => (as_.as_str(), attributes, *content),
        };
        let want = if position == Position::Block {
            "div"
        } else {
            "span"
        };
        if as_ != want {
            return Err(invalid(format!(
                "ProseMirrorToCarve: the ProseMirror node \"{ty}\" is mapped to a {as_} and cannot stand as {}",
                if position == Position::Block {
                    "a block"
                } else {
                    "an inline"
                }
            )));
        }
        if !has_content && !content.is_empty() {
            return Err(invalid(format!(
                "ProseMirrorToCarve: the ProseMirror node \"{ty}\" has content; map it with content: true"
            )));
        }
        let class = match entry {
            MappingEntry::Declared { class, .. } => class.as_str(),
            MappingEntry::Callback { .. } => "",
        };
        let mut node = typed(want);
        node.insert(
            "attrs".into(),
            Value::Object(mapped_attrs(class, attributes, &attrs_of(pm))),
        );
        let children = if position == Position::Block {
            self.blocks(&content, pointer, 0)?
        } else {
            self.inlines(&content, pointer)?
        };
        node.insert("children".into(), array(children));
        Ok(node)
    }

    /// Definition lists and line blocks.
    fn mixed(&mut self, pm: &Node, ty: &str, pointer: &str) -> Result<Node> {
        let mut node = with(typed(ty), tree_fields(&carve_of(pm)));
        let children = self.blocks(&content(pm), pointer, 0)?;
        kit::or_insert(&mut node, "children", array(children));
        Ok(node)
    }

    fn captioned(&mut self, pm: &Node, pointer: &str) -> Result<Option<Node>> {
        let content = content(pm);
        let mut node = match content.first() {
            Some(first) => self.block(first, &format!("{pointer}/content/0"))?,
            None => None,
        };
        let caption = content.get(1);
        // An empty caption the editor made (to fill the wrapper) is no caption.
        let empty = caption
            .is_none_or(|c| carve_of(c).get("$empty") != Some(&Value::Bool(true)) && !filled(c));
        if let (Some(node), Some(caption), false) = (node.as_mut(), caption, empty)
            && let Some(caption) = self.block(caption, &format!("{pointer}/content/1"))?
        {
            node.insert("caption".into(), Value::Object(caption));
        }
        Ok(node)
    }

    fn textblock(&mut self, pm: &Node, pointer: &str) -> Result<Node> {
        let Some((tree, owned)) = kit::by_name(kit::TEXTBLOCKS, &type_of(pm)) else {
            return Ok(typed("paragraph"));
        };
        let carve = carve_of(pm);
        let ty = carve
            .get("$type")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(Value::String(tree.into()));
        let mut node = Map::new();
        node.insert("type".into(), ty);
        let mut node = with(node, tree_fields(&carve));
        let attrs = attrs_of(pm);
        for key in owned {
            if kit::isset(&attrs, key) {
                node.insert(
                    (*key).into(),
                    attrs.get(*key).cloned().unwrap_or(Value::Null),
                );
            }
        }
        let children = self.inlines(&content(pm), pointer)?;
        node.insert("children".into(), array(children));
        Ok(node)
    }

    fn container(&mut self, pm: &Node, pointer: &str) -> Result<Node> {
        let Some((tree, owned)) = kit::by_name(kit::CONTAINERS, &type_of(pm)) else {
            return Ok(typed("div"));
        };
        let mut node = with(typed(tree), tree_fields(&carve_of(pm)));
        let attrs = attrs_of(pm);
        for key in owned {
            if kit::isset(&attrs, key) {
                node.insert(
                    (*key).into(),
                    attrs.get(*key).cloned().unwrap_or(Value::Null),
                );
            }
        }
        let mut content = content(pm);
        let mut offset = 0;
        if content.first().is_some_and(|c| type_of(c) == "carveTitle") {
            let title_node = content.remove(0);
            let title =
                self.inlines(&self::content(&title_node), &format!("{pointer}/content/0"))?;
            if !title.is_empty() {
                node.insert("title".into(), array(title));
            }
            offset = 1;
        }
        let children = self.blocks(&content, pointer, offset)?;
        node.insert("children".into(), array(children));
        Ok(node)
    }

    fn list_node(&mut self, pm: &Node, pointer: &str) -> Result<Option<Node>> {
        let fields = tree_fields(&carve_of(pm));
        // An item with nothing in it is what Enter or a toggle leaves behind, unless it was loaded empty.
        let items: Vec<(usize, Node)> = content(pm)
            .into_iter()
            .enumerate()
            .filter(|(_, item)| {
                carve_of(item).get("$empty") == Some(&Value::Bool(true)) || filled(item)
            })
            .collect();
        if items.is_empty() {
            return Ok(None);
        }
        let mut node = typed("list");
        node.insert("spacing".into(), Value::String("tight".into()));
        let mut node = with(node, fields.clone());
        node.insert(
            "marker".into(),
            Value::Object(marker(pm, fields.get("marker"))),
        );
        let mut children = Vec::with_capacity(items.len());
        for (at, item) in &items {
            let at_pointer = format!("{pointer}/content/{at}");
            let child = self.list_item(item, &at_pointer)?;
            children.push(tag(child, &at_pointer));
        }
        node.insert("children".into(), array(children));
        Ok(Some(node))
    }

    fn list_item(&mut self, item: &Node, pointer: &str) -> Result<Node> {
        let item_fields = tree_fields(&carve_of(item));
        let mut child = with(typed("list_item"), item_fields.clone());
        if type_of(item) == "taskItem" {
            let checked = attrs_of(item).get("checked") == Some(&Value::Bool(true));
            if !item_fields.contains_key("task") || task_checked(item_fields.get("task")) != checked
            {
                let mut task = Map::new();
                task.insert(
                    "mark".into(),
                    Value::String(if checked { "x" } else { " " }.into()),
                );
                child.insert("task".into(), Value::Object(task));
            }
        }
        let children = self.blocks(&content(item), pointer, 0)?;
        child.insert("children".into(), array(children));
        Ok(child)
    }

    fn table(&mut self, pm: &Node, pointer: &str) -> Result<Node> {
        let carve = carve_of(pm);
        let mut sections = kit::nodes(carve.get("$sections"));
        if sections.is_empty() {
            sections.push(typed("table_body"));
        }
        let mut rows = Vec::new();
        for (at, row) in content(pm).iter().enumerate() {
            let mut cells = Vec::new();
            let mut cell_pointers = Vec::new();
            for (index, cell) in content(row).into_iter().enumerate() {
                if carve_of(&cell).get("$pad") != Some(&Value::Bool(true)) || filled(&cell) {
                    cell_pointers.push(format!("{pointer}/content/{at}/content/{index}"));
                    cells.push(cell);
                }
            }
            rows.push(Row {
                carve: carve_of(row),
                cells,
                pointer: format!("{pointer}/content/{at}"),
                cell_pointers,
                section: 0,
            });
        }
        let lead = leading_head(&sections, &rows);
        let mut shift = 0;
        if lead > 0 {
            shift = find(&sections, |s, _| !s.contains_key("$node")).max(0);
            sections.insert(usize::try_from(shift).unwrap_or(0), typed("table_head"));
        }
        // A body row added after a head that has no body behind it needs one; loaded rows keep their sections.
        let head = find(&sections, |s, _| section_type(s) == Some("table_head"));
        let mut added = -1;
        if head >= 0 && body_after(&sections, head) < 0 {
            sections.insert(usize::try_from(head + 1).unwrap_or(0), typed("table_body"));
            added = head + 1;
        }
        place(
            &mut rows,
            &sections,
            lead,
            if lead > 0 { shift } else { -1 },
            added,
            head,
        );
        let mut rows_of: Vec<Vec<Node>> = vec![Vec::new(); sections.len()];
        let bare = carve.get("$bareSpans") == Some(&Value::Bool(true));
        let width = self.span_rows(&rows, bare, &mut rows_of)?;
        let mut node = with(typed("table"), tree_fields(&carve));
        node.insert("children".into(), array(table_sections(&sections, rows_of)));
        let mut columns: Vec<Value> = match node.get("columns") {
            Some(Value::Array(c)) => c.iter().take(width).cloned().collect(),
            Some(Value::Object(c)) => c.values().take(width).cloned().collect(),
            _ => Vec::new(),
        };
        while columns.len() < width {
            columns.push(Value::Object(typed("table_column")));
        }
        node.insert("columns".into(), Value::Array(columns));
        Ok(node)
    }

    /// Saves the rows into `rows_of`; returns the width of the widest row. A
    /// span becomes the cell's counts and the mark cells it covers (C2-B-172
    /// to C2-B-174).
    fn span_rows(
        &mut self,
        placed: &[Row],
        bare: bool,
        rows_of: &mut [Vec<Node>],
    ) -> Result<usize> {
        let mut below: BTreeMap<usize, i64> = BTreeMap::new();
        let mut width = 0;
        for (index, row) in placed.iter().enumerate() {
            if index > 0
                && placed
                    .get(index - 1)
                    .is_some_and(|p| p.section != row.section)
            {
                below.clear();
            }
            let left = placed
                .iter()
                .skip(index)
                .take_while(|p| p.section == row.section)
                .count();
            let left = i64::try_from(left).unwrap_or(i64::MAX);
            let mut cells: Vec<Node> = Vec::new();
            let mut column = 0usize;
            for (at, pm_cell) in row.cells.iter().enumerate() {
                covered(&below, &mut column, &mut cells, bare);
                let cell_pointer = row.cell_pointers.get(at).cloned().unwrap_or_default();
                let cell = self.cell(pm_cell, &cell_pointer)?;
                let (colspan, rowspan) = spanned(cell, pm_cell, left, bare, &mut cells);
                if rowspan > 1 {
                    for offset in 0..colspan {
                        below.insert(column + offset, rowspan);
                    }
                }
                column += colspan;
            }
            covered(&below, &mut column, &mut cells, bare);
            width = width.max(column);
            for v in below.values_mut() {
                *v = (*v - 1).max(0);
            }
            let Some(out) = rows_of.get_mut(row.section) else {
                continue;
            };
            for comment in kit::nodes(row.carve.get("$before")) {
                out.push(comment);
            }
            let mut table_row = with(typed("table_row"), tree_fields(&row.carve));
            kit::or_insert(&mut table_row, "children", array(cells));
            kit::or_insert(&mut table_row, POINTER, Value::String(row.pointer.clone()));
            out.push(table_row);
        }
        Ok(width)
    }

    fn cell(&mut self, pm: &Node, pointer: &str) -> Result<Node> {
        let content = content(pm);
        let single = content.iter().filter(|c| !is_placeholder(c)).count() == 1;
        let mut children = Vec::new();
        for (at, child) in content.iter().enumerate() {
            if is_placeholder(child) {
                continue;
            }
            let Some(mut node) = self.block(child, &format!("{pointer}/content/{at}"))? else {
                continue;
            };
            let inline = matches!(type_of(child).as_str(), "paragraph" | "image");
            if inline && !carve_of(child).contains_key("$type") && single {
                node.insert("type".into(), Value::String("cell_paragraph".into()));
                node.remove("role");
            }
            children.push(node);
        }
        // A cell the editor created empty still needs content; an empty cell reads as one empty cell paragraph.
        if children.is_empty() && carve_of(pm).get("$empty") != Some(&Value::Bool(true)) {
            let mut p = typed("cell_paragraph");
            p.insert("children".into(), Value::Array(Vec::new()));
            children.push(p);
        }
        let mut fields = tree_fields(&carve_of(pm));
        fields.remove("colspan");
        fields.remove("rowspan");
        let mut node = with(typed("table_cell"), fields);
        node.insert("header".into(), Value::Bool(type_of(pm) == "tableHeader"));
        node.insert("children".into(), array(children));
        node.insert(POINTER.into(), Value::String(pointer.into()));
        Ok(node)
    }

    /// Rebuilds the nested inline containers from the marks of each leaf: a
    /// longer run opens outside a shorter one.
    fn inlines(&mut self, content: &[Node], base: &str) -> Result<Vec<Node>> {
        let mut leaves = Vec::with_capacity(content.len());
        for (at, pm) in content.iter().enumerate() {
            let pointer = format!("{base}/content/{at}");
            let mut marks = Vec::new();
            let mut mark_pointers = Vec::new();
            for (index, mark) in kit::nodes(pm.get("marks")).into_iter().enumerate() {
                if type_of(&mark) != "code" {
                    mark_pointers.push(format!("{pointer}/marks/{index}"));
                    marks.push(mark);
                }
            }
            let keys = marks.iter().map(mark_key).collect();
            leaves.push(Leaf {
                pm: pm.clone(),
                pointer,
                marks,
                mark_pointers,
                keys,
            });
        }
        let mut runs: Vec<BTreeMap<String, usize>> = vec![BTreeMap::new(); leaves.len()];
        for index in (0..leaves.len()).rev() {
            let next = runs.get(index + 1).cloned().unwrap_or_default();
            if let (Some(leaf), Some(run)) = (leaves.get(index), runs.get_mut(index)) {
                for key in &leaf.keys {
                    run.insert(key.clone(), 1 + next.get(key).copied().unwrap_or(0));
                }
            }
        }
        // The nodes in order of creation; a container's children are the ids under it in kids, None is the root.
        let mut built: Vec<Node> = Vec::new();
        let mut kids: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
        kids.insert(None, Vec::new());
        let mut stack: Vec<(String, usize)> = Vec::new();
        let mapping = Arc::clone(&self.mapping);
        for (index, leaf) in leaves.iter().enumerate() {
            let keep = stack
                .iter()
                .take_while(|(key, _)| leaf.keys.contains(key))
                .count();
            stack.truncate(keep);
            let opened: Vec<String> = stack.iter().map(|(k, _)| k.clone()).collect();
            let run = runs.get(index).cloned().unwrap_or_default();
            for entry in fresh(leaf, &opened, &run) {
                let id = built.len();
                let mark = leaf.marks.get(entry.order).cloned().unwrap_or_default();
                let node = self.mark_node(&mark)?;
                let pointer = leaf
                    .mark_pointers
                    .get(entry.order)
                    .cloned()
                    .unwrap_or_default();
                built.push(tag(node, &pointer));
                kids.entry(stack.last().map(|(_, id)| *id))
                    .or_default()
                    .push(id);
                kids.insert(Some(id), Vec::new());
                stack.push((entry.key, id));
            }
            let id = built.len();
            let node = match mapping.node(&type_of(&leaf.pm)) {
                Some(host) => self.save_mapped(&leaf.pm, host, Position::Inline, &leaf.pointer)?,
                None => leaf_node(&leaf.pm)?,
            };
            built.push(tag(node, &leaf.pointer));
            kids.entry(stack.last().map(|(_, id)| *id))
                .or_default()
                .push(id);
        }
        Ok(assemble(&built, &kids, None))
    }

    fn mark_node(&mut self, mark: &Node) -> Result<Node> {
        let ty = type_of(mark);
        let mapping = Arc::clone(&self.mapping);
        if let Some(host) = mapping.mark(&ty) {
            return self.save_mapped_mark(mark, host);
        }
        let Some((tree, owned)) = kit::by_name(kit::MARKS, &ty) else {
            return Err(invalid(format!(
                "ProseMirrorToCarve: no Carve node for the ProseMirror mark \"{ty}\""
            )));
        };
        let mut node = with(typed(tree), tree_fields(&carve_of(mark)));
        let attrs = attrs_of(mark);
        for key in owned {
            if kit::isset(&attrs, key) {
                node.insert(
                    (*key).into(),
                    attrs.get(*key).cloned().unwrap_or(Value::Null),
                );
            }
        }
        node.insert("children".into(), Value::Array(Vec::new()));
        Ok(node)
    }

    fn save_mapped_mark(&mut self, mark: &Node, entry: &MappingEntry) -> Result<Node> {
        match entry {
            MappingEntry::Declared {
                class, attributes, ..
            } => {
                let mut node = typed("span");
                node.insert(
                    "attrs".into(),
                    Value::Object(mapped_attrs(class, attributes, &attrs_of(mark))),
                );
                node.insert("children".into(), Value::Array(Vec::new()));
                Ok(node)
            }
            MappingEntry::Callback { to_tree, .. } => {
                let Some(to_tree) = to_tree else {
                    return Err(invalid(format!(
                        "ProseMirrorToCarve: the mapping of the ProseMirror mark \"{}\" has no toTree",
                        type_of(mark)
                    )));
                };
                let mut ctx = Ctx {
                    saver: self,
                    position: Position::Inline,
                    pointer: String::new(),
                };
                let mut node = kit::map(Some(&to_tree(&Value::Object(mark.clone()), &mut ctx)?));
                node.insert("children".into(), Value::Array(Vec::new()));
                Ok(node)
            }
        }
    }
}

/// The callback context of the saver: `blocks` and `inlines` save
/// ProseMirror content.
struct Ctx<'s> {
    saver: &'s mut Saver,
    position: Position,
    pointer: String,
}

impl Context for Ctx<'_> {
    fn position(&self) -> Position {
        self.position
    }

    fn blocks(&mut self, children: &Value) -> Result<Vec<Value>> {
        let nodes = kit::nodes(Some(children));
        Ok(self
            .saver
            .blocks(&nodes, &self.pointer, 0)?
            .into_iter()
            .map(Value::Object)
            .collect())
    }

    fn inlines(&mut self, children: &Value) -> Result<Vec<Value>> {
        let nodes = kit::nodes(Some(children));
        Ok(self
            .saver
            .inlines(&nodes, &self.pointer)?
            .into_iter()
            .map(Value::Object)
            .collect())
    }
}

struct Row {
    carve: Node,
    cells: Vec<Node>,
    pointer: String,
    cell_pointers: Vec<String>,
    section: usize,
}

struct Leaf {
    pm: Node,
    pointer: String,
    marks: Vec<Node>,
    mark_pointers: Vec<String>,
    keys: Vec<String>,
}

struct Fresh {
    order: usize,
    key: String,
}

fn content(pm: &Node) -> Vec<Node> {
    kit::nodes(pm.get("content"))
}

fn tag(mut node: Node, pointer: &str) -> Node {
    kit::or_insert(&mut node, POINTER, Value::String(pointer.into()));
    node
}

/// An empty paragraph with nothing carried is editor state (a placeholder, a
/// fresh line), not content.
fn is_placeholder(pm: &Node) -> bool {
    if type_of(pm) != "paragraph" || !content(pm).is_empty() {
        return false;
    }
    carve_of(pm).keys().all(|k| k == "$index")
}

/// Whether `pm` holds anything but placeholders.
fn filled(pm: &Node) -> bool {
    content(pm).iter().any(|c| !is_placeholder(c))
}

fn text_of(pm: &Node) -> String {
    content(pm).iter().map(|c| str_of(c.get("text"))).collect()
}

fn code_block(pm: &Node) -> Node {
    let carve = carve_of(pm);
    let mut node = with(typed("code_block"), tree_fields(&carve));
    let text = text_of(pm);
    let keep = carve.get("$newline") == Some(&Value::Bool(false)) || text.ends_with('\n');
    let value = if keep { text } else { format!("{text}\n") };
    node.insert("value".into(), Value::String(value));
    if let Some(Value::String(language)) = attrs_of(pm).get("language")
        && !language.is_empty()
    {
        node.insert("syntax".into(), Value::String(language.clone()));
    }
    node
}

fn task_checked(task: Option<&Value>) -> bool {
    matches!(
        task.and_then(|t| t.get("mark")).and_then(Value::as_str),
        Some("x" | "X")
    )
}

fn marker(pm: &Node, marker: Option<&Value>) -> Map<String, Value> {
    let ty = type_of(pm);
    let wanted = match ty.as_str() {
        "orderedList" => Some("ordered"),
        "bulletList" => Some("bullet"),
        _ => None,
    };
    let mut marker = kit::map(marker);
    if marker.is_empty()
        || wanted.is_some_and(|w| marker.get("kind").and_then(Value::as_str) != Some(w))
    {
        marker = default_marker(&ty);
    }
    let start = attrs_of(pm).get("start").cloned();
    if ty == "orderedList"
        && let Some(start) = start.filter(|s| s.is_i64() || s.is_u64())
        && Some(&start)
            != Some(
                marker
                    .get("start")
                    .filter(|v| !v.is_null())
                    .unwrap_or(&Value::from(1)),
            )
    {
        marker.insert("start".into(), start);
    }
    marker
}

fn default_marker(ty: &str) -> Map<String, Value> {
    let pairs: &[(&str, &str)] = if ty == "orderedList" {
        &[("kind", "ordered"), ("style", "1"), ("delimiter", ".")]
    } else {
        &[("kind", "bullet"), ("char", "-")]
    };
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), Value::String((*v).to_string())))
        .collect()
}

fn section_type(section: &Node) -> Option<&str> {
    section.get("type").and_then(Value::as_str)
}

/// The index of the first entry `test` accepts, or -1.
fn find(list: &[Node], test: impl Fn(&Node, i64) -> bool) -> i64 {
    list.iter()
        .enumerate()
        .find(|(i, s)| test(s, i64::try_from(*i).unwrap_or(i64::MAX)))
        .map_or(-1, |(i, _)| i64::try_from(i).unwrap_or(-1))
}

fn body_after(sections: &[Node], index: i64) -> i64 {
    find(sections, |s, i| {
        i > index && section_type(s) == Some("table_body")
    })
}

fn header_row(cells: &[Node]) -> bool {
    !cells.is_empty() && cells.iter().all(|c| type_of(c) == "tableHeader")
}

/// C2-B-178: in a table without a head, the number of leading rows of header
/// cells, which are the head. A row loaded so from a body stays there.
fn leading_head(sections: &[Node], rows: &[Row]) -> usize {
    if find(sections, |s, _| section_type(s) == Some("table_head")) >= 0 {
        return 0;
    }
    rows.iter()
        .take_while(|row| {
            row.carve.get("$th") != Some(&Value::Bool(true)) && header_row(&row.cells)
        })
        .count()
}

/// The section each row goes to: a loaded row its own, a new row that of
/// the row before it.
fn place(rows: &mut [Row], sections: &[Node], lead: usize, shift: i64, added: i64, head: i64) {
    let mut current = find(sections, |s, _| section_type(s) == Some("table_body"));
    if current < 0 {
        current = find(sections, |s, _| !s.contains_key("$node"));
    }
    let count = i64::try_from(sections.len()).unwrap_or(i64::MAX);
    for (index, row) in rows.iter_mut().enumerate() {
        let mut from = row
            .carve
            .get("$section")
            .filter(|v| v.is_i64() || v.is_u64())
            .and_then(Value::as_i64);
        if let Some(f) = from.as_mut() {
            if shift >= 0 && *f >= shift {
                *f += 1;
            }
            if added >= 0 && *f >= added {
                *f += 1;
            }
        }
        if index < lead {
            current = head;
        } else if let Some(f) = from.filter(|f| *f >= 0 && *f < count) {
            current = f;
        } else if usize::try_from(current)
            .ok()
            .and_then(|c| sections.get(c))
            .is_some_and(|s| section_type(s) == Some("table_head"))
            && !header_row(&row.cells)
        {
            current = body_after(sections, current);
        }
        // A row whose section is -1 has no place; PHP appends it under key -1, which no section reads.
        row.section = usize::try_from(current).unwrap_or(usize::MAX);
    }
}

/// Steps `column` past the columns a row span from above covers, with a `^`
/// mark cell for each.
fn covered(below: &BTreeMap<usize, i64>, column: &mut usize, cells: &mut Vec<Node>, bare: bool) {
    while below.get(column).copied().unwrap_or(0) > 0 {
        if !bare {
            cells.push(mark_cell("rowspan"));
        }
        *column += 1;
    }
}

/// Pushes `cell` with its counts and, unless the spans are bare, the `<`
/// mark cells it covers; returns its columns and the rows it spans.
fn spanned(
    mut cell: Node,
    pm_cell: &Node,
    left: i64,
    bare: bool,
    cells: &mut Vec<Node>,
) -> (usize, i64) {
    let attrs = attrs_of(pm_cell);
    let colspan = kit::span_count(attrs.get("colspan"));
    let mut rowspan = kit::span_count(attrs.get("rowspan")).min(left);
    if colspan > 1 && rowspan > 1 && !bare {
        rowspan = 1;
    }
    if colspan > 1 {
        cell.insert("colspan".into(), Value::from(colspan));
    }
    if rowspan > 1 {
        cell.insert("rowspan".into(), Value::from(rowspan));
    }
    cells.push(cell);
    let colspan = usize::try_from(colspan).unwrap_or(1);
    if !bare {
        for _ in 1..colspan {
            cells.push(mark_cell("colspan"));
        }
    }
    (colspan, rowspan)
}

fn mark_cell(span: &str) -> Node {
    let mut cell = typed("table_cell");
    cell.insert("header".into(), Value::Bool(false));
    cell.insert("span".into(), Value::String(span.into()));
    cell.insert("children".into(), Value::Array(Vec::new()));
    cell
}

fn table_sections(sections: &[Node], rows_of: Vec<Vec<Node>>) -> Vec<Node> {
    let mut children = Vec::new();
    for (section, mut rows) in sections.iter().zip(rows_of) {
        if let Some(node) = section.get("$node") {
            children.push(kit::map(Some(node)));
            continue;
        }
        let has_row = rows.iter().any(|r| type_of(r) == "table_row");
        if !has_row && section.get("$empty") != Some(&Value::Bool(true)) {
            continue;
        }
        rows.extend(kit::nodes(section.get("$after")));
        let mut node = tree_fields(section);
        node.insert(
            "type".into(),
            section
                .get("type")
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or(Value::String("table_body".into())),
        );
        node.insert("children".into(), array(rows));
        children.push(node);
    }
    children
}

/// A mark's identity: the loaded node it came from, or for a mark the editor
/// added, its type and attributes.
fn mark_key(mark: &Node) -> String {
    match carve_of(mark).get("$id") {
        Some(Value::Null) | None => format!("{}:{}", type_of(mark), Value::Object(attrs_of(mark))),
        Some(Value::String(s)) => format!("#{s}"),
        Some(Value::Number(n)) => format!("#{n}"),
        Some(Value::Bool(b)) => format!("#{}", if *b { "1" } else { "" }),
        Some(other) => format!("#{{\"id\":{other}}}"),
    }
}

/// The marks of `leaf` not open yet, in the order they open: the longest run
/// first, then the loaded depth.
fn fresh(leaf: &Leaf, opened: &[String], run: &BTreeMap<String, usize>) -> Vec<Fresh> {
    let mut out: Vec<(Fresh, f64)> = Vec::new();
    for (order, mark) in leaf.marks.iter().enumerate() {
        let Some(key) = leaf.keys.get(order) else {
            continue;
        };
        if opened.contains(key) {
            continue;
        }
        let depth = carve_of(mark)
            .get("$depth")
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY);
        out.push((
            Fresh {
                order,
                key: key.clone(),
            },
            depth,
        ));
    }
    out.sort_by(|(a, da), (b, db)| {
        let ra = run.get(&a.key).copied().unwrap_or(0);
        let rb = run.get(&b.key).copied().unwrap_or(0);
        rb.cmp(&ra)
            .then(da.total_cmp(db))
            .then(a.order.cmp(&b.order))
    });
    out.into_iter().map(|(f, _)| f).collect()
}

fn assemble(
    nodes: &[Node],
    kids: &BTreeMap<Option<usize>, Vec<usize>>,
    parent: Option<usize>,
) -> Vec<Node> {
    let mut out = Vec::new();
    for &id in kids.get(&parent).map_or(&[][..], Vec::as_slice) {
        let Some(node) = nodes.get(id) else { continue };
        let mut node = node.clone();
        if kids.contains_key(&Some(id)) {
            node.insert("children".into(), array(assemble(nodes, kids, Some(id))));
        }
        out.push(node);
    }
    out
}

fn leaf_node(pm: &Node) -> Result<Node> {
    let carve = carve_of(pm);
    let attrs = attrs_of(pm);
    let ty = type_of(pm);
    Ok(match ty.as_str() {
        "text" => text(pm),
        "hardBreak" => with(typed("hard_break"), tree_fields(&carve)),
        "carveSoftBreak" => with(typed("soft_break"), tree_fields(&carve)),
        "image" => image(pm),
        "carveMath" => {
            let mut node = with(typed("math"), tree_fields(&carve));
            kit::or_insert(
                &mut node,
                "display",
                Value::Bool(attrs.get("display") == Some(&Value::Bool(true))),
            );
            kit::or_insert(
                &mut node,
                "value",
                Value::String(str_of(attrs.get("value"))),
            );
            node
        }
        "carveFootnoteRef" => {
            let mut node = with(typed("footnote_ref"), tree_fields(&carve));
            kit::or_insert(&mut node, "key", Value::String(str_of(attrs.get("key"))));
            kit::or_insert(&mut node, "namespace", Value::String("footnote".into()));
            node
        }
        "carveInline" => kit::map(Some(&clean(&Value::Object(carve)))),
        _ => {
            return Err(invalid(format!(
                "ProseMirrorToCarve: no Carve node for the ProseMirror inline \"{ty}\""
            )));
        }
    })
}

fn text(pm: &Node) -> Node {
    let value = Value::String(str_of(pm.get("text")));
    for mark in kit::nodes(pm.get("marks")) {
        if type_of(&mark) == "code" {
            let mut node = with(typed("code"), tree_fields(&carve_of(&mark)));
            kit::or_insert(&mut node, "value", value);
            return node;
        }
    }
    let mut node = typed("text");
    node.insert("value".into(), value);
    node
}

fn image(pm: &Node) -> Node {
    let attrs = attrs_of(pm);
    let mut node = with(typed("image"), tree_fields(&carve_of(pm)));
    for (attr, key) in [("src", "src"), ("alt", "alt"), ("title", "tooltip")] {
        if let Some(Value::String(s)) = attrs.get(attr) {
            node.insert(key.into(), Value::String(s.clone()));
        }
    }
    // An editor image with no alt text is what `![](src)` parses to; the tree
    // schema wants `alt` unless the image is decorative.
    if !node.contains_key("alt") && node.get("decorative") != Some(&Value::Bool(true)) {
        node.insert("alt".into(), Value::String(String::new()));
    }
    node
}

/// The Carve attributes of a declared node or mark: its class, and each
/// declared attribute with a value as a key/value pair.
fn mapped_attrs(class: &str, attributes: &[String], attrs: &Node) -> Map<String, Value> {
    let mut key_values = Vec::new();
    for name in attributes {
        if let Some(value) = attrs.get(name).filter(|v| !v.is_null()) {
            key_values.push(Value::Array(vec![
                Value::String(name.clone()),
                Value::String(kit::attribute_text(value)),
            ]));
        }
    }
    let mut out = Map::new();
    out.insert(
        "classes".into(),
        Value::Array(vec![Value::String(class.into())]),
    );
    if !key_values.is_empty() {
        out.insert("keyValues".into(), Value::Array(key_values));
    }
    out
}

/// The tree with the role on every paragraph that its content gives on
/// read-back (C2-B-157, C2-B-158). The saved tree leaves `role` as the kit
/// does; the source must not lose it.
pub(super) fn with_roles(node: &mut Value) {
    match node {
        Value::Object(map) => {
            for value in map.values_mut() {
                with_roles(value);
            }
            if map.get("type").and_then(Value::as_str) == Some("paragraph") {
                with_role(map);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(with_roles),
        _ => {}
    }
}

/// A paragraph that holds one image or one display math, and no other node
/// but blank text, reads as an image or math paragraph.
fn with_role(node: &mut Node) {
    node.remove("role");
    let kept: Vec<&Value> = match node.get("children") {
        Some(Value::Array(children)) => children
            .iter()
            .filter(|child| {
                let blank = child.get("type").and_then(Value::as_str) == Some("text")
                    && child
                        .get("value")
                        .and_then(Value::as_str)
                        .is_some_and(|v| v.trim_matches([' ', '\t']).is_empty());
                !blank
            })
            .collect(),
        _ => Vec::new(),
    };
    let role = match kept.as_slice() {
        [only] => match only.get("type").and_then(Value::as_str) {
            Some("image") => Some("image"),
            Some("math") if only.get("display") == Some(&Value::Bool(true)) => Some("math"),
            _ => None,
        },
        _ => None,
    };
    if let Some(role) = role {
        node.insert("type".into(), Value::String("paragraph".into()));
        node.insert("role".into(), Value::String(role.into()));
    }
}
