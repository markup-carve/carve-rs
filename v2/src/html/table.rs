//! Tables (C2-H-055 to C2-H-061, C2-H-152, C2-H-153).

use super::Renderer;
use super::attrs::El;
use crate::ast::{Node, Table, TableCellScope};
use crate::hash::AddrMap;
use crate::resolve::order::addr;
use crate::resolve::table::{GroupKind, layout, rows};
use std::fmt::Write as _;

/// The table attributes that are read and never written (C2-H-153).
const READ_ONLY_KEYS: &[&str] = &["aligns", "widths", "header-rows", "footer-rows"];

impl<'a> Renderer<'a> {
    pub(super) fn table(&mut self, n: &'a Node, depth: usize, panel: bool) {
        let Node::Table(t) = n else { return };
        let mut el = self.el(t.attrs.as_deref(), t.lang.as_deref(), n);
        el.skip_keys = READ_ONLY_KEYS;
        if panel {
            el.base.push("carve-figure-panel".into());
        }
        let open = format!("<table{}>", el.write(self.filter()));
        self.line(depth, &open);
        if let Some(c) = &t.caption {
            self.caption(c, "caption", depth + 1);
        }
        self.colgroup(t, depth + 1);
        let columns = cell_columns(t);
        if crosses_groups(t) {
            self.line(depth + 1, "<tbody>");
            for (row, kind) in rows(t) {
                self.row(t, row, kind, &columns, depth + 2);
            }
            self.line(depth + 1, "</tbody>");
        } else {
            for g in &t.children {
                self.group(t, g, &columns, depth + 1);
            }
        }
        self.line(depth, "</table>");
    }

    /// C2-H-152.
    fn colgroup(&mut self, t: &Table, depth: usize) {
        let widths: Vec<Option<f64>> = t
            .columns
            .iter()
            .map(|c| {
                if let Node::TableColumn(c) = c {
                    c.width
                } else {
                    None
                }
            })
            .collect();
        if widths.iter().all(Option::is_none) {
            return;
        }
        self.line(depth, "<colgroup>");
        for w in widths {
            match w {
                Some(w) => self.line(
                    depth + 1,
                    &format!("<col style=\"width: {}%;\">", percent(w)),
                ),
                None => self.line(depth + 1, "<col>"),
            }
        }
        self.line(depth, "</colgroup>");
    }

    fn group(&mut self, t: &'a Table, g: &'a Node, columns: &AddrMap<usize, usize>, depth: usize) {
        let (tag, kind, children) = match g {
            Node::TableHead(x) => ("thead", GroupKind::Head, &x.children),
            Node::TableBody(x) => ("tbody", GroupKind::Body, &x.children),
            Node::TableFoot(x) => ("tfoot", GroupKind::Foot, &x.children),
            _ => return,
        };
        if !children.iter().any(|r| matches!(r, Node::TableRow(_))) {
            return;
        }
        let el = self.el(g.attrs(), None, g);
        self.line(depth, &format!("<{tag}{}>", el.write(self.filter())));
        for r in children {
            self.row(t, r, kind, columns, depth + 1);
        }
        self.line(depth, &format!("</{tag}>"));
    }

    fn row(
        &mut self,
        t: &'a Table,
        r: &'a Node,
        kind: GroupKind,
        columns: &AddrMap<usize, usize>,
        depth: usize,
    ) {
        let Node::TableRow(row) = r else { return };
        let el = self.el(row.attrs.as_deref(), row.lang.as_deref(), r);
        let mut s = format!("<tr{}>", el.write(self.filter()));
        for c in &row.children {
            self.cell(t, c, kind, columns.get(&addr(c)).copied(), &mut s);
        }
        s.push_str("</tr>");
        self.line(depth, &s);
    }

    /// C2-H-056 to C2-H-058.
    fn cell(
        &mut self,
        t: &Table,
        c: &'a Node,
        kind: GroupKind,
        col: Option<usize>,
        out: &mut String,
    ) {
        let Node::TableCell(x) = c else { return };
        if x.span.is_some() {
            return;
        }
        let column = col.and_then(|i| t.columns.get(i)).and_then(|c| {
            if let Node::TableColumn(c) = c {
                Some(c)
            } else {
                None
            }
        });
        let mut el = El::new(x.attrs.as_deref(), x.lang.as_deref());
        if let Some(v) = x.rowspan.filter(|v| *v > 1) {
            el.structural.push(("rowspan", Some(v.to_string())));
        }
        if let Some(v) = x.colspan.filter(|v| *v > 1) {
            el.structural.push(("colspan", Some(v.to_string())));
        }
        let scope = x.scope.map(TableCellScope::as_str).or_else(|| {
            x.header.then_some(if kind == GroupKind::Head {
                "col"
            } else {
                "row"
            })
        });
        if let Some(s) = scope {
            el.structural.push(("scope", Some(s.to_string())));
        }
        if let Some(ids) = &x.header_ids {
            el.structural.push(("headers", Some(ids.join(" "))));
        }
        let align = x
            .align
            .map(super::super::ast::TableCellAlign::as_str)
            .or_else(|| {
                column
                    .and_then(|c| c.resolved_align)
                    .map(super::super::ast::TableColumnResolvedAlign::as_str)
            });
        if let Some(a) = align {
            el.style_after.push(format!("text-align: {a};"));
        }
        let valign = x
            .valign
            .map(super::super::ast::TableCellValign::as_str)
            .or_else(|| {
                column
                    .and_then(|c| c.valign)
                    .map(super::super::ast::TableColumnValign::as_str)
            });
        if let Some(v) = valign {
            el.style_after.push(format!("vertical-align: {v};"));
        }
        let tag = if x.header { "th" } else { "td" };
        let _ = write!(out, "<{tag}{}>", el.write(self.filter()));
        self.level += 1;
        let inner = self.cell_content(&x.children);
        self.level -= 1;
        out.push_str(&inner);
        let _ = write!(out, "</{tag}>");
    }

    fn cell_content(&mut self, children: &'a [Node]) -> String {
        if let [Node::CellParagraph(p)] = children {
            return self.leaf(&p.children);
        }
        let saved = std::mem::take(&mut self.out);
        let started = std::mem::replace(&mut self.started, false);
        for c in children {
            match c {
                Node::CellParagraph(p) => {
                    let s = self.leaf(&p.children);
                    self.line(0, &s);
                }
                other => self.block(other, 0),
            }
        }
        self.started = started;
        std::mem::replace(&mut self.out, saved)
    }
}

/// The grid column each cell starts in.
fn cell_columns(t: &Table) -> AddrMap<usize, usize> {
    layout(t).iter().map(|p| (addr(p.node), p.col)).collect()
}

/// C2-H-060: a row span that reaches past the end of its group.
fn crosses_groups(t: &Table) -> bool {
    let all = rows(t);
    let placed = layout(t);
    placed.iter().any(|p| {
        let last = p.row + usize::try_from(p.rowspan.max(1)).unwrap_or(1) - 1;
        let kind = all.get(p.row).map(|r| r.1);
        (p.row..=last).any(|r| all.get(r).is_some_and(|x| Some(x.1) != kind))
    })
}

/// A width as a percentage without trailing zeros (C2-H-152).
fn percent(w: f64) -> String {
    let s = format!("{:.6}", w * 100.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}
