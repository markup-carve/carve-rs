//! Lint of accepted include sources (C2-N-085 to C2-N-088).

use super::{LintOptions, declares_below_0_2, lint_core};
use crate::diagnostic::{Diagnostic, IncludeSite};
use crate::include::Resolver;
use crate::view::V;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

fn field<'a>(value: &'a Value, path: &str) -> &'a Value {
    value.pointer(path).unwrap_or(&Value::Null)
}

pub(super) type Sources = Arc<Mutex<HashMap<String, Vec<u8>>>>;

struct Context<'a> {
    options: &'a LintOptions,
    sources: &'a HashMap<String, Vec<u8>>,
    /// Raw findings of an included slice, keyed by file, slice start and length; a file included at many sites is linted once.
    linted: HashMap<(String, usize, usize), Vec<Diagnostic>>,
    places: HashMap<String, HashMap<String, String>>,
    defaults: crate::Options,
}

impl Context<'_> {
    /// The path of every node of the file's own tree, built once per file.
    fn places(&mut self, file: &str, text: &str) -> &HashMap<String, String> {
        self.places
            .entry(file.to_string())
            .or_insert_with(|| file_places(text, &self.defaults))
    }

    /// The findings of `slice` of `file` in the slice's own positions, linted once per slice.
    fn findings(&mut self, file: &str, slice: &str, start: usize) -> Vec<Diagnostic> {
        self.linted
            .entry((file.to_string(), start, slice.len()))
            .or_insert_with(|| {
                crate::with_lint_tree(
                    slice,
                    &self.defaults,
                    &self.defaults,
                    true,
                    |tree, arena| {
                        let child_options = included_options(self.options, start, tree);
                        lint_core(slice, &child_options, tree, arena)
                    },
                )
                .0
            })
            .clone()
    }
}

/// The include findings of one expanded tree, before the entry findings dedupe them.
pub(super) struct Scanned {
    findings: Vec<Diagnostic>,
    snippets: Option<SnippetUse>,
}

/// What [`reconcile_snippets`] reads of the tree.
struct SnippetUse {
    used: HashSet<String>,
    names: HashMap<(String, u64), String>,
    winners: HashSet<(String, u64)>,
}

/// The include config with a resolver that keeps every accepted source.
pub(super) fn recorded(config: &crate::include::Includes) -> (crate::include::Includes, Sources) {
    let stored: Sources = Arc::new(Mutex::new(HashMap::new()));
    let mut config = config.clone();
    config.resolver = recording(config.resolver, Arc::clone(&stored));
    (config, stored)
}

/// Lint with a second expansion, for a lint whose own tree is not the default render tree.
pub(super) fn scan(source: &str, options: &LintOptions, output: &mut Vec<Diagnostic>) {
    let Some(config) = options.includes.as_ref() else {
        return;
    };
    let (config, stored) = recorded(config);
    let render = crate::Options {
        includes: Some(config),
        ..crate::Options::default()
    };
    let Some(tree) = crate::tree_json_value(source, &render) else {
        return;
    };
    let unused = output.iter().any(|d| d.code == "unused-snippet");
    let scanned = collect(V::Json(&tree), options, &stored, unused);
    finish(scanned, options, output);
}

/// Lints the accepted includes of the expanded `tree`; `unused` says an entry finding is `unused-snippet`.
pub(super) fn collect(
    tree: V<'_>,
    options: &LintOptions,
    stored: &Sources,
    unused: bool,
) -> Scanned {
    let mut findings = Vec::new();
    let Ok(sources) = stored.lock() else {
        return Scanned {
            findings,
            snippets: None,
        };
    };
    let file = options.includes.as_ref().and_then(|c| c.file.as_deref());
    let mut context = Context {
        options,
        sources: &sources,
        linted: HashMap::new(),
        places: HashMap::new(),
        defaults: crate::Options::default(),
    };
    let mut seen = HashSet::new();
    let mut pending = vec![(tree, String::new(), file, Vec::new())];
    while let Some((node, path, parent_file, chain)) = pending.pop() {
        let mut child_chain = chain;
        if node.ty().as_str() == Some("include") {
            // C2-T-020: a site in the top-level document names no file.
            let site_file = parent_file.filter(|_| !child_chain.is_empty());
            if let Some(site) = include_site(node, site_file) {
                child_chain.push(site);
            }
            lint_include(
                node,
                &path,
                &child_chain,
                &mut context,
                &mut seen,
                &mut findings,
            );
        }
        if let Some(children) = node.at("children").as_array() {
            let next_file = node.at("file").as_str().or(parent_file);
            for index in (0..children.len()).rev() {
                let Some(child) = children.get(index) else {
                    continue;
                };
                pending.push((
                    child,
                    format!("{path}/children/{index}"),
                    next_file,
                    child_chain.clone(),
                ));
            }
        }
    }
    let snippets = (unused || findings.iter().any(|d| d.code == "unused-snippet"))
        .then(|| match tree {
            V::Json(tree) => Some(snippet_use(tree)),
            // The registry reads the file of each definition, which only the stamped tree carries.
            V::Doc(doc) => Some(snippet_use(&crate::stamped_value(doc))),
            _ => None,
        })
        .flatten();
    Scanned { findings, snippets }
}

/// Adds the include findings an entry finding does not already report, then reconciles snippets.
pub(super) fn finish(scanned: Scanned, options: &LintOptions, output: &mut Vec<Diagnostic>) {
    let file = options.includes.as_ref().and_then(|c| c.file.as_deref());
    let mut seen = HashSet::new();
    if let Some(file) = file {
        for finding in output.iter() {
            if let Some(pos) = finding.pos {
                seen.insert(finding_key(file, pos.start_offset, pos.end_offset, finding));
            }
        }
    }
    for finding in scanned.findings {
        let entry = finding.pos.is_some_and(|pos| {
            seen.contains(&finding_key(
                finding.file.as_deref().unwrap_or_default(),
                pos.start_offset,
                pos.end_offset,
                &finding,
            ))
        });
        if !entry {
            output.push(finding);
        }
    }
    if let Some(snippets) = scanned.snippets {
        reconcile_snippets(&snippets, output);
    }
}

/// The snippet definitions of an expanded tree in registry order (C2-N-090),
/// keyed by the file and the offset of their `$`.
#[derive(Default)]
struct Registry<'t> {
    names: HashMap<(String, u64), &'t str>,
    winners: HashSet<(String, u64)>,
    taken: HashSet<&'t str>,
}

impl<'t> Registry<'t> {
    /// An include may stand in a container, as `- {{ a.crv }}`; its definitions count where it stands.
    fn walk(&mut self, node: &'t Value, file: &str) {
        let Some(map) = node.as_object() else {
            return;
        };
        for (slot, value) in map {
            if slot == "pos" || slot == "attrs" {
                continue;
            }
            match value {
                Value::Array(items) => self.list(items, file),
                Value::Object(_) if value["type"].is_string() => {
                    self.list(std::slice::from_ref(value), file);
                }
                _ => {}
            }
        }
    }

    fn list(&mut self, items: &'t [Value], file: &str) {
        for child in items {
            if child["type"] == "include" {
                if child.get("children").is_some() {
                    self.walk(child, child["file"].as_str().unwrap_or(file));
                }
                continue;
            }
            if child["type"] != "snippet_definition" {
                self.walk(child, file);
                continue;
            }
            let (Some(name), Some(start)) = (
                child["name"].as_str(),
                field(child, "/pos/startOffset").as_u64(),
            ) else {
                continue;
            };
            let file = field(child, "/pos/file").as_str().unwrap_or(file);
            let key = (file.to_string(), start + 1);
            if self.taken.insert(name) {
                self.winners.insert(key.clone());
            }
            self.names.insert(key, name);
        }
    }
}

/// The snippet names used anywhere in the expanded tree and its definitions in registry order.
fn snippet_use(tree: &Value) -> SnippetUse {
    let mut used = HashSet::new();
    let mut pending = vec![tree];
    while let Some(node) = pending.pop() {
        if node["type"] == "snippet_use"
            && let Some(name) = node["name"].as_str()
        {
            used.insert(name.to_string());
        }
        match node {
            Value::Object(map) => {
                pending.extend(map.iter().filter(|(k, _)| *k != "pos").map(|(_, v)| v));
            }
            Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    let mut registry = Registry::default();
    registry.walk(tree, "");
    let Registry { names, winners, .. } = registry;
    SnippetUse {
        used,
        names: names
            .into_iter()
            .map(|(key, name)| (key, name.to_string()))
            .collect(),
        winners,
    }
}

/// C2-N-096 across files: a definition that a use in another file of the
/// expansion names is used, and one that loses to an earlier definition of
/// its name (C2-N-090) takes no hint.
fn reconcile_snippets(snippets: &SnippetUse, output: &mut Vec<Diagnostic>) {
    if !output.iter().any(|d| d.code == "unused-snippet") {
        return;
    }
    let SnippetUse {
        used,
        names,
        winners,
    } = snippets;
    output.retain(|d| {
        if d.code != "unused-snippet" {
            return true;
        }
        let Some(pos) = d.pos else { return true };
        // A finding of the entry file has no include chain, and its nodes name no file.
        let file = if d.included_by.is_empty() {
            String::new()
        } else {
            d.file.clone().unwrap_or_default()
        };
        let key = (file, u64::from(pos.start_offset));
        names
            .get(&key)
            .is_none_or(|name| winners.contains(&key) && !used.contains(name))
    });
}

fn include_site(node: V<'_>, file: Option<&str>) -> Option<IncludeSite> {
    let pos = node.pos()?;
    Some(IncludeSite {
        file: file.map(str::to_string),
        line: u32::try_from(pos.at("startLine").as_u64()?).ok()?,
        column: u32::try_from(pos.at("startColumn").as_u64()?).ok()?,
        start_offset: usize::try_from(pos.at("startOffset").as_u64()?).ok()?,
        end_offset: usize::try_from(pos.at("endOffset").as_u64()?).ok()?,
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
    node: V<'_>,
    path: &str,
    chain: &[IncludeSite],
    context: &mut Context<'_>,
    seen: &mut HashSet<FindingKey>,
    output: &mut Vec<Diagnostic>,
) {
    // C2-N-086: only an include the expansion accepted is linted; a refused one has no children.
    let (Some(file), Some(_)) = (node.at("file").as_str(), node.get("children")) else {
        return;
    };
    let Some(bytes) = context.sources.get(file) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };
    let (slice, start, line_shift, selected) = selected_source(node, text);
    let places = selected.map(|_| section_paths(node, context.places(file, text)));
    let findings = context.findings(file, slice, start);
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
        if node.at("options").get("code").is_some() && finding.code != "bidi-control-in-source" {
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
            _ if node.at("options").get("code").is_some() => "/children/0".to_string(),
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

fn included_options(
    options: &LintOptions,
    start: usize,
    tree: Option<crate::view::V<'_>>,
) -> LintOptions {
    LintOptions {
        includes: None,
        upgrade: options.upgrade || (start == 0 && tree.is_some_and(declares_below_0_2)),
        ..options.clone()
    }
}

fn selected_source<'a>(node: V<'_>, text: &'a str) -> (&'a str, usize, usize, Option<(u32, u32)>) {
    if let Some(value) = node.at("options").at("lines").as_str()
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
    if node.at("section").as_str().is_some() {
        return (text, 0, 0, selected_lines(node));
    }
    if node.at("options").get("region").is_some()
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

fn selected_lines(node: V<'_>) -> Option<(u32, u32)> {
    let children = node.at("children").as_array()?;
    let range = (0..children.len())
        .filter_map(|index| children.get(index))
        .filter(|child| {
            !matches!(
                child.ty().as_str(),
                Some(
                    "link_reference_definition"
                        | "footnote"
                        | "abbreviation_def"
                        | "snippet_definition"
                        | "citation_definition"
                )
            )
        })
        .filter_map(|child| {
            Some((
                child.field("/pos/startLine").as_u64()?,
                child.field("/pos/endLine").as_u64()?,
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

fn selected_offsets(node: V<'_>, length: usize) -> Option<(usize, usize)> {
    let children = node.at("children").as_array()?;
    let children = (0..children.len()).filter_map(|index| children.get(index));
    let start = children
        .clone()
        .filter_map(|c| c.field("/pos/startOffset").as_u64())
        .min()?;
    let end = children
        .filter_map(|c| c.field("/pos/endOffset").as_u64())
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

/// The path of every node of the file's own tree, by its place.
fn file_places(text: &str, defaults: &crate::Options) -> HashMap<String, String> {
    let mut places = HashMap::new();
    if let Some(tree) = crate::tree_json_value(text, defaults) {
        let mut pending = vec![(&tree, String::new())];
        while let Some((value, path)) = pending.pop() {
            if let Some(key) = place_key(V::Json(value)) {
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
    places
}

/// C2-N-087: a selected block keeps its place in the file, so its path in the file's own tree is found by that place.
fn section_paths(node: V<'_>, places: &HashMap<String, String>) -> HashMap<String, String> {
    let mut paths = HashMap::new();
    let children = node.at("children").as_array();
    for (index, child) in children
        .into_iter()
        .flat_map(|children| (0..children.len()).filter_map(move |index| children.get(index)))
        .enumerate()
    {
        if let Some(path) = place_key(child).and_then(|key| places.get(&key)) {
            paths.insert(path.clone(), format!("/children/{index}"));
        }
    }
    paths
}

fn place_key(node: V<'_>) -> Option<String> {
    let start = node.field("/pos/startOffset").as_u64()?;
    let end = node.field("/pos/endOffset").as_u64()?;
    Some(format!("{}\0{start}\0{end}", node.ty().as_str()?))
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
