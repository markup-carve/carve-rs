//! Lint of accepted include sources (C2-N-085 to C2-N-088).

use super::{LintOptions, declares_below_0_2, lint_core};
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
    let Some(tree) = crate::tree_json_value(source, &render) else {
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
                seen.insert(finding_key(file, pos.start_offset, pos.end_offset, finding));
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
    seen: &mut HashSet<FindingKey>,
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
    let places = selected.map(|_| section_paths(node, text));
    let mut child_options = LintOptions {
        includes: None,
        upgrade: false,
        ..context.options.clone()
    };
    let (findings, _) = crate::with_lint_tree(
        slice,
        &crate::Options::default(),
        &crate::Options::default(),
        true,
        |tree, arena| {
            child_options.upgrade =
                context.options.upgrade || (start == 0 && tree.is_some_and(declares_below_0_2));
            lint_core(slice, &child_options, tree, arena)
        },
    );
    for mut finding in findings {
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
        if !seen.insert(finding_key(
            file,
            pos.start_offset,
            pos.end_offset,
            &finding,
        )) {
            continue;
        }
        let own = finding.path.take().unwrap_or_default();
        // A code include holds its payload as one child; the file was not parsed as Carve there.
        // A finding of a section outside every selected block, such as one on the file as a whole, names the include.
        let under = match &places {
            _ if node["options"].get("code").is_some() => "/children/0".to_string(),
            Some(places) => path_under(places, &own).unwrap_or_default(),
            None => own,
        };
        finding.file = Some(file.to_string());
        finding.pos = Some(pos);
        finding.path = Some(format!("{path}{under}"));
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

/// A file included twice repeats a finding with only another path; per-column findings share a range.
type FindingKey = (String, u32, u32, &'static str, String);

fn finding_key(file: &str, start: u32, end: u32, finding: &Diagnostic) -> FindingKey {
    (
        file.to_string(),
        start,
        end,
        finding.code,
        finding.message.clone(),
    )
}

/// C2-N-087: a selected block keeps its place in the file, so its path in the file's own tree is found by that place.
fn section_paths(node: &Value, text: &str) -> HashMap<String, String> {
    let mut places = HashMap::new();
    if let Some(tree) = crate::tree_json_value(text, &crate::Options::default()) {
        let mut pending = vec![(&tree, String::new())];
        while let Some((value, path)) = pending.pop() {
            if let Some(key) = place_key(value) {
                places.entry(key).or_insert_with(|| path.clone());
            }
            let Some(object) = value.as_object() else {
                continue;
            };
            for (slot, child) in object {
                if slot == "pos" || slot == "attrs" {
                    continue;
                }
                if child["type"].is_string() {
                    pending.push((child, format!("{path}/{slot}")));
                } else if let Some(items) = child.as_array() {
                    for (index, item) in items.iter().enumerate() {
                        if item["type"].is_string() {
                            pending.push((item, format!("{path}/{slot}/{index}")));
                        }
                    }
                }
            }
        }
    }
    let mut paths = HashMap::new();
    for (index, child) in node["children"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let Some(path) = place_key(child).and_then(|key| places.get(&key)) {
            paths.insert(path.clone(), format!("/children/{index}"));
        }
    }
    paths
}

fn place_key(node: &Value) -> Option<String> {
    let start = field(node, "/pos/startOffset").as_u64()?;
    let end = field(node, "/pos/endOffset").as_u64()?;
    Some(format!("{}\0{start}\0{end}", node["type"].as_str()?))
}

/// The path under the include of a finding at `path` in the file's own tree, by the longest selected block around it.
fn path_under(paths: &HashMap<String, String>, path: &str) -> Option<String> {
    let mut end = path.len();
    while end > 0 {
        if let Some(under) = paths.get(&path[..end]) {
            return Some(format!("{under}{}", &path[end..]));
        }
        end = path[..end].rfind('/').unwrap_or(0);
    }
    None
}
