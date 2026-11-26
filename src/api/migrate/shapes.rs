//! The node shapes that changed between 0.1 and 2.0 (migrate/shapes.ts of
//! carve-js).

use super::{
    Fields, State, at_i, field, folded, integer, joined_pos, loss, put, string, table, ty,
};
use serde_json::{Value, json};

/// C2-A-058: a start is 0 to 999999999 and omitted when it is 1.
fn kept_start(start: f64) -> bool {
    start.fract() == 0.0 && (0.0..=999_999_999.0).contains(&start) && (start - 1.0).abs() > 0.5
}

/// `Number(value)` of JavaScript for the values a tree holds.
fn js_number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse().unwrap_or(f64::NAN)
            }
        }
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Null => 0.0,
        _ => f64::NAN,
    }
}

/// A finite number as `JSON.stringify` writes it, `null` otherwise.
fn number_value(n: f64) -> Value {
    #[allow(clippy::cast_possible_truncation)]
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15 {
        Value::from(n as i64)
    } else {
        serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
    }
}

fn drop_empty_attrs(node: &mut Fields) {
    if node
        .get("attrs")
        .and_then(Value::as_object)
        .is_some_and(serde_json::Map::is_empty)
    {
        node.remove("attrs");
    }
}

fn set_pairs(node: &mut Fields, pairs: Vec<Value>) {
    if let Some(attrs) = node.get_mut("attrs").and_then(Value::as_object_mut) {
        if pairs.is_empty() {
            attrs.remove("keyValues");
        } else {
            attrs.insert("keyValues".into(), Value::Array(pairs));
        }
    }
    drop_empty_attrs(node);
}

fn key_value_pairs(node: &Fields) -> Option<Vec<Value>> {
    Some(
        node.get("attrs")?
            .get("keyValues")?
            .as_array()?
            .iter()
            .filter(|p| p.is_array())
            .cloned()
            .collect(),
    )
}

fn list_start(node: &mut Fields, marker: &mut Value) {
    let Some(mut pairs) = key_value_pairs(node) else {
        return;
    };
    let Some(at) = pairs.iter().position(|p| at_i(p, 0) == "start") else {
        return;
    };
    let start = js_number(at_i(&pairs.remove(at), 1));
    if kept_start(start) {
        put(marker, "start", number_value(start));
    }
    set_pairs(node, pairs);
}

fn marker_of(old: &Fields) -> Value {
    if old.get("ordered") != Some(&Value::Bool(true)) {
        return json!({"kind": "bullet", "char": string(old.get("bulletChar")).unwrap_or("-")});
    }
    let mut marker = json!({
        "kind": "ordered",
        "style": string(old.get("olType")).unwrap_or("1"),
        "delimiter": string(old.get("delim")).unwrap_or("."),
    });
    #[allow(clippy::cast_precision_loss)]
    if let Some(start) = integer(old.get("start")).filter(|s| kept_start(*s as f64)) {
        put(&mut marker, "start", start.into());
    }
    marker
}

fn list_marker(node: &mut Fields, old: &Fields) {
    if let Some(marker) = node.get_mut("marker").and_then(Value::as_object_mut) {
        marker.remove("bare");
        return;
    }
    let mut marker = marker_of(old);
    let loose = old.get("tight") == Some(&Value::Bool(false));
    node.insert(
        "spacing".into(),
        if loose { "loose" } else { "tight" }.into(),
    );
    if field(&marker, "kind") == "ordered" {
        list_start(node, &mut marker);
    }
    node.insert("marker".into(), marker);
    for field in [
        "ordered",
        "tight",
        "olType",
        "delim",
        "start",
        "bulletChar",
        "bareMarker",
    ] {
        node.remove(field);
    }
}

fn task(node: &mut Fields, old: &Fields) {
    if let Some(Value::Object(t)) = node.get_mut("task") {
        t.remove("status");
    } else if old.contains_key("checked") || old.contains_key("taskState") {
        let checked = old.get("checked") == Some(&Value::Bool(true));
        let mark = string(old.get("taskState")).unwrap_or(if checked { "x" } else { " " });
        node.insert("task".into(), json!({"mark": mark}));
    }
    node.remove("checked");
    node.remove("taskState");
}

fn either<'v>(a: Option<&'v Value>, b: Option<&'v Value>) -> Option<&'v Value> {
    a.filter(|v| !v.is_null()).or(b)
}

fn link_reference(node: &mut Fields, old: &Fields) {
    let raw = string(old.get("rawRef")).map(|r| {
        if ty(node) == "image" {
            r.strip_prefix('!').unwrap_or(r)
        } else {
            r
        }
    });
    let collapsed = raw.is_some_and(|r| r.ends_with("][]"));
    let closing = raw.and_then(|r| r.rfind("][]")).unwrap_or(0);
    let key = if collapsed && closing > 0 {
        raw.and_then(|r| r.get(r.chars().next().map_or(0, char::len_utf8)..closing))
            .map(crate::chars::fold_label)
    } else {
        folded(either(old.get("ref"), old.get("key")))
    };
    if let Some(key) = key {
        node.insert("key".into(), key.into());
        node.insert("namespace".into(), "link".into());
        if !node.contains_key("refForm") {
            let form = if collapsed { "collapsed" } else { "full" };
            node.insert("refForm".into(), form.into());
        }
        node.remove("tooltip");
        if old.get("resolved") != Some(&Value::Bool(true)) {
            for field in ["src", "href"] {
                if node.get(field).is_some_and(|v| v == "") {
                    node.remove(field);
                }
            }
        }
    }
    for field in ["ref", "rawRef", "resolved"] {
        node.remove(field);
    }
}

fn reference(node: &mut Fields, old: &Fields) {
    let t = ty(node).to_string();
    if t == "xref" && old.contains_key("target") {
        match folded(old.get("target")) {
            Some(k) => node.insert("key".into(), k.into()),
            None => node.remove("key"),
        };
        node.insert("namespace".into(), "heading".into());
        node.remove("target");
    }
    if t == "link" || t == "image" {
        link_reference(node, old);
    }
    if !matches!(
        t.as_str(),
        "footnote"
            | "footnote_ref"
            | "link_reference_definition"
            | "citation_definition"
            | "abbreviation"
            | "abbreviation_def"
            | "citation"
    ) {
        return;
    }
    let key = folded(either(
        node.get("key"),
        either(old.get("label"), old.get("abbr")),
    ));
    match key {
        Some(k) => node.insert("key".into(), k.into()),
        None => node.remove("key"),
    };
    let namespace = if t.starts_with("abbreviation") {
        "abbr"
    } else if t.starts_with("footnote") {
        "footnote"
    } else if t.starts_with("citation") {
        "citation"
    } else {
        "link"
    };
    node.insert("namespace".into(), namespace.into());
}

fn line_contents(children: &[Value]) -> Vec<Value> {
    let count = children
        .iter()
        .take_while(|c| field(c, "type") == "non_breaking_space")
        .count();
    if count == 0 {
        return children.to_vec();
    }
    let next = children.get(count).filter(|n| field(n, "type") == "text");
    let value = " ".repeat(count) + next.and_then(|n| field(n, "value").as_str()).unwrap_or("");
    let end = if next.is_some() { count + 1 } else { count };
    let mut text = json!({"type": "text", "value": value});
    if let (Some(s), Some(e)) = (
        children.first(),
        end.checked_sub(1).and_then(|i| children.get(i)),
    ) && field(s, "pos").is_object()
        && field(e, "pos").is_object()
    {
        put(
            &mut text,
            "pos",
            joined_pos(field(s, "pos"), field(e, "pos")),
        );
    }
    let mut out = vec![text];
    out.extend(children.get(end..).unwrap_or(&[]).iter().cloned());
    out
}

fn line_node(children: &[Value]) -> Value {
    let mut node = json!({"type": "line", "children": line_contents(children)});
    if let (Some(first), Some(last)) = (children.first(), children.last())
        && field(first, "pos").is_object()
        && field(last, "pos").is_object()
    {
        put(
            &mut node,
            "pos",
            joined_pos(field(first, "pos"), field(last, "pos")),
        );
    }
    node
}

fn paragraph_lines(contents: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    for (at, child) in contents.iter().enumerate() {
        if !child.is_object() {
            continue;
        }
        let brk = field(child, "type") == "hard_break" || field(child, "type") == "soft_break";
        if brk && at + 1 < contents.len() {
            out.push(line_node(&current));
            current.clear();
        } else {
            current.push(child.clone());
        }
    }
    out.push(line_node(&current));
    out
}

fn line_block(node: &mut Fields, path: &str, state: &mut State<'_>) {
    let mut out: Vec<Value> = Vec::new();
    let blocks = node
        .get("children")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for (index, block) in blocks.iter().enumerate() {
        if !block.is_object() {
            continue;
        }
        if field(block, "type") == "comment" {
            out.push(line_node(std::slice::from_ref(block)));
            continue;
        }
        if field(block, "type") != "paragraph" {
            loss(
                state,
                &format!("{path}/children/{index}"),
                "a line block holds only lines",
            );
            continue;
        }
        if !out.is_empty() {
            out.push(json!({"type": "line", "children": []}));
        }
        let contents = field(block, "children")
            .as_array()
            .cloned()
            .unwrap_or_default();
        out.extend(paragraph_lines(&contents));
    }
    node.insert("children".into(), Value::Array(out));
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn utf16_slice(units: &[u16], from: usize, to: usize) -> String {
    String::from_utf16_lossy(units.get(from.min(to)..to.min(units.len())).unwrap_or(&[]))
}

fn offset(pos: &Value, key: &str) -> Option<usize> {
    integer(pos.get(key)).and_then(|n| usize::try_from(n).ok())
}

fn shorthand_language(node: &Fields, value: &str, state: &State<'_>) -> bool {
    let (Some(source), Some(pos)) = (state.source, node.get("pos").filter(|p| p.is_object()))
    else {
        return false;
    };
    let start = offset(pos, "startOffset").unwrap_or(0);
    let end = offset(pos, "endOffset").unwrap_or(start);
    let units = utf16(source);
    let brace = u16::from(b'{');
    let opening = units
        .get(..=start.min(units.len().saturating_sub(1)))
        .and_then(|head| head.iter().rposition(|&u| u == brace))
        .unwrap_or(0);
    utf16_slice(&units, opening, end).contains(&format!(":{value}"))
}

fn attribute_pairs(node: &mut Fields, source: &Fields, output: &mut Fields, state: &State<'_>) {
    let Some(values) = source.get("keyValues") else {
        return;
    };
    if values.is_array() {
        output.insert("keyValues".into(), values.clone());
        return;
    }
    let Some(values) = values.as_object() else {
        return;
    };
    let mut names: Vec<String> = Vec::new();
    let order = source.get("order").and_then(Value::as_array);
    for name in order
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .chain(values.keys().map(String::as_str))
    {
        if !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    for name in names {
        let Some(value) = string(values.get(&name)) else {
            continue;
        };
        let lower = name.to_lowercase();
        if lower == "id" {
            output.insert("id".into(), value.into());
        } else if lower == "class" {
            output.insert("classes".into(), json!([value]));
        } else if name == "lang" && shorthand_language(node, value, state) {
            node.insert("lang".into(), value.into());
        } else if let Some(p) = pairs.iter_mut().find(|p| p.0.to_lowercase() == lower) {
            p.1 = value.into();
        } else {
            pairs.push((name.clone(), value.into()));
        }
    }
    if !pairs.is_empty() {
        output.insert("keyValues".into(), json!(pairs));
    }
}

fn attrs(node: &mut Fields, state: &State<'_>) {
    let Some(Value::Object(source)) = node.get("attrs").cloned() else {
        return;
    };
    let mut output = Fields::new();
    if let Some(id) = source.get("id").filter(|i| i.is_string()) {
        output.insert("id".into(), id.clone());
    }
    if let Some(c) = source
        .get("classes")
        .filter(|c| c.as_array().is_some_and(|a| !a.is_empty()))
    {
        output.insert("classes".into(), c.clone());
    }
    attribute_pairs(node, &source, &mut output, state);
    if output.is_empty() {
        node.remove("attrs");
    } else {
        node.insert("attrs".into(), Value::Object(output));
    }
}

fn paragraph(node: &mut Fields, old: &Fields) {
    if old.get("blockImage") == Some(&Value::Bool(true)) {
        node.insert("role".into(), "image".into());
    }
    if old.get("blockMath") == Some(&Value::Bool(true)) {
        node.insert("role".into(), "math".into());
    }
    let only = match node.get("children").and_then(Value::as_array) {
        Some(c) if c.len() == 1 => c.first().cloned(),
        _ => None,
    };
    if let Some(child) = only.filter(|_| !node.contains_key("role")) {
        let math = field(&child, "type") == "math"
            && field(&child, "display") == true
            && field(&child, "value") != "";
        let image = field(&child, "type") == "image"
            && (!field(&child, "pos").is_object()
                || field(field(&child, "pos"), "startLine")
                    == field(field(&child, "pos"), "endLine"));
        if math {
            node.insert("role".into(), "math".into());
        } else if image {
            node.insert("role".into(), "image".into());
        }
    }
    node.remove("blockImage");
    node.remove("blockMath");
}

fn comment(node: &mut Fields, old: &Fields) {
    let value = string(old.get("content"))
        .or_else(|| string(old.get("value")))
        .unwrap_or("");
    node.insert("value".into(), value.into());
    let form = if old.get("block") == Some(&Value::Bool(true)) {
        "block"
    } else if old.get("delimited") == Some(&Value::Bool(true)) {
        "braced"
    } else {
        "line"
    };
    node.insert("form".into(), form.into());
    for f in ["content", "block", "delimited"] {
        node.remove(f);
    }
}

fn citation(node: &mut Fields, old: &Fields) {
    if old.get("suppressAuthor") == Some(&Value::Bool(true)) {
        node.insert("mode".into(), "suppress-author".into());
    }
    node.remove("suppressAuthor");
    let label = string(old.get("locatorLabel"));
    let value = old.get("locatorValue");
    if label.is_some() || value.is_some() {
        let children = match value {
            Some(v @ Value::Array(_)) => v.clone(),
            Some(Value::String(s)) => json!([{"type": "text", "value": s}]),
            _ => json!([]),
        };
        let locator = match label {
            Some(l) => json!({"label": l, "children": children}),
            None => json!({"children": children}),
        };
        node.insert("locator".into(), locator);
    }
    node.remove("locatorLabel");
    node.remove("locatorValue");
}

fn figure(node: &mut Fields, old: &Fields) {
    if let Some(Value::Array(caption)) = node.get("caption") {
        let wrapped =
            json!({"type": "caption", "children": [{"type": "paragraph", "children": caption}]});
        node.insert("caption".into(), wrapped);
    }
    // The slot walk has migrated `shortCaption` already; carve-js reads the
    // unmigrated one from `old`.
    let short = node.remove("shortCaption");
    if let (Some(short @ Value::Array(_)), true) = (short, old.get("shortCaption").is_some()) {
        if let Some(Value::Object(caption)) = node.get_mut("caption") {
            caption.insert("short".into(), short);
        }
    }
}

fn admonition(node: &mut Fields) {
    const KINDS: &[&str] = &[
        "note", "tip", "warning", "danger", "info", "success", "example", "quote",
    ];
    let Some(kind) = string(node.get("kind")).map(str::to_string) else {
        return;
    };
    if KINDS.contains(&kind.as_str()) {
        return;
    }
    node.insert("type".into(), "div".into());
    let mut attrs = node
        .get("attrs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut classes = vec![Value::from(kind)];
    classes.extend(
        attrs
            .get("classes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|c| c.is_string())
            .cloned(),
    );
    attrs.insert("classes".into(), Value::Array(classes));
    node.insert("attrs".into(), Value::Object(attrs));
    node.remove("kind");
}

fn inline_title(node: &mut Fields) {
    if let Some(title) = string(node.get("title")).map(str::to_string) {
        node.insert("title".into(), json!([{"type": "text", "value": title}]));
    }
}

fn code_block(node: &mut Fields, state: &State<'_>) {
    inline_title(node);
    let Some(pairs) = key_value_pairs(node) else {
        return;
    };
    let start = node
        .get("pos")
        .and_then(|p| offset(p, "startOffset"))
        .unwrap_or(0);
    let prefix = state.source.map_or(String::new(), |s| {
        let units = utf16(s);
        utf16_slice(&units, 0, start).trim_end().to_string()
    });
    let previous = prefix.rsplit('\n').next().unwrap_or("");
    let authored = previous.starts_with('{') && previous.contains("title=");
    let kept = pairs
        .into_iter()
        .filter(|p| at_i(p, 0) != "title" || authored)
        .collect();
    set_pairs(node, kept);
}

fn frontmatter(node: &mut Fields) {
    if let Some(v) = string(node.get("value"))
        && !v.is_empty()
        && !v.ends_with('\n')
    {
        let value = format!("{v}\n");
        node.insert("value".into(), value.into());
    }
}

fn punctuation(node: &mut Fields, old: &Fields) {
    let source = string(old.get("value")).unwrap_or("").to_string();
    node.insert("source".into(), source.into());
    let kind = string(node.get("kind")).unwrap_or("");
    let found = crate::tables::SMART_RUNS
        .iter()
        .filter(|r| r.1)
        .map(|r| (r.2, r.0, r.3))
        .chain(crate::tables::SMART_QUOTES.iter().copied())
        .chain(crate::tables::SMART_DASHES.iter().copied())
        .find(|r| r.0 == kind);
    if let Some((_, source, glyph)) = found {
        node.insert("glyph".into(), glyph.into());
        node.insert("source".into(), source.into());
    }
    node.remove("value");
}

fn option_pair(value: &Value) -> Option<(String, String)> {
    match value {
        Value::Array(p) => Some((p.first()?.as_str()?.into(), p.get(1)?.as_str()?.into())),
        Value::Object(m) => Some((
            m.get("name")?.as_str()?.into(),
            m.get("value")?.as_str()?.into(),
        )),
        _ => None,
    }
}

fn decode_option(raw: &str) -> String {
    if raw.encode_utf16().count() < 2 {
        return raw.into();
    }
    let inner = || raw.get(1..raw.len() - 1).unwrap_or("").to_string();
    if raw.starts_with('"') && raw.ends_with('"') {
        return serde_json::from_str::<String>(raw).unwrap_or_else(|_| inner());
    }
    if raw.starts_with('\'') && raw.ends_with('\'') {
        return inner();
    }
    raw.into()
}

fn typed_option(name: &str, value: String) -> Value {
    match name {
        "shift" if value == "auto" => value.into(),
        "shift" => number_value(js_number(&Value::String(value))),
        "code" if value.is_empty() => true.into(),
        "cols" if value.contains(',') => value.split(',').map(str::trim).collect(),
        _ => value.into(),
    }
}

fn include_options(node: &mut Fields, old: &Fields) {
    let Some(Value::Array(list)) = old.get("options") else {
        return;
    };
    let mut options = Fields::new();
    for (name, raw) in list.iter().filter_map(option_pair) {
        if options.contains_key(&name) {
            continue;
        }
        let value = typed_option(&name, decode_option(&raw));
        options.insert(name, value);
    }
    node.insert("options".into(), Value::Object(options));
}

fn block_shape(node: &mut Fields, old: &Fields, state: &mut State<'_>, path: &str) {
    match ty(node) {
        "list" => list_marker(node, old),
        "list_item" => task(node, old),
        "paragraph" => paragraph(node, old),
        "comment" => comment(node, old),
        "line_block" => line_block(node, path, state),
        "admonition" => {
            if let Some(label) = node.remove("label") {
                node.insert("groupLabel".into(), label);
            }
            admonition(node);
            inline_title(node);
        }
        "code_block" => code_block(node, state),
        "frontmatter" => frontmatter(node),
        "include" => include_options(node, old),
        "directive" | "div" => inline_title(node),
        _ => {}
    }
}

fn table_shape(node: &mut Fields, old: &Fields, state: &State<'_>) {
    match ty(node) {
        "table" => {
            table::migrate_table(node, old);
            if let Some(Value::Array(groups)) = node.get_mut("children") {
                for group in groups.iter_mut().filter_map(Value::as_object_mut) {
                    attrs(group, state);
                }
            }
            figure(node, old);
        }
        "table_cell" => table::migrate_cell(node, old),
        "table_row" => {
            node.remove("continuationLines");
        }
        "figure" => {
            if let Some(kind @ Value::Object(_)) = node.get("kind").cloned() {
                node.insert("children".into(), json!([kind]));
                node.remove("kind");
            }
            figure(node, old);
        }
        "figure_group" => figure(node, old),
        _ => {}
    }
}

/// C2-A-102: the 0.1 label of a display equation is its number prefix.
fn math(node: &mut Fields, old: &Fields, path: &str, state: &mut State<'_>) {
    let display = old.get("display") == Some(&Value::Bool(true));
    node.insert("display".into(), display.into());
    let label = string(old.get("label")).map(str::to_string);
    node.remove("label");
    match label {
        Some(l) if display => {
            node.insert("numberPrefix".into(), l.into());
        }
        other => {
            if other.is_some() {
                loss(
                    state,
                    &format!("{path}/label"),
                    "inline math has no number prefix",
                );
            }
            node.remove("number");
        }
    }
}

fn inline_shape(node: &mut Fields, old: &Fields, path: &str, state: &mut State<'_>) {
    match ty(node) {
        "smart_punctuation" => punctuation(node, old),
        "mention" => {
            let kind = string(old.get("kind")).unwrap_or("user").to_string();
            node.insert("kind".into(), kind.into());
        }
        "math" => math(node, old, path, state),
        "citation" => citation(node, old),
        "autolink" if node.get("text") == node.get("href") => {
            node.remove("text");
        }
        _ => {}
    }
}

pub(super) fn shape(node: &mut Fields, old: &Fields, state: &mut State<'_>, path: &str) {
    attrs(node, state);
    reference(node, old);
    block_shape(node, old, state, path);
    table_shape(node, old, state);
    inline_shape(node, old, path, state);
}
