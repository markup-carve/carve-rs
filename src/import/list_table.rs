//! C2-M-112: with `listTable` on, a table whose cells hold blocks a pipe row cannot spell becomes
//! the list table that `carve.listTable` reads (C2-B-188).

use super::dom::Id;
use super::{Code, Cx};
use crate::write::{RowSlot, table_slots};
use serde_json::{Map, Value, json};

const COUNT_KEYS: [&str; 3] = ["header-rows", "header-cols", "footer-rows"];

fn kind(v: &Value) -> &str {
    v["type"].as_str().unwrap_or("")
}

fn children(v: &Value) -> &[Value] {
    v["children"].as_array().map_or(&[], Vec::as_slice)
}

/// C2-M-112: a cell of two or more blocks, or of one block that is not a paragraph.
fn holds_blocks(cell: &Value) -> bool {
    match children(cell) {
        [only] => !matches!(kind(only), "cell_paragraph" | "paragraph"),
        _ => true,
    }
}

fn groups(table: &Value) -> impl Iterator<Item = &Value> {
    children(table).iter().filter(|g| kind(g) != "comment")
}

fn table_rows(group: &Value) -> impl Iterator<Item = &Value> {
    children(group).iter().filter(|r| kind(r) == "table_row")
}

pub(super) fn wants_list_table(table: &Value) -> bool {
    groups(table).any(|g| table_rows(g).any(|r| children(r).iter().any(holds_blocks)))
}

/// A slot of a row: its cell, or the cell a span mark continues (C2-W-145).
struct Slot<'a> {
    cell: &'a Value,
    mark: Option<&'static str>,
}

fn bullet() -> Value {
    json!({ "kind": "bullet", "char": "-" })
}

fn list(spacing: &str, items: Vec<Value>) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), "list".into());
    m.insert("marker".into(), bullet());
    m.insert("spacing".into(), spacing.into());
    m.insert("children".into(), Value::Array(items));
    Value::Object(m)
}

fn text_item(value: &str) -> Value {
    json!({ "type": "list_item", "children": [
        { "type": "paragraph", "children": [{ "type": "text", "value": value }] },
    ] })
}

fn has_facts(v: &Value) -> bool {
    ["attrs", "lang", "dir"].iter().any(|k| v.get(k).is_some())
}

/// The smallest number of leading header slots over the rows below the head, a covered slot
/// counting as its cell.
fn header_columns(rows: &[Vec<Slot<'_>>], head: usize) -> usize {
    rows.iter()
        .skip(head)
        .map(|slots| {
            slots
                .iter()
                .take_while(|s| s.cell["header"].as_bool() == Some(true))
                .count()
        })
        .min()
        .unwrap_or(0)
}

/// C2-M-112: a cell of only the text `<` or `^` is a literal-text span, which C2-X-076 never
/// reads as a mark.
fn literal_mark(blocks: &mut [Value]) {
    let [only] = blocks else { return };
    if kind(only) != "paragraph" {
        return;
    }
    let text = match children(only) {
        [t] if kind(t) == "text" => t["value"].as_str().unwrap_or(""),
        _ => return,
    };
    if text == "<" || text == "^" {
        only["children"] = json!([{ "type": "literal_inline", "value": text }]);
    }
}

impl Cx {
    fn list_cell_item(&mut self, cell: &Value, header: bool, id: Id) -> Value {
        let mut blocks: Vec<Value> = children(cell)
            .iter()
            .filter_map(|block| {
                if kind(block) != "cell_paragraph" {
                    return Some(block.clone());
                }
                // C2-M-069: an empty cell paragraph gives no paragraph.
                let inline = children(block);
                (!inline.is_empty()).then(|| json!({ "type": "paragraph", "children": inline }))
            })
            .collect();
        literal_mark(&mut blocks);
        // C2-M-112: an item that holds no block holds an empty line comment, as `- %%` parses.
        if blocks.is_empty() {
            blocks.push(json!({ "type": "comment", "form": "line", "value": "" }));
        }
        if cell.get("scope").is_some() {
            self.report(
                id,
                Code::AttributeDropped,
                "The scope of a list table cell dropped",
            );
        }
        if cell.get("headerIds").is_some() {
            self.report(
                id,
                Code::AttributeDropped,
                "The headers of a list table cell dropped",
            );
        }
        let mut attrs = cell.get("attrs").cloned();
        let own: Vec<String> = attrs
            .as_ref()
            .and_then(|a| a["keyValues"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|kv| kv[0].as_str().map(str::to_string))
            .collect();
        let mut keys: Vec<Value> = Vec::new();
        if header && !own.iter().any(|k| k == "header") {
            keys.push(json!(["header", ""]));
        }
        for axis in ["align", "valign"] {
            if let Some(v) = cell.get(axis)
                && !own.iter().any(|k| k == axis)
            {
                keys.push(json!([axis, v]));
            }
        }
        if !keys.is_empty() {
            let a = attrs.get_or_insert_with(|| json!({}));
            let kv = a
                .as_object_mut()
                .map(|m| m.entry("keyValues").or_insert_with(|| json!([])));
            if let Some(Value::Array(list)) = kv {
                list.extend(keys);
            }
        }
        let mut m = Map::new();
        m.insert("type".into(), "list_item".into());
        m.insert("children".into(), Value::Array(blocks));
        if let Some(a) = attrs {
            m.insert("attrs".into(), a);
        }
        for key in ["lang", "dir"] {
            if let Some(v) = cell.get(key) {
                m.insert(key.into(), v.clone());
            }
        }
        Value::Object(m)
    }

    fn drop_group_facts(&mut self, table: &Value, id: Id) {
        let mut bodies = 0;
        for group in groups(table) {
            if kind(group) == "table_body" {
                bodies += 1;
            }
            if has_facts(group) {
                let what = kind(group).to_string();
                self.report(
                    id,
                    Code::AttributeDropped,
                    format!("The attributes of a {what} of a list table dropped"),
                );
            }
            for row in table_rows(group) {
                if has_facts(row) {
                    self.report(
                        id,
                        Code::AttributeDropped,
                        "The attributes of a list table row dropped",
                    );
                }
            }
        }
        if bodies > 1 {
            self.report(
                id,
                Code::StructureRepaired,
                "The boundary between two bodies of a list table dropped",
            );
        }
        let widths = table["columns"]
            .as_array()
            .is_some_and(|c| c.iter().any(|col| col.get("width").is_some()));
        if widths {
            self.report(
                id,
                Code::AttributeDropped,
                "A column width of a list table dropped",
            );
        }
    }

    /// The rows of the table that give an item, in group order, each with its slots, and the
    /// rows of them in the head and in the foot.
    fn slot_rows<'a>(&mut self, table: &'a Value, id: Id) -> (Vec<Vec<Slot<'a>>>, usize, usize) {
        for _ in children(table).iter().filter(|g| kind(g) == "comment") {
            self.report(
                id,
                Code::StructureRepaired,
                "A comment between the groups of a list table dropped",
            );
        }
        let (placed, unspelled) = table_slots(table);
        for _ in &unspelled {
            self.report(
                id,
                Code::StructureRepaired,
                "A row span that no span mark of a list table can spell dropped",
            );
        }
        let (mut head, mut foot) = (0, 0);
        let mut rows = Vec::new();
        for (g, r, slots) in placed {
            let group = children(table).get(g).unwrap_or(&Value::Null);
            let row = children(group).get(r).unwrap_or(&Value::Null);
            // C2-M-112: a row with no cell and no covered slot gives no item, and the counts skip it.
            if slots.is_empty() {
                self.report(
                    id,
                    Code::StructureRepaired,
                    "A list table row with no cell dropped",
                );
                continue;
            }
            rows.push(
                slots
                    .into_iter()
                    .map(|slot| match slot {
                        RowSlot::Cell(k) => Slot {
                            cell: children(row).get(k).unwrap_or(&Value::Null),
                            mark: None,
                        },
                        RowSlot::Mark(span, at) => Slot {
                            cell: table.pointer(&at).unwrap_or(&Value::Null),
                            mark: Some(if span == "colspan" { "<" } else { "^" }),
                        },
                    })
                    .collect(),
            );
            match kind(group) {
                "table_head" => head += 1,
                "table_foot" => foot += 1,
                _ => {}
            }
        }
        (rows, head, foot)
    }

    fn div_attrs(&mut self, table: &Value, counts: [(&str, usize); 3], id: Id) -> Value {
        let attrs = &table["attrs"];
        let mut key_values = Vec::new();
        for kv in attrs["keyValues"].as_array().into_iter().flatten() {
            let key = kv[0].as_str().unwrap_or("");
            if COUNT_KEYS.contains(&key) {
                self.report(
                    id,
                    Code::AttributeDropped,
                    format!("A {key} attribute of a list table dropped"),
                );
            } else {
                key_values.push(kv.clone());
            }
        }
        for (key, count) in counts {
            if count > 0 {
                key_values.push(json!([key, count.to_string()]));
            }
        }
        let mut classes = vec![Value::from("list-table")];
        classes.extend(attrs["classes"].as_array().into_iter().flatten().cloned());
        let mut m = Map::new();
        if let Some(i) = attrs.get("id") {
            m.insert("id".into(), i.clone());
        }
        m.insert("classes".into(), Value::Array(classes));
        if !key_values.is_empty() {
            m.insert("keyValues".into(), Value::Array(key_values));
        }
        Value::Object(m)
    }

    /// C2-M-112: the table as a list table: one tight item per row, each a list of one item per
    /// cell slot.
    pub(super) fn list_table(&mut self, table: &Value, id: Id) -> Value {
        self.drop_group_facts(table, id);
        let (rows, head, foot) = self.slot_rows(table, id);
        let header_cols = header_columns(&rows, head);
        let mut items = Vec::with_capacity(rows.len());
        for (index, slots) in rows.iter().enumerate() {
            let cells: Vec<Value> = slots
                .iter()
                .enumerate()
                .map(|(column, slot)| {
                    if let Some(mark) = slot.mark {
                        return text_item(mark);
                    }
                    let header = slot.cell["header"].as_bool() == Some(true)
                        && index >= head
                        && column >= header_cols;
                    self.list_cell_item(slot.cell, header, id)
                })
                .collect();
            // C2-B-062: a list with one item is tight.
            let loose = cells.len() > 1 && cells.iter().any(|item| children(item).len() > 1);
            let inner = list(if loose { "loose" } else { "tight" }, cells);
            items.push(json!({ "type": "list_item", "children": [inner] }));
        }
        let title = table
            .get("caption")
            .and_then(|caption| match children(caption) {
                [only] if kind(only) == "paragraph" => Some(only["children"].clone()),
                _ => None,
            });
        if table.get("caption").is_some() && title.is_none() {
            self.report(
                id,
                Code::ElementDropped,
                "A caption of a list table that is not one paragraph dropped",
            );
        }
        let counts = [
            ("header-rows", head),
            ("header-cols", header_cols),
            ("footer-rows", foot),
        ];
        let attrs = self.div_attrs(table, counts, id);
        let mut m = Map::new();
        m.insert("type".into(), "div".into());
        m.insert("children".into(), json!([list("tight", items)]));
        if let Some(t) = title {
            m.insert("title".into(), t);
        }
        m.insert("attrs".into(), attrs);
        for key in ["lang", "dir"] {
            if let Some(v) = table.get(key) {
                m.insert(key.into(), v.clone());
            }
        }
        Value::Object(m)
    }
}
