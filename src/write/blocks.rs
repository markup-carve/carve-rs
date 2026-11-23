//! The block nodes as Carve source: the layout of C2-W-017 to C2-W-034 and the
//! canonical forms of C2-W-035 to C2-W-060. A block writes its lines behind
//! `prefix`, the prefix of its containers.

use super::attrs::{
    BlockParts, ListMarkers, brace_block, is_name, letter_style_pair, list_markers,
    longest_bare_run, plain_block, quoted, same_list_kind,
};
use super::inline::{
    BARE_BREAKS, IN_FIGURE_GROUP, braced_comment, line_comment_text, write_content, write_inlines,
};
use super::node::{N, caption, children, classes, has, id, int, list, s, text, ty};
use super::state::{Below, Writer, emit, emit_lines, lose, note_place};
use super::table::{table_attribute, write_table};
use crate::view::{Arr, V};
use serde_json::Value;

const MIN_RUN: usize = 3;
const MAX_HEADING_LEVEL: i64 = 6;
/// C2-W-028: the blank lines that keep two lists of one kind apart.
const LIST_BOUNDARY_BLANKS: usize = 3;
pub const HARD_BREAKS_CLASS: &str = "hardbreaks";

fn refs(nodes: Arr<'_>) -> Vec<V<'_>> {
    nodes.iter().collect()
}

pub fn div_kind(name: &str) -> Option<&'static str> {
    crate::tables::DIV_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, k)| *k)
}

/// C2-W-047: the attributes of a block, as one brace block on the line above it.
fn attribute_line(w: &mut Writer<'_>, node: V<'_>, prefix: &str, extra: &BlockParts) {
    if w.prewritten == Some(id(node)) {
        w.prewritten = None;
        return;
    }
    let block = brace_block(node, extra);
    if !block.is_empty() {
        emit(w, prefix, &block);
    }
}

pub fn is_fenced(w: &Writer<'_>, node: V<'_>) -> bool {
    w.fact(node).and_then(|f| f.fenced) == Some(true)
}

fn has_colon_run(w: &Writer<'_>, node: V<'_>) -> bool {
    match ty(node) {
        "block_quote" => is_fenced(w, node),
        "div" | "admonition" | "directive" | "figure_group" | "line_block" => true,
        _ => false,
    }
}

fn runs_below(w: &mut Writer<'_>, node: V<'_>) -> usize {
    if let Some(&k) = w.colon_runs.get(&id(node)) {
        return k;
    }
    let mut longest = 0;
    for c in children(node) {
        let below = runs_below(w, c);
        longest = longest.max(if has_colon_run(w, c) {
            below + 1
        } else {
            below
        });
    }
    if longest > 0 && longest < MIN_RUN {
        longest = MIN_RUN;
    }
    w.colon_runs.insert(id(node), longest);
    longest
}

/// C2-W-039: the shortest colon run of at least three that is longer than the run of every div inside.
fn colon_run(w: &mut Writer<'_>, node: V<'_>) -> String {
    ":".repeat(MIN_RUN.max(runs_below(w, node) + 1))
}

fn is_left_open(w: &Writer<'_>, node: V<'_>) -> bool {
    w.form(node) > 0
}

/// The lines of a verbatim value. C2-W-033: a value with no final line break is written with one.
fn value_lines<'v>(w: &mut Writer<'_>, node: V<'v>) -> Vec<&'v str> {
    let value = text(node);
    if value.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = value.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    } else {
        lose(w, node, Some("value"));
    }
    lines
}

/// C2-W-040: the quoted title and the label of an opener.
fn opener_tail(w: &mut Writer<'_>, node: V<'_>, title: Option<&str>, spelled: &str) -> String {
    let mut out = String::new();
    match title {
        Some(t) if t.contains('\n') => lose(w, node, Some("title")),
        Some(_) => {
            out.push_str(" \"");
            out.push_str(spelled);
            out.push('"');
        }
        None => {}
    }
    let Some(label) = s(node, "groupLabel") else {
        return out;
    };
    if label.contains(']') || label.contains('\n') {
        lose(w, node, Some("groupLabel"));
    } else {
        out.push_str(" [");
        out.push_str(label);
        out.push(']');
    }
    out
}

fn is_language_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"_-#+./".contains(&b)
}

/// Whether `word` reads whole as the language word of a fence, or with `raw` as the format of a raw block.
pub fn is_fence_word(word: &str, raw: bool) -> bool {
    let b = word.as_bytes();
    if word.is_empty() {
        return false;
    }
    if raw {
        return b.first().is_some_and(u8::is_ascii_alphabetic)
            && b.iter()
                .all(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-');
    }
    b.first() != Some(&b'=') && b.iter().all(|&c| is_language_char(c))
}

fn fence_info(w: &mut Writer<'_>, node: V<'_>) -> String {
    if ty(node) == "raw_block" {
        let format = s(node, "format").unwrap_or("");
        if !is_fence_word(format, true) {
            lose(w, node, Some("format"));
        }
        return format!("={format}");
    }
    let syntax = s(node, "syntax").unwrap_or("");
    if syntax.is_empty() || is_fence_word(syntax, false) {
        return syntax.to_string();
    }
    lose(w, node, Some("syntax"));
    String::new()
}

fn code_title(w: &mut Writer<'_>, node: V<'_>) -> Option<String> {
    let title = list(node, "title")?;
    if title.iter().any(|p| ty(p) != "text") {
        lose(w, node, Some("title"));
    }
    let t = super::node::plain_text(title);
    if t.contains('`') {
        lose(w, node, Some("title"));
        return None;
    }
    Some(t)
}

fn code_block(w: &mut Writer<'_>, node: V<'_>, prefix: &str) {
    attribute_line(w, node, prefix, &BlockParts::default());
    let lines = value_lines(w, node);
    let run = "`".repeat(MIN_RUN.max(longest_bare_run(lines.iter().copied(), b'`') + 1));
    let mut info = fence_info(w, node);
    if ty(node) == "code_block" {
        let title = code_title(w, node);
        let spelled = title.as_deref().map_or(String::new(), |t| {
            let q = quoted(t, false);
            q.get(1..q.len() - 1).unwrap_or("").to_string()
        });
        info.push_str(&opener_tail(w, node, title.as_deref(), &spelled));
    }
    emit(w, prefix, &(run.clone() + &info));
    for line in lines {
        emit(w, prefix, line);
    }
    if !is_left_open(w, node) {
        emit(w, prefix, &run);
    }
}

/// C2-W-059: a line comment, and a comment block with the shortest percent run its body allows.
fn comment(w: &mut Writer<'_>, node: V<'_>, prefix: &str) {
    let value = text(node);
    attribute_line(w, node, prefix, &BlockParts::default());
    if s(node, "form") != Some("block") {
        if value.contains('\n') {
            lose(w, node, Some("value"));
        }
        let line = if s(node, "form") == Some("braced") {
            braced_comment(value)
        } else {
            line_comment_text(w, node, value)
        };
        emit(w, prefix, &line);
        return;
    }
    let lines: Vec<&str> = if value.is_empty() {
        Vec::new()
    } else {
        value.split('\n').collect()
    };
    let run = "%".repeat(MIN_RUN.max(longest_bare_run(lines.iter().copied(), b'%') + 1));
    emit(w, prefix, &run);
    for line in lines {
        emit(w, prefix, line);
    }
    if !is_left_open(w, node) {
        emit(w, prefix, &run);
    }
}

fn frontmatter(w: &mut Writer<'_>, node: V<'_>, prefix: &str) {
    let format = s(node, "format").unwrap_or("yaml");
    let leading = w.leading_frontmatter == Some(id(node));
    if !leading {
        attribute_line(w, node, prefix, &BlockParts::default());
    }
    emit(
        w,
        prefix,
        &if format == "yaml" && leading {
            "---".to_string()
        } else {
            format!("---{format}")
        },
    );
    for line in value_lines(w, node) {
        emit(w, prefix, line);
    }
    emit(w, prefix, "---");
}

fn text_leaf<'t>(
    w: &mut Writer<'t>,
    node: N<'t>,
    prefix: &str,
    flags: u32,
    mark: &str,
    indent: &str,
) {
    if children(node).is_empty() {
        lose(w, node, None);
        return;
    }
    attribute_line(w, node, prefix, &BlockParts::default());
    if ty(node) == "paragraph" {
        note_place(w, node, prefix);
    }
    let top = w.lead.is_none() && prefix.is_empty() && w.top_blocks.contains(&id(node));
    w.lead = Some(w.lead.take().unwrap_or_else(|| prefix.to_string()) + mark);
    let mut content = write_content(w, &refs(children(node)), flags);
    // C2-W-154: no escape keeps an em dash node, so a first line that reads as a
    // typed metadata opener is indented by one space, which C2-B-232 leaves as text.
    let below = w.below_list.get(&id(node)).copied();
    if below == Some(Below::Unspellable) {
        lose(w, node, None);
    }
    if top && ty(node) == "paragraph" && below != Some(Below::Direct) {
        let first = content.split('\n').next().unwrap_or("");
        if crate::block::shapes::typed_frontmatter_opener(first) {
            content.insert(0, ' ');
        }
    }
    emit_lines(w, prefix, indent, &content);
}

fn heading<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    let mut level = int(node, "level").unwrap_or(1).max(1);
    if level > MAX_HEADING_LEVEL {
        lose(w, node, Some("level"));
        level = MAX_HEADING_LEVEL;
    }
    let n = usize::try_from(level).unwrap_or(1);
    text_leaf(
        w,
        node,
        prefix,
        flags,
        &format!("{} ", "=".repeat(n)),
        &" ".repeat(n + 1),
    );
}

fn path_quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn is_bare_path(path: &str) -> bool {
    !path.is_empty()
        && !path
            .chars()
            .any(|c| matches!(c, ' ' | '#' | '@' | '}' | '"' | '\u{201C}'))
}

fn is_bare_value(value: &str) -> bool {
    !value.starts_with(['"', '\'']) && !value.chars().any(|c| matches!(c, ' ' | '\t' | '@' | '}'))
}

fn decoded_spelling(name: &str, spelling: &str) -> Option<Value> {
    crate::block::include::parse(&format!("{{{{ x @{name}:{spelling} }}}}"))?
        .options
        .get(name)
        .cloned()
}

fn include_option(name: &str, value: V<'_>, spelling: Option<&str>) -> String {
    if value.as_bool() == Some(true) {
        return format!("@{name}");
    }
    if let Some(sp) = spelling {
        if decoded_spelling(name, sp) == Some(value.to_value()) {
            return format!("@{name}:{sp}");
        }
    }
    if let Some(items) = value.as_array() {
        let names: Vec<String> = items
            .iter()
            .filter_map(V::as_str)
            .map(|p| {
                p.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace(',', "\\,")
            })
            .collect();
        return format!("@{name}:\"{}\"", names.join(","));
    }
    let text = value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_string);
    if text.is_empty() {
        return format!("@{name}");
    }
    if is_bare_value(&text) {
        format!("@{name}:{text}")
    } else {
        format!("@{name}:{}", path_quoted(&text))
    }
}

/// C2-W-058, C2-H-171: the one spelling of an include directive.
fn include_source(w: &Writer<'_>, node: V<'_>) -> String {
    let spelling = w.fact(node).and_then(|f| f.option_spelling.as_ref());
    include_spelled(node, spelling)
}

/// The spelling of an include directive with no source facts.
pub fn include_spelling<'t>(node: impl Into<V<'t>>) -> String {
    include_spelled(node.into(), None)
}

fn include_spelled(
    node: V<'_>,
    spelling: Option<&std::collections::BTreeMap<String, String>>,
) -> String {
    let path = s(node, "path").unwrap_or("");
    let mut parts = vec![if is_bare_path(path) {
        path.to_string()
    } else {
        path_quoted(path)
    }];
    if let Some(section) = s(node, "section") {
        parts.push(format!("#{section}"));
    }
    let options = node.get("options");
    for (_, name, _, _) in crate::tables::INCLUDE_OPTIONS {
        if let Some(value) = options.and_then(|o| o.get(name)) {
            parts.push(include_option(
                name,
                value,
                spelling.and_then(|sp| sp.get(*name)).map(String::as_str),
            ));
        }
    }
    for entry in list(node, "invalid").unwrap_or_else(Arr::empty) {
        parts.push(entry.as_str().unwrap_or("").to_string());
    }
    format!("{{{{ {} }}}}", parts.join(" "))
}

/// C2-W-058: the field of `node` that its spelling `source` does not read back.
fn unread_field(node: V<'_>, source: &str) -> Option<&'static str> {
    let Some(parts) = crate::block::include::parse(source) else {
        return Some("invalid");
    };
    let options = node.get("options");
    for (_, name, _, _) in crate::tables::INCLUDE_OPTIONS {
        if parts.options.get(*name) != options.and_then(|o| o.get(name)).map(V::to_value).as_ref() {
            return Some("options");
        }
    }
    let invalid: Vec<&str> = list(node, "invalid")
        .unwrap_or_else(Arr::empty)
        .iter()
        .filter_map(V::as_str)
        .collect();
    (parts
        .invalid
        .iter()
        .map(String::as_str)
        .ne(invalid.iter().copied()))
    .then_some("invalid")
}

fn include<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    attribute_line(w, node, prefix, &BlockParts::default());
    let path = s(node, "path").unwrap_or("");
    if path.is_empty() || path.contains('\n') {
        lose(w, node, Some("path"));
    }
    let source = include_source(w, node);
    if let Some(field) = unread_field(node, &source) {
        lose(w, node, Some(field));
    }
    emit(w, prefix, &source);
    write_caption(w, caption(node), prefix, flags);
}

fn abbreviation_definition(w: &mut Writer<'_>, node: V<'_>, prefix: &str) {
    attribute_line(w, node, prefix, &BlockParts::default());
    let original = s(node, "expansion").unwrap_or("");
    let expansion = original.replace('\n', " ");
    let kept = expansion.trim_end_matches([' ', '\t']);
    if kept.len() < original.len() || expansion != original {
        lose(w, node, Some("expansion"));
    }
    emit(
        w,
        prefix,
        &format!("*[{}]: {kept}", s(node, "key").unwrap_or("")),
    );
}

/// C2-I-079: a key of only spaces, tabs and line feeds folds to nothing.
fn lose_blank_key(w: &mut Writer<'_>, node: V<'_>) {
    if s(node, "key")
        .unwrap_or("")
        .chars()
        .all(|c| matches!(c, ' ' | '\t' | '\n'))
    {
        lose(w, node, Some("key"));
    }
}

fn definition_destination(href: &str) -> String {
    let b = href.as_bytes();
    let mut out = String::with_capacity(href.len());
    for (at, c) in href.char_indices() {
        if c == '\\' && b.get(at + 1).is_some_and(|n| b"()\\".contains(n)) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn link_definition(w: &mut Writer<'_>, node: V<'_>, prefix: &str) {
    attribute_line(w, node, prefix, &BlockParts::default());
    lose_blank_key(w, node);
    let href = definition_destination(s(node, "href").unwrap_or(""));
    if href.is_empty() {
        lose(w, node, Some("href"));
    }
    let title = s(node, "tooltip").map_or(String::new(), |t| format!(" {}", quoted(t, false)));
    let gap = if href.is_empty() && title.is_empty() {
        ""
    } else {
        " "
    };
    emit(
        w,
        prefix,
        &format!("[{}]:{gap}{href}{title}", s(node, "key").unwrap_or("")),
    );
}

/// C2-W-024, C2-W-027: the blocks of a body behind its marker, the first on the marker line.
fn body<'t>(
    w: &mut Writer<'t>,
    node: N<'t>,
    prefix: &str,
    flags: u32,
    mark: &str,
    width: usize,
    tight: bool,
) {
    let behind = w
        .lead
        .as_ref()
        .map_or(0, |l| l.len().saturating_sub(prefix.len()));
    w.lead = Some(w.lead.take().unwrap_or_else(|| prefix.to_string()) + mark);
    let inner = format!("{prefix}{}", " ".repeat(behind + width));
    write_blocks(w, children(node), &inner, flags, tight);
    if w.lead.is_none() {
        return;
    }
    // C2-W-057: an empty body has no spelling.
    lose(w, node, None);
    emit(w, prefix, "");
}

fn footnote<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    attribute_line(w, node, prefix, &BlockParts::default());
    lose_blank_key(w, node);
    let mark = format!("[^{}]: ", s(node, "key").unwrap_or(""));
    let width = if children(node)
        .first()
        .is_some_and(|c| ty(c) == "definition_list")
    {
        mark.len()
    } else {
        2
    };
    body(w, node, prefix, flags, &mark, width, false);
}

/// C2-W-025, C2-B-040: an uppercase marker with a dot takes two spaces.
fn marker_gap(marker: &str) -> &'static str {
    let first = marker.bytes().next().unwrap_or(b'-');
    if marker.len() >= 2 && marker.ends_with('.') && first.is_ascii_uppercase() {
        "  "
    } else {
        " "
    }
}

fn list_item<'t>(
    w: &mut Writer<'t>,
    node: N<'t>,
    prefix: &str,
    flags: u32,
    marker: &str,
    tight: bool,
) {
    let head = format!("{marker}{}", plain_block(node));
    let gap = marker_gap(marker);
    let task = node
        .get("task")
        .and_then(|t| s(t, "mark"))
        .map_or(String::new(), |m| format!("[{m}] "));
    w.bare_dot_lead = marker == ".";
    body(
        w,
        node,
        prefix,
        flags,
        &format!("{head}{gap}{task}"),
        marker.len() + gap.len(),
        tight,
    );
}

fn list_attributes(node: V<'_>, start: Option<&String>) -> String {
    let mut parts = BlockParts::default();
    if let Some(st) = start {
        parts.keys.push(("start".into(), st.clone()));
    }
    brace_block(node, &parts)
}

fn write_list<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    let items = children(node);
    if items.is_empty() {
        lose(w, node, None);
        return;
    }
    let ListMarkers { markers, start } = list_markers(w, node);
    let block = list_attributes(node, start.as_ref());
    if w.prewritten == Some(id(node)) {
        w.prewritten = None;
    } else if !block.is_empty() {
        emit(w, prefix, &block);
    }
    let tight = s(node, "spacing") != Some("loose");
    for (at, item) in items.iter().enumerate() {
        if at > 0 && !tight {
            emit(w, prefix, "");
        }
        list_item(
            w,
            item,
            prefix,
            flags,
            markers.get(at).map_or("-", String::as_str),
            tight,
        );
    }
}

fn definition_list<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    if children(node).is_empty() {
        lose(w, node, None);
        return;
    }
    attribute_line(w, node, prefix, &BlockParts::default());
    write_siblings(w, children(node), prefix, flags, Layout::Entries);
}

/// A block between an opener and a closer of one colon run. C2-W-032: an empty body is one blank line.
fn fenced<'t>(
    w: &mut Writer<'t>,
    node: V<'_>,
    prefix: &str,
    opener: &str,
    blank_when_empty: bool,
    write: &mut dyn FnMut(&mut Writer<'t>),
) {
    let run = colon_run(w, node);
    emit(w, prefix, &format!("{run}{opener}"));
    let mark = w.lines.len();
    write(w);
    if w.lines.len() == mark && blank_when_empty {
        emit(w, prefix, "");
    }
    emit(w, prefix, &run);
}

fn block_quote<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    attribute_line(w, node, prefix, &BlockParts::default());
    let kids = children(node);
    if is_fenced(w, node) {
        fenced(w, node, prefix, " >", false, &mut |w| {
            write_blocks(w, kids, prefix, flags, false);
        });
        write_caption(w, caption(node), prefix, flags);
        return;
    }
    if kids.is_empty() {
        emit(w, prefix, ">");
        write_caption(w, caption(node), prefix, flags);
        return;
    }
    w.lead = Some(w.lead.take().unwrap_or_else(|| prefix.to_string()) + "> ");
    write_blocks(w, kids, &format!("{prefix}> "), flags, false);
    if w.lead.is_some() {
        emit(w, prefix, "");
    }
    write_caption(w, caption(node), prefix, flags);
}

fn title_tail<'t>(w: &mut Writer<'t>, node: N<'t>, flags: u32) -> String {
    let title = list(node, "title").map(|t| write_inlines(w, &refs(t), flags));
    let spelled = title.as_deref().map_or(String::new(), |t| {
        let q = quoted(t, false);
        q.get(1..q.len() - 1).unwrap_or("").to_string()
    });
    opener_tail(w, node, title.as_deref(), &spelled)
}

/// C2-W-039: the name of a plain div is its first class, where that class can be a name with no other meaning.
fn div_name<'n>(w: &Writer<'_>, node: V<'n>) -> Option<&'n str> {
    let first = classes(node).into_iter().next()?;
    if !is_name(first) || div_kind(first).is_some() {
        return None;
    }
    (!w.matchers
        .iter()
        .any(|m| m.form == "div" && m.name == first))
    .then_some(first)
}

/// C2-W-081: the opener of a hard-break block stands for the class, which it adds last.
fn uses_hard_break_opener(w: &Writer<'_>, node: V<'_>) -> bool {
    if classes(node).last() != Some(&HARD_BREAKS_CLASS)
        || has(node, "title")
        || has(node, "groupLabel")
    {
        return false;
    }
    w.fact(node)
        .and_then(|f| f.hard_break_opener)
        .unwrap_or(true)
}

fn skip_first() -> BlockParts {
    BlockParts {
        skip_classes: vec![0],
        ..BlockParts::default()
    }
}

fn div<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    let count = classes(node).len();
    if uses_hard_break_opener(w, node) {
        let parts = BlockParts {
            skip_classes: vec![count.saturating_sub(1)],
            ..BlockParts::default()
        };
        attribute_line(w, node, prefix, &parts);
        fenced(w, node, prefix, " \\", true, &mut |w| {
            write_blocks(w, children(node), prefix, flags | BARE_BREAKS, false);
        });
        return;
    }
    let named = div_name(w, node);
    let name = if w.prewritten == Some(id(node))
        && named.is_some()
        && brace_block(node, &skip_first()).is_empty()
    {
        None
    } else {
        named
    };
    let parts = if name.is_some() {
        skip_first()
    } else {
        BlockParts::default()
    };
    attribute_line(w, node, prefix, &parts);
    let opener = name.map_or(String::new(), |n| format!(" {n}")) + &title_tail(w, node, flags);
    fenced(w, node, prefix, &opener, true, &mut |w| {
        write_blocks(w, children(node), prefix, flags, false);
    });
}

fn named_div<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    attribute_line(w, node, prefix, &BlockParts::default());
    if ty(node) == "directive" && has(node, "tocTarget") {
        lose(w, node, Some("tocTarget"));
    }
    let opener = format!(
        " {}{}",
        s(node, "kind").unwrap_or(""),
        title_tail(w, node, flags)
    );
    fenced(w, node, prefix, &opener, true, &mut |w| {
        write_blocks(w, children(node), prefix, flags, false);
    });
}

/// C2-W-053: `^`, one space and the content, directly below the block.
fn write_caption<'t>(w: &mut Writer<'t>, node: Option<N<'t>>, prefix: &str, flags: u32) {
    let Some(node) = node else { return };
    if has(node, "short") {
        lose(w, node, Some("short"));
    }
    let blocks = children(node);
    if blocks.len() != 1 {
        lose(w, node, None);
    }
    let mut parts = Vec::new();
    for block in blocks {
        parts.push(write_content(w, &refs(children(block)), flags));
    }
    w.lead = Some(w.lead.take().unwrap_or_else(|| prefix.to_string()) + "^ ");
    emit_lines(w, prefix, "  ", &parts.join(" "));
}

fn trailing_comments(nodes: Arr<'_>) -> usize {
    nodes
        .iter()
        .rev()
        .take_while(|n| ty(n) == "comment")
        .count()
}

fn figure_group<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    if flags & IN_FIGURE_GROUP != 0 {
        lose(w, node, None);
    }
    attribute_line(w, node, prefix, &BlockParts::default());
    let kids = children(node);
    let cap = caption(node);
    let after = if cap.is_none() {
        0
    } else {
        trailing_comments(kids)
    };
    let split = kids.len() - after;
    let (inside, late) = (kids.slice(0, split).unwrap_or(kids), kids.from(split));
    fenced(w, node, prefix, " figure", true, &mut |w| {
        write_blocks(w, inside, prefix, flags | IN_FIGURE_GROUP, false);
    });
    for c in late {
        comment(w, c, prefix);
    }
    write_caption(w, cap, prefix, flags);
}

/// C2-W-059, C2-A-082: a code block with a caption, the comments behind it, then the caption.
fn captioned_code<'t>(
    w: &mut Writer<'t>,
    nodes: Arr<'t>,
    at: usize,
    prefix: &str,
    flags: u32,
) -> usize {
    let Some(node) = nodes.get(at) else { return at };
    code_block(w, node, prefix);
    let mut last = at;
    while let Some(next) = nodes.get(last + 1) {
        if ty(next) != "comment" || s(next, "form") != Some("line") {
            break;
        }
        comment(w, next, prefix);
        last += 1;
    }
    write_caption(w, caption(node), prefix, flags);
    last
}

fn line_block<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    attribute_line(w, node, prefix, &BlockParts::default());
    fenced(w, node, prefix, " |", false, &mut |w| {
        for line in children(node) {
            if ty(line) == "comment" {
                comment(w, line, prefix);
                continue;
            }
            let kids = children(line);
            let lazy = kids
                .last()
                .filter(|l| ty(l) == "comment" && s(l, "form") == Some("line"));
            let content = if lazy.is_some() {
                kids.slice(0, kids.len() - 1).unwrap_or_else(Arr::empty)
            } else {
                kids
            };
            let written = line_content(w, &refs(content), flags | BARE_BREAKS);
            emit(w, prefix, &written);
            if let Some(lazy) = lazy {
                lazy_comment(w, lazy, prefix);
            }
        }
    });
}

fn lazy_comment(w: &mut Writer<'_>, node: V<'_>, prefix: &str) {
    if prefix.is_empty() {
        lose(w, node, None);
        return;
    }
    if text(node).contains('\n') {
        lose(w, node, Some("value"));
    }
    let line = line_comment_text(w, node, &text(node).replace('\n', " "));
    emit(w, "", &line);
}

fn line_content<'t>(w: &mut Writer<'t>, nodes: &[N<'t>], flags: u32) -> String {
    match nodes.last() {
        Some(last) if ty(last) == "hard_break" => {
            let head = write_inlines(w, nodes.get(..nodes.len() - 1).unwrap_or(&[]), flags);
            format!("{head}\\")
        }
        _ => write_content(w, nodes, flags),
    }
}

fn children_only<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    lose(w, node, None);
    let kids = list(node, "fallback").unwrap_or_else(|| children(node));
    write_blocks(w, kids, prefix, flags, false);
}

/// C2-X-025: whether a matcher of `form` can claim `name`.
pub fn is_claimable_name(form: &str, name: &str, titled: bool) -> bool {
    match form {
        "inline" => {
            let b = name.as_bytes();
            b.first()
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
                && b.iter()
                    .all(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        }
        "fence" => is_fence_word(name, false),
        _ => {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                && div_kind(name).is_none_or(|k| k == "figure_group" && titled)
        }
    }
}

fn source_lines(body: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = body.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

fn extension_spelling(node: V<'_>) -> Option<String> {
    let source = node.get("source")?;
    let form = s(source, "form")?;
    if form == "inline" {
        return None;
    }
    let name = s(source, "name").unwrap_or("");
    if !is_claimable_name(
        form,
        name,
        has(source, "title") || has(source, "groupLabel"),
    ) {
        return None;
    }
    let body = s(source, "body").unwrap_or("");
    let lines = source_lines(body);
    let mark = if form == "div" { b':' } else { b'`' };
    let run = char::from(mark)
        .to_string()
        .repeat(MIN_RUN.max(longest_bare_run(lines.iter().copied(), mark) + 1));
    let title = s(source, "title").map_or(String::new(), |t| format!(" {}", quoted(t, false)));
    let label = s(source, "groupLabel").map_or(String::new(), |l| format!(" [{l}]"));
    let opener = if form == "div" {
        format!("{run} {name}{title}{label}")
    } else {
        format!("{run}{name}{title}{label}")
    };
    let mut out = vec![opener];
    out.extend(lines.into_iter().map(str::to_string));
    out.push(run);
    Some(out.join("\n") + "\n")
}

fn block_extension<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    if let Some(spelling) = extension_spelling(node) {
        let keys = if w.extensions.is_empty() {
            super::LinkKeys::default()
        } else {
            std::sync::Arc::clone(&w.link_keys)
        };
        if super::extension::canonical_body(node, &keys) {
            let attrs = plain_block(node);
            let probe = if attrs.is_empty() {
                spelling.clone()
            } else {
                format!("{attrs}\n{spelling}")
            };
            let chunk = super::check::Chunk::new(probe);
            let same = if w.extensions.is_empty() {
                let parsed = children(chunk.parse_keys(w, None)).first();
                super::extension::source_core_equivalent(node, parsed)
            } else {
                // C2-X-026: the body was written in the document's context, so a
                // shortcut reference in it reads back with the document's keys.
                let parsed = children(chunk.parse(w)).first();
                parsed.is_some_and(|p| super::node::trees_equal(node, p))
            };
            if same {
                attribute_line(w, node, prefix, &BlockParts::default());
                for line in spelling.trim_end().split('\n') {
                    emit(w, prefix, line);
                }
                return;
            }
        }
    }
    lose(w, node, None);
    write_blocks(
        w,
        list(node, "fallback").unwrap_or_else(Arr::empty),
        prefix,
        flags,
        false,
    );
}

fn write_block<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    match ty(node) {
        "paragraph" => {
            text_leaf(w, node, prefix, flags, "", "");
            write_caption(w, caption(node), prefix, flags);
        }
        "heading" => heading(w, node, prefix, flags),
        "thematic_break" => {
            attribute_line(w, node, prefix, &BlockParts::default());
            emit(w, prefix, "***");
        }
        "code_block" => {
            code_block(w, node, prefix);
            write_caption(w, caption(node), prefix, flags);
        }
        "table" => {
            write_table(w, node, prefix, flags);
            write_caption(w, caption(node), prefix, flags);
        }
        "raw_block" => code_block(w, node, prefix),
        "comment" => comment(w, node, prefix),
        "frontmatter" => frontmatter(w, node, prefix),
        "include" => include(w, node, prefix, flags),
        "link_reference_definition" => link_definition(w, node, prefix),
        "abbreviation_def" => abbreviation_definition(w, node, prefix),
        _ => container_block(w, node, prefix, flags),
    }
}

fn container_block<'t>(w: &mut Writer<'t>, node: N<'t>, prefix: &str, flags: u32) {
    match ty(node) {
        "footnote" => footnote(w, node, prefix, flags),
        "list" => write_list(w, node, prefix, flags),
        "definition_term" => text_leaf(w, node, prefix, flags, ":: ", "   "),
        "definition_description" => body(w, node, prefix, flags, ": ", 2, false),
        "definition_list" => definition_list(w, node, prefix, flags),
        "block_quote" => block_quote(w, node, prefix, flags),
        "div" => div(w, node, prefix, flags),
        "admonition" | "directive" => named_div(w, node, prefix, flags),
        "figure_group" => figure_group(w, node, prefix, flags),
        "line_block" => line_block(w, node, prefix, flags),
        "block_extension" => block_extension(w, node, prefix, flags),
        _ => children_only(w, node, prefix, flags),
    }
}

const LAZY_CONTAINERS: &[&str] = &[
    "list",
    "list_item",
    "block_quote",
    "definition_list",
    "definition_description",
    "footnote",
];
const OPEN_CONTAINERS: &[&str] = &[
    "list",
    "list_item",
    "definition_list",
    "definition_description",
    "footnote",
];

/// Whether a comment-only line directly below `node` would be read into it.
fn takes_line_below(w: &Writer<'_>, node: V<'_>) -> bool {
    let t = ty(node);
    if matches!(t, "paragraph" | "heading" | "definition_term" | "table") || caption(node).is_some()
    {
        return true;
    }
    if !LAZY_CONTAINERS.contains(&t) || is_fenced(w, node) {
        return false;
    }
    if t == "block_quote" {
        return true;
    }
    children(node)
        .last()
        .is_some_and(|l| takes_line_below(w, l))
}

fn is_open_container(w: &Writer<'_>, node: V<'_>) -> bool {
    OPEN_CONTAINERS.contains(&ty(node)) && !is_fenced(w, node)
}

fn last_fence<'t>(w: &Writer<'_>, node: N<'t>) -> Option<N<'t>> {
    let mut at = node;
    while is_open_container(w, at) {
        at = children(at).last()?;
    }
    (matches!(ty(at), "code_block" | "raw_block")
        || (ty(at) == "comment" && s(at, "form") == Some("block")))
    .then_some(at)
}

fn is_comment_line(node: Option<V<'_>>) -> bool {
    node.is_some_and(|n| ty(n) == "comment" && s(n, "form") != Some("block"))
}

/// The index of the first block behind `at` that is no comment line, or `nodes.len()`.
pub fn comment_lines_end(nodes: Arr<'_>, at: usize) -> usize {
    let mut next = at + 1;
    while is_comment_line(nodes.get(next)) {
        next += 1;
    }
    next
}

fn is_region_marker(node: V<'_>) -> bool {
    ty(node) == "comment"
        && s(node, "form") == Some("line")
        && crate::include::lines::is_carve_region_marker(&format!("%% {}", text(node)))
}

fn blank_lines_before_comment(w: &mut Writer<'_>, before: V<'_>, node: V<'_>) -> usize {
    if ty(before) == "code_block"
        && caption(before).is_some()
        && ty(node) == "comment"
        && s(node, "form") == Some("line")
    {
        return 0;
    }
    if is_comment_line(Some(node)) && is_open_container(w, before) && !is_region_marker(node) {
        if last_fence(w, before).is_some() {
            return 0;
        }
        lose(w, node, None);
        return 1;
    }
    usize::from(takes_line_below(w, before) || w.form(node) > 0)
}

pub fn following_attributes(w: &Writer<'_>, node: Option<V<'_>>) -> String {
    let Some(node) = node else {
        return String::new();
    };
    match ty(node) {
        "list" => list_attributes(node, list_markers(w, node).start.as_ref()),
        "div" => {
            let count = classes(node).len();
            if uses_hard_break_opener(w, node) {
                return brace_block(
                    node,
                    &BlockParts {
                        skip_classes: vec![count.saturating_sub(1)],
                        ..BlockParts::default()
                    },
                );
            }
            let name = div_name(w, node);
            let block = brace_block(
                node,
                &if name.is_some() {
                    skip_first()
                } else {
                    BlockParts::default()
                },
            );
            if block.is_empty() && name.is_some() {
                plain_block(node)
            } else {
                block
            }
        }
        "table" => {
            let a = table_attribute(w, node);
            if a.is_empty() {
                "{footer-rows=0}".into()
            } else {
                a
            }
        }
        _ => plain_block(node),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    Blocks,
    TightItem,
    Entries,
}

/// C2-W-017, C2-W-018, C2-W-028, C2-W-059: the blank lines between `before` and the block `node` below it.
pub fn blank_lines_between(
    w: &mut Writer<'_>,
    before: V<'_>,
    list_before: Option<V<'_>>,
    node: V<'_>,
    nested: bool,
) -> usize {
    if ty(node) == "comment" {
        return blank_lines_before_comment(w, before, node);
    }
    if w.below_list.get(&id(node)) == Some(&Below::Direct) {
        return 0;
    }
    if let Some(l) = list_before {
        if same_list_kind(l, node) {
            return LIST_BOUNDARY_BLANKS;
        }
        if letter_style_pair(l, node) {
            // The next list's attribute line would be a lazy line of the open paragraph above it (C2-B-024).
            let attrs = list_attributes(node, list_markers(w, node).start.as_ref());
            return usize::from(!attrs.is_empty() && takes_line_below(w, l));
        }
    }
    usize::from(!nested)
}

/// Whether a column-0 line directly below `node` would be a lazy line of a block in it (C2-B-024).
fn takes_lazy_line(w: &Writer<'_>, node: V<'_>) -> bool {
    match ty(node) {
        "paragraph" | "definition_term" => true,
        _ if is_open_container(w, node) || (ty(node) == "block_quote" && !is_fenced(w, node)) => {
            children(node)
                .last()
                .is_some_and(|last| takes_lazy_line(w, last))
        }
        _ => false,
    }
}

/// Whether the first line of a paragraph, an em dash node with a format name behind it, reads as a typed opener.
fn starts_typed_opener(node: V<'_>) -> bool {
    let kids = children(node);
    if !kids
        .first()
        .is_some_and(|k| ty(k) == "smart_punctuation" && s(k, "source") == Some("---"))
    {
        return false;
    }
    let mut line = String::from("---");
    for k in kids.iter().skip(1) {
        match ty(k) {
            "soft_break" | "hard_break" => break,
            "text" => line.push_str(text(k).split('\n').next().unwrap_or("")),
            _ => return false,
        }
    }
    crate::block::shapes::typed_frontmatter_opener(&line)
}

/// A paragraph without attributes whose first line would read as a typed opener (C2-W-154).
pub fn typed_opener_paragraph(node: V<'_>) -> bool {
    ty(node) == "paragraph" && !has(node, "attrs") && starts_typed_opener(node)
}

/// C2-W-154, C2-W-155: the spelling of each top-level paragraph without attributes whose first line would read as a
/// typed opener, directly after a list, footnote or description list, and the top-level blocks written one space in
/// for it. Directly below a container whose last block takes no lazy line the opener closes it (C2-B-026, C2-B-232).
pub fn below_list(
    w: &Writer<'_>,
    nodes: Arr<'_>,
) -> (
    crate::hash::AddrMap<usize, Below>,
    crate::hash::AddrMap<usize, bool>,
) {
    let mut out = crate::hash::AddrMap::default();
    let mut indented = crate::hash::AddrMap::default();
    for at in 1..nodes.len() {
        let (Some(before), Some(node)) = (nodes.get(at - 1), nodes.get(at)) else {
            continue;
        };
        if !matches!(ty(before), "list" | "footnote" | "definition_list")
            || !typed_opener_paragraph(node)
        {
            continue;
        }
        let below = if !takes_lazy_line(w, before) {
            Below::Direct
        } else if ty(before) == "footnote" {
            Below::Unspellable
        } else if let Some(run) = indent_run(w, nodes, at - 1) {
            indented.extend(run);
            Below::Indent
        } else {
            Below::Unspellable
        };
        out.insert(id(node), below);
    }
    (out, indented)
}

/// The smallest departure that keeps a block written one space in out of the open container directly above it
/// (C2-W-005, C2-W-155).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Departure {
    /// Its attribute line stands at column 0 and closes the container (C2-B-097).
    AttributeLine,
    /// The container is written one space in as well.
    IndentAbove,
}

/// C2-W-155: the departures in the order the clause takes them.
const DEPARTURES: [Departure; 2] = [Departure::AttributeLine, Departure::IndentAbove];

/// C2-W-155: the run of top-level blocks written one space in, from the block at `from` upward, each with whether its
/// attribute line stays at column 0. None where the run reaches a footnote, which has no indented spelling.
fn indent_run(w: &Writer<'_>, nodes: Arr<'_>, from: usize) -> Option<Vec<(usize, bool)>> {
    let mut run = Vec::new();
    let mut at = from;
    loop {
        let node = nodes.get(at)?;
        // A block comment closes the container above it; a comment line does not (C2-B-137).
        let above_at = (0..at).rev().find(|&i| !is_comment_line(nodes.get(i)));
        let Some(above) = above_at
            .and_then(|i| nodes.get(i))
            .filter(|a| matches!(ty(a), "list" | "definition_list" | "footnote"))
        else {
            run.push((id(node), false));
            return Some(run);
        };
        let departure = DEPARTURES.into_iter().find(|d| match d {
            Departure::AttributeLine => has_attribute_line(w, node),
            Departure::IndentAbove => ty(above) != "footnote",
        })?;
        // C2-B-137: a comment line closes no container, and below the blank line the container takes it too.
        if above_at != at.checked_sub(1) {
            return None;
        }
        run.push((id(node), departure == Departure::AttributeLine));
        if departure == Departure::AttributeLine {
            return Some(run);
        }
        at -= 1;
    }
}

/// Whether the list or description list `node` is written with an attribute line above its first line.
fn has_attribute_line(w: &Writer<'_>, node: V<'_>) -> bool {
    if ty(node) == "list" {
        !list_attributes(node, list_markers(w, node).start.as_ref()).is_empty()
    } else {
        !brace_block(node, &BlockParts::default()).is_empty()
    }
}

fn is_nested_list(w: &Writer<'_>, before: V<'_>, node: V<'_>, at: usize) -> bool {
    if at != 1 || ty(node) != "list" || ty(before) != "paragraph" {
        return false;
    }
    let marker = node.get("marker");
    if marker.and_then(|m| s(m, "kind")) == Some("ordered")
        && marker
            .and_then(|m| m.get("start"))
            .and_then(V::as_i64)
            .unwrap_or(1)
            != 1
    {
        return false;
    }
    list_attributes(node, list_markers(w, node).start.as_ref()).is_empty()
}

fn mark_fence_before_comment(w: &mut Writer<'_>, node: V<'_>, next: Option<V<'_>>) {
    if !is_comment_line(next) || !is_open_container(w, node) {
        return;
    }
    if let Some(fence) = last_fence(w, node) {
        w.forms.insert(id(fence), 1);
    }
}

fn before_sibling<'t>(
    w: &mut Writer<'t>,
    nodes: Arr<'t>,
    at: usize,
    (before, following): (Option<N<'t>>, Option<V<'t>>),
    list_before: Option<N<'t>>,
    prefix: &str,
    layout: Layout,
) {
    let Some(node) = nodes.get(at) else { return };
    let attributes = match before {
        Some(b)
            if is_comment_line(Some(node))
                && is_open_container(w, b)
                && w.prewritten != following.map(id) =>
        {
            following_attributes(w, following)
        }
        _ => String::new(),
    };
    if let Some(b) = before {
        let nested = layout == Layout::Entries
            || (layout == Layout::TightItem && is_nested_list(w, b, node, at));
        let blanks = if !attributes.is_empty() {
            1
        } else if w.prewritten == Some(id(node)) {
            0
        } else {
            blank_lines_between(w, b, list_before, node, nested)
        };
        for _ in 0..blanks {
            emit(w, prefix, "");
        }
    }
    if let (false, Some(f)) = (attributes.is_empty(), following) {
        emit(w, prefix, &attributes);
        w.prewritten = Some(id(f));
    }
}

fn write_siblings<'t>(
    w: &mut Writer<'t>,
    nodes: Arr<'t>,
    prefix: &str,
    flags: u32,
    layout: Layout,
) {
    let mut before: Option<N<'t>> = None;
    let mut list_before: Option<N<'t>> = None;
    // Kept across a run of comment lines, which is walked once rather than once per comment.
    let mut after = 0;
    let mut at = 0;
    while let Some(node) = nodes.get(at) {
        let start = w.lines.len();
        if after <= at {
            after = comment_lines_end(nodes, at);
        }
        before_sibling(
            w,
            nodes,
            at,
            (before, nodes.get(after)),
            list_before,
            prefix,
            layout,
        );
        mark_fence_before_comment(w, node, nodes.get(at + 1));
        let written = w.lines.len();
        if ty(node) == "code_block" && caption(node).is_some() {
            at = captioned_code(w, nodes, at, prefix, flags);
        } else if let Some(&column_0) = w.indented.get(&id(node)) {
            // C2-W-155: one space in; an attribute line at column 0 closes the container above.
            write_block(w, node, &format!("{prefix} "), flags);
            if column_0 && let Some(line) = w.lines.get_mut(written) {
                line.remove(0);
            }
        } else {
            write_block(w, node, prefix, flags);
        }
        at += 1;
        if w.lines.len() == written {
            // C2-W-029: a block that writes nothing leaves no blank line behind.
            w.lines.truncate(start);
            continue;
        }
        before = Some(node);
        if ty(node) != "comment" {
            list_before = Some(node);
        }
    }
}

/// Writes sibling blocks with the blank lines between them (C2-W-018).
pub fn write_blocks<'t>(w: &mut Writer<'t>, nodes: Arr<'t>, prefix: &str, flags: u32, tight: bool) {
    write_siblings(
        w,
        nodes,
        prefix,
        flags,
        if tight {
            Layout::TightItem
        } else {
            Layout::Blocks
        },
    );
}
