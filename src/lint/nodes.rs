//! Checks tied to parsed nodes and their source positions (C2-W-107).

use super::{LintOptions, active, parse_tree_value};
use crate::diagnostic::Diagnostic;
use crate::source::Source;
use serde_json::Value;

fn field<'a>(value: &'a Value, path: &str) -> &'a Value {
    value.pointer(path).unwrap_or(&Value::Null)
}
use std::collections::{HashMap, HashSet};

struct Item<'a> {
    node: &'a Value,
    path: String,
    parents: Vec<String>,
}

struct Sink<'a> {
    source: &'a str,
    positions: Source<'a>,
    options: &'a LintOptions,
    output: &'a mut Vec<Diagnostic>,
}

impl Sink<'_> {
    fn emit(
        &mut self,
        code: &'static str,
        item: &Item<'_>,
        values: &[(&str, &str)],
        message: usize,
    ) {
        let Some((from, to)) = bounds(item.node) else {
            return;
        };
        self.emit_range(code, from, to, &item.path, values, message);
    }

    fn emit_range(
        &mut self,
        code: &'static str,
        from: usize,
        to: usize,
        path: &str,
        values: &[(&str, &str)],
        message: usize,
    ) {
        if !active(code, self.options) {
            return;
        }
        let pos = self.positions.pos(from, to);
        self.output.retain(|d| {
            !(code != "table-attribute-invalid"
                && d.code == code
                && d.pos.is_some_and(|other| {
                    other.start_offset < pos.end_offset && pos.start_offset < other.end_offset
                }))
        });
        let mut finding = Diagnostic::nth(code, message, values, None, Some(pos));
        finding.path = Some(path.to_string());
        self.output.push(finding);
    }

    fn raw<'a>(&'a self, item: &Item<'_>) -> &'a str {
        bounds(item.node)
            .and_then(|(a, b)| self.source.get(a..b))
            .unwrap_or("")
    }
}

fn bounds(node: &Value) -> Option<(usize, usize)> {
    let pos = node.get("pos")?;
    Some((
        usize::try_from(pos["startOffset"].as_u64()?).ok()?,
        usize::try_from(pos["endOffset"].as_u64()?).ok()?,
    ))
}

fn collect(root: &Value) -> Vec<Item<'_>> {
    let mut out = Vec::new();
    let mut pending = vec![(root, String::new(), Vec::<String>::new())];
    while let Some((value, path, parents)) = pending.pop() {
        match value {
            Value::Object(map) => {
                let mut next = parents;
                if let Some(kind) = value["type"].as_str() {
                    out.push(Item {
                        node: value,
                        path: path.clone(),
                        parents: next.clone(),
                    });
                    next.push(kind.to_string());
                }
                for (key, child) in map {
                    if matches!(key.as_str(), "data" | "props" | "pos" | "segments") {
                        continue;
                    }
                    pending.push((child, format!("{path}/{key}"), next.clone()));
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    pending.push((child, format!("{path}/{index}"), parents.clone()));
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|item| bounds(item.node).map_or(0, |(start, _)| start));
    out
}

pub(super) fn scan(source: &str, options: &LintOptions, output: &mut Vec<Diagnostic>) {
    let json = crate::tree_json(source, &crate::Options::default());
    let Some(tree) = parse_tree_value(&json) else {
        return;
    };
    let items = collect(&tree);
    let mut sink = Sink {
        source,
        positions: Source::new(source),
        options,
        output,
    };
    let ids = ids(&items);
    let targets = id_targets(&items);
    for item in &items {
        check_node(item, &ids, &tree, &mut sink);
        if item.node["type"] == "xref" {
            crossref(item, &targets, &mut sink);
        }
    }
    check_document(&items, &mut sink);
}

struct IdTarget<'a> {
    id: &'a str,
    kind: &'a str,
    eligible: bool,
}

fn id_targets<'a>(items: &'a [Item<'a>]) -> Vec<IdTarget<'a>> {
    items
        .iter()
        .filter_map(|item| {
            let id = field(item.node, "/attrs/id")
                .as_str()
                .or_else(|| item.node["generatedId"].as_str())?;
            let kind = item.node["type"].as_str().unwrap_or("node");
            let captioned = field(item.node, "/caption/children")
                .as_array()
                .is_some_and(|children| {
                    children.iter().any(|child| {
                        child["children"].as_array().is_some_and(|inlines| {
                            inlines
                                .iter()
                                .any(|inline| inline["type"] == "caption_number")
                        })
                    })
                });
            Some(IdTarget {
                id,
                kind,
                eligible: kind == "heading" || captioned,
            })
        })
        .collect()
}

fn crossref(item: &Item<'_>, targets: &[IdTarget<'_>], sink: &mut Sink<'_>) {
    if item.node.get("href").is_some() {
        return;
    }
    let key = item.node["key"].as_str().unwrap_or("");
    if let Some(target) = targets.iter().find(|target| target.id == key) {
        sink.emit(
            "broken-crossref",
            item,
            &[("target", key), ("id", target.id), ("kind", target.kind)],
            0,
        );
    } else if let Some(target) = targets
        .iter()
        .find(|target| target.eligible && target.id.eq_ignore_ascii_case(key))
    {
        let phrase = format!("the id \"{}\" differs only in case", target.id);
        sink.emit(
            "broken-crossref",
            item,
            &[("target", key), ("caseOnlyPhrase", &phrase)],
            1,
        );
    } else {
        sink.emit("broken-crossref", item, &[("target", key)], 2);
    }
}

fn ids(items: &[Item<'_>]) -> HashSet<String> {
    let mut ids = HashSet::new();
    for item in items {
        if let Some(id) = field(item.node, "/attrs/id").as_str() {
            ids.insert(id.to_string());
        }
        if let Some(id) = item.node["generatedId"].as_str() {
            ids.insert(id.to_string());
        }
    }
    ids
}

fn check_node(item: &Item<'_>, ids: &HashSet<String>, root: &Value, sink: &mut Sink<'_>) {
    overindented_opener(item, sink);
    semantic(item, sink);
    check_block_node(item, ids, root, sink);
    check_inline_node(item, sink);
}

fn overindented_opener(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.parents.iter().any(|parent| parent == "list_item") {
        return;
    }
    let kind = match item.node["type"].as_str().unwrap_or("") {
        "block_quote" => "quote",
        "heading" => "heading",
        "definition_term" => "term",
        "div" | "admonition" | "directive" => "div",
        "code_block" => "code fence",
        "line_block" => "line block",
        "thematic_break" => "thematic break",
        "table" => "table row",
        _ => return,
    };
    let Some((from, _)) = bounds(item.node) else {
        return;
    };
    let before = sink.source.get(..from).unwrap_or("");
    let prefix = before.rsplit('\n').next().unwrap_or("");
    let raw = sink.source.get(from..).unwrap_or("");
    let marker = raw
        .bytes()
        .take_while(|byte| match kind {
            "quote" => *byte == b'>',
            "heading" => matches!(byte, b'#' | b'='),
            "term" | "div" | "line block" => *byte == b':',
            "code fence" => matches!(byte, b'`' | b'~'),
            "thematic break" => matches!(byte, b'-' | b'*' | b'_'),
            "table row" => *byte == b'|',
            _ => false,
        })
        .count();
    if marker == 0 || kind == "code fence" && marker < 3 {
        return;
    }
    let quote = item
        .parents
        .iter()
        .rev()
        .any(|parent| parent == "block_quote");
    let (owner, standard) = if quote {
        ("a quote", prefix.rfind('>').map_or(0, |at| at + 2))
    } else if item.parents.iter().any(|parent| {
        matches!(
            parent.as_str(),
            "div" | "admonition" | "directive" | "figure_group"
        )
    }) {
        ("a div", 0)
    } else if item
        .parents
        .iter()
        .any(|parent| parent == "footnote_definition")
    {
        ("a footnote body", 0)
    } else if item
        .parents
        .iter()
        .any(|parent| parent == "definition_description")
    {
        ("a description", 0)
    } else {
        ("document", 0)
    };
    if prefix.len() <= standard
        || !prefix
            .get(standard..)
            .is_some_and(|rest| rest.bytes().all(|byte| matches!(byte, b' ' | b'\t')))
    {
        return;
    }
    let past = prefix.len() - standard;
    sink.emit_range(
        "block-opener-overindented",
        from,
        from + marker,
        &item.path,
        &[
            ("kind", kind),
            ("past", &past.to_string()),
            ("columns", if past == 1 { "column" } else { "columns" }),
            ("owner", owner),
        ],
        0,
    );
}

fn check_block_node(item: &Item<'_>, ids: &HashSet<String>, root: &Value, sink: &mut Sink<'_>) {
    check_container_node(item, ids, root, sink);
    check_leaf_node(item, sink);
}

fn check_container_node(item: &Item<'_>, ids: &HashSet<String>, root: &Value, sink: &mut Sink<'_>) {
    match item.node["type"].as_str().unwrap_or("") {
        "image" => {
            if item.node["alt"] == "" && item.node["decorative"] != true {
                sink.emit("a11y/image-alt", item, &[], 0);
            }
            if item.node.get("key").is_some() && item.node.get("src").is_none() {
                unresolved_reference(item, root, sink);
            }
        }
        "link" => link(item, ids, root, sink),
        "list" => list(item, root, sink),
        "list_item" => list_item(item, sink),
        "heading" => heading(item, sink),
        "frontmatter" => frontmatter(item, sink),
        "figure_group" => figure(item, sink),
        "div" => div(item, sink),
        "directive" => directive(item, sink),
        _ => {}
    }
}

fn check_leaf_node(item: &Item<'_>, sink: &mut Sink<'_>) {
    match item.node["type"].as_str().unwrap_or("") {
        "table" => table(item, sink),
        "include" => include(item, sink),
        "code" | "math" => bidi(item, sink),
        _ => {}
    }
}

fn check_inline_node(item: &Item<'_>, sink: &mut Sink<'_>) {
    match item.node["type"].as_str().unwrap_or("") {
        "text" => text_node(item, sink),
        "footnote_ref" => footnote_ref(item, sink),
        "definition_term" => definition_term(item, sink),
        "emphasis" => emphasis(item, sink),
        "paragraph" => paragraph(item, sink),
        "strong" => strong(item, sink),
        "strike" => strike(item, sink),
        "highlight" => highlight(item, sink),
        "comment" => comment(item, sink),
        "code_block" => code_block(item, sink),
        "mention" => mention(item, sink),
        _ => {}
    }
}

fn link(item: &Item<'_>, ids: &HashSet<String>, root: &Value, sink: &mut Sink<'_>) {
    let node = item.node;
    if node["href"] == "" {
        sink.emit("empty-link-destination", item, &[], 0);
    }
    if let Some(href) = node["href"].as_str() {
        if let Some(raw_fragment) = href.strip_prefix('#') {
            let fragment = percent_decode_fragment(raw_fragment);
            if sink.options.extensions.is_empty()
                && !fragment.eq_ignore_ascii_case("top")
                && !ids.contains(&fragment)
            {
                let case_only = ids.iter().find(|id| id.eq_ignore_ascii_case(&fragment));
                if let Some(case_only) = case_only {
                    sink.emit(
                        "broken-fragment-link",
                        item,
                        &[("href", href), ("caseOnly", case_only)],
                        0,
                    );
                } else {
                    sink.emit("broken-fragment-link", item, &[("href", href)], 1);
                }
            }
        }
    }
    let label = node["children"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|child| child["value"].as_str())
                .collect::<String>()
        })
        .unwrap_or_default();
    let folded = label
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let aria = field(node, "/attrs/keyValues")
        .as_array()
        .and_then(|pairs| {
            pairs.iter().find_map(|pair| {
                (pair[0]
                    .as_str()
                    .is_some_and(|key| key.eq_ignore_ascii_case("aria-label")))
                .then(|| pair[1].as_str())
                .flatten()
            })
        })
        .unwrap_or("")
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if ["click here", "here", "read more"].contains(&folded.as_str())
        && (aria.is_empty() || ["click here", "here", "read more"].contains(&aria.as_str()))
    {
        sink.emit("non-descriptive-link-text", item, &[], 0);
    }
    if node.get("key").is_some() && node.get("href").is_none() {
        unresolved_reference(item, root, sink);
    }
}

fn percent_decode_fragment(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes.get(at) == Some(&b'%')
            && let (Some(a), Some(b)) = (bytes.get(at + 1), bytes.get(at + 2))
            && let (Some(high), Some(low)) =
                (char::from(*a).to_digit(16), char::from(*b).to_digit(16))
        {
            decoded.push(u8::try_from(high * 16 + low).unwrap_or_default());
            at += 3;
        } else {
            if let Some(byte) = bytes.get(at) {
                decoded.push(*byte);
            }
            at += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn unresolved_reference(item: &Item<'_>, root: &Value, sink: &mut Sink<'_>) {
    let raw = sink.raw(item).to_string();
    let key = item.node["key"].as_str().unwrap_or("");
    let similar = collect(root).into_iter().find_map(|target| {
        (target.node["type"] == "link_reference_definition")
            .then(|| target.node["key"].as_str())
            .flatten()
            .filter(|label| *label != key && label.eq_ignore_ascii_case(key))
            .map(str::to_string)
    });
    let image = item.node["type"] == "image";
    let message = usize::from(similar.is_none()) + usize::from(image) * 2;
    let phrase = similar.map(|label| format!("the label \"{label}\" differs only in case"));
    sink.emit(
        "unresolved-reference-link",
        item,
        &[
            ("rawRef", &raw),
            ("caseOnlyPhrase", phrase.as_deref().unwrap_or("")),
        ],
        message,
    );
}

fn list(item: &Item<'_>, root: &Value, sink: &mut Sink<'_>) {
    list_loose(item, sink);
    list_nested(item, root, sink);
    list_ordered(item, sink);
    list_spacing(item, sink);
    list_indent(item, root, sink);
}

fn list_nested(item: &Item<'_>, root: &Value, sink: &mut Sink<'_>) {
    if !item.parents.iter().any(|parent| parent == "list_item") {
        return;
    }
    let Some((from, to)) = bounds(item.node) else {
        return;
    };
    if sink
        .source
        .get(..from)
        .is_some_and(|before| before.trim_end_matches([' ', '\t']).ends_with("\n\n"))
    {
        return;
    }
    let parent = item
        .path
        .rsplit_once("/children/")
        .and_then(|(path, _)| root.pointer(path));
    let content = parent
        .and_then(|parent| parent["children"].as_array())
        .and_then(|children| children.first())
        .and_then(|first| field(first, "/pos/startColumn").as_u64())
        .unwrap_or(0);
    let column = field(item.node, "/pos/startColumn").as_u64().unwrap_or(0);
    if content > 0 && column > content {
        sink.emit_range(
            "list-item-block-overindented",
            from,
            to,
            &item.path,
            &[("contentColumn", &content.to_string())],
            0,
        );
    }
}

fn list_loose(item: &Item<'_>, sink: &mut Sink<'_>) {
    if field(item.node, "/attrs/keyValues")
        .as_array()
        .is_some_and(|pairs| {
            pairs.iter().any(|pair| {
                pair[0]
                    .as_str()
                    .is_some_and(|key| key.eq_ignore_ascii_case("loose"))
            })
        })
    {
        sink.emit("deprecated-loose-attribute", item, &[], 0);
    }
}

fn list_ordered(item: &Item<'_>, sink: &mut Sink<'_>) {
    if field(item.node, "/marker/kind") == "ordered" {
        let children = item.node["children"].as_array();
        if let Some(children) = children
            && let Some(first) = children.first()
        {
            let initial = field(first, "/pos/startColumn").as_u64().unwrap_or(0);
            if children
                .iter()
                .skip(1)
                .any(|child| field(child, "/pos/startColumn").as_u64().unwrap_or(initial) < initial)
            {
                if let Some((from, to)) = children.get(1).and_then(bounds) {
                    sink.emit_range(
                        "ordered-marker-right-aligned",
                        from,
                        to,
                        &format!("{}/children/1", item.path),
                        &[],
                        0,
                    );
                }
            }
        }
    }
}

fn list_spacing(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.node["spacing"] == "tight" {
        if let Some(children) = item.node["children"].as_array() {
            for (index, child) in children.iter().enumerate() {
                let blocks = child["children"].as_array().map_or(0, Vec::len);
                let raw = bounds(child)
                    .and_then(|(a, b)| sink.source.get(a..b))
                    .unwrap_or("");
                if blocks > 1
                    && raw.contains("\n\n")
                    && !raw
                        .split("\n\n")
                        .skip(1)
                        .all(|part| part.trim_start().starts_with("%%"))
                {
                    if let Some((from, to)) = bounds(child) {
                        sink.emit_range(
                            "tight-list-item-with-blank-line",
                            from,
                            to,
                            &format!("{}/children/{index}", item.path),
                            &[],
                            0,
                        );
                    }
                }
            }
        }
    }
}

fn list_indent(item: &Item<'_>, root: &Value, sink: &mut Sink<'_>) {
    let parent_column = item
        .path
        .rsplit_once("/children/")
        .and_then(|(path, _)| root.pointer(path))
        .and_then(|parent| field(parent, "/pos/startColumn").as_u64())
        .unwrap_or(0);
    let first_column = item.node["children"]
        .as_array()
        .and_then(|children| children.first())
        .and_then(|first| field(first, "/pos/startColumn").as_u64());
    if let Some(first_column) = first_column {
        for (index, child) in item.node["children"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .skip(1)
        {
            if field(child, "/pos/startColumn")
                .as_u64()
                .is_some_and(|column| {
                    column < first_column && (parent_column == 0 || column <= parent_column)
                })
            {
                if let Some((from, to)) = bounds(child) {
                    sink.emit_range(
                        "uneven-list-indent",
                        from,
                        to,
                        &format!("{}/children/{index}", item.path),
                        &[],
                        0,
                    );
                }
            }
        }
    }
}

fn list_item(item: &Item<'_>, sink: &mut Sink<'_>) {
    let raw = sink.raw(item).to_string();
    let Some((from, _)) = bounds(item.node) else {
        return;
    };
    let marker_column = field(item.node, "/pos/startColumn").as_u64().unwrap_or(1);
    let content_column = field(item.node, "/children/0/pos/startColumn")
        .as_u64()
        .unwrap_or(marker_column + 2);
    for (offset, line) in raw.split_inclusive('\n').scan(0usize, |at, line| {
        let out = (*at, line);
        *at += line.len();
        Some(out)
    }) {
        let indent = line
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        let column = u64::try_from(indent).unwrap_or(u64::MAX) + 1;
        if offset > 0
            && !line.trim().is_empty()
            && column > marker_column
            && column < content_column
            && !super::lines::list_marker(line.trim())
            && item.node["children"].as_array().is_some_and(|children| {
                children.iter().any(|child| {
                    field(child, "/pos/startOffset").as_u64()
                        == u64::try_from(from + offset + indent).ok()
                })
            })
        {
            let start = from + offset + indent;
            sink.emit_range(
                "list-item-body-below-content-column",
                start,
                start + line.trim().len(),
                &item.path,
                &[],
                0,
            );
        }
    }
}

fn heading(item: &Item<'_>, sink: &mut Sink<'_>) {
    let raw = sink.raw(item).to_string();
    let trimmed = raw.trim_end();
    if let Some(run) = trimmed.split_whitespace().last()
        && run.chars().all(|c| c == '#')
        && raw.starts_with('#')
        && let Some((from, _)) = bounds(item.node)
        && let Some(offset) = trimmed.rfind(run)
    {
        sink.emit_range(
            "heading-closing-hashes",
            from + offset,
            from + offset + run.len(),
            &item.path,
            &[("run", run)],
            0,
        );
    }
    if let Some(at) = raw.rfind("{.")
        && raw.ends_with('}')
    {
        let block = &raw[at..];
        let from = bounds(item.node).map_or(0, |(start, _)| start + at);
        sink.emit_range(
            "heading-trailing-attribute",
            from,
            from + block.len(),
            &item.path,
            &[("block", block)],
            0,
        );
    }
    heading_continuation(item, &raw, sink);
}

fn heading_continuation(item: &Item<'_>, raw: &str, sink: &mut Sink<'_>) {
    let Some((from, _)) = bounds(item.node) else {
        return;
    };
    let mut offset = 0;
    for (index, line) in raw.split_inclusive('\n').enumerate() {
        let at = offset;
        offset += line.len();
        if index == 0 || line.trim_start().starts_with("%%") {
            continue;
        }
        if line.starts_with([' ', '\t']) && !line.trim().is_empty() {
            sink.emit_range(
                "deprecated-indented-line-under-heading",
                from + at,
                from + at + line.trim_end().len(),
                &item.path,
                &[],
                0,
            );
        }
        break;
    }
}

fn frontmatter(item: &Item<'_>, sink: &mut Sink<'_>) {
    let value = item.node["value"].as_str().unwrap_or("");
    let raw = sink.raw(item).to_string();
    if raw
        .lines()
        .next()
        .is_some_and(|line| line.trim_end() == "---")
        && (raw.lines().nth(1).is_none_or(|line| line.trim().is_empty()) || !value.contains(':'))
    {
        sink.emit("frontmatter-shaped-thematic-break", item, &[], 0);
    }
    if let Some((from, _)) = bounds(item.node) {
        let mut offset = 0;
        for line in raw.split_inclusive('\n') {
            if let Some((key, rest)) = line.split_once(':') {
                let key = key.trim();
                let version = rest.trim();
                if matches!(key, "carve" | "carve-version")
                    && !version.is_empty()
                    && !version.contains(char::is_whitespace)
                    && let Some(message) = unsupported_carve_version(version)
                {
                    let start = from + offset + line.len() - rest.trim_start().len();
                    sink.emit_range(
                        "carve-version-unsupported",
                        start,
                        start + version.len(),
                        &item.path,
                        &[("version", version), ("SPEC_VERSION", "0.2.0")],
                        message,
                    );
                    break;
                }
            }
            offset += line.len();
        }
    }
}

fn unsupported_carve_version(version: &str) -> Option<usize> {
    let parts: Option<Vec<u64>> = version
        .split('.')
        .map(|part| {
            (!part.is_empty())
                .then(|| part.parse::<u64>().ok())
                .flatten()
        })
        .collect();
    let Some(parts) = parts else { return Some(0) };
    (parts.first().copied().unwrap_or(0) > 0
        || parts.get(1).copied().unwrap_or(0) > 2
        || (parts.get(1).copied().unwrap_or(0) == 2 && parts.iter().skip(2).any(|part| *part > 0)))
    .then_some(1)
}

fn figure(item: &Item<'_>, sink: &mut Sink<'_>) {
    let children = item.node["children"].as_array();
    let panels = children.map_or(0, |a| {
        a.iter()
            .filter(|node| node.get("caption").is_some())
            .count()
    });
    let tables =
        children.is_some_and(|children| children.iter().any(|node| node["type"] == "table"));
    if panels == 0 && !tables {
        sink.emit("figure-group-empty", item, &[], 0);
    }
    if panels == 1 {
        sink.emit("figure-group-single-panel", item, &[], 0);
    }
    if item.parents.iter().any(|kind| kind == "figure_group") {
        sink.emit("figure-group-nested", item, &[], 0);
    }
}

fn div(item: &Item<'_>, sink: &mut Sink<'_>) {
    let figure = field(item.node, "/attrs/classes")
        .as_array()
        .is_some_and(|classes| classes.iter().any(|class| class == "figure"));
    if figure && (item.node.get("title").is_some() || item.node.get("label").is_some()) {
        sink.emit("figure-group-opener-metadata", item, &[], 0);
    }
}

fn directive(item: &Item<'_>, sink: &mut Sink<'_>) {
    let kind = item.node["kind"].as_str().unwrap_or("");
    if !item.parents.iter().any(|parent| parent != "document") {
        return;
    }
    match kind {
        "footnotes" => sink.emit("footnotes-placement-in-container", item, &[], 0),
        "references" => sink.emit("references-placement-in-container", item, &[], 0),
        _ => {}
    }
}

fn table(item: &Item<'_>, sink: &mut Sink<'_>) {
    table_attributes(item, sink);
    comment_splits_table_row(item, sink);
    let mut header_width = None;
    for (group_index, group) in item.node["children"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        for (row_index, row) in group["children"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if row["type"] != "table_row" {
                continue;
            }
            let row_raw = bounds(row)
                .and_then(|(a, b)| sink.source.get(a..b))
                .unwrap_or("");
            if row_index == 1 && row_raw.starts_with('|') && row_raw.ends_with('|') {
                let cells_only = row_raw.trim_matches('|').trim();
                if cells_only.len() >= 3
                    && cells_only
                        .chars()
                        .all(|c| matches!(c, '-' | ':' | '|' | ' '))
                {
                    let path = format!("{}/children/{group_index}/children/{row_index}", item.path);
                    if let Some((from, to)) = bounds(row) {
                        sink.emit_range("deprecated-separator-row", from, to, &path, &[], 0);
                    }
                }
            }
            let count = row["children"].as_array().map_or(0, Vec::len);
            if header_width.is_none() {
                header_width = Some(count);
            }
            if let Some(width) = header_width
                && count != width
            {
                let path = format!("{}/children", item.path);
                if let Some((from, to)) = bounds(row) {
                    let cells = count.to_string();
                    let width = width.to_string();
                    sink.emit_range(
                        "table-row-cell-count",
                        from,
                        to,
                        &path,
                        &[("cells", &cells), ("width", &width)],
                        0,
                    );
                }
            }
            table_cells(
                row,
                group_index,
                row_index,
                group["type"] == "table_head",
                &item.path,
                sink,
            );
        }
    }
}

fn table_attributes(item: &Item<'_>, sink: &mut Sink<'_>) {
    let raw = sink.raw(item).to_string();
    let pipe = raw.lines().any(|line| line.trim_start().starts_with('|'));
    for pair in field(item.node, "/attrs/keyValues")
        .as_array()
        .into_iter()
        .flatten()
    {
        let key = pair[0].as_str().unwrap_or("");
        let value = pair[1].as_str().unwrap_or("");
        if key == "header-rows" && pipe && value.bytes().all(|b| b.is_ascii_digit()) {
            sink.emit("deprecated-table-attribute", item, &[("key", key)], 0);
        } else if matches!(
            key,
            "aligns" | "valigns" | "widths" | "header-rows" | "footer-rows"
        ) && let Some((from, _)) = bounds(item.node)
            && let Some(at) = raw.find(&format!("{key}="))
        {
            let token = raw[at..]
                .split(|ch: char| ch.is_whitespace() || ch == '}')
                .next()
                .unwrap_or("");
            sink.emit_range(
                "table-attribute-invalid",
                from + at,
                from + at + token.len(),
                &item.path,
                &[("key", key)],
                0,
            );
        }
    }
    for key in ["header-rows", "footer-rows"] {
        if let Some(at) = raw.find(&format!("{key}=")) {
            let count_text = raw.get(at + key.len() + 1..).unwrap_or("");
            let digits = count_text.bytes().take_while(u8::is_ascii_digit).count();
            let count = count_text
                .get(..digits)
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0);
            let rows = item.node["children"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|group| group["children"].as_array().into_iter().flatten())
                .filter(|row| row["type"] == "table_row")
                .count();
            if count > rows && rows > 0 {
                let count = count.to_string();
                let effective = rows.to_string();
                if let Some((from, _)) = bounds(item.node) {
                    sink.emit_range(
                        "table-row-count-clamped",
                        from + at,
                        from + at + key.len() + 1 + digits,
                        &item.path,
                        &[("key", key), ("count", &count), ("effective", &effective)],
                        0,
                    );
                }
            }
        }
    }
}

fn comment_splits_table_row(item: &Item<'_>, sink: &mut Sink<'_>) {
    let Some((from, _)) = bounds(item.node) else {
        return;
    };
    let raw = sink.raw(item).to_string();
    let mut offset = 0;
    let mut previous_row = false;
    let mut pending = None;
    for line in raw.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("%%") && previous_row {
            pending = Some((from + offset, from + offset + line.trim_end().len()));
        } else if trimmed.starts_with('+') && trimmed.ends_with('|') {
            if let Some((start, end)) = pending {
                sink.emit_range("comment-splits-table-row", start, end, &item.path, &[], 0);
            }
            pending = None;
            previous_row = true;
        } else if trimmed.starts_with('|')
            && let Some(comment) = line.find("%%")
            && line
                .get(..comment)
                .is_some_and(|prefix| prefix.trim_end().ends_with('|'))
        {
            pending = Some((
                from + offset + comment.saturating_sub(1),
                from + offset + line.trim_end().len(),
            ));
            previous_row = true;
        } else if trimmed.starts_with('|') && trimmed.ends_with('|') {
            pending = None;
            previous_row = true;
        } else if !trimmed.is_empty() {
            pending = None;
            previous_row = false;
        }
        offset += line.len();
    }
}

fn table_cells(
    row: &Value,
    group_index: usize,
    row_index: usize,
    head_row: bool,
    table_path: &str,
    sink: &mut Sink<'_>,
) {
    let Some(cells) = row["children"].as_array() else {
        return;
    };
    for (cell_index, cell) in cells.iter().enumerate() {
        let path = format!(
            "{table_path}/children/{group_index}/children/{row_index}/children/{cell_index}"
        );
        let raw = bounds(cell)
            .and_then(|(a, b)| sink.source.get(a..b))
            .unwrap_or("")
            .trim();
        let text = cell["children"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|p| p["children"].as_array())
            .and_then(|a| a.first())
            .and_then(|text| text["value"].as_str())
            .unwrap_or("");
        cell_alignment(cell, raw, text, &path, sink);
        cell_blocked(cell, text, row_index, cell_index, &path, sink);
        cell_span_attribute(cell, raw, head_row, &path, sink);
        cell_row_attribute(row, cell, raw, cell_index + 1 == cells.len(), &path, sink);
    }
}

fn cell_alignment(cell: &Value, raw: &str, text: &str, path: &str, sink: &mut Sink<'_>) {
    if let Some(mark) = text.as_bytes().first().copied()
        && matches!(mark, b'<' | b'>' | b'~')
        && text.as_bytes().get(1) == Some(&b' ')
        && text.len() > 2
        && raw.as_bytes().first() == Some(&mark)
    {
        let prefix = (mark as char).to_string();
        let braced = format!("{{{prefix}}}");
        if let Some((start, _)) = bounds(cell) {
            let at = sink
                .source
                .get(start..)
                .and_then(|tail| tail.find(mark as char))
                .map_or(start, |at| start + at);
            sink.emit_range(
                "deprecated-cell-alignment",
                at,
                at + 1,
                path,
                &[("prefix", &prefix), ("braced", &braced)],
                0,
            );
        }
    }
}

fn cell_blocked(
    cell: &Value,
    text: &str,
    row_index: usize,
    cell_index: usize,
    path: &str,
    sink: &mut Sink<'_>,
) {
    if (text == "^" && row_index == 0 || text == "<" && cell_index == 0)
        && cell.get("span").is_none()
        && !bounds(cell)
            .and_then(|(from, to)| sink.source.get(from..to))
            .is_some_and(|raw| raw.contains("\\<"))
    {
        if let Some((from, to)) = bounds(cell) {
            sink.emit_range(
                "table-span-marker-blocked",
                from,
                to,
                path,
                &[],
                if text == "^" { 2 } else { 0 },
            );
        }
    }
}

fn cell_span_attribute(cell: &Value, raw: &str, head_row: bool, path: &str, sink: &mut Sink<'_>) {
    if cell["span"].as_str().is_some()
        && cell["header"] == false
        && raw.starts_with('=')
        && !head_row
        && let Some((from, _)) = bounds(cell)
    {
        sink.emit_range(
            "table-span-marker-attribute",
            from,
            from + 1,
            path,
            &[("marker", "^"), ("direction", "above it")],
            1,
        );
    }
    if let Some(span) = cell["span"].as_str()
        && raw.contains('{')
    {
        let marker = if span == "rowspan" { "^" } else { "<" };
        let direction = if span == "rowspan" {
            "above it"
        } else {
            "to its left"
        };
        if let Some((from, to)) = bounds(cell) {
            sink.emit_range(
                "table-span-marker-attribute",
                from,
                to,
                path,
                &[("marker", marker), ("direction", direction)],
                0,
            );
        }
    }
}

fn cell_row_attribute(
    row: &Value,
    cell: &Value,
    raw: &str,
    last: bool,
    path: &str,
    sink: &mut Sink<'_>,
) {
    if last
        && (cell.get("attrs").is_none()
            && crate::attr::parse_glued(raw, 0).is_some_and(|(_, end)| end == raw.len())
            || raw.starts_with('{') && raw.ends_with('}'))
    {
        if let Some((from, to)) = bounds(row) {
            let row_raw = sink.source.get(from..to).unwrap_or("");
            if row_raw
                .rfind(raw)
                .and_then(|at| row_raw.as_bytes().get(at.wrapping_sub(1)))
                == Some(&b'|')
            {
                return;
            }
            let row_path = path
                .rsplit_once("/children/")
                .map_or(path, |(parent, _)| parent);
            sink.emit_range("table-row-attribute-as-cell", from, to, row_path, &[], 0);
        }
    }
}

fn include(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.node.get("attrs").is_some() && item.node["options"].get("code").is_none() {
        sink.emit("include-attributes-unused", item, &[], 0);
    }
    if item.node["options"].get("lines").is_some()
        && matches!(
            field(item.node, "/options/code").as_str(),
            Some("csv" | "tsv")
        )
        && field(item.node, "/attrs/keyValues")
            .as_array()
            .is_some_and(|pairs| pairs.iter().any(|pair| pair[0] == "table"))
        && let Some((from, _)) = bounds(item.node)
        && let Some(at) = sink.raw(item).find("@lines:")
    {
        let token = sink.raw(item)[at..]
            .split(char::is_whitespace)
            .next()
            .unwrap_or("");
        sink.emit_range(
            "include-lines-table-rows-hint",
            from + at,
            from + at + token.len(),
            &item.path,
            &[],
            0,
        );
    }
}

/// C2-H-075 keys with those a registered `semanticSpan` adds (carve-js#5060).
fn semantic_keys(options: &LintOptions) -> crate::api::extensions::SemanticKeys {
    if options.extensions.iter().any(|e| e == "carve.semanticSpan") {
        crate::api::extensions::SEMANTIC_SPAN_KEYS
    } else {
        crate::html::CORE_SEMANTIC_KEYS
    }
}

fn semantic(item: &Item<'_>, sink: &mut Sink<'_>) {
    let kind = item.node["type"].as_str().unwrap_or("");
    let keys = semantic_keys(sink.options);
    let pairs = field(item.node, "/attrs/keyValues").as_array();
    let Some(((from, to), pairs)) = bounds(item.node).zip(pairs) else {
        return;
    };
    let source = sink.source;
    let raw = source.get(from..to).unwrap_or("");
    if kind == "span" {
        for pair in pairs {
            let name = pair[0].as_str().unwrap_or("");
            let value = pair[1].as_str().unwrap_or("");
            if !value.is_empty()
                && keys
                    .iter()
                    .any(|&(key, target)| key == name && target.is_none())
            {
                let (at, end) = attribute_range(raw, name, true)
                    .map_or((from, to), |(a, e)| (from + a, from + e));
                sink.emit_range(
                    "semantic-attribute-value-ignored",
                    at,
                    end,
                    &item.path,
                    &[("name", name)],
                    0,
                );
            }
        }
        return;
    }
    for &(name, _) in keys {
        let Some(value) = pairs
            .iter()
            .find(|pair| {
                pair[0]
                    .as_str()
                    .is_some_and(|key| key.eq_ignore_ascii_case(name))
            })
            .map(|pair| pair[1].as_str().unwrap_or(""))
        else {
            continue;
        };
        let Some((at, end)) = attribute_range(raw, name, false) else {
            continue;
        };
        sink.emit_range(
            "semantic-attribute-outside-span",
            from + at,
            from + end,
            &item.path,
            &[("name", name), ("type", kind), ("value", value)],
            0,
        );
    }
}

/// The first `key` or `key=value` token of the node source, as carve-js
/// `attributeRange` matches it; `whole` takes the value in.
fn attribute_range(raw: &str, key: &str, whole: bool) -> Option<(usize, usize)> {
    let ends = |at: usize| {
        raw[at..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == '}')
    };
    for (start, _) in raw.match_indices(key) {
        if raw[..start]
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_whitespace() && c != '{')
        {
            continue;
        }
        let after = start + key.len();
        if let Some(value) = raw[after..].strip_prefix('=') {
            let quoted = value
                .chars()
                .next()
                .filter(|c| matches!(c, '"' | '\''))
                .and_then(|q| value[1..].find(q).map(|e| e + 2));
            let bare = value.find(|c: char| c.is_whitespace() || c == '}');
            let length = quoted
                .filter(|&l| ends(after + 1 + l))
                .or(bare.filter(|&l| l > 0));
            if let Some(length) = length {
                return Some((start, if whole { after + 1 + length } else { after }));
            }
        }
        if ends(after) {
            return Some((start, after));
        }
    }
    None
}

fn text_delimiters(item: &Item<'_>, sink: &mut Sink<'_>, value: &str, from: usize) {
    for (mark, code) in [
        ("**", "markdown-strong-double-star"),
        ("~~", "markdown-strikethrough-double-tilde"),
    ] {
        if let Some(start) = value.find(mark)
            && let Some(tail) = value.get(start + mark.len()..)
            && let Some(end) = tail.find(mark)
            && end > 0
        {
            let to = start + mark.len() + end + mark.len();
            let byte = mark.as_bytes().first().copied().unwrap_or_default();
            let open = value.as_bytes().get(start.wrapping_sub(1)) != Some(&byte)
                && value.as_bytes().get(start + mark.len()) != Some(&byte);
            let close = value.as_bytes().get(to - mark.len() - 1) != Some(&byte)
                && value.as_bytes().get(to) != Some(&byte);
            let inner = &tail[..end];
            if open
                && close
                && !escaped(value.as_bytes(), start)
                && !escaped(value.as_bytes(), to - mark.len())
                && !inner.starts_with(char::is_whitespace)
                && !inner.ends_with(char::is_whitespace)
                && !inner.contains("\n\n")
                && (mark != "**" || !inner.contains('*'))
            {
                sink.emit_range(code, from + start, from + to, &item.path, &[], 0);
            }
        }
    }
    for (mark, code) in [
        (b'_', "deprecated-bare-underline"),
        (b'~', "deprecated-bare-strike"),
        (b'=', "deprecated-bare-highlight"),
    ] {
        let mut offset = 0;
        while let Some((start, end)) = value
            .as_bytes()
            .get(offset..)
            .and_then(|rest| bare_pair(rest, mark))
        {
            sink.emit_range(
                code,
                from + offset + start,
                from + offset + end,
                &item.path,
                &[],
                0,
            );
            offset += end;
        }
    }
}

fn text_node(item: &Item<'_>, sink: &mut Sink<'_>) {
    let value = sink.raw(item).to_string();
    let Some((from, _)) = bounds(item.node) else {
        return;
    };
    text_delimiters(item, sink, &value, from);
    if item.parents.iter().any(|parent| parent == "line_block")
        && value.trim_start().starts_with("%%")
    {
        sink.emit("comment-line-in-line-block", item, &[], 0);
    }
    if let Some(at) = value.find("{%").filter(|_| !value.contains("%}")) {
        sink.emit_range(
            "unclosed-braced-comment",
            from + at,
            from + value.len(),
            &item.path,
            &[],
            0,
        );
    }
    emphasis_before_attrs(item, sink, &value, from);
    if let Some(at) = value
        .find("{.")
        .into_iter()
        .chain(value.find("{#"))
        .min()
        .filter(|at| *at > 0)
        && value.get(at..).is_some_and(|tail| tail.contains('}'))
        && !escaped(value.as_bytes(), at)
        && sink
            .source
            .get(..from + at)
            .is_some_and(|prefix| !prefix.rsplit('\n').next().unwrap_or("").trim().is_empty())
    {
        let end = value
            .get(at..)
            .and_then(|tail| tail.find('}'))
            .map_or(value.len(), |end| at + end + 1);
        sink.emit_range(
            "unattached-inline-attribute",
            from + at,
            from + end,
            &item.path,
            &[],
            0,
        );
    }
    sigils(item, sink, &value, from);
    if sink.options.platforms.iter().any(|p| p == "github")
        && !item.parents.iter().any(|kind| kind == "footnote")
    {
        for (at, _) in value.match_indices('#') {
            let digits = value
                .get(at + 1..)
                .unwrap_or("")
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            if digits > 0 {
                let token = value.get(at..at + digits + 1).unwrap_or("");
                sink.emit_range(
                    "platform-issue-reference",
                    from + at,
                    from + at + token.len(),
                    &item.path,
                    &[("host", "GitHub"), ("text", token)],
                    0,
                );
            }
        }
    }
    djot_text(item, sink, &value, from);
    unclosed_emphasis(item, sink, from);
    let parent = item.parents.last().map_or("", String::as_str);
    let delimiter = if parent == "strong" {
        "{*"
    } else if parent == "emphasis" {
        "{/"
    } else {
        ""
    };
    if !delimiter.is_empty()
        && let Some(at) = value.find(delimiter)
    {
        sink.emit_range(
            "braced-span-opener-as-text",
            from + at,
            from + at + 2,
            &item.path,
            &[],
            0,
        );
    }
    bidi(item, sink);
}

fn emphasis_before_attrs(item: &Item<'_>, sink: &mut Sink<'_>, raw: &str, from: usize) {
    for (at, ch) in raw.char_indices() {
        if !matches!(ch, '*' | '/')
            || raw.as_bytes().get(at + 1) != Some(&b'{')
            || escaped(raw.as_bytes(), at)
        {
            continue;
        }
        let Some((_, end)) = crate::attr::parse_glued(raw, at + 1) else {
            continue;
        };
        if end <= at + 1 {
            continue;
        }
        let marker = if ch == '*' { "*" } else { "/" };
        sink.emit_range(
            "emphasis-marker-before-attributes",
            from + at,
            from + at + 1,
            &item.path,
            &[("ch", marker)],
            0,
        );
    }
}

fn sigils(item: &Item<'_>, sink: &mut Sink<'_>, value: &str, from: usize) {
    for (at, ch) in value.char_indices() {
        if at == 0 || !matches!(ch, '@' | '#') {
            continue;
        }
        let before = value.get(..at).and_then(|prefix| prefix.chars().last());
        let after = value
            .get(at + ch.len_utf8()..)
            .and_then(|tail| tail.chars().next());
        if !before.is_some_and(|c| c.is_alphanumeric() && !c.is_ascii())
            || !after.is_some_and(|c| c.is_ascii_alphanumeric())
        {
            continue;
        }
        let mut name = value
            .get(at + ch.len_utf8()..)
            .unwrap_or("")
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
        if !name.any(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        sink.emit_range(
            "sigil-after-letter",
            from + at,
            from + at + ch.len_utf8(),
            &item.path,
            &[],
            0,
        );
    }
}

fn unclosed_emphasis(item: &Item<'_>, sink: &mut Sink<'_>, from: usize) {
    let raw = sink.raw(item).to_string();
    let bytes = raw.as_bytes();
    for at in 0..bytes.len().saturating_sub(1) {
        let mark = bytes.get(at).copied().unwrap_or_default();
        if !matches!(mark, b'*' | b'/')
            || escaped(bytes, at)
            || !bytes.get(at + 1).is_some_and(u8::is_ascii_alphanumeric)
            || (at > 0 && bytes.get(at - 1) == Some(&b'{'))
            || at > 0
                && !bytes
                    .get(at - 1)
                    .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'(' | b'[' | b'{'))
            || bytes.get(at + 1..).is_some_and(|tail| tail.contains(&mark))
        {
            continue;
        }
        let span = if mark == b'*' { "strong" } else { "emphasis" };
        sink.emit_range(
            "unclosed-inline-emphasis",
            from + at,
            from + at + 1,
            &item.path,
            &[("span", span)],
            0,
        );
        break;
    }
}

fn bidi(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.node["type"] == "code_block"
        || item.path.contains("/title/") && item.parents.iter().any(|kind| kind == "code_block")
    {
        return;
    }
    let Some((from, _)) = bounds(item.node) else {
        return;
    };
    let raw = sink.raw(item).to_string();
    for (at, ch) in raw.char_indices() {
        if matches!(ch as u32, 0x202a..=0x202e | 0x2066..=0x2069) {
            let code = format!("{:04X}", ch as u32);
            sink.emit_range(
                "bidi-control-in-source",
                from + at,
                from + at + ch.len_utf8(),
                &item.path,
                &[("code", &code)],
                0,
            );
        }
    }
}

fn djot_text(item: &Item<'_>, sink: &mut Sink<'_>, value: &str, from: usize) {
    for (mark, code) in [
        ("_", "djot-emphasis-underscore"),
        ("~", "djot-subscript-tilde"),
        ("{=", "djot-highlight-braces"),
        ("^", "djot-superscript-caret"),
    ] {
        let close = if mark == "{=" { "=}" } else { mark };
        if let Some(start) = value.find(mark)
            && let Some(tail) = value.get(start + mark.len()..)
            && let Some(end) = tail.find(close)
            && end > 0
        {
            sink.emit_range(
                code,
                from + start,
                from + start + mark.len() + end + close.len(),
                &item.path,
                &[],
                0,
            );
        }
    }
    let bytes = value.as_bytes();
    for (open, _) in value.match_indices('_') {
        if open == 0
            || bytes
                .get(open - 1)
                .is_none_or(|c| !c.is_ascii_alphanumeric())
            || bytes.get(open + 1).is_none_or(u8::is_ascii_whitespace)
            || escaped(bytes, open)
        {
            continue;
        }
        if let Some((close, _)) = value[open + 1..].match_indices('_').find(|(at, _)| {
            let close = open + 1 + at;
            close > open + 1
                && bytes
                    .get(close - 1)
                    .is_some_and(|c| !c.is_ascii_whitespace())
                && bytes.get(close + 1).is_some_and(u8::is_ascii_alphanumeric)
                && !escaped(bytes, close)
        }) {
            sink.emit_range(
                "djot-intraword-underscore",
                from + open,
                from + open + 1 + close + 1,
                &item.path,
                &[],
                0,
            );
            break;
        }
    }
}

fn definition_term(item: &Item<'_>, sink: &mut Sink<'_>) {
    let first_line = field(item.node, "/pos/startLine").as_u64().unwrap_or(0);
    if item.node["children"].as_array().is_some_and(|children| {
        children.iter().any(|child| {
            child["type"] != "comment"
                && (field(child, "/pos/startLine")
                    .as_u64()
                    .unwrap_or(first_line)
                    > first_line
                    || field(child, "/pos/endLine").as_u64().unwrap_or(first_line) > first_line)
        })
    }) {
        let raw = sink.raw(item).to_string();
        if let Some((from, _)) = bounds(item.node)
            && let Some((head, tail)) = raw.split_once('\n')
        {
            let line = tail.split('\n').next().unwrap_or("");
            let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
            sink.emit_range(
                "description-term-multi-line",
                from + head.len() + 1 + indent,
                from + head.len() + 1 + line.trim_end().len(),
                &item.path,
                &[],
                0,
            );
        }
    }
}

fn emphasis(item: &Item<'_>, sink: &mut Sink<'_>) {
    if sink.raw(item).starts_with("/*") && sink.raw(item).ends_with("*/") {
        sink.emit("comment-shaped-emphasis", item, &[], 0);
    }
    span_before_attrs(item, sink, '/', "emphasis");
}

fn paragraph_delimiters(item: &Item<'_>, sink: &mut Sink<'_>) {
    let raw = sink.raw(item).to_string();
    if let Some((from, _)) = bounds(item.node) {
        if raw.contains('\n') {
            for (mark, code) in [
                (b'=', "deprecated-bare-highlight"),
                (b'~', "deprecated-bare-strike"),
            ] {
                if let Some((start, end)) = bare_pair(raw.as_bytes(), mark)
                    && raw.get(start..end).is_some_and(|pair| pair.contains('\n'))
                {
                    sink.emit_range(code, from + start, from + end, &item.path, &[], 0);
                }
            }
            if let Some(start) = raw.find("**")
                && let Some(end) = raw.get(start + 2..).and_then(|tail| tail.find("**"))
                && raw
                    .get(start + 2..start + 2 + end)
                    .is_some_and(|inner| inner.contains('\n') && !inner.contains("\n\n"))
            {
                sink.emit_range(
                    "markdown-strong-double-star",
                    from + start,
                    from + start + end + 4,
                    &item.path,
                    &[],
                    0,
                );
            }
        }
        if let Some(start) = raw.find("{/")
            && raw
                .get(start + 2..)
                .and_then(|tail| tail.find("*/}"))
                .is_some_and(|end| !escaped(raw.as_bytes(), start + 2 + end))
            && raw.get(..start).is_some_and(|head| head.contains('*'))
        {
            sink.emit_range(
                "braced-span-opener-as-text",
                from + start,
                from + start + 2,
                &item.path,
                &[],
                0,
            );
        }
    }
}

fn paragraph(item: &Item<'_>, sink: &mut Sink<'_>) {
    paragraph_delimiters(item, sink);
    let children = item.node["children"].as_array();
    if children.is_some_and(|children| {
        let visible: Vec<_> = children
            .iter()
            .filter(|node| {
                node["type"] != "comment"
                    && !(node["type"] == "text"
                        && node["value"]
                            .as_str()
                            .is_some_and(|text| text.trim().is_empty()))
            })
            .collect();
        visible.len() == 1
            && visible.first().is_some_and(|node| {
                matches!(node["type"].as_str(), Some("emphasis" | "strong"))
                    && node["children"]
                        .as_array()
                        .is_some_and(|nodes| !nodes.is_empty())
            })
    }) {
        sink.emit("emphasis-as-heading", item, &[], 0);
    }
}

fn strong(item: &Item<'_>, sink: &mut Sink<'_>) {
    if sink.raw(item).starts_with("*/") || sink.raw(item).ends_with("/*") {
        sink.emit("comment-shaped-emphasis", item, &[], 0);
    }
    let content = item.node["children"]
        .as_array()
        .map(|children| {
            children
                .iter()
                .filter_map(|child| child["value"].as_str())
                .collect::<String>()
        })
        .unwrap_or_default();
    if content.contains(".js") || content.contains("src/") {
        sink.emit("glob-shaped-strong", item, &[], 0);
    }
    if sink.raw(item).starts_with("{*") && sink.raw(item).contains("{**}") {
        sink.emit("braced-span-opener-as-text", item, &[], 0);
    }
    span_before_attrs(item, sink, '*', "strong");
}

fn span_before_attrs(item: &Item<'_>, sink: &mut Sink<'_>, marker: char, name: &str) {
    if item.node.get("attrs").is_none() {
        return;
    }
    let raw = sink.raw(item).to_string();
    if !raw.starts_with(marker) || raw.starts_with("{*") || raw.starts_with("{/") {
        return;
    }
    let Some(close) = raw.rfind(&format!("{marker}{{")) else {
        return;
    };
    let inner = raw.get(1..close).unwrap_or("");
    if !inner
        .char_indices()
        .any(|(at, ch)| ch == marker && !escaped(inner.as_bytes(), at))
    {
        return;
    }
    let Some((from, _)) = bounds(item.node) else {
        return;
    };
    let opening = sink.positions.pos(from, from);
    sink.emit_range(
        "emphasis-marker-before-attributes",
        from + close,
        from + close + marker.len_utf8(),
        &item.path,
        &[
            ("marker", &marker.to_string()),
            ("name", name),
            ("line", &opening.start_line.to_string()),
            ("column", &opening.start_column.to_string()),
        ],
        1,
    );
}

fn strike(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.node["children"].as_array().is_some_and(|children| {
        children.iter().any(|child| {
            child["type"] == "text"
                && bounds(child)
                    .and_then(|(from, to)| sink.source.get(from..to))
                    .is_some_and(|raw| {
                        raw.match_indices("~>")
                            .any(|(at, _)| !escaped(raw.as_bytes(), at))
                    })
        })
    }) {
        sink.emit("deprecated-substitution-arrow", item, &[], 0);
    }
    if sink.raw(item).starts_with("{~") {
        sink.emit("djot-subscript-tilde-braced", item, &[], 0);
    }
}

fn highlight(item: &Item<'_>, sink: &mut Sink<'_>) {
    if sink.raw(item).starts_with("{=") {
        sink.emit("djot-highlight-braces", item, &[], 0);
    }
}

fn mention(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.parents.iter().any(|kind| kind == "footnote") {
        return;
    }
    let raw = sink.raw(item).to_string();
    sink.emit(
        "platform-mention-token",
        item,
        &[("host", "GitHub"), ("what", "a mention"), ("text", &raw)],
        0,
    );
}

fn comment(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.node["form"] == "line" {
        let Some((from, to)) = bounds(item.node) else {
            return;
        };
        let prefix = sink.source.get(..from).unwrap_or("");
        let line_start = prefix.rfind('\n').map_or(0, |at| at + 1);
        let before = prefix.get(line_start..).unwrap_or("");
        if let Some(open) = before.rfind('[')
            && !before.get(open..).unwrap_or("").contains(']')
            && before
                .get(..open)
                .is_some_and(|head| head.trim_end().ends_with(']'))
        {
            sink.emit_range("comment-in-reference-label", from, to, &item.path, &[], 0);
        }
        return;
    }
    if item.node["form"] != "block" {
        return;
    }
    let Some((from, _)) = bounds(item.node) else {
        return;
    };
    let raw = sink.raw(item).to_string();
    let mut width = 0;
    for (offset, line) in raw.split_inclusive('\n').scan(0usize, |at, line| {
        let out = (*at, line);
        *at += line.len();
        Some(out)
    }) {
        let text = line.trim_end_matches('\n');
        let run = text.chars().take_while(|c| *c == '%').count();
        if width == 0 {
            if run >= 3 {
                width = run;
            }
            continue;
        }
        if offset > 0 && run >= width && text.get(run..).is_some_and(|tail| !tail.trim().is_empty())
        {
            sink.emit_range(
                "comment-fence-closer-with-text",
                from + offset,
                from + offset + text.len(),
                &item.path,
                &[],
                0,
            );
        }
    }
}

fn code_block(item: &Item<'_>, sink: &mut Sink<'_>) {
    let Some((from, to)) = bounds(item.node) else {
        return;
    };
    let raw = sink.source.get(from..to).unwrap_or("");
    let Some(first) = raw.find("```").or_else(|| raw.find("~~~")) else {
        return;
    };
    let marker = raw.as_bytes().get(first).copied().unwrap_or(b'`');
    let width = raw
        .get(first..)
        .unwrap_or("")
        .bytes()
        .take_while(|b| *b == marker)
        .count();
    let last_line_start = sink
        .source
        .get(..to)
        .and_then(|text| text.rfind('\n'))
        .map_or(0, |at| at + 1);
    let closer = sink.source.get(last_line_start..to).unwrap_or("");
    let body = closer
        .trim_start_matches([' ', '\t'])
        .trim_end_matches([' ', '\t']);
    if body.len() < width || !body.bytes().all(|b| b == marker) {
        return;
    }
    let before_first = sink
        .source
        .get(..from + first)
        .unwrap_or("")
        .rsplit('\n')
        .next()
        .unwrap_or("");
    let before_last = closer
        .get(..closer.len() - closer.trim_start_matches([' ', '\t']).len())
        .unwrap_or("");
    if before_first.len() != before_last.len() {
        sink.emit_range(
            "fence-closer-indented",
            last_line_start + before_last.len(),
            last_line_start + before_last.len() + body.len(),
            &item.path,
            &[],
            0,
        );
    }
    bidi(item, sink);
}

fn bare_pair(text: &[u8], mark: u8) -> Option<(usize, usize)> {
    for start in 0..text.len() {
        if !bare_open(text, mark, start) {
            continue;
        }
        for end in start + 2..text.len() {
            if bare_close(text, mark, start, end) {
                return Some((start, end + 1));
            }
        }
    }
    None
}

fn bare_open(text: &[u8], mark: u8, start: usize) -> bool {
    let Some(prefix) = text
        .get(..start)
        .and_then(|part| std::str::from_utf8(part).ok())
    else {
        return false;
    };
    let before = prefix.chars().next_back();
    text.get(start) == Some(&mark)
        && !escaped(text, start)
        && !before.is_some_and(|ch| ch == char::from(mark) || ch.is_alphanumeric())
        && text
            .get(start + 1)
            .is_some_and(|next| *next != mark && !next.is_ascii_whitespace())
}

fn bare_close(text: &[u8], mark: u8, start: usize, end: usize) -> bool {
    let after = std::str::from_utf8(text.get(end + 1..).unwrap_or_default())
        .ok()
        .and_then(|s| s.chars().next());
    text.get(end) == Some(&mark)
        && !escaped(text, end)
        && text.get(start + 1..end).is_some_and(|inner| {
            !inner.iter().enumerate().any(|(offset, byte)| {
                *byte == mark
                    && text
                        .get(start + offset)
                        .is_some_and(|before| !before.is_ascii_whitespace())
            })
        })
        && text
            .get(end - 1)
            .is_some_and(|before| *before != mark && !before.is_ascii_whitespace())
        && !after.is_some_and(char::is_alphanumeric)
}

fn escaped(text: &[u8], at: usize) -> bool {
    text.get(..at)
        .unwrap_or(&[])
        .iter()
        .rev()
        .take_while(|b| **b == b'\\')
        .count()
        % 2
        == 1
}

fn footnote_ref(item: &Item<'_>, sink: &mut Sink<'_>) {
    if item.node.get("number").is_none() {
        let id = item.node["key"].as_str().unwrap_or("");
        sink.emit("unresolved-footnote", item, &[("id", id)], 0);
    }
}

fn check_document(items: &[Item<'_>], sink: &mut Sink<'_>) {
    let Some(root) = items.iter().find(|item| item.path.is_empty()) else {
        return;
    };
    heading_outline(items, sink);
    ordered_split(root, sink);
    unused_definitions(items, sink);
    unused_footnotes(items, sink);
    duplicate_urls(items, sink);
}

fn duplicate_urls(items: &[Item<'_>], sink: &mut Sink<'_>) {
    let mut urls = HashSet::new();
    for item in items
        .iter()
        .filter(|item| item.node["type"] == "link_reference_definition")
    {
        let href = item.node["href"].as_str().unwrap_or("");
        if !urls.insert(href) {
            sink.emit("duplicate-reference-url", item, &[], 0);
        }
    }
}

fn heading_outline(items: &[Item<'_>], sink: &mut Sink<'_>) {
    let mut previous = 0;
    let mut markers = HashSet::new();
    for item in items.iter().filter(|item| item.node["type"] == "heading") {
        let level = item.node["level"].as_u64().unwrap_or(0);
        if previous > 0 && level > previous + 1 {
            let before = previous.to_string();
            let now = level.to_string();
            sink.emit(
                "heading-level-skip",
                item,
                &[("previous", &before), ("level", &now)],
                0,
            );
        }
        previous = level;
        if let Some(marker) = sink.raw(item).chars().next() {
            markers.insert(marker);
        }
    }
    if markers.contains(&'#') && markers.contains(&'=') {
        if let Some(item) = items.iter().find(|item| item.node["type"] == "heading") {
            sink.emit("heading-marker-mixed", item, &[], 0);
        }
    }
}

fn ordered_split(root: &Item<'_>, sink: &mut Sink<'_>) {
    let Some(children) = root.node["children"].as_array() else {
        return;
    };
    for (index, pair) in children.windows(2).enumerate() {
        let [first, second] = pair else { continue };
        if first["type"] != "list" || second["type"] != "list" {
            continue;
        }
        let a = field(first, "/marker/style").as_str().unwrap_or("");
        let b = field(second, "/marker/style").as_str().unwrap_or("");
        // C2-B-044: a one-item list of a one-letter roman-letter marker, fewer
        // than three blank lines before the next list.
        let roman_letter = bounds(first)
            .and_then(|(from, _)| sink.source.get(from..))
            .map(|s| s.trim_start_matches([' ', '\t']).trim_start_matches('('))
            .is_some_and(|s| {
                let mut c = s.chars();
                c.next().is_some_and(|l| "ivxlcdmIVXLCDM".contains(l))
                    && matches!(c.next(), Some('.' | ')'))
            });
        if first["children"].as_array().is_some_and(|a| a.len() == 1)
            && roman_letter
            && matches!((a, b), ("i", "a") | ("a", "i") | ("I", "A") | ("A", "I"))
            && bounds(first)
                .zip(bounds(second))
                .is_some_and(|((_, end), (start, _))| {
                    sink.source
                        .get(end..start)
                        .is_some_and(|between| between.matches('\n').count() < 4)
                })
        {
            let path = format!("{}/children/{index}", root.path);
            if let Some((from, to)) = bounds(first) {
                sink.emit_range("ordered-list-kind-split", from, to, &path, &[], 0);
            }
        }
    }
}

fn unused_definitions(items: &[Item<'_>], sink: &mut Sink<'_>) {
    let mut used = HashSet::new();
    let mut definitions = HashMap::new();
    for item in items {
        match item.node["type"].as_str() {
            Some("link" | "image") if item.node.get("key").is_some() => {
                if let Some(key) = item.node["key"].as_str() {
                    used.insert(key.to_string());
                }
            }
            Some("link_reference_definition") => {
                if let Some(key) = item.node["key"].as_str() {
                    definitions.insert(key.to_string(), item);
                }
            }
            _ => {}
        }
    }
    for (key, item) in definitions {
        if !used.contains(&key) {
            sink.emit("unused-link-reference-definition", item, &[], 0);
        }
    }
}

fn unused_footnotes(items: &[Item<'_>], sink: &mut Sink<'_>) {
    let used: HashSet<&str> = items
        .iter()
        .filter(|item| item.node["type"] == "footnote_ref")
        .filter_map(|item| item.node["key"].as_str())
        .collect();
    for item in items.iter().filter(|item| item.node["type"] == "footnote") {
        if let Some(key) = item.node["key"].as_str()
            && !used.contains(key)
            && let Some((from, _)) = bounds(item.node)
            && let Some(end) = sink.raw(item).find("]:")
        {
            sink.emit_range(
                "unused-footnote-definition",
                from,
                from + end + 1,
                &item.path,
                &[("label", key)],
                0,
            );
        }
    }
}
