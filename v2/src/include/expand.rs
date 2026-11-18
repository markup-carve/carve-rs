//! The expansion walk (C2-N-015 to C2-N-026, C2-N-061 to C2-N-066): each
//! included file is parsed as a document of its own and its selected blocks
//! become the `children` of the include node, a node before the nodes inside it.

use super::lines::{FileLines, LineRange, file_lines, line_range, region_range};
use super::{Cause, Includes, Request, code, section, shift};
use crate::Arena;
use crate::ast::{self, Document, Node};
use crate::diagnostic::Diagnostic;
use crate::options::Options;
use crate::tables::{INCLUDE_CAUSES, INCLUDE_CONFLICTS, INCLUDE_OPTIONS};
use crate::tree::Scope;
use serde_json::Value;
use std::collections::HashMap;

pub(super) struct Exp<'c, 'a> {
    pub cfg: &'c Includes,
    pub opts: &'c Options,
    pub arena: &'a Arena,
    /// C2-N-065: the bytes of the texts the cap accepted.
    spent: usize,
    pub diags: &'c mut Vec<Diagnostic>,
    shifts: bool,
    sources: HashMap<&'a str, &'a str>,
}

/// The file whose includes are expanded.
struct Frame<'f> {
    file: Option<&'f str>,
    depth: usize,
    around: Vec<&'f str>,
    scope: &'f Scope,
}

/// A failure: its cause in `data/includes.json` and the values its message names.
pub(super) struct Failure {
    pub cause: &'static str,
    pub values: Vec<(&'static str, String)>,
}

impl Failure {
    pub fn new(cause: &'static str) -> Self {
        Self {
            cause,
            values: Vec::new(),
        }
    }

    pub fn with(cause: &'static str, key: &'static str, value: impl Into<String>) -> Self {
        Self {
            cause,
            values: vec![(key, value.into())],
        }
    }
}

/// The children a selection gives, and the scope of their file for the
/// includes among them; `None` for a code include, whose child holds none.
pub(super) struct Selected<'a> {
    pub children: &'a mut [Node<'a>],
    pub scope: Option<Scope>,
}

/// C2-N-006: expands the includes of `doc` in place, in tree order.
pub(crate) fn expand<'a>(
    doc: &mut Document<'a>,
    scope: &Scope,
    cfg: &Includes,
    opts: &Options,
    arena: &'a Arena,
    diags: &mut Vec<Diagnostic>,
) -> HashMap<&'a str, &'a str> {
    let mut exp = Exp {
        cfg,
        opts,
        arena,
        spent: 0,
        diags,
        shifts: false,
        sources: HashMap::new(),
    };
    let frame = Frame {
        file: cfg.file.as_deref(),
        depth: 0,
        around: Vec::new(),
        scope,
    };
    walk(doc.children, &mut exp, &frame);
    if exp.shifts {
        shift::apply(doc.children, exp.diags);
    }
    exp.sources
}

/// C2-N-003: without a resolver every include is reported and stays unexpanded.
pub(crate) fn no_resolver(nodes: &mut [Node<'_>], diags: &mut Vec<Diagnostic>) {
    for n in nodes.iter_mut() {
        if let Node::Include(i) = n {
            diags.push(Diagnostic::new(
                "include-no-resolver",
                &[("path", i.path)],
                None,
                i.pos,
            ));
        } else if holds_blocks(n) {
            n.for_each_child_mut(&mut |c| no_resolver(std::slice::from_mut(c), diags));
        }
    }
}

/// Block nodes whose slots can hold an include (C2-N-002: only block lines make one).
pub(crate) fn holds_blocks(n: &Node<'_>) -> bool {
    matches!(
        n,
        Node::List(_)
            | Node::ListItem(_)
            | Node::BlockQuote(_)
            | Node::Div(_)
            | Node::Admonition(_)
            | Node::Directive(_)
            | Node::FigureGroup(_)
            | Node::DefinitionList(_)
            | Node::DefinitionDescription(_)
            | Node::Footnote(_)
            | Node::Section(_)
            | Node::BlockExtension(_)
    )
}

fn walk<'a>(nodes: &mut [Node<'a>], exp: &mut Exp<'_, 'a>, frame: &Frame<'_>) {
    for n in nodes.iter_mut() {
        if let Node::Include(i) = n {
            expand_one(i, exp, frame);
        } else if holds_blocks(n) {
            n.for_each_child_mut(&mut |c| walk(std::slice::from_mut(c), exp, frame));
        }
    }
}

fn fail(exp: &mut Exp<'_, '_>, node: &ast::Include<'_>, frame: &Frame<'_>, f: &Failure) {
    let code = INCLUDE_CAUSES
        .iter()
        .find(|c| c.0 == f.cause)
        .map_or("include-unreadable", |c| c.1);
    let mut values: Vec<(&str, &str)> = vec![("path", node.path)];
    values.extend(f.values.iter().map(|(k, v)| (*k, v.as_str())));
    // C2-N-063: a finding names an included file only; the top level has no file.
    let file = frame.file.filter(|_| frame.depth > 0);
    exp.diags
        .push(Diagnostic::new(code, &values, file, node.pos));
}

fn expand_one<'a>(node: &mut ast::Include<'a>, exp: &mut Exp<'_, 'a>, frame: &Frame<'_>) {
    if let Some(f) = before_resolver(exp, node, frame) {
        return fail(exp, node, frame, &f);
    }
    let (text, id) = match ask(exp, node, frame) {
        Ok(answer) => answer,
        Err(cause) => return fail(exp, node, frame, &Failure::new(cause.name())),
    };
    let id = exp.arena.str(&id);
    node.file = Some(id);
    if exp.cfg.file.as_deref() == Some(id) || frame.around.contains(&id) {
        return fail(exp, node, frame, &Failure::new("cycle"));
    }
    let limit = exp.cfg.bytes_limit();
    if exp.spent + text.len() > limit {
        return fail(
            exp,
            node,
            frame,
            &Failure::with("size_exceeded", "limit", limit.to_string()),
        );
    }
    exp.spent += text.len();
    let text = exp.arena.str(&text);
    exp.sources.insert(id, text);
    let selected = match select(exp, node, text, frame.scope, id) {
        Ok(s) => s,
        Err(f) => return fail(exp, node, frame, &f),
    };
    exp.shifts |= option(node, "shift").is_some();
    let Selected { children, scope } = selected;
    let mut around = frame.around.clone();
    around.push(id);
    if let Some(scope) = scope {
        let inner = Frame {
            file: Some(id),
            depth: frame.depth + 1,
            around,
            scope: &scope,
        };
        walk(children, exp, &inner);
    }
    node.children = Some(children);
}

/// A valid option of the node (C2-A-181).
pub(super) fn option<'v>(node: &ast::Include<'v>, key: &str) -> Option<&'v Value> {
    node.options.and_then(|o| o.get(key))
}

fn holds(node: &ast::Include<'_>, name: &str) -> bool {
    if name == "section" {
        node.section.is_some()
    } else {
        option(node, name).is_some()
    }
}

/// C2-N-045: a `csv` or `tsv` code include whose attributes carry the `table` flag.
pub(super) fn is_table(node: &ast::Include<'_>) -> bool {
    let code = option(node, "code").and_then(Value::as_str);
    matches!(code, Some("csv" | "tsv"))
        && node.attrs.is_some_and(|a| {
            a.key_values
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("table"))
        })
}

/// C2-N-024: why the options break a row of `conflicts`.
fn conflict(node: &ast::Include<'_>) -> Option<String> {
    for (rule, names) in INCLUDE_CONFLICTS {
        let present = names.iter().filter(|n| holds(node, n)).count();
        let first = names.first().copied().unwrap_or("");
        match *rule {
            "requires_table_code" if present > 0 && !is_table(node) => {
                return Some(format!(
                    "@{first} needs a code include of a csv or tsv table"
                ));
            }
            "exclusive" if present == names.len() => {
                return Some("a section and @code exclude each other".into());
            }
            "at_most_one" if present > 1 => {
                return Some("a directive takes at most one selection".into());
            }
            _ => {}
        }
    }
    None
}

/// C2-B-206: why an invalid entry is invalid, as `include-option-invalid` says it.
fn invalid_reason(node: &ast::Include<'_>, entry: &str) -> &'static str {
    let body = entry.strip_prefix('@').unwrap_or(entry);
    let (key, value) = body
        .split_once(':')
        .map_or((body, None), |(k, v)| (k, Some(v)));
    let Some(row) = INCLUDE_OPTIONS.iter().find(|o| o.1 == key) else {
        return "has a key that is not an include option";
    };
    if option(node, key).is_some()
        || node
            .invalid
            .unwrap_or_default()
            .iter()
            .filter(|e| e.starts_with(&format!("@{key}")))
            .count()
            > 1
    {
        return "repeats a key that stands earlier in the directive";
    }
    match value {
        None | Some("" | "\"\"" | "''") if row.3 => "needs a value",
        _ => "has a value outside the shape of its key",
    }
}

/// C2-N-062: the causes tested before the resolver is called.
fn before_resolver(
    exp: &Exp<'_, '_>,
    node: &ast::Include<'_>,
    frame: &Frame<'_>,
) -> Option<Failure> {
    if let Some(entry) = node.invalid.and_then(|v| v.first()) {
        return Some(Failure {
            cause: "option_invalid",
            values: vec![
                ("option", (*entry).to_string()),
                ("reason", invalid_reason(node, entry).into()),
            ],
        });
    }
    if let Some(reason) = conflict(node) {
        return Some(Failure::with("conflict", "reason", reason));
    }
    let depth = exp.cfg.depth_limit();
    if frame.depth + 1 > depth {
        return Some(Failure::with("depth", "limit", depth.to_string()));
    }
    let bytes = exp.cfg.bytes_limit();
    if exp.spent >= bytes {
        return Some(Failure::with("size_spent", "limit", bytes.to_string()));
    }
    None
}

/// C2-N-007, C2-N-011: the text and id the resolver returns, or its cause.
fn ask(
    exp: &Exp<'_, '_>,
    node: &ast::Include<'_>,
    frame: &Frame<'_>,
) -> Result<(String, String), Cause> {
    let empty = Value::Object(serde_json::Map::new());
    let request = Request {
        path: node.path,
        section: node.section,
        options: node.options.unwrap_or(&empty),
        from: frame.file,
    };
    let (bytes, id) = (exp.cfg.resolver)(&request)?;
    let text = String::from_utf8(bytes).map_err(|_| Cause::NonText)?;
    if text.contains('\0') {
        return Err(Cause::NonText);
    }
    Ok((text, id))
}

/// C2-N-026, C2-N-034, C2-N-037: the lines a directive reads, or its failure.
fn selected_lines(
    node: &ast::Include<'_>,
    text: &str,
    lines: &FileLines,
) -> Result<LineRange, Failure> {
    if let Some(v) = option(node, "lines").and_then(Value::as_str) {
        return line_range(v, lines.len()).ok_or_else(|| Failure::new("lines_past_end"));
    }
    let Some(name) = option(node, "region").and_then(Value::as_str) else {
        return Ok(LineRange {
            from: 0,
            to: lines.len(),
        });
    };
    let code = option(node, "code").is_some();
    let quoted = if is_table(node) {
        code::quoted_lines(text, lines, node)
    } else {
        Vec::new()
    };
    let skip = |i: usize| quoted.get(i).copied().unwrap_or(false);
    region_range(text, lines, name, code, &skip).map_err(|reason| Failure {
        cause: "region_invalid",
        values: vec![("name", name.to_string()), ("reason", reason.to_string())],
    })
}

/// C2-N-027 to C2-N-047: the children of the include, read from the returned text.
fn select<'a>(
    exp: &mut Exp<'_, 'a>,
    node: &ast::Include<'a>,
    text: &'a str,
    scope: &Scope,
    id: &str,
) -> Result<Selected<'a>, Failure> {
    if let Some(name) = node.section {
        let (mut doc, inner) =
            parse_file(text, scope, exp.arena, Some((&mut *exp.diags, id)), true);
        let children = section::select(&mut doc, name, exp.opts.heading_ids, exp.arena)
            .ok_or_else(|| Failure::with("section_missing", "name", name))?;
        return Ok(Selected {
            children,
            scope: Some(inner),
        });
    }
    let lines = file_lines(text);
    let range = selected_lines(node, text, &lines)?;
    if option(node, "code").is_some() {
        return select_code(exp, node, text, id, &lines, range);
    }
    // C2-N-020, C2-N-036: the selected lines are a document of their own,
    // positioned in the file.
    let (start, end) = slice(text, &lines, range);
    let mut found = Vec::new();
    let (whole, whole_scope) = parse_file(text, scope, exp.arena, None, true);
    let (doc, _) = parse_file(
        text.get(start..end).unwrap_or(""),
        &whole_scope,
        exp.arena,
        Some((&mut found, id)),
        range.from == 0,
    );
    if start > 0 {
        let lines_before = u32::try_from(range.from).unwrap_or(u32::MAX);
        let bytes = u32::try_from(start).unwrap_or(u32::MAX);
        for n in doc.children.iter_mut() {
            shift_node(n, lines_before, bytes);
        }
        for p in found.iter_mut().filter_map(|d| d.pos.as_mut()) {
            shift_pos(p, lines_before, bytes);
        }
    }
    let mut all = doc
        .children
        .iter_mut()
        .map(|n| std::mem::replace(n, empty_node(exp.arena)))
        .collect::<Vec<_>>();
    for n in whole.children.iter_mut() {
        if !matches!(
            n,
            Node::LinkReferenceDefinition(_)
                | Node::Footnote(_)
                | Node::AbbreviationDef(_)
                | Node::CitationDefinition(_)
        ) {
            continue;
        }
        let offset = n.pos().map_or(0, |p| p.start_offset as usize);
        if offset < start || offset >= end {
            all.push(std::mem::replace(n, empty_node(exp.arena)));
        }
    }
    exp.diags.extend(found);
    Ok(Selected {
        children: exp.arena.fill(all.into_iter()),
        scope: Some(whole_scope),
    })
}

fn select_code<'a>(
    exp: &mut Exp<'_, 'a>,
    node: &ast::Include<'a>,
    text: &str,
    id: &str,
    lines: &FileLines,
    range: LineRange,
) -> Result<Selected<'a>, Failure> {
    let children = match code::child(node, text, lines, range, exp.arena)? {
        Some((child, malformed)) => {
            if malformed {
                let pos = child.pos().copied();
                exp.diags.push(Diagnostic::new(
                    "delimited-table-malformed",
                    &[],
                    Some(id),
                    pos,
                ));
            }
            exp.arena.one(child)
        }
        None => &mut [],
    };
    Ok(Selected {
        children,
        scope: None,
    })
}

fn empty_node(arena: &Arena) -> Node<'_> {
    Node::SoftBreak(arena.alloc(ast::SoftBreak::default()))
}

/// The byte range of the selected lines; a slice from line 1 keeps a U+FEFF.
fn slice(text: &str, lines: &FileLines, range: LineRange) -> (usize, usize) {
    if range.from >= range.to {
        return (0, 0);
    }
    let start = if range.from == 0 {
        0
    } else {
        lines.starts.get(range.from).copied().unwrap_or(0)
    };
    let end = lines.nexts.get(range.to - 1).copied().unwrap_or(text.len());
    (start, end)
}

fn shift_pos(p: &mut crate::ast::Pos, lines: u32, bytes: u32) {
    p.start_line = p.start_line.saturating_add(lines);
    p.end_line = p.end_line.saturating_add(lines);
    p.start_offset = p.start_offset.saturating_add(bytes);
    p.end_offset = p.end_offset.saturating_add(bytes);
}

fn shift_node(n: &mut Node<'_>, lines: u32, bytes: u32) {
    if let Some(Some(p)) = n.pos_slot() {
        shift_pos(p, lines, bytes);
    }
    n.for_each_child_mut(&mut |c| shift_node(c, lines, bytes));
}

/// C2-N-015: a file parsed as a document of its own, with the scope of the
/// files around it for its references and abbreviations.
pub(super) fn parse_file<'a>(
    text: &str,
    scope: &Scope,
    arena: &'a Arena,
    report: Option<(&mut Vec<Diagnostic>, &str)>,
    allow_frontmatter: bool,
) -> (Document<'a>, Scope) {
    let text = arena.str(text);
    let src = crate::source::Source::new(text);
    let out = crate::block::scan_with_frontmatter(&src, allow_frontmatter);
    let (doc, inner) = crate::tree::build_scoped(&src, arena, out.events, Some(scope));
    if let Some((diags, file)) = report {
        let found = out.findings.iter().chain(&inner.findings);
        diags.extend(found.map(|f| Diagnostic::from_finding(f, &src, Some(file))));
    }
    (doc, inner)
}
