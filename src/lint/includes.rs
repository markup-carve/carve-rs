//! Lint of accepted include sources (C2-N-085 to C2-N-088).

use super::{LintOptions, lint_core, parse_tree_value};
use crate::diagnostic::{Diagnostic, IncludeSite};
use crate::include::Resolver;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

fn field<'a>(value: &'a Value, path: &str) -> &'a Value {
    value.pointer(path).unwrap_or(&Value::Null)
}

type Sources = Arc<Mutex<HashMap<String, Vec<u8>>>>;

struct Context<'a> {
    options: &'a LintOptions,
    sources: &'a HashMap<String, Vec<u8>>,
}

pub(super) fn scan(source: &str, options: &LintOptions, output: &mut Vec<Diagnostic>) {
    let Some(config) = options.includes.as_ref() else {
        return;
    };
    let stored: Sources = Arc::new(Mutex::new(HashMap::new()));
    let mut config = config.clone();
    config.resolver = recording(config.resolver, Arc::clone(&stored));
    let render = crate::Options {
        includes: Some(config.clone()),
        ..crate::Options::default()
    };
    let text = crate::tree_json(source, &render);
    let Some(tree) = parse_tree_value(&text) else {
        return;
    };
    let Ok(sources) = stored.lock() else { return };
    let context = Context {
        options,
        sources: &sources,
    };
    let mut seen = HashSet::new();
    if let Some(file) = &config.file {
        for finding in output.iter() {
            if let Some(pos) = finding.pos {
                seen.insert((file.clone(), pos.start_line, pos.start_column, finding.code));
            }
        }
    }
    let mut pending = vec![(&tree, String::new(), config.file.as_deref(), Vec::new())];
    while let Some((node, path, parent_file, chain)) = pending.pop() {
        let mut child_chain = chain;
        if node["type"] == "include" {
            if let Some(site) = include_site(node, parent_file) {
                child_chain.push(site);
            }
            lint_include(node, &path, &child_chain, &context, &mut seen, output);
        }
        if let Some(children) = node["children"].as_array() {
            let next_file = node["file"].as_str().or(parent_file);
            for (index, child) in children.iter().enumerate().rev() {
                pending.push((
                    child,
                    format!("{path}/children/{index}"),
                    next_file,
                    child_chain.clone(),
                ));
            }
        }
    }
}

fn include_site(node: &Value, file: Option<&str>) -> Option<IncludeSite> {
    let pos = node.get("pos")?;
    Some(IncludeSite {
        file: file.map(str::to_string),
        line: u32::try_from(pos["startLine"].as_u64()?).ok()?,
        column: u32::try_from(pos["startColumn"].as_u64()?).ok()?,
    })
}

fn recording(original: Resolver, stored: Sources) -> Resolver {
    Arc::new(move |request| {
        let result = original(request)?;
        if let Ok(mut sources) = stored.lock() {
            sources
                .entry(result.1.clone())
                .or_insert_with(|| result.0.clone());
        }
        Ok(result)
    })
}

fn lint_include(
    node: &Value,
    path: &str,
    chain: &[IncludeSite],
    context: &Context<'_>,
    seen: &mut HashSet<(String, u32, u32, &'static str)>,
    output: &mut Vec<Diagnostic>,
) {
    let Some(file) = node["file"].as_str() else {
        return;
    };
    let Some(bytes) = context.sources.get(file) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };
    let (slice, start, line_shift, selected) = selected_source(node, text);
    let child_options = LintOptions {
        includes: None,
        ..context.options.clone()
    };
    for mut finding in lint_core(slice, &child_options) {
        let Some(mut pos) = finding.pos else { continue };
        pos.start_offset = pos.start_offset.saturating_add(crate::ast::Pos::at(start));
        pos.end_offset = pos.end_offset.saturating_add(crate::ast::Pos::at(start));
        pos.start_line = pos
            .start_line
            .saturating_add(crate::ast::Pos::at(line_shift));
        pos.end_line = pos.end_line.saturating_add(crate::ast::Pos::at(line_shift));
        if selected.is_some_and(|(from, to)| pos.start_line < from || pos.start_line > to) {
            continue;
        }
        if node["options"].get("code").is_some() && finding.code != "bidi-control-in-source" {
            continue;
        }
        if !seen.insert((
            file.to_string(),
            pos.start_line,
            pos.start_column,
            finding.code,
        )) {
            continue;
        }
        finding.file = Some(file.to_string());
        finding.pos = Some(pos);
        finding.path = Some(path_at(node, path, file, pos.start_offset, pos.end_offset));
        finding.included_by = chain.to_vec();
        output.push(finding);
    }
}

fn selected_source<'a>(node: &Value, text: &'a str) -> (&'a str, usize, usize, Option<(u32, u32)>) {
    if let Some(value) = node["options"].get("lines").and_then(Value::as_str)
        && let Some((start, end)) = line_span(text, value)
    {
        let shift = text
            .get(..start)
            .unwrap_or("")
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
        return (text.get(start..end).unwrap_or(""), start, shift, None);
    }
    if node["section"].as_str().is_some() {
        return (text, 0, 0, selected_lines(node));
    }
    if node["options"].get("region").is_some()
        && let Some((start, end)) = selected_offsets(node, text.len())
    {
        let start = text[..start].rfind('\n').map_or(0, |offset| offset + 1);
        let end = text[end..]
            .find('\n')
            .map_or(text.len(), |offset| end + offset + 1);
        let shift = text
            .get(..start)
            .unwrap_or("")
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
        return (text.get(start..end).unwrap_or(""), start, shift, None);
    }
    (text, 0, 0, None)
}

fn line_span(text: &str, range: &str) -> Option<(usize, usize)> {
    let (from, to) = range.split_once('-')?;
    let (from, to) = (from.parse::<usize>().ok()?, to.parse::<usize>().ok()?);
    if from == 0 || to < from {
        return None;
    }
    let mut start = None;
    let mut end = text.len();
    let mut offset = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if index + 1 == from {
            start = Some(offset);
        }
        offset += line.len();
        if index + 1 == to {
            end = offset;
            break;
        }
    }
    Some((start?, end))
}

fn selected_lines(node: &Value) -> Option<(u32, u32)> {
    let children = node["children"].as_array()?;
    let range = children
        .iter()
        .filter(|child| {
            !matches!(
                child["type"].as_str(),
                Some(
                    "link_reference_definition"
                        | "footnote"
                        | "abbreviation_def"
                        | "citation_definition"
                )
            )
        })
        .filter_map(|child| {
            Some((
                field(child, "/pos/startLine").as_u64()?,
                field(child, "/pos/endLine").as_u64()?,
            ))
        })
        .fold(None, |range: Option<(u64, u64)>, (start, end)| {
            Some(range.map_or((start, end), |(a, b)| (a.min(start), b.max(end))))
        });
    range.map(|(a, b)| {
        (
            crate::ast::Pos::at(usize::try_from(a).unwrap_or(usize::MAX)),
            crate::ast::Pos::at(usize::try_from(b).unwrap_or(usize::MAX)),
        )
    })
}

fn selected_offsets(node: &Value, length: usize) -> Option<(usize, usize)> {
    let children = node["children"].as_array()?;
    let start = children
        .iter()
        .filter_map(|c| field(c, "/pos/startOffset").as_u64())
        .min()?;
    let end = children
        .iter()
        .filter_map(|c| field(c, "/pos/endOffset").as_u64())
        .max()?;
    let start = usize::try_from(start).ok()?.min(length);
    let end = usize::try_from(end).ok()?.min(length);
    (start <= end).then_some((start, end))
}

fn path_at(node: &Value, base: &str, file: &str, from: u32, to: u32) -> String {
    let mut best = base.to_string();
    let mut pending = vec![(node, base.to_string())];
    while let Some((value, path)) = pending.pop() {
        if field(value, "/pos/file") == file
            && field(value, "/pos/startOffset")
                .as_u64()
                .is_some_and(|start| start <= u64::from(from))
            && field(value, "/pos/endOffset")
                .as_u64()
                .is_some_and(|end| end >= u64::from(to))
            && path.len() > best.len()
        {
            best.clone_from(&path);
        }
        if let Some(children) = value["children"].as_array() {
            for (index, child) in children.iter().enumerate() {
                pending.push((child, format!("{path}/children/{index}")));
            }
        }
    }
    best
}
