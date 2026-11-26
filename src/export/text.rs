//! The plain-text and ANSI exports. They share one walk; the ANSI target adds
//! SGR styling, box tables and a quote bar.

// The handlers of a dispatch table share one signature, so some return `Ok` only.
#![allow(clippy::unnecessary_wraps)]

use super::common::{
    Exporter, children, each_node, flag, footnote_definitions, int, is_skipped_break,
    is_unresolved, key_value, list, reference_label, slot, strip_bidi, sv, ty,
};
use crate::api::Error;
use serde_json::Value;
use std::collections::HashSet;
use std::fmt::Write as _;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const ITALIC: &str = "\x1b[3m";
const UNDERLINE: &str = "\x1b[4m";
const STRIKE: &str = "\x1b[9m";
const REVERSE: &str = "\x1b[7m";
const FG_RED: &str = "\x1b[31m";
const FG_GREEN: &str = "\x1b[32m";
const FG_YELLOW: &str = "\x1b[33m";
const FG_BLUE: &str = "\x1b[34m";
const FG_MAGENTA: &str = "\x1b[35m";
const FG_CYAN: &str = "\x1b[36m";
const FG_BRIGHT_BLACK: &str = "\x1b[90m";
const FG_BRIGHT_GREEN: &str = "\x1b[92m";
const FG_BRIGHT_YELLOW: &str = "\x1b[93m";
const FG_BRIGHT_BLUE: &str = "\x1b[94m";
const FG_BRIGHT_MAGENTA: &str = "\x1b[95m";
const FG_BRIGHT_CYAN: &str = "\x1b[96m";
const FG_BRIGHT_WHITE: &str = "\x1b[97m";
const HEADING_COLORS: [&str; 6] = [
    FG_BRIGHT_MAGENTA,
    FG_BRIGHT_CYAN,
    FG_BRIGHT_BLUE,
    FG_BRIGHT_GREEN,
    FG_BRIGHT_YELLOW,
    FG_BRIGHT_WHITE,
];

// U+0000 stands for a non-breaking space until the end, so the trims leave it alone.
const NBSP_HOLD: char = '\0';

type R = Result<String, Error>;

struct State<'x, 'v> {
    x: &'x mut Exporter<'v>,
    ansi: bool,
    list_depth: usize,
    /// The list depth whose indentation an enclosing continuation prefix already wrote.
    indent_base: usize,
    /// Above 0 while the children of an inline node are written.
    inline_level: usize,
    /// Inside a span with an authored `abbr`, whose value is written instead of nested expansions.
    abbr_depth: usize,
    /// Above 0 while a title is written: bold inside it is a no-op.
    unwrap_strong: usize,
    /// The host whose caption a figure-group panel writes before it.
    uncaptioned: Option<*const Value>,
    footnotes: &'x HashSet<&'v str>,
    referenced: &'x HashSet<&'v str>,
    expanded: HashSet<(String, String)>,
    expanded_before: &'x HashSet<(String, String)>,
}

impl State<'_, '_> {
    fn style(&self, text: &str, codes: &str) -> String {
        if self.ansi {
            format!("{codes}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    /// Author text on its way out. The plain target keeps the C0 controls as
    /// content and drops CR, DEL and the C1 range; the terminal acts on every
    /// control, so ANSI keeps only tab and line feed.
    fn clean(&self, text: &str) -> String {
        if self.ansi {
            text.chars()
                .filter(|&c| !c.is_control() || c == '\t' || c == '\n')
                .collect()
        } else {
            text.chars()
                .filter(|&c| c != '\r' && !('\u{7f}'..='\u{9f}').contains(&c))
                .collect()
        }
    }

    fn caption<'n>(&self, node: &'n Value) -> Option<&'n Value> {
        if self.uncaptioned == Some(std::ptr::from_ref(node)) {
            return None;
        }
        slot(node, "caption")
    }
}

pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("\x1b[") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let end = after
            .find(|c: char| !(c.is_ascii_digit() || c == ';'))
            .unwrap_or(after.len());
        if after[end..].starts_with('m') {
            rest = &after[end + 1..];
        } else {
            out.push_str("\x1b[");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// Trims spaces, tabs and line breaks, keeping a held non-breaking space.
fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

fn trim_end(text: &str) -> &str {
    text.trim_end_matches(is_space)
}

fn prefix_lines(content: &str, prefix: &str) -> String {
    content
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Folds every run of line breaks, with the spaces around it, into one space.
fn fold_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending = String::new();
    let mut folding = false;
    for c in text.chars() {
        match c {
            ' ' | '\t' | '\r' => pending.push(c),
            '\n' => {
                pending.clear();
                folding = true;
            }
            _ => {
                if folding {
                    out.push(' ');
                    folding = false;
                    pending.clear();
                }
                out.push_str(&pending);
                pending.clear();
                out.push(c);
            }
        }
    }
    if folding {
        out.push(' ');
    } else {
        out.push_str(&pending);
    }
    out
}

/// The content of a line of a line block, whose own line end stands for a final hard break.
fn without_final_break(list: &[Value]) -> &[Value] {
    match list.split_last() {
        Some((end, rest)) if ty(end) == "hard_break" => rest,
        _ => list,
    }
}

fn blocks(s: &mut State<'_, '_>, nodes: &[Value]) -> R {
    s.x.enter()?;
    let mut out = String::new();
    for node in nodes {
        out.push_str(&block(s, node)?);
    }
    s.x.leave();
    Ok(out)
}

/// The list of inlines to write: in a title, each strong gives way to its children.
fn flattened<'n>(s: &State<'_, '_>, nodes: &'n [Value]) -> Vec<&'n Value> {
    fn push<'n>(nodes: &'n [Value], out: &mut Vec<&'n Value>) {
        for node in nodes {
            if ty(node) == "strong" {
                push(children(node), out);
            } else {
                out.push(node);
            }
        }
    }
    if s.unwrap_strong == 0 {
        return nodes.iter().collect();
    }
    let mut out = Vec::with_capacity(nodes.len());
    push(nodes, &mut out);
    out
}

fn inlines(s: &mut State<'_, '_>, nodes: &[Value]) -> R {
    s.x.enter()?;
    let inside = s.inline_level > 0;
    let list = flattened(s, nodes);
    let mut out = String::new();
    s.inline_level += 1;
    for (at, node) in list.iter().enumerate() {
        if matches!(ty(node), "soft_break" | "hard_break") && is_skipped_break(&list, at, inside) {
            continue;
        }
        match inline(s, node) {
            Ok(text) => out.push_str(&text),
            Err(e) => {
                s.inline_level -= 1;
                return Err(e);
            }
        }
    }
    s.inline_level -= 1;
    s.x.leave();
    Ok(out)
}

/// Inlines of a slot other than `children`, which a title's unwrapping does not reach.
fn slot_inlines(s: &mut State<'_, '_>, nodes: &[Value]) -> R {
    let unwrap = std::mem::take(&mut s.unwrap_strong);
    let out = inlines(s, nodes);
    s.unwrap_strong = unwrap;
    out
}

/// A caption on one line: its blocks with their breaks folded into spaces.
fn caption_line(s: &mut State<'_, '_>, caption: &Value) -> R {
    Ok(fold_breaks(trim(&blocks(s, children(caption))?)))
}

fn caption_block(s: &mut State<'_, '_>, caption: &Value) -> R {
    let line = caption_line(s, caption)?;
    Ok(if s.ansi {
        format!("{}\n\n", s.style(&line, &format!("{ITALIC}{DIM}")))
    } else {
        format!("{line}\n\n")
    })
}

/// A captioned host: the host, then its caption on the next line, or after a blank line below a quote.
fn captioned(s: &mut State<'_, '_>, host: String, caption: Option<&Value>, quote: bool) -> R {
    let Some(caption) = caption else {
        return Ok(host);
    };
    Ok(format!(
        "{}{}{}",
        trim_end(&host),
        if quote { "\n\n" } else { "\n" },
        caption_block(s, caption)?
    ))
}

fn title_line(s: &mut State<'_, '_>, node: &Value) -> R {
    let mut out = String::new();
    let title = list(node, "title");
    if !title.is_empty() {
        if s.ansi {
            s.unwrap_strong += 1;
        }
        let text = inlines(s, title);
        if s.ansi {
            s.unwrap_strong -= 1;
        }
        let text = text?;
        if !text.is_empty() {
            let _ = write!(out, "{}\n\n", s.style(&text, BOLD));
        }
    }
    if let Some(label) = sv(node, "groupLabel").filter(|l| !l.is_empty()) {
        let label = s.clean(label);
        let _ = write!(out, "{}\n\n", s.style(&label, BOLD));
    }
    Ok(out)
}

fn pair_key(term: &str, expansion: &str) -> (String, String) {
    (term.to_string(), expansion.to_string())
}

fn block(s: &mut State<'_, '_>, node: &Value) -> R {
    BLOCKS
        .iter()
        .find(|(name, _)| *name == ty(node))
        .map_or_else(|| Ok(String::new()), |(_, handler)| handler(s, node))
}

type BlockFn = fn(s: &mut State<'_, '_>, node: &Value) -> R;

const BLOCKS: &[(&str, BlockFn)] = &[
    ("section", block_section),
    ("heading", block_heading),
    ("paragraph", block_paragraph),
    ("code_block", block_code_block),
    ("block_quote", block_block_quote),
    ("list", block_list),
    ("thematic_break", block_thematic_break),
    ("table", block_table),
    ("admonition", block_admonition),
    ("directive", block_admonition),
    ("div", block_admonition),
    ("line_block", block_line_block),
    ("definition_list", block_definition_list),
    ("figure_group", block_figure_group),
    ("include", block_include),
    ("block_extension", block_block_extension),
    ("raw_block", block_raw_block),
    ("abbreviation_def", block_abbreviation_def),
    ("link_reference_definition", block_link_reference_definition),
];

fn block_section(s: &mut State<'_, '_>, node: &Value) -> R {
    blocks(s, children(node))
}

fn block_heading(s: &mut State<'_, '_>, node: &Value) -> R {
    let content = inlines(s, children(node))?;
    Ok(heading(s, int(node, "level").unwrap_or(1), &content))
}

fn block_paragraph(s: &mut State<'_, '_>, node: &Value) -> R {
    let host = format!("{}\n\n", inlines(s, children(node))?);
    let caption = s.caption(node);
    captioned(s, host, caption, false)
}

fn block_code_block(s: &mut State<'_, '_>, node: &Value) -> R {
    let host = code_block(s, node)?;
    let caption = s.caption(node);
    captioned(s, host, caption, false)
}

fn block_block_quote(s: &mut State<'_, '_>, node: &Value) -> R {
    let host = block_quote(s, children(node))?;
    let caption = s.caption(node);
    captioned(s, host, caption, true)
}

fn block_list(s: &mut State<'_, '_>, node: &Value) -> R {
    list_block(s, node)
}

fn block_thematic_break(s: &mut State<'_, '_>, _node: &Value) -> R {
    Ok(if s.ansi {
        format!("{}\n\n", s.style(&"─".repeat(40), DIM))
    } else {
        "---\n\n".into()
    })
}

fn block_table(s: &mut State<'_, '_>, node: &Value) -> R {
    table(s, node)
}

fn block_admonition(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok(format!(
        "{}{}",
        title_line(s, node)?,
        blocks(s, children(node))?
    ))
}

fn block_line_block(s: &mut State<'_, '_>, node: &Value) -> R {
    let mut out = String::new();
    for line in children(node) {
        if ty(line) == "line" {
            out.push_str(&inlines(s, without_final_break(children(line)))?);
            out.push('\n');
        }
    }
    out.push('\n');
    Ok(out)
}

fn block_definition_list(s: &mut State<'_, '_>, node: &Value) -> R {
    let mut out = String::new();
    for child in children(node) {
        match ty(child) {
            "definition_term" => {
                let text = inlines(s, children(child))?;
                out.push_str(&s.style(&text, &format!("{BOLD}{FG_YELLOW}")));
                out.push('\n');
            }
            "definition_description" => {
                out.push_str("  ");
                out.push_str(trim(&blocks(s, children(child))?));
                out.push('\n');
            }
            _ => {}
        }
    }
    out.push('\n');
    Ok(out)
}

fn block_figure_group(s: &mut State<'_, '_>, node: &Value) -> R {
    let mut out = match slot(node, "caption") {
        Some(caption) => caption_block(s, caption)?,
        None => String::new(),
    };
    for child in children(node) {
        out.push_str(&panel(s, child)?);
    }
    Ok(out)
}

fn block_include(s: &mut State<'_, '_>, node: &Value) -> R {
    let caption = s.caption(node);
    if node.get("children").is_none() {
        let line = s.x.unexpanded_include(node);
        let host = format!("{}\n\n", s.clean(&line));
        return captioned(s, host, caption, false);
    }
    let host = blocks(s, children(node))?;
    captioned(s, host, caption, false)
}

fn block_block_extension(s: &mut State<'_, '_>, node: &Value) -> R {
    let fallback = list(node, "fallback");
    if fallback.is_empty() {
        blocks(s, children(node))
    } else {
        blocks(s, fallback)
    }
}

fn block_raw_block(s: &mut State<'_, '_>, node: &Value) -> R {
    if s.ansi {
        let value = s.clean(sv(node, "value").unwrap_or(""));
        let value = value.strip_suffix('\n').unwrap_or(&value);
        let text = format!(
            "[raw:{}] {value}",
            s.clean(sv(node, "format").unwrap_or(""))
        );
        return Ok(format!("{}\n\n", s.style(&text, DIM)));
    }
    s.x.raw_dropped(node, true);
    Ok(String::new())
}

fn block_abbreviation_def(s: &mut State<'_, '_>, node: &Value) -> R {
    let key = sv(node, "key").unwrap_or("");
    let expansion = sv(node, "expansion").unwrap_or("");
    // A definition whose expansion this run writes beside its occurrences loses its line.
    if s.expanded_before.contains(&pair_key(key, expansion)) {
        return Ok(String::new());
    }
    let text = format!("*[{}]: {}", s.clean(key), s.clean(expansion));
    Ok(format!("{}\n\n", s.style(&text, DIM)))
}

fn block_link_reference_definition(s: &mut State<'_, '_>, node: &Value) -> R {
    let key = sv(node, "key").unwrap_or("");
    if s.referenced.contains(key) {
        return Ok(String::new());
    }
    let authored = strip_bidi(&s.clean(sv(node, "href").unwrap_or("")));
    let href = s.x.allowed_destination(&authored, node);
    let title = sv(node, "tooltip").map_or_else(String::new, |t| format!(" \"{}\"", s.clean(t)));
    let text = format!(
        "[{}]:{}{title}",
        s.clean(key),
        if href.is_empty() {
            String::new()
        } else {
            format!(" {href}")
        }
    );
    Ok(format!("{}\n\n", s.style(&text, DIM)))
}

/// A panel of a figure group: its caption first, then the host, so the
/// caption does not read as the next panel's.
fn panel(s: &mut State<'_, '_>, node: &Value) -> R {
    let caption = match ty(node) {
        "paragraph" | "code_block" | "block_quote" => slot(node, "caption"),
        _ => None,
    };
    let Some(caption) = caption else {
        return block(s, node);
    };
    let line = caption_line(s, caption)?;
    let held = s.uncaptioned.replace(std::ptr::from_ref(node));
    let host = block(s, node);
    s.uncaptioned = held;
    let host = host?;
    let line = if s.ansi {
        s.style(&line, &format!("{ITALIC}{DIM}"))
    } else {
        line
    };
    Ok(format!("{line}\n{}\n\n", trim(&host)))
}

fn heading(s: &State<'_, '_>, level: i64, content: &str) -> String {
    if !s.ansi {
        return format!("{content}\n\n");
    }
    let color = usize::try_from(level.clamp(1, 6) - 1)
        .ok()
        .and_then(|at| HEADING_COLORS.get(at))
        .copied()
        .unwrap_or(FG_BRIGHT_WHITE);
    let mut out = s.style(content, &format!("{BOLD}{color}"));
    if level <= 2 {
        let rule = if level == 1 { "═" } else { "─" };
        let width = super::terminal::display_width(&strip_ansi(content));
        out.push('\n');
        out.push_str(&s.style(&rule.repeat(width), color));
    }
    format!("{out}\n\n")
}

fn code_block(s: &mut State<'_, '_>, node: &Value) -> R {
    let content = s.clean(sv(node, "value").unwrap_or(""));
    let mut out = String::new();
    let title = list(node, "title");
    if !title.is_empty() {
        let text = slot_inlines(s, title)?;
        let _ = write!(out, "{}\n\n", s.style(&text, BOLD));
    }
    if let Some(label) = sv(node, "groupLabel").filter(|l| !l.is_empty()) {
        let label = s.clean(label);
        let _ = write!(out, "{}\n\n", s.style(&label, BOLD));
    }
    if !s.ansi {
        return Ok(format!("{out}{content}\n"));
    }
    if let Some(syntax) = sv(node, "syntax").filter(|l| !l.is_empty()) {
        let text = format!("┌── {} ", s.clean(syntax));
        out.push_str(&s.style(&text, DIM));
        out.push('\n');
    }
    let ends = content.ends_with('\n');
    let lines: Vec<&str> = if content.is_empty() {
        Vec::new()
    } else if ends {
        content[..content.len() - 1].split('\n').collect()
    } else {
        content.split('\n').collect()
    };
    for (index, line) in lines.iter().enumerate() {
        out.push_str(&s.style(&format!("  {line}"), FG_BRIGHT_WHITE));
        if index < lines.len() - 1 || ends {
            out.push('\n');
        }
    }
    out.push('\n');
    Ok(out)
}

fn block_quote(s: &mut State<'_, '_>, nodes: &[Value]) -> R {
    let body = blocks(s, nodes)?;
    if !s.ansi {
        return Ok(format!("\"{}\"\n\n", trim(&body)));
    }
    // The bar marks every line the quote contains, and none of the blank lines between its blocks.
    let bar = format!("{} ", s.style("│", &format!("{FG_CYAN}{DIM}")));
    Ok(prefix_lines(&body, &bar))
}

fn item_marker(s: &State<'_, '_>, ordered: bool, counter: i64, task: Option<&str>) -> String {
    if ordered {
        return s.style(&format!("{counter}."), FG_YELLOW);
    }
    if !s.ansi {
        return "-".into();
    }
    match task {
        None => s.style("•", FG_CYAN),
        Some("x" | "X") => s.style("☑", FG_GREEN),
        Some(_) => s.style("☐", FG_BRIGHT_BLACK),
    }
}

/// The blocks of an item after its first: a sublist under the line above,
/// any other block at the content column.
fn item_rest(s: &mut State<'_, '_>, rest: &[&Value], tight: bool, first: String) -> R {
    let mut content = first;
    for child in rest {
        if ty(child) == "list" {
            if !s.ansi && tight && content.ends_with("\n\n") {
                content.pop();
            }
            content.push_str(&blocks(s, std::slice::from_ref(*child))?);
            continue;
        }
        let base = s.indent_base;
        s.indent_base = s.list_depth;
        let written = blocks(s, std::slice::from_ref(*child));
        s.indent_base = base;
        content.push_str(&prefix_lines(&written?, &"  ".repeat(s.list_depth - base)));
    }
    Ok(content)
}

fn list_block(s: &mut State<'_, '_>, node: &Value) -> R {
    s.list_depth += 1;
    let marker = node.get("marker").cloned().unwrap_or(Value::Null);
    let start = int(&marker, "start").unwrap_or(1);
    let ordered = sv(&marker, "kind") == Some("ordered");
    let tight = sv(node, "spacing") == Some("tight");
    let mut out = String::new();
    for (item, counter) in children(node).iter().zip(start..) {
        let indent = "  ".repeat(s.list_depth - 1 - s.indent_base);
        let task = item.get("task").and_then(|t| sv(t, "mark"));
        let mark = item_marker(s, ordered, counter, task);
        // A comment writes nothing, so it does not take the marker line from the first visible block.
        let visible: Vec<&Value> = children(item)
            .iter()
            .filter(|c| ty(c) != "comment")
            .collect();
        let first = match visible.first() {
            Some(first) => blocks(s, std::slice::from_ref(*first)),
            None => Ok(String::new()),
        };
        let content =
            first.and_then(|first| item_rest(s, visible.get(1..).unwrap_or(&[]), tight, first));
        let content = match content {
            Ok(c) => c,
            Err(e) => {
                s.list_depth -= 1;
                return Err(e);
            }
        };
        let _ = writeln!(out, "{indent}{mark} {}", trim(&content));
    }
    s.list_depth -= 1;
    if !(s.ansi && s.list_depth > 0) {
        out.push('\n');
    }
    Ok(out)
}

struct Cell {
    text: String,
    header: bool,
}

fn cell_text(s: &mut State<'_, '_>, cell: &Value) -> R {
    let mut out = String::new();
    for child in children(cell) {
        if ty(child) == "cell_paragraph" {
            out.push_str(&inlines(s, children(child))?);
        } else {
            out.push_str(&blocks(s, std::slice::from_ref(child))?);
            out.push('\n');
        }
    }
    Ok(fold_breaks(trim(&out)))
}

fn row_cells(s: &mut State<'_, '_>, row: &Value) -> Result<Vec<Cell>, Error> {
    let mut cells = Vec::new();
    for cell in children(row) {
        if int(cell, "colspan").unwrap_or(1) > 1 || int(cell, "rowspan").unwrap_or(1) > 1 {
            let target = s.x.target.name();
            s.x.loss(
                cell,
                "table-span-flattened",
                format!("Flattened a spanned table cell while rendering {target}"),
            );
        }
        // A cell a span covers writes nothing in plain text, and an empty cell in a box.
        let header = flag(cell, "header");
        if cell.get("span").is_none() {
            cells.push(Cell {
                text: cell_text(s, cell)?,
                header,
            });
        } else if s.ansi {
            cells.push(Cell {
                text: String::new(),
                header,
            });
        }
    }
    Ok(cells)
}

fn table(s: &mut State<'_, '_>, node: &Value) -> R {
    let mut rows = Vec::new();
    for section in children(node) {
        if ty(section) == "comment" {
            continue;
        }
        for row in children(section) {
            if ty(row) != "comment" {
                rows.push(row_cells(s, row)?);
            }
        }
    }
    let out = if s.ansi {
        box_table(s, &rows)
    } else {
        let mut out = String::new();
        for cells in &rows {
            let line: Vec<&str> = cells.iter().map(|c| c.text.as_str()).collect();
            let _ = writeln!(out, "{}", line.join(" | "));
        }
        out
    };
    let Some(caption) = slot(node, "caption") else {
        return Ok(format!("{out}\n"));
    };
    if s.ansi {
        return Ok(format!("{out}{}", caption_block(s, caption)?));
    }
    Ok(format!(
        "{}\n{}\n\n",
        trim_end(&out),
        caption_line(s, caption)?
    ))
}

fn box_table(s: &State<'_, '_>, rows: &[Vec<Cell>]) -> String {
    use super::terminal::display_width;
    if rows.is_empty() {
        return String::new();
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0; columns];
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(display_width(&strip_ansi(&cell.text)));
        }
    }
    let border = |left: &str, cross: &str, right: &str| {
        let line = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join(cross);
        format!("{}\n", s.style(&format!("{left}{line}{right}"), DIM))
    };
    let bar = s.style("│", DIM);
    let mut out = border("┌", "┬", "┐");
    let mut headed = false;
    for row in rows {
        let bold = !row.is_empty() && row.iter().all(|c| c.header);
        let parts: Vec<String> = widths
            .iter()
            .enumerate()
            .map(|(at, w)| {
                let text = row.get(at).map_or("", |c| c.text.as_str());
                let padded = format!("{text}{}", " ".repeat(w - display_width(&strip_ansi(text))));
                format!(" {} ", if bold { s.style(&padded, BOLD) } else { padded })
            })
            .collect();
        let _ = writeln!(out, "{bar}{}{bar}", parts.join(&bar));
        if bold && !headed {
            out.push_str(&border("├", "┼", "┤"));
            headed = true;
        }
    }
    out + &border("└", "┴", "┘")
}

fn link(s: &mut State<'_, '_>, node: &Value) -> R {
    if let Some(key) = sv(node, "key").filter(|_| is_unresolved(node)) {
        let label = s.clean(&reference_label(node, key));
        return Ok(format!("[{}]{label}", inlines(s, children(node))?));
    }
    // A link inside link text writes its content only.
    if s.x.link_depth > 0 {
        return inlines(s, children(node));
    }
    s.x.link_depth += 1;
    let label = inlines(s, children(node));
    s.x.link_depth -= 1;
    let label = label?;
    if !s.ansi {
        return Ok(label);
    }
    let authored = strip_bidi(&s.clean(sv(node, "href").unwrap_or("")));
    let mut out = s.style(&label, &format!("{UNDERLINE}{FG_BLUE}"));
    if !authored.is_empty() && !authored.starts_with('#') && authored != strip_ansi(&label) {
        let dest = s.x.allowed_destination(&authored, node);
        out.push_str(&s.style(&format!(" ({dest})"), DIM));
    }
    Ok(out)
}

fn image(s: &State<'_, '_>, node: &Value) -> String {
    let alt = s.clean(sv(node, "alt").unwrap_or(""));
    if let Some(key) = sv(node, "key").filter(|_| is_unresolved(node)) {
        return format!("![{alt}]{}", s.clean(&reference_label(node, key)));
    }
    if !s.ansi {
        return alt;
    }
    format!(
        "{}{}{}",
        s.style("[img:", FG_MAGENTA),
        if alt.is_empty() {
            String::new()
        } else {
            format!(" {alt}")
        },
        s.style("]", FG_MAGENTA)
    )
}

fn note_ref(s: &mut State<'_, '_>, node: &Value) -> R {
    if ty(node) == "inline_footnote" {
        let depth = std::mem::take(&mut s.x.link_depth);
        let inner = inlines(s, children(node));
        s.x.link_depth = depth;
        return Ok(format!("({})", inner?));
    }
    let raw = sv(node, "key").unwrap_or("");
    let key = s.clean(raw);
    // An unresolved reference stays as typed and unstyled: the construct did not form.
    if node.get("number").is_none() && !s.footnotes.contains(raw) {
        return Ok(format!("[^{key}]"));
    }
    Ok(s.style(&format!("[{key}]"), &format!("{FG_CYAN}{BOLD}")))
}

fn styled(s: &mut State<'_, '_>, node: &Value, codes: &str) -> R {
    let text = inlines(s, children(node))?;
    Ok(s.style(&text, codes))
}

fn inline(s: &mut State<'_, '_>, node: &Value) -> R {
    INLINES
        .iter()
        .find(|(name, _)| *name == ty(node))
        .map_or_else(|| Ok(String::new()), |(_, handler)| handler(s, node))
}

type InlineFn = fn(s: &mut State<'_, '_>, node: &Value) -> R;

const INLINES: &[(&str, InlineFn)] = &[
    ("text", inline_text),
    ("literal_inline", inline_text),
    ("emphasis", inline_emphasis),
    ("strong", inline_strong),
    ("underline", inline_underline),
    ("strike", inline_strike),
    ("highlight", inline_highlight),
    ("insert", inline_insert),
    ("delete", inline_delete),
    ("subscript", inline_subscript),
    ("superscript", inline_superscript),
    ("small_caps", inline_small_caps),
    ("span", inline_span),
    ("code", inline_code),
    ("math", inline_math),
    ("link", inline_link),
    ("image", inline_image),
    ("autolink", inline_autolink),
    ("xref", inline_xref),
    ("footnote_ref", inline_footnote_ref),
    ("inline_footnote", inline_footnote_ref),
    ("abbreviation", inline_abbreviation),
    ("ruby", inline_ruby),
    ("raw_inline", inline_raw_inline),
    ("symbol", inline_symbol),
    ("mention", inline_mention),
    ("tag", inline_tag),
    ("inline_extension", inline_inline_extension),
    ("critic_comment", inline_critic_comment),
    ("caption_number", inline_caption_number),
    ("citation_group", inline_citation_group),
    ("smart_punctuation", inline_smart_punctuation),
    ("non_breaking_space", inline_non_breaking_space),
    ("soft_break", inline_soft_break),
    ("hard_break", inline_hard_break),
];

fn inline_text(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok(s.clean(sv(node, "value").unwrap_or("")))
}

fn inline_emphasis(s: &mut State<'_, '_>, node: &Value) -> R {
    styled(s, node, ITALIC)
}

fn inline_strong(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let kids = children(node);
        if let [only] = kids
            && s.ansi
            && ty(only) == "emphasis"
        {
            let text = inlines(s, children(only))?;
            s.style(&text, &format!("{BOLD}{ITALIC}"))
        } else {
            styled(s, node, BOLD)?
        }
    })
}

fn inline_underline(s: &mut State<'_, '_>, node: &Value) -> R {
    styled(s, node, UNDERLINE)
}

fn inline_strike(s: &mut State<'_, '_>, node: &Value) -> R {
    styled(s, node, STRIKE)
}

fn inline_highlight(s: &mut State<'_, '_>, node: &Value) -> R {
    styled(s, node, &format!("{REVERSE}{FG_YELLOW}"))
}

fn inline_insert(s: &mut State<'_, '_>, node: &Value) -> R {
    styled(s, node, &format!("{FG_GREEN}{UNDERLINE}"))
}

fn inline_delete(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        if s.ansi {
            styled(s, node, &format!("{STRIKE}{FG_RED}"))?
        } else {
            format!("~{}~", inlines(s, children(node))?)
        }
    })
}

fn inline_subscript(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let text = inlines(s, children(node))?;
        if s.ansi {
            super::terminal::to_subscript(&text)
        } else {
            text
        }
    })
}

fn inline_superscript(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let text = inlines(s, children(node))?;
        if s.ansi {
            super::terminal::to_superscript(&text)
        } else {
            text
        }
    })
}

fn inline_small_caps(s: &mut State<'_, '_>, node: &Value) -> R {
    inlines(s, children(node))
}

fn inline_span(s: &mut State<'_, '_>, node: &Value) -> R {
    span(s, node)
}

fn inline_code(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let text = s.clean(sv(node, "value").unwrap_or(""));
        s.style(&text, FG_BRIGHT_YELLOW)
    })
}

fn inline_math(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let number = match int(node, "number") {
            Some(n) if flag(node, "display") => {
                format!(" {} {n}", s.clean(sv(node, "numberPrefix").unwrap_or("")))
            }
            _ => String::new(),
        };
        let text = s.clean(sv(node, "value").unwrap_or(""));
        s.style(&text, FG_BRIGHT_MAGENTA) + &number
    })
}

fn inline_link(s: &mut State<'_, '_>, node: &Value) -> R {
    link(s, node)
}

fn inline_image(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok(image(s, node))
}

fn inline_autolink(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let text = s.clean(sv(node, "text").or_else(|| sv(node, "href")).unwrap_or(""));
        if s.x.link_depth > 0 {
            text
        } else {
            s.style(&text, &format!("{UNDERLINE}{FG_BLUE}"))
        }
    })
}

fn inline_xref(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let (text, resolved) = s.x.xref_text(node);
        let text = s.clean(&text);
        if resolved && s.x.link_depth == 0 {
            s.style(&text, &format!("{UNDERLINE}{FG_BLUE}"))
        } else {
            text
        }
    })
}

fn inline_footnote_ref(s: &mut State<'_, '_>, node: &Value) -> R {
    note_ref(s, node)
}

fn inline_abbreviation(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let raw = sv(node, "key").unwrap_or("");
        let term = s.clean(raw);
        let Some(expansion) = sv(node, "expansion") else {
            return Ok(term);
        };
        if s.abbr_depth > 0 || !s.x.draw_derived(node, expansion) {
            return Ok(term);
        }
        s.expanded.insert(pair_key(raw, expansion));
        let tail = format!(" ({})", s.clean(expansion));
        format!("{term}{}", s.style(&tail, DIM))
    })
}

fn inline_ruby(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let target = s.x.target.name();
        s.x.loss(
            node,
            "ruby-flattened",
            format!("Flattened ruby annotation while rendering {target}"),
        );
        let mut out = String::new();
        for pair in list(node, "pairs") {
            out.push_str(&slot_inlines(s, list(pair, "base"))?);
            out.push('(');
            out.push_str(&slot_inlines(s, list(pair, "annotation"))?);
            out.push(')');
        }
        out
    })
}

fn inline_raw_inline(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        s.x.raw_dropped(node, false);
        String::new()
    })
}

fn inline_symbol(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok(format!(":{}:", s.clean(sv(node, "name").unwrap_or(""))))
}

fn inline_mention(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok(match sv(node, "label") {
        Some(label) => s.clean(label),
        None => s.clean(&format!("@{}", sv(node, "target").unwrap_or(""))),
    })
}

fn inline_tag(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok(format!("#{}", s.clean(sv(node, "name").unwrap_or(""))))
}

fn inline_inline_extension(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let dotted = sv(node, "name").is_some_and(|n| n.contains('.'));
        match node.get("fallback").and_then(Value::as_array) {
            Some(fallback) if dotted => slot_inlines(s, fallback)?,
            _ => inlines(s, children(node))?,
        }
    })
}

fn inline_critic_comment(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok({
        let target = s.x.target.name();
        s.x.loss(
            node,
            "editorial-comment-flattened",
            format!("Flattened an editorial comment while rendering {target}"),
        );
        s.clean(sv(node, "value").unwrap_or(""))
    })
}

fn inline_caption_number(_s: &mut State<'_, '_>, node: &Value) -> R {
    Ok(int(node, "n").map_or_else(|| "#".into(), |n| n.to_string()))
}

fn inline_citation_group(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok(s.clean(sv(node, "raw").unwrap_or("")))
}

fn inline_smart_punctuation(s: &mut State<'_, '_>, node: &Value) -> R {
    Ok(s.x.smart_text(node).to_string())
}

fn inline_non_breaking_space(_s: &mut State<'_, '_>, _node: &Value) -> R {
    Ok(NBSP_HOLD.to_string())
}

fn inline_soft_break(_s: &mut State<'_, '_>, _node: &Value) -> R {
    Ok(" ".into())
}

fn inline_hard_break(_s: &mut State<'_, '_>, _node: &Value) -> R {
    Ok("\n".into())
}

/// A span with an authored `abbr` writes its value after the content, and no expansion nested in it.
fn span(s: &mut State<'_, '_>, node: &Value) -> R {
    let Some(abbr) = key_value(node, "abbr") else {
        return inlines(s, children(node));
    };
    s.abbr_depth += 1;
    let inner = inlines(s, children(node));
    s.abbr_depth -= 1;
    let inner = inner?;
    if abbr.is_empty() {
        return Ok(inner);
    }
    let probe = serde_json::json!({ "type": "text", "value": abbr });
    if !s.x.draw_derived(&probe, abbr) {
        return Ok(inner);
    }
    let tail = format!(" ({})", s.clean(abbr));
    Ok(format!("{inner}{}", s.style(&tail, DIM)))
}

fn footnote_section(s: &mut State<'_, '_>, document: &Value) -> R {
    let mut out = String::new();
    for child in footnote_definitions(document) {
        let depth = std::mem::take(&mut s.x.link_depth);
        let body = blocks(s, children(child));
        s.x.link_depth = depth;
        let body = body?;
        let body = trim(&body);
        let key = s.clean(sv(child, "key").unwrap_or(""));
        if s.ansi {
            let mark = s.style(&format!("[^{key}]"), &format!("{FG_CYAN}{DIM}"));
            let _ = writeln!(out, "{mark} {body}");
        } else {
            let _ = writeln!(out, "[^{key}]: {body}");
        }
    }
    Ok(out)
}

struct Keys<'v> {
    footnotes: HashSet<&'v str>,
    referenced: HashSet<&'v str>,
    abbreviations: bool,
}

fn collect_keys(document: &Value) -> Keys<'_> {
    let mut keys = Keys {
        footnotes: HashSet::new(),
        referenced: HashSet::new(),
        abbreviations: false,
    };
    each_node(document, &mut |node| match ty(node) {
        "footnote" => {
            keys.footnotes.insert(sv(node, "key").unwrap_or(""));
        }
        "abbreviation_def" => keys.abbreviations = true,
        "link" | "image" => {
            if let Some(key) = sv(node, "key").filter(|_| !is_unresolved(node)) {
                keys.referenced.insert(key);
            }
        }
        _ => {}
    });
    keys
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    for c in text.chars() {
        if c == '\n' {
            run += 1;
            if run > 2 {
                continue;
            }
        } else {
            run = 0;
        }
        out.push(c);
    }
    out
}

fn run<'v>(
    x: &mut Exporter<'v>,
    ansi: bool,
    keys: &Keys<'v>,
    before: &HashSet<(String, String)>,
) -> Result<(String, HashSet<(String, String)>), Error> {
    let document = x.document;
    let mut s = State {
        x,
        ansi,
        list_depth: 0,
        indent_base: 0,
        inline_level: 0,
        abbr_depth: 0,
        unwrap_strong: 0,
        uncaptioned: None,
        footnotes: &keys.footnotes,
        referenced: &keys.referenced,
        expanded: HashSet::new(),
        expanded_before: before,
    };
    let body = format!(
        "{}{}",
        blocks(&mut s, children(document))?,
        footnote_section(&mut s, document)?
    );
    let body = collapse_blank_lines(&body);
    // The plain target keeps the indentation of its first line, which may be code; ANSI trims both ends.
    let text = if ansi {
        trim(&body)
    } else {
        trim_end(body.trim_start_matches('\n'))
    };
    let text = format!("{text}\n").replace(NBSP_HOLD, " ");
    Ok((strip_bidi(&text), s.expanded))
}

/// The text of a resolved tree: plain, or styled for a terminal.
pub fn export_text(x: &mut Exporter<'_>, ansi: bool) -> R {
    let keys = collect_keys(x.document);
    // A definition's line depends on whether the occurrences after it wrote its
    // expansion, so a document with one runs twice, the first time only to
    // learn that, with its losses and budget thrown away.
    if !keys.abbreviations {
        return Ok(run(x, ansi, &keys, &HashSet::new())?.0);
    }
    let entries = x.loss_count();
    let budget = x.derived_left;
    let (_, expanded) = run(x, ansi, &keys, &HashSet::new())?;
    x.truncate_losses(entries);
    x.derived_left = budget;
    x.derived_reported = false;
    Ok(run(x, ansi, &keys, &expanded)?.0)
}
