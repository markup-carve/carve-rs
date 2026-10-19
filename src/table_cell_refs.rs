use std::cell::RefCell;
use std::collections::HashMap;

use crate::ast::*;
use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};
use crate::table_spans::TableSpanPlan;

pub(crate) fn canonical_source(
    target: &str,
    row: &TableSelector,
    column: &TableSelector,
) -> String {
    fn selector(value: &TableSelector) -> String {
        match value {
            TableSelector::Index(n) => n.to_string(),
            TableSelector::Name(name) => {
                format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
            }
        }
    }
    format!("@{{#{target}[{}][{}]}}", selector(row), selector(column))
}

struct Prepared {
    table: Table,
    grid: Vec<Vec<Option<(usize, usize)>>>,
    body: Vec<usize>,
    head: usize,
}

impl Prepared {
    fn new(table: Table) -> Self {
        let plan = TableSpanPlan::new(&table);
        let mut grid: Vec<Vec<Option<(usize, usize)>>> =
            table.rows.iter().map(|_| Vec::new()).collect();
        for r in 0..table.rows.len() {
            for c in 0..table.rows[r].cells.len() {
                let entry = plan.entry(r, c);
                if entry.skip {
                    continue;
                }
                for rr in r..r + entry.rowspan {
                    if rr >= grid.len() {
                        break;
                    }
                    for cc in c..c + entry.colspan {
                        if grid[rr].len() <= cc {
                            grid[rr].resize(cc + 1, None);
                        }
                        grid[rr][cc] = Some((r, c));
                    }
                }
            }
        }
        let head = table.row_groups.as_ref().map_or_else(
            || {
                table
                    .rows
                    .iter()
                    .take_while(|row| row.cells.iter().all(|cell| cell.header))
                    .count()
            },
            |groups| groups.head_rows,
        );
        let mut body = Vec::new();
        if let Some(groups) = &table.row_groups {
            let mut offset = head;
            for group in &groups.bodies {
                offset += group.head_rows;
                body.extend(offset..offset + group.body_rows);
                offset += group.body_rows;
            }
        } else {
            for r in head..table.rows.len() {
                if !table.rows[r].cells.iter().all(|cell| cell.header) {
                    body.push(r);
                }
            }
        }
        Self {
            table,
            grid,
            body,
            head,
        }
    }

    fn cell(&self, row: usize, col: usize) -> Option<&TableCell> {
        let (r, c) = self.grid.get(row)?.get(col)?.as_ref()?;
        self.table.rows.get(*r)?.cells.get(*c)
    }
}

#[derive(Default)]
struct Resolver {
    tables: HashMap<String, Option<Prepared>>,
    values: RefCell<HashMap<(usize, bool), Option<String>>>,
    keys: RefCell<HashMap<String, Option<HashMap<String, usize>>>>,
    cycles: RefCell<std::collections::HashSet<usize>>,
}

impl Resolver {
    fn register(&mut self, table: &Table) {
        if let Some(id) = table.attrs.as_ref().and_then(|attrs| attrs.id.as_ref()) {
            if self.tables.contains_key(id) {
                self.tables.insert(id.clone(), None);
            } else {
                self.tables
                    .insert(id.clone(), Some(Prepared::new(table.clone())));
            }
        }
    }

    fn plain(&self, nodes: &[InlineNode], active: &mut Vec<usize>, source: bool) -> Option<String> {
        let mut value = String::new();
        for node in nodes {
            match node {
                InlineNode::Text(n) => value.push_str(&n.value),
                InlineNode::EscapedText(n) => value.push_str(&n.value),
                InlineNode::SmartPunctuation(n) => value.push_str(if source {
                    &n.value
                } else {
                    smart_punctuation_glyph(n)
                }),
                InlineNode::Code(n) => value.push_str(&n.value),
                InlineNode::Image(n) => value.push_str(&n.alt),
                InlineNode::Link(n) => value.push_str(&self.plain(&n.children, active, source)?),
                InlineNode::Emphasis(n) => {
                    value.push_str(&self.plain(&n.children, active, source)?)
                }
                InlineNode::Span(n) => value.push_str(&self.plain(&n.children, active, source)?),
                InlineNode::TableCellRef(n) => value.push_str(&self.resolve(n, active)?),
                InlineNode::SoftBreak(_) | InlineNode::CellWrap(_) => value.push(' '),
                InlineNode::HardBreak(_) => value.push('\n'),
                InlineNode::NonBreakingSpace(_) => value.push('\u{a0}'),
                InlineNode::AutoLink(n) => value.push_str(&n.text),
                InlineNode::Mention(n) => {
                    value.push('@');
                    value.push_str(&n.user);
                }
                InlineNode::Tag(n) => {
                    value.push('#');
                    value.push_str(&n.name);
                }
                InlineNode::Abbreviation(n) => value.push_str(&n.abbr),
                InlineNode::LiteralInline(n) => value.push_str(&n.content),
                InlineNode::Math(n) => {
                    value.push_str(&n.content);
                    if let (Some(label), Some(number)) = (&n.label, n.number) {
                        value.push(' ');
                        value.push_str(label);
                        value.push(' ');
                        value.push_str(&number.to_string());
                    }
                }
                InlineNode::Symbol(n) => {
                    value.push(':');
                    value.push_str(&n.name);
                    value.push(':');
                }
                InlineNode::Ruby(n) => {
                    for pair in &n.pairs {
                        value.push_str(&self.plain(&pair.base, active, source)?);
                        value.push('(');
                        value.push_str(&self.plain(&pair.annotation, active, source)?);
                        value.push(')');
                    }
                }
                InlineNode::Extension(n) => {
                    value.push_str(&self.plain(&n.children, active, source)?)
                }
                InlineNode::CriticInsert(n) => {
                    value.push_str(&self.plain(&n.children, active, source)?)
                }
                InlineNode::CriticDelete(n) => {
                    value.push('~');
                    value.push_str(&self.plain(&n.children, active, source)?);
                    value.push('~');
                }
                InlineNode::CriticSubstitute(n) => {
                    value.push('~');
                    value.push_str(&self.plain(&n.old, active, source)?);
                    value.push('~');
                    value.push_str(&self.plain(&n.new, active, source)?);
                }
                InlineNode::CriticComment(n) => value.push_str(&n.text),
                InlineNode::Comment(_) | InlineNode::RawInline(_) => {}
                InlineNode::CitationGroup(n) => value.push_str(&n.raw),
                InlineNode::Footnote(n) => {
                    if let Some(inline) = &n.inline {
                        value.push('(');
                        value.push_str(&self.plain(inline, active, source)?);
                        value.push(')');
                    } else if let Some(id) = &n.id {
                        value.push_str(&format!("[^{id}]"));
                    }
                }
                InlineNode::CrossRef(n) => value.push_str(&format!("</#{}>", n.target)),
                InlineNode::CaptionNumber(n) => value.push_str(
                    &n.number
                        .map_or_else(|| "#".to_string(), |number| number.to_string()),
                ),
            }
        }
        Some(value)
    }

    fn cell_value(
        &self,
        table: &Prepared,
        row: usize,
        col: usize,
        active: &mut Vec<usize>,
        source: bool,
    ) -> Option<String> {
        let cell = table.cell(row, col)?;
        if cell.blocks.is_some() {
            return None;
        }
        let key = (cell as *const TableCell as usize, source);
        if let Some(value) = self.values.borrow().get(&key) {
            return value.clone();
        }
        let value = self.plain(&cell.children, active, source);
        self.values.borrow_mut().insert(key, value.clone());
        value
    }

    fn column(
        &self,
        table: &Prepared,
        selector: &TableSelector,
        active: &mut Vec<usize>,
    ) -> Option<usize> {
        match selector {
            TableSelector::Index(n) => n.checked_sub(1),
            TableSelector::Name(name) if !name.is_empty() && table.head > 0 => {
                let mut found = None;
                for col in 0..table.grid[table.head - 1].len() {
                    let Some(cell) = table.cell(table.head - 1, col) else {
                        continue;
                    };
                    let key = cell as *const TableCell as usize;
                    if active.contains(&key) {
                        continue;
                    }
                    active.push(key);
                    let value = self.cell_value(table, table.head - 1, col, active, true);
                    active.pop();
                    if value.as_deref() == Some(name) {
                        if found.is_some() {
                            return None;
                        }
                        found = Some(col);
                    }
                }
                found
            }
            _ => None,
        }
    }

    fn resolve(&self, reference: &TableCellRef, active: &mut Vec<usize>) -> Option<String> {
        let table = self.tables.get(&reference.target)?.as_ref()?;
        let col = self.column(table, &reference.column, active)?;
        let row = match &reference.row {
            TableSelector::Index(n) => *table.body.get(n.checked_sub(1)?)?,
            TableSelector::Name(name) => {
                let cached_row = self
                    .keys
                    .borrow()
                    .get(&reference.target)
                    .map(|cached| cached.as_ref().and_then(|rows| rows.get(name).copied()));
                if let Some(cached) = cached_row {
                    return cached.and_then(|r| {
                        let cell = table.cell(r, col)?;
                        let key = cell as *const TableCell as usize;
                        if active.contains(&key) {
                            let mut cycles = self.cycles.borrow_mut();
                            cycles.insert(key);
                            cycles.extend(active.iter().copied());
                            return None;
                        }
                        active.push(key);
                        let value = self.cell_value(table, r, col, active, false);
                        active.pop();
                        value
                    });
                }
                let key_col = match table
                    .table
                    .attrs
                    .as_ref()
                    .and_then(|a| a.key_values.get("row-key"))
                {
                    Some(header) => {
                        self.column(table, &TableSelector::Name(header.clone()), active)?
                    }
                    None => 0,
                };
                let mut keys = HashMap::new();
                for &r in &table.body {
                    let cell = table.cell(r, key_col)?;
                    if cell.span.is_some()
                        || cell.colspan.unwrap_or(1) > 1
                        || cell.rowspan.unwrap_or(1) > 1
                        || !cell.children.iter().all(|n| {
                            matches!(
                                n,
                                InlineNode::Text(_)
                                    | InlineNode::EscapedText(_)
                                    | InlineNode::SmartPunctuation(_)
                            )
                        })
                    {
                        self.keys
                            .borrow_mut()
                            .insert(reference.target.clone(), None);
                        return None;
                    }
                    let value = self.cell_value(table, r, key_col, active, true)?;
                    if value.is_empty() || keys.insert(value.clone(), r).is_some() {
                        self.keys
                            .borrow_mut()
                            .insert(reference.target.clone(), None);
                        return None;
                    }
                }
                let found = keys.get(name).copied();
                self.keys
                    .borrow_mut()
                    .insert(reference.target.clone(), Some(keys));
                found?
            }
        };
        let cell = table.cell(row, col)?;
        let key = cell as *const TableCell as usize;
        if active.contains(&key) {
            let mut cycles = self.cycles.borrow_mut();
            cycles.insert(key);
            cycles.extend(active.iter().copied());
            return None;
        }
        active.push(key);
        let result = self.cell_value(table, row, col, active, false);
        active.pop();
        result
    }

    fn reason(&self, reference: &TableCellRef) -> &'static str {
        let Some(entry) = self.tables.get(&reference.target) else {
            return "table id is absent";
        };
        let Some(table) = entry else {
            return "table id is ambiguous";
        };
        let mut active = Vec::new();
        let Some(col) = self.column(table, &reference.column, &mut active) else {
            return "column is absent or ambiguous";
        };
        let row = match &reference.row {
            TableSelector::Index(n) => match n.checked_sub(1).and_then(|i| table.body.get(i)) {
                Some(&row) => row,
                None => return "body row is out of range",
            },
            TableSelector::Name(name) => {
                if table
                    .table
                    .attrs
                    .as_ref()
                    .and_then(|a| a.key_values.get("row-key"))
                    .is_some_and(|header| {
                        self.column(table, &TableSelector::Name(header.clone()), &mut active)
                            .is_none()
                    })
                {
                    return "row-key column is absent or ambiguous";
                }
                match self.keys.borrow().get(&reference.target) {
                    Some(None) => return "row keys are invalid or duplicate",
                    Some(Some(rows)) => match rows.get(name) {
                        Some(&row) => row,
                        None => return "row key is absent",
                    },
                    None => return "row keys could not resolve",
                }
            }
        };
        let Some(cell) = table.cell(row, col) else {
            return "cell is absent";
        };
        if cell.blocks.is_some() {
            return "cell contains blocks";
        }
        if self
            .cycles
            .borrow()
            .contains(&(cell as *const TableCell as usize))
        {
            return "cell references form a cycle";
        }
        "nested cell value could not resolve"
    }
}

impl SubtreeVisitor for Resolver {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for block in blocks {
            match block {
                BlockNode::Table(table) => self.register(table),
                BlockNode::Figure(figure) => {
                    if let FigureTarget::Table(table) = &*figure.target {
                        self.register(table);
                    }
                }
                _ => {}
            }
            visit_block_children(block, self);
        }
    }
    fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
        for node in nodes {
            visit_inline_children(node, self);
        }
    }
}

pub(crate) fn resolve(doc: &mut Document) {
    if crate::render_depth::refuse_if_too_deep(doc, "table_cell_ref").is_err() {
        return;
    }
    let mut collect = Resolver::default();
    collect.blocks(&mut doc.children);
    for body in doc.footnote_defs.values_mut() {
        collect.blocks(body);
    }
    let mut apply = Apply { resolver: &collect };
    apply.blocks(&mut doc.children);
    for body in doc.footnote_defs.values_mut() {
        apply.blocks(body);
    }
}

struct Apply<'a> {
    resolver: &'a Resolver,
}
impl SubtreeVisitor for Apply<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for block in blocks {
            visit_block_children(block, self);
        }
    }
    fn inlines(&mut self, nodes: &mut Vec<InlineNode>) {
        for node in nodes {
            if let InlineNode::TableCellRef(reference) = node {
                reference.value = self.resolver.resolve(reference, &mut Vec::new());
                reference.reason = reference
                    .value
                    .is_none()
                    .then(|| self.resolver.reason(reference));
            }
            visit_inline_children(node, self);
        }
    }
}
