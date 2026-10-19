use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{NodeData, RcDom};
use serde_json::{json, Value};

fn decode_url(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            let digits = std::str::from_utf8(hex).ok()?;
            decoded.push(u8::from_str_radix(digits, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn mapped_path(source: &str, path: &str) -> Option<String> {
    let mut parts: Vec<&str> = source.split('/').collect();
    parts.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

fn html_ids(html: String) -> HashSet<String> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut ids = HashSet::new();
    let mut pending = vec![dom.document.clone()];
    while let Some(node) = pending.pop() {
        for child in node.children.borrow().iter() {
            if let NodeData::Element { name, attrs, .. } = &child.data {
                for attr in attrs.borrow().iter() {
                    if &*attr.name.local == "id"
                        || (&*attr.name.local == "name" && &*name.local == "a")
                    {
                        ids.insert(attr.value.to_string());
                    }
                }
            }
            pending.push(child.clone());
        }
    }
    ids
}

fn heading_aliases(
    html: String,
    mut raw: HashMap<String, usize>,
) -> HashMap<String, Option<String>> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut aliases = HashMap::new();
    let mut pending = vec![dom.document.clone()];
    while let Some(node) = pending.pop() {
        for child in node.children.borrow().iter() {
            if let NodeData::Element { name, attrs, .. } = &child.data {
                let heading_section = &*name.local == "section" && child.children.borrow().iter().any(|nested| {
                    matches!(&nested.data, NodeData::Element { name, .. } if matches!(&*name.local, "h1" | "h2" | "h3" | "h4" | "h5" | "h6"))
                });
                let nested_heading =
                    matches!(&*name.local, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
                if heading_section || nested_heading {
                    for attr in attrs
                        .borrow()
                        .iter()
                        .filter(|attr| &*attr.name.local == "id")
                    {
                        let id = attr.value.to_string();
                        if let Some(count) = raw.get_mut(&id) {
                            if *count > 0 {
                                *count -= 1;
                                continue;
                            }
                        }
                        if id.rsplit_once('-').is_some_and(|(_, number)| {
                            !number.is_empty()
                                && !number.starts_with('0')
                                && number != "1"
                                && number.bytes().all(|byte| byte.is_ascii_digit())
                        }) {
                            continue;
                        }
                        let key = id.to_lowercase();
                        aliases
                            .entry(key)
                            .and_modify(|old: &mut Option<String>| {
                                if old.as_ref() != Some(&id) {
                                    *old = None;
                                }
                            })
                            .or_insert(Some(id));
                    }
                }
            }
            pending.push(child.clone());
        }
    }
    aliases
}

fn encode_fragment(value: &str) -> String {
    let mut result = String::new();
    for ch in value.chars() {
        if ch.is_whitespace()
            || ch.is_control()
            || matches!(ch, '"' | '<' | '>' | '\\' | '`' | '^' | '{' | '|' | '}')
        {
            let mut buffer = [0; 4];
            for byte in ch.encode_utf8(&mut buffer).bytes() {
                result.push_str(&format!("%{byte:02X}"));
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn normalize_html_url(value: &str) -> String {
    value
        .trim_matches(|ch| ch <= '\u{20}')
        .chars()
        .filter(|ch| !matches!(ch, '\t' | '\n' | '\r'))
        .collect()
}

fn srcset_urls(value: &str) -> Vec<String> {
    let bytes = value.as_bytes();
    let mut urls = Vec::new();
    let mut index = 0;
    let space = |byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c');
    while index < bytes.len() {
        while index < bytes.len() && (space(bytes[index]) || bytes[index] == b',') {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && !space(bytes[index]) {
            index += 1;
        }
        let raw = &value[start..index];
        let url = raw.trim_end_matches(',');
        if !url.is_empty() {
            urls.push(normalize_html_url(url));
        }
        if raw.ends_with(',') {
            continue;
        }
        let mut in_parentheses = false;
        while index < bytes.len() {
            let byte = bytes[index];
            index += 1;
            if byte == b'(' {
                in_parentheses = true;
            } else if byte == b')' {
                in_parentheses = false;
            } else if byte == b',' && !in_parentheses {
                break;
            }
        }
    }
    urls
}

fn html_urls(html: &str) -> Vec<(String, bool)> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut urls = Vec::new();
    let mut pending = vec![dom.document.clone()];
    while let Some(node) = pending.pop() {
        for child in node.children.borrow().iter() {
            if let NodeData::Element { name, attrs, .. } = &child.data {
                for attr in attrs.borrow().iter() {
                    let key = &*attr.name.local;
                    let tag = &*name.local;
                    if key == "srcset" || key == "imagesrcset" {
                        let value = attr.value.to_string();
                        if value.to_ascii_lowercase().contains("data:") {
                            urls.push((value, true));
                        } else {
                            for target in srcset_urls(&value) {
                                urls.push((target, false));
                            }
                        }
                    } else if key == "href"
                        || key == "src"
                        || key == "poster"
                        || (key == "data" && tag == "object")
                        || (key == "action" && tag == "form")
                        || (key == "formaction" && (tag == "button" || tag == "input"))
                    {
                        urls.push((normalize_html_url(&attr.value), false));
                    }
                }
            }
            pending.push(child.clone());
        }
    }
    urls
}

/// A fragment rewritten to its heading id's case: the fragment as written, and as rewritten.
type Recased = Option<(String, String)>;

fn inspect_link(
    destination: &str,
    source: &str,
    outputs: &HashSet<String>,
    ids: &std::collections::HashMap<String, HashSet<String>>,
    aliases: &HashMap<String, HashMap<String, Option<String>>>,
) -> Option<(String, Value, Recased)> {
    if destination.starts_with("//")
        || destination.split(':').next().is_some_and(|prefix| {
            prefix
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                && prefix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+.-".contains(&b))
                && destination.contains(':')
        })
    {
        return None;
    }
    if destination.starts_with('/') {
        return Some((
            destination.to_string(),
            json!({"target": destination, "status": "unresolved", "reason": "site-root path needs route mapping"}),
            None,
        ));
    }
    let (before_fragment, raw_fragment) = destination
        .split_once('#')
        .map_or((destination, None), |(before, fragment)| {
            (before, Some(fragment))
        });
    let (raw_path, query) = before_fragment
        .split_once('?')
        .map_or((before_fragment, ""), |(path, _)| {
            (path, &before_fragment[path.len()..])
        });
    let failure = |reason: &str| {
        (
            destination.to_string(),
            json!({"target": destination, "status": "unresolved", "reason": reason}),
            None,
        )
    };
    let Some(path) = decode_url(raw_path) else {
        return Some(failure("invalid URL encoding"));
    };
    let Some(fragment) = raw_fragment.map(decode_url).unwrap_or(Some(String::new())) else {
        return Some(failure("invalid URL encoding"));
    };
    let Some(target) = (if path.is_empty() {
        Some(source.to_string())
    } else {
        mapped_path(source, &path)
    }) else {
        return Some(failure("outside source directory"));
    };
    let mapped = if target.to_ascii_lowercase().ends_with(".markdown") {
        format!("{}.crv", &target[..target.len() - 9])
    } else if target.to_ascii_lowercase().ends_with(".md") {
        format!("{}.crv", &target[..target.len() - 3])
    } else {
        target.clone()
    };
    if !outputs.contains(&mapped) {
        return Some(failure("target missing from output"));
    }
    if !fragment.is_empty() && !ids.contains_key(&mapped) {
        return Some(failure("asset fragment needs site check"));
    }
    let resolved = if ids.get(&mapped).is_some_and(|ids| ids.contains(&fragment)) {
        Some(fragment.as_str())
    } else {
        aliases
            .get(&mapped)
            .and_then(|map| map.get(&fragment.to_lowercase()))
            .and_then(Option::as_deref)
    };
    if !fragment.is_empty() && resolved.is_none() {
        return Some(failure("fragment missing from target"));
    }
    if mapped != target
        && !path.is_empty()
        && !raw_path.to_ascii_lowercase().ends_with(".md")
        && !raw_path.to_ascii_lowercase().ends_with(".markdown")
    {
        return Some(failure("encoded extension needs manual rewrite"));
    }
    let new_path = if mapped == target || path.is_empty() {
        raw_path.to_string()
    } else if raw_path.to_ascii_lowercase().ends_with(".markdown") {
        format!("{}.crv", &raw_path[..raw_path.len() - 9])
    } else {
        format!("{}.crv", &raw_path[..raw_path.len() - 3])
    };
    let recased = raw_fragment
        .zip(resolved)
        .filter(|(_, id)| !fragment.is_empty() && *id != fragment)
        .map(|(raw, id)| (raw.to_string(), encode_fragment(id)));
    let href = format!(
        "{new_path}{query}{}",
        raw_fragment.map_or(String::new(), |raw| format!(
            "#{}",
            recased.as_ref().map_or(raw, |(_, to)| to.as_str())
        ))
    );
    let status = if href == destination {
        "valid"
    } else {
        "rewritten"
    };
    Some((
        href,
        json!({"target": destination, "status": status}),
        recased,
    ))
}

#[derive(Default)]
struct Found<'a> {
    links: Vec<Value>,
    fixups: Vec<carve::MigrationDiagnostic>,
    /// The file's converted destinations mapped to their authored spelling.
    authored: Option<&'a HashMap<String, String>>,
}

fn check_value(
    value: &mut Value,
    source: &str,
    outputs: &HashSet<String>,
    ids: &std::collections::HashMap<String, HashSet<String>>,
    aliases: &HashMap<String, HashMap<String, Option<String>>>,
    found: &mut Found,
    changed: &mut bool,
) {
    match value {
        Value::Object(object) => {
            if matches!(
                object.get("type").and_then(Value::as_str),
                Some("raw_inline" | "raw_block")
            ) && object.get("format").and_then(Value::as_str) == Some("html")
            {
                if let Some(content) = object.get("content").and_then(Value::as_str) {
                    for (destination, manual) in html_urls(content) {
                        if manual {
                            found.links.push(json!({"target": destination, "status": "unresolved", "reason": "raw HTML srcset needs manual review"}));
                            continue;
                        }
                        if let Some((_, mut check, _)) =
                            inspect_link(&destination, source, outputs, ids, aliases)
                        {
                            if check["status"] == "rewritten" {
                                check = json!({"target": destination, "status": "unresolved", "reason": "raw HTML link needs manual rewrite"});
                            }
                            found.links.push(check);
                        }
                    }
                }
            }
            let key = match object.get("type").and_then(Value::as_str) {
                Some("link") => Some("href"),
                Some("image") => Some("src"),
                _ => None,
            };
            if let Some(key) = key {
                if let Some(destination) = object
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|href| !href.is_empty())
                {
                    if let Some((href, mut check, recased)) =
                        inspect_link(destination, source, outputs, ids, aliases)
                    {
                        if href != destination && object.get("ref").is_some() {
                            check = json!({"target": destination, "status": "unresolved", "reason": "reference link needs manual rewrite"});
                        } else if href != destination {
                            if let Some((from, to)) = recased {
                                // CARVE-P2-031: the row names the destination as the
                                // Markdown link wrote it.
                                let written = found
                                    .authored
                                    .and_then(|map| map.get(destination))
                                    .map_or(destination, String::as_str);
                                let from =
                                    written.split_once('#').map_or(from.as_str(), |(_, f)| f);
                                found
                                    .fixups
                                    .push(carve::fragment_fixup_diagnostic(written, from, &to));
                            }
                            object.insert(key.to_string(), Value::String(href));
                            *changed = true;
                        }
                        found.links.push(check);
                    }
                }
            }
            for (key, child) in object.iter_mut() {
                if key != "attrs" && key != "position" {
                    check_value(child, source, outputs, ids, aliases, found, changed);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                check_value(item, source, outputs, ids, aliases, found, changed);
            }
        }
        _ => {}
    }
}

fn check_links(
    temporary: &Path,
    files: &mut [Value],
    assets: &[String],
    authored: &HashMap<String, HashMap<String, String>>,
) -> Result<HashSet<String>, String> {
    let outputs: HashSet<String> = files
        .iter()
        .filter_map(|file| file["output"].as_str().map(str::to_string))
        .chain(assets.iter().cloned())
        .collect();
    let mut ids = std::collections::HashMap::new();
    let mut aliases = HashMap::new();
    for file in files.iter() {
        let output = file["output"].as_str().ok_or("invalid output name")?;
        let text = fs::read_to_string(temporary.join(output)).map_err(|e| e.to_string())?;
        let html = carve::to_html(&text);
        ids.insert(output.to_string(), html_ids(html.clone()));
        let raw = carve::migration_raw_heading_ids(&text);
        aliases.insert(output.to_string(), heading_aliases(html, raw));
    }
    let mut broken = HashSet::new();
    for file in files.iter_mut() {
        let source = file["source"]
            .as_str()
            .ok_or("invalid source name")?
            .to_string();
        let output = file["output"]
            .as_str()
            .ok_or("invalid output name")?
            .to_string();
        let path = temporary.join(&output);
        let original = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut document = serde_json::from_str::<Value>(&carve::to_json(&carve::parse(&original)))
            .map_err(|e| e.to_string())?;
        let mut changed = false;
        let mut found = Found {
            authored: authored.get(&source),
            ..Found::default()
        };
        check_value(
            &mut document,
            &source,
            &outputs,
            &ids,
            &aliases,
            &mut found,
            &mut changed,
        );
        let Found { links, fixups, .. } = found;
        if !fixups.is_empty() {
            let report: Value =
                serde_json::from_str(&super::migration_report_json(&carve::MigrationReport {
                    schema_version: 2,
                    source_format: carve::SourceFormat::Markdown,
                    mode: None,
                    adapter: None,
                    diagnostics: fixups,
                }))
                .map_err(|e| e.to_string())?;
            let rows = report["diagnostics"]
                .as_array()
                .ok_or("invalid migration report")?;
            file["diagnostics"]
                .as_array_mut()
                .ok_or("invalid migration report")?
                .extend(rows.iter().cloned());
        }
        if links.iter().any(|link| link["status"] == "unresolved") {
            broken.insert(source);
        }
        file["links"] = json!(links);
        if changed {
            let document = carve::from_json(&document.to_string()).map_err(|e| e.to_string())?;
            let written = carve::render_carve(&document).map_err(|e| e.to_string())?;
            fs::write(path, written).map_err(|e| e.to_string())?;
        }
    }
    Ok(broken)
}

#[derive(Clone, Copy)]
enum Kind {
    Markdown,
    Asset,
    Skipped,
}

/// The name without its `.md` or `.markdown` extension, matched ASCII
/// case-insensitively: a Unicode lowercase fold (the Kelvin sign is `k`) would
/// accept a name whose byte length no longer matches the extension.
fn markdown_stem(name: &str) -> Option<&str> {
    [".markdown", ".md"].into_iter().find_map(|extension| {
        let cut = name.len().checked_sub(extension.len())?;
        name.as_bytes()[cut..]
            .eq_ignore_ascii_case(extension.as_bytes())
            .then(|| &name[..cut])
    })
}

/// The file the walk saw, as (device, inode). A read compares it with what it
/// opened, so an entry swapped for a symlink or another file in between is
/// refused instead of followed.
type Identity = Option<(u64, u64)>;

#[cfg(unix)]
fn identity(metadata: &fs::Metadata) -> Identity {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn identity(_: &fs::Metadata) -> Identity {
    None
}

/// Open an entry for reading only if it is still the regular file the walk saw.
fn open_unchanged(path: &Path, seen: Identity, name: &str) -> Result<fs::File, String> {
    let changed = || format!("changed during migration: {name}");
    let before = fs::symlink_metadata(path).map_err(|_| changed())?;
    if !before.is_file() || identity(&before) != seen {
        return Err(changed());
    }
    let file = fs::File::open(path).map_err(|e| format!("cannot read {name}: {e}"))?;
    let opened = file
        .metadata()
        .map_err(|e| format!("cannot read {name}: {e}"))?;
    if !opened.is_file() || identity(&opened) != seen {
        return Err(changed());
    }
    Ok(file)
}

fn collect(
    root: &Path,
    dir: &Path,
    entries: &mut Vec<(PathBuf, Kind, Identity)>,
) -> Result<(), String> {
    let mut children = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        let path = entry.path();
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        let kind = metadata.file_type();
        if kind.is_dir() {
            collect(root, &path, entries)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_path_buf();
            let name = entry.file_name();
            let classification = if !kind.is_file() {
                Kind::Skipped
            } else if markdown_stem(&name.to_string_lossy()).is_some() {
                Kind::Markdown
            } else {
                Kind::Asset
            };
            entries.push((relative, classification, identity(&metadata)));
        }
    }
    Ok(())
}

fn display(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn future_path(path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or("cannot resolve output path")?
                        .to_os_string(),
                );
                ancestor = ancestor
                    .parent()
                    .ok_or("cannot resolve output path")?
                    .to_path_buf();
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let mut resolved = fs::canonicalize(ancestor).map_err(|e| e.to_string())?;
    for name in missing.iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

pub fn run(source: &str, output: &str, check_loss: bool) -> Result<u8, String> {
    let source =
        fs::canonicalize(source).map_err(|e| format!("cannot open source directory: {e}"))?;
    if !source.is_dir() {
        return Err("source is not a directory".into());
    }
    let requested = PathBuf::from(output);
    let absolute_output = if requested.is_absolute() {
        requested
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(requested)
    };
    let output = future_path(&absolute_output)?;
    if fs::symlink_metadata(&output).is_ok() {
        return Err(format!("output already exists: {}", output.display()));
    }
    if output.starts_with(&source) {
        return Err("output must be outside the source directory".into());
    }
    let mut entries = Vec::new();
    collect(&source, &source, &mut entries)?;
    entries.sort_by_key(|(path, _, _)| display(path).into_bytes());
    if !entries
        .iter()
        .any(|(_, kind, _)| matches!(kind, Kind::Markdown))
    {
        return Err("source contains no Markdown files".into());
    }
    let parent = output.parent().ok_or("output needs a directory name")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(
        ".{}-{}-{}",
        output.file_name().unwrap().to_string_lossy(),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&temporary).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(usize, usize, usize, usize), String> {
        let mut files = Vec::<Value>::new();
        let mut authored = HashMap::<String, HashMap<String, String>>::new();
        let mut assets = Vec::<String>::new();
        let mut skipped = Vec::<String>::new();
        let mut review = Vec::<String>::new();
        let mut destinations = HashSet::from(["migration-report.json".to_string()]);
        for (relative, kind, seen) in &entries {
            let source_name = display(relative);
            if matches!(kind, Kind::Skipped) {
                skipped.push(source_name);
                continue;
            }
            let destination = if matches!(kind, Kind::Markdown) {
                let name = relative
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("Markdown filename is not UTF-8")?;
                let stem = markdown_stem(name).ok_or("invalid Markdown filename")?;
                relative.with_file_name(format!("{stem}.crv"))
            } else {
                relative.clone()
            };
            let destination_name = display(&destination);
            if !destinations.insert(destination_name.to_lowercase()) {
                return Err(format!(
                    "two inputs map to the same output: {destination_name}"
                ));
            }
            let target = temporary.join(&destination);
            fs::create_dir_all(target.parent().ok_or("invalid output path")?)
                .map_err(|e| e.to_string())?;
            let mut input = open_unchanged(&source.join(relative), *seen, &source_name)?;
            if matches!(kind, Kind::Asset) {
                let permissions = input.metadata().map_err(|e| e.to_string())?.permissions();
                let mut copy = fs::File::create(&target).map_err(|e| e.to_string())?;
                std::io::copy(&mut input, &mut copy)
                    .map_err(|e| format!("cannot read {source_name}: {e}"))?;
                fs::set_permissions(&target, permissions).map_err(|e| e.to_string())?;
                assets.push(source_name);
            } else {
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut input, &mut bytes)
                    .map_err(|e| format!("cannot read {source_name}: {e}"))?;
                // CARVE-P0-023: a document's ill-formed bytes are replaced, as
                // single-file migration does.
                let text = String::from_utf8_lossy(&bytes);
                let (migrated, destinations) =
                    carve::try_migrate_markdown_for_directory_with_destinations(&text)
                        .map_err(|e| format!("{source_name}: {e}"))?;
                authored.insert(source_name.clone(), destinations);
                fs::write(target, migrated.value).map_err(|e| e.to_string())?;
                let report: Value =
                    serde_json::from_str(&super::migration_report_json(&migrated.report))
                        .map_err(|e| e.to_string())?;
                let diagnostics = report["diagnostics"]
                    .as_array()
                    .ok_or("invalid migration report")?
                    .clone();
                if diagnostics
                    .iter()
                    .any(|row| matches!(row["fidelity"].as_str(), Some("degraded" | "dropped")))
                {
                    review.push(source_name.clone());
                }
                files.push(json!({"source": source_name, "output": destination_name, "diagnostics": diagnostics}));
            }
        }
        let broken = check_links(&temporary, &mut files, &assets, &authored)?;
        for file in &files {
            let source = file["source"].as_str().ok_or("invalid source name")?;
            if broken.contains(source) && !review.iter().any(|item| item == source) {
                review.push(source.to_string());
            }
        }
        review.sort();
        let counts = (files.len(), assets.len(), review.len(), skipped.len());
        let report = json!({"schemaVersion": 1, "sourceFormat": "markdown", "files": files, "assets": assets, "skipped": skipped, "review": review});
        fs::write(
            temporary.join("migration-report.json"),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            ),
        )
        .map_err(|e| e.to_string())?;
        // Claim the output with an exclusive mkdir: `rename` replaces an
        // empty directory on POSIX, so one created after the existence check
        // would otherwise be replaced silently.
        fs::create_dir(&output)
            .map_err(|_| format!("output appeared during conversion: {}", output.display()))?;
        #[cfg(not(unix))]
        fs::remove_dir(&output).map_err(|e| e.to_string())?;
        if let Err(error) = fs::rename(&temporary, &output) {
            let _ = fs::remove_dir(&output);
            return Err(error.to_string());
        }
        Ok(counts)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    let (files, assets, review, skipped) = result?;
    println!("{files} file(s) converted; {assets} asset(s) copied; {review} file(s) need review; {skipped} entry(s) skipped. Report: {}", output.join("migration-report.json").display());
    Ok(if check_loss && (review > 0 || skipped > 0) {
        1
    } else {
        0
    })
}

#[cfg(test)]
mod tests {
    use super::run;
    use serde_json::{json, Value};
    use std::fs;

    #[test]
    fn converts_a_tree_and_refuses_overwrites_and_collisions() {
        let root = std::env::temp_dir().join(format!(
            "carve-folder-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        let output = root.join("output");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/clean.md"), "# Clean\n").unwrap();
        fs::write(
            source.join("loss.markdown"),
            "A footnote[^a].\n\n[^a]: Note.\n",
        )
        .unwrap();
        fs::write(source.join("asset.svg"), "<svg/>").unwrap();
        assert_eq!(
            run(source.to_str().unwrap(), output.to_str().unwrap(), true).unwrap(),
            1
        );
        assert!(fs::read_to_string(output.join("nested/clean.crv"))
            .unwrap()
            .contains("# Clean"));
        assert_eq!(
            fs::read_to_string(output.join("asset.svg")).unwrap(),
            "<svg/>"
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("migration-report.json")).unwrap())
                .unwrap();
        assert_eq!(report["files"].as_array().unwrap().len(), 2);
        assert_eq!(report["review"], serde_json::json!(["loss.markdown"]));
        assert!(
            run(source.to_str().unwrap(), output.to_str().unwrap(), false)
                .unwrap_err()
                .contains("already exists")
        );
        let nested = source.join("new/out");
        assert!(
            run(source.to_str().unwrap(), nested.to_str().unwrap(), false)
                .unwrap_err()
                .contains("outside")
        );
        assert!(!source.join("new").exists());
        let traversal = source.join("../source/new/out");
        assert!(
            run(source.to_str().unwrap(), traversal.to_str().unwrap(), false)
                .unwrap_err()
                .contains("outside")
        );
        assert!(!source.join("new").exists());
        #[cfg(unix)]
        {
            let alias = root.join("alias");
            std::os::unix::fs::symlink(&source, &alias).unwrap();
            let through_alias = alias.join("new/out");
            assert!(run(
                source.to_str().unwrap(),
                through_alias.to_str().unwrap(),
                false
            )
            .unwrap_err()
            .contains("outside"));
            assert!(!source.join("new").exists());
        }
        fs::write(source.join("migration-report.json"), "source").unwrap();
        let collision = root.join("collision");
        assert!(
            run(source.to_str().unwrap(), collision.to_str().unwrap(), false)
                .unwrap_err()
                .contains("same output")
        );
        assert!(!collision.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dot_md_is_converted_as_a_document() {
        let root = std::env::temp_dir().join(format!(
            "carve-dot-md-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join(".md"), "# Dot\n").unwrap();
        let output = root.join("output");
        assert_eq!(
            run(source.to_str().unwrap(), output.to_str().unwrap(), true).unwrap(),
            0
        );
        assert!(output.join(".crv").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn an_entry_swapped_for_a_symlink_after_the_walk_is_refused() {
        let root = std::env::temp_dir().join(format!(
            "carve-swap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let entry = root.join("a.md");
        let outside = root.join("secret.txt");
        fs::write(&entry, "# A\n").unwrap();
        fs::write(&outside, "secret").unwrap();
        let seen = super::identity(&fs::symlink_metadata(&entry).unwrap());
        assert!(super::open_unchanged(&entry, seen, "a.md").is_ok());
        fs::remove_file(&entry).unwrap();
        std::os::unix::fs::symlink(&outside, &entry).unwrap();
        assert_eq!(
            super::open_unchanged(&entry, seen, "a.md").unwrap_err(),
            "changed during migration: a.md"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ill_formed_bytes_are_replaced_and_extensions_match_ascii_only() {
        let root = std::env::temp_dir().join(format!(
            "carve-ill-formed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("bad.MD"), b"# before\xffafter\n").unwrap();
        // U+212A KELVIN SIGN lowercases to `k`; this name is an asset.
        fs::write(source.join("x.mar\u{212A}down"), "asset").unwrap();
        let output = root.join("output");
        assert_eq!(
            run(source.to_str().unwrap(), output.to_str().unwrap(), false).unwrap(),
            0
        );
        assert!(fs::read_to_string(output.join("bad.crv"))
            .unwrap()
            .contains("before\u{fffd}after"));
        assert_eq!(
            fs::read_to_string(output.join("x.mar\u{212A}down")).unwrap(),
            "asset"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_case_only_fragment_fixups() {
        let root = std::env::temp_dir().join(format!(
            "carve-fixup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("a.md"), "# Guide\n\nSee [steps](b.md#install-steps), [same](b.md#Install-Steps), [unicode](b.md#über-uns) and [whole](b.md).\n").unwrap();
        fs::write(
            source.join("b.md"),
            "# Install Steps\n\nRun it.\n\n# Über Uns\n\n# Twin\n\n# twin\n",
        )
        .unwrap();
        fs::write(
            source.join("c.md"),
            "[twin](b.md#TWIN) [none](b.md#nothing)\n",
        )
        .unwrap();
        let output = root.join("output");
        assert_eq!(
            run(source.to_str().unwrap(), output.to_str().unwrap(), true).unwrap(),
            1
        );
        assert_eq!(
            fs::read_to_string(output.join("a.crv")).unwrap(),
            "# Guide\n\nSee [steps](b.crv#Install-Steps), [same](b.crv#Install-Steps), [unicode](b.crv#Über-Uns) and [whole](b.crv).\n"
        );
        let report: Value = serde_json::from_str(
            &fs::read_to_string(output.join("migration-report.json")).unwrap(),
        )
        .unwrap();
        let file = |name: &str| {
            report["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|file| file["source"] == name)
                .unwrap()
                .clone()
        };
        let fixups = |file: &Value| {
            file["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["code"] == "link-fragment-case-rewritten")
                .cloned()
                .collect::<Vec<_>>()
        };
        let a = file("a.md");
        assert!(a["links"]
            .as_array()
            .unwrap()
            .contains(&json!({"target": "b.md#install-steps", "status": "rewritten"})));
        assert_eq!(
            fixups(&a),
            vec![
                json!({
                    "code": "link-fragment-case-rewritten",
                    "message": "Rewrote link fragment #install-steps to #Install-Steps, the one heading id it matches without regard to case.",
                    "severity": "info",
                    "fidelity": "normalized",
                    "confidence": "inferred",
                    "path": "link:b.md#install-steps",
                }),
                json!({
                    "code": "link-fragment-case-rewritten",
                    "message": "Rewrote link fragment #über-uns to #Über-Uns, the one heading id it matches without regard to case.",
                    "severity": "info",
                    "fidelity": "normalized",
                    "confidence": "inferred",
                    "path": "link:b.md#über-uns",
                })
            ]
        );
        let c = file("c.md");
        assert_eq!(
            c["links"],
            json!([
                {"target": "b.md#TWIN", "status": "unresolved", "reason": "fragment missing from target"},
                {"target": "b.md#nothing", "status": "unresolved", "reason": "fragment missing from target"},
            ])
        );
        assert!(fixups(&c).is_empty());
        assert_eq!(report["review"], json!(["c.md"]));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checks_links_and_rewrites_only_verified_targets() {
        let root = std::env::temp_dir().join(format!(
            "carve-links-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        fs::create_dir_all(source.join("chapters")).unwrap();
        fs::write(source.join("index.md"), "# Home\n\n[good](chapters/intro.md#Intro) [self](#Home) [self-low](#home) [query](?view=1) [encoded](chapters/a%23b.md#A) [bad](chapters/intro.md#intro) [third](chapters/intro.md#intro-2) [quote](chapters/intro.md#quote-head) [empty](chapters/intro.md#) [gone](missing.md) [route](/docs/intro) ![image](lost.png) [asset](manual.pdf#page=2)\n").unwrap();
        fs::write(source.join("chapters/a#b.md"), "# A\n").unwrap();
        fs::write(source.join("manual.pdf"), "pdf").unwrap();
        fs::write(
            source.join("chapters/intro.md"),
            "# Intro\n\n[home](../index.md#Home)\n\n# Intro\n\n# Intro\n\n> # Quote Head\n",
        )
        .unwrap();
        let output = root.join("output");
        assert_eq!(
            run(source.to_str().unwrap(), output.to_str().unwrap(), true).unwrap(),
            1
        );
        assert!(fs::read_to_string(output.join("index.crv"))
            .unwrap()
            .contains("(chapters/intro.crv#Intro)"));
        assert!(fs::read_to_string(output.join("index.crv"))
            .unwrap()
            .contains("[bad](chapters/intro.crv#Intro)"));
        assert!(fs::read_to_string(output.join("index.crv"))
            .unwrap()
            .contains("[empty](chapters/intro.crv#)"));
        assert!(fs::read_to_string(output.join("index.crv"))
            .unwrap()
            .contains("[third](chapters/intro.md#intro-2)"));
        assert!(fs::read_to_string(output.join("index.crv"))
            .unwrap()
            .contains("[quote](chapters/intro.crv#Quote-Head)"));
        assert!(fs::read_to_string(output.join("index.crv"))
            .unwrap()
            .contains("(chapters/a%23b.crv#A)"));
        assert!(fs::read_to_string(output.join("chapters/intro.crv"))
            .unwrap()
            .contains("(../index.crv#Home)"));
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("migration-report.json")).unwrap())
                .unwrap();
        assert_eq!(report["review"], serde_json::json!(["index.md"]));
        let links = report["files"][2]["links"].as_array().unwrap();
        assert!(links
            .iter()
            .any(|link| link["target"] == "#Home" && link["status"] == "valid"));
        assert!(links
            .iter()
            .any(|link| link["target"] == "#home" && link["status"] == "rewritten"));
        assert!(links
            .iter()
            .any(|link| link["target"] == "?view=1" && link["status"] == "valid"));
        assert!(links
            .iter()
            .any(|link| link["target"] == "manual.pdf#page=2"
                && link["reason"] == "asset fragment needs site check"));
        assert!(links
            .iter()
            .any(|link| link["target"] == "lost.png" && link["status"] == "unresolved"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn data_id_is_not_a_fragment_target() {
        let root = std::env::temp_dir().join(format!(
            "carve-link-id-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("page.md"),
            "# Page\n\n[false anchor](other.md#shadow) [false code](other.md#ghost) [raw heading](other.md#fake) [native](other.md#same) [legacy](other.md#legacy)\n\n<a href=\" other.md \">raw</a>\n\n<img src=\" chart.png \" srcset=\"missing-responsive.png 2x, chart,v2.png 1x\">\n\n<link rel=\"preload\" as=\"image\" imagesrcset=\"chart,v2.png 2x\">\n\n<video poster=\"missing-poster.png\"></video>\n",
        )
        .unwrap();
        fs::write(source.join("chart.png"), "png").unwrap();
        fs::write(source.join("chart,v2.png"), "png").unwrap();
        fs::write(
            source.join("other.md"),
            "# Other\n\n<div data-id=\"shadow\">Text</div>\n\n`id=\"ghost\"`\n\n<h1 id=\"Fake\">Raw</h1>\n\n# Same\n\n<h1 id=\"Same\">Raw</h1>\n\n<a name=\"legacy\"></a>\n",
        )
        .unwrap();
        let output = root.join("output");
        run(source.to_str().unwrap(), output.to_str().unwrap(), false).unwrap();
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("migration-report.json")).unwrap())
                .unwrap();
        let page = report["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["source"] == "page.md")
            .unwrap();
        assert!(page["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|link| link["target"] == "other.md#shadow"
                && link["reason"] == "fragment missing from target"));
        assert!(page["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|link| link["target"] == "other.md#ghost"
                && link["reason"] == "fragment missing from target"));
        assert!(page["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|link| link["target"] == "other.md#fake"
                && link["reason"] == "fragment missing from target"));
        assert!(page["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|link| link["target"] == "other.md#same" && link["status"] == "rewritten"));
        assert!(page["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|link| link["target"] == "other.md#legacy" && link["status"] == "rewritten"));
        assert!(page["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|link| link["target"] == "other.md"
                && link["reason"] == "raw HTML link needs manual rewrite"));
        let links = page["links"].as_array().unwrap();
        assert!(links
            .iter()
            .any(|link| link["target"] == "chart.png" && link["status"] == "valid"));
        assert_eq!(
            links
                .iter()
                .filter(|link| link["target"] == "chart,v2.png" && link["status"] == "valid")
                .count(),
            2
        );
        assert!(links
            .iter()
            .any(|link| link["target"] == "missing-responsive.png"
                && link["reason"] == "target missing from output"));
        assert!(links
            .iter()
            .any(|link| link["target"] == "missing-poster.png"
                && link["reason"] == "target missing from output"));
        fs::remove_dir_all(root).unwrap();
    }
}
