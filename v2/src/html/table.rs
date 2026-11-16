//! Tables (C2-H-055 to C2-H-061, C2-H-152, C2-H-153).

use super::Renderer;
use super::attrs::El;
use crate::ast::{Node, Table, TableCellScope};
use crate::hash::AddrMap;
use crate::resolve::order::addr;
use crate::resolve::table::{GroupKind, Placed, layout, rows};

/// The table attributes that are read and never written (C2-H-153).
const READ_ONLY_KEYS: &[&str] = &["aligns", "widths", "header-rows", "footer-rows"];

impl<'r, 'a> Renderer<'r, 'a> {
    pub(super) fn table(&mut self, n: &'r Node<'a>, depth: usize, panel: bool) {
        let Node::Table(t) = n else { return };
        let mut el = self.el(t.attrs, t.lang, n);
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
        let placed = layout(t);
        let columns: AddrMap<usize, usize> = placed.iter().map(|p| (addr(p.node), p.col)).collect();
        if crosses_groups(t, &placed) {
            self.line(depth + 1, "<tbody>");
            for (row, kind) in rows(t) {
                self.row(t, row, kind, &columns, depth + 2);
            }
            self.line(depth + 1, "</tbody>");
        } else {
            for g in t.children.iter() {
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

    fn group(
        &mut self,
        t: &'r Table<'a>,
        g: &'r Node<'a>,
        columns: &AddrMap<usize, usize>,
        depth: usize,
    ) {
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
        let f = self.filter();
        self.line_with(depth, |_, out| {
            out.push('<');
            out.push_str(tag);
            el.write_into(f, out);
            out.push('>');
            true
        });
        for r in children.iter() {
            self.row(t, r, kind, columns, depth + 1);
        }
        self.line_with(depth, |_, out| {
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
            true
        });
    }

    fn row(
        &mut self,
        t: &'r Table<'a>,
        r: &'r Node<'a>,
        kind: GroupKind,
        columns: &AddrMap<usize, usize>,
        depth: usize,
    ) {
        let Node::TableRow(row) = r else { return };
        let el = self.el(row.attrs, row.lang, r);
        let f = self.filter();
        self.line_with(depth, |r, out| {
            out.push_str("<tr");
            el.write_into(f, out);
            out.push('>');
            for c in row.children.iter() {
                r.cell(t, c, kind, columns.get(&addr(c)).copied(), out);
            }
            out.push_str("</tr>");
            true
        });
    }

    /// C2-H-056 to C2-H-058.
    fn cell(
        &mut self,
        t: &Table,
        c: &'r Node<'a>,
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
        let mut el = El::new(x.attrs, x.lang);
        if let Some(v) = x.rowspan.filter(|v| *v > 1) {
            el.structural.push(("rowspan", Some(v.to_string().into())));
        }
        if let Some(v) = x.colspan.filter(|v| *v > 1) {
            el.structural.push(("colspan", Some(v.to_string().into())));
        }
        let scope = x.scope.map(TableCellScope::as_str).or_else(|| {
            x.header.then_some(if kind == GroupKind::Head {
                "col"
            } else {
                "row"
            })
        });
        if let Some(s) = scope {
            el.structural.push(("scope", Some(s.into())));
        }
        if let Some(ids) = &x.header_ids {
            el.structural.push(("headers", Some(ids.join(" ").into())));
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
        out.push('<');
        out.push_str(tag);
        el.write_into(self.filter(), out);
        out.push('>');
        self.level += 1;
        self.cell_into(x.children, out);
        self.level -= 1;
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }

    fn cell_into(&mut self, children: &'r [Node<'a>], out: &mut String) {
        if let [Node::CellParagraph(p)] = children {
            self.leaf_into(p.children, out);
        } else {
            let inner = self.cell_content(children);
            out.push_str(&inner);
        }
    }

    fn cell_content(&mut self, children: &'r [Node<'a>]) -> String {
        let saved = std::mem::take(&mut self.out);
        let started = std::mem::replace(&mut self.started, false);
        for c in children {
            match c {
                Node::CellParagraph(p) => {
                    let s = self.leaf(p.children);
                    self.line(0, &s);
                }
                other => self.block(other, 0),
            }
        }
        self.started = started;
        std::mem::replace(&mut self.out, saved)
    }
}

/// C2-H-060: a row span that reaches past the end of its group.
fn crosses_groups(t: &Table, placed: &[Placed]) -> bool {
    let all = rows(t);
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
