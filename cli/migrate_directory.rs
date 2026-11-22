use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{NodeData, RcDom};
use serde_json::{Value, json};

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
    let dom =
        html5ever::parse_document(RcDom::default(), html5ever::ParseOpts::default()).one(html);
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

fn encode_fragment(value: &str) -> String {
    let mut result = String::new();
    for ch in value.chars() {
        if ch.is_whitespace()
            || ch.is_control()
            || matches!(ch, '"' | '<' | '>' | '\\' | '`' | '^' | '{' | '|' | '}')
        {
            let mut buffer = [0; 4];
            for byte in ch.encode_utf8(&mut buffer).bytes() {
                use std::fmt::Write;
                let _ = write!(result, "%{byte:02X}");
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
    let dom =
        html5ever::parse_document(RcDom::default(), html5ever::ParseOpts::default()).one(html);
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

#[allow(clippy::too_many_lines)]
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
    let Some(fragment) = raw_fragment.map_or(Some(String::new()), decode_url) else {
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

fn check_links(
    temporary: &Path,
    files: &mut [Value],
    assets: &[String],
    trees: &HashMap<String, carve::api::Tree>,
) -> Result<HashSet<String>, String> {
    let outputs: HashSet<String> = files
        .iter()
        .filter_map(|f| f["output"].as_str().map(str::to_owned))
        .chain(assets.iter().cloned())
        .collect();
    let mut ids = HashMap::new();
    let mut aliases = HashMap::new();
    for file in files.iter() {
        let output = file["output"].as_str().ok_or("invalid output name")?;
        let tree = trees.get(output).ok_or("missing imported tree")?;
        let html = carve::api::render_tree(tree, &carve::api::Options::default())
            .map_err(|e| e.to_string())?;
        let file_ids = html_ids(html);
        let mut file_aliases = HashMap::new();
        for id in &file_ids {
            file_aliases
                .entry(id.to_lowercase())
                .and_modify(|old: &mut Option<String>| {
                    if old.as_ref() != Some(id) {
                        *old = None;
                    }
                })
                .or_insert(Some(id.clone()));
        }
        ids.insert(output.to_owned(), file_ids);
        aliases.insert(output.to_owned(), file_aliases);
    }
    let host = carve::api::Carve::new();
    let mut broken = HashSet::new();
    for file in files.iter_mut() {
        let source = file["source"]
            .as_str()
            .ok_or("invalid source name")?
            .to_owned();
        let output = file["output"]
            .as_str()
            .ok_or("invalid output name")?
            .to_owned();
        let path = temporary.join(&output);
        let tree = trees.get(&output).ok_or("missing imported tree")?;
        let mut document: Value =
            serde_json::from_str(&carve::api::encode_tree(tree)).map_err(|e| e.to_string())?;
        let mut links = Vec::new();
        let mut changed = false;
        check_value(
            &mut document,
            &source,
            &outputs,
            &ids,
            &aliases,
            &mut links,
            &mut changed,
        );
        if links
            .iter()
            .any(|link: &Value| link["status"] == "unresolved")
        {
            broken.insert(source);
        }
        file["links"] = json!(links);
        if changed {
            let tree = carve::api::decode_tree(&document.to_string()).map_err(|e| e.to_string())?;
            let written = host
                .write(&tree, &carve::api::WriteOptions::default())
                .map_err(|e| e.to_string())?;
            fs::write(path, written).map_err(|e| e.to_string())?;
        }
    }
    Ok(broken)
}

#[allow(clippy::cognitive_complexity)]
fn check_value(
    value: &mut Value,
    source: &str,
    outputs: &HashSet<String>,
    ids: &HashMap<String, HashSet<String>>,
    aliases: &HashMap<String, HashMap<String, Option<String>>>,
    links: &mut Vec<Value>,
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
                            links.push(json!({"target":destination,"status":"unresolved","reason":"raw HTML srcset needs manual review"}));
                        } else if let Some((_, mut check, _)) =
                            inspect_link(&destination, source, outputs, ids, aliases)
                        {
                            if check["status"] == "rewritten" {
                                check = json!({"target":destination,"status":"unresolved","reason":"raw HTML link needs manual rewrite"});
                            }
                            links.push(check);
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
                    .filter(|s| !s.is_empty())
                {
                    if let Some((href, mut check, _)) =
                        inspect_link(destination, source, outputs, ids, aliases)
                    {
                        if href != destination && object.get("ref").is_some() {
                            check = json!({"target":destination,"status":"unresolved","reason":"reference link needs manual rewrite"});
                        } else if href != destination {
                            object.insert(key.to_owned(), Value::String(href));
                            *changed = true;
                        }
                        links.push(check);
                    }
                }
            }
            for (key, child) in object.iter_mut() {
                if key != "attrs" && key != "pos" {
                    check_value(child, source, outputs, ids, aliases, links, changed);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                check_value(item, source, outputs, ids, aliases, links, changed);
            }
        }
        _ => {}
    }
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
#[allow(clippy::unnecessary_wraps)]
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
    children.sort_by_key(fs::DirEntry::file_name);
    for entry in children {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
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

#[allow(clippy::too_many_lines)]
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
        let mut trees = HashMap::<String, carve::api::Tree>::new();
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
                let migrated = carve::api::try_migrate_markdown_for_directory(&text)
                    .map_err(|e| format!("{source_name}: {e}"))?;
                fs::write(target, migrated.source).map_err(|e| e.to_string())?;
                let diagnostics: Vec<Value> = migrated
                    .diagnostics
                    .iter()
                    .map(|d| serde_json::to_value(d).map_err(|e| e.to_string()))
                    .collect::<Result<_, _>>()?;
                if diagnostics
                    .iter()
                    .any(|row| row["severity"] == "warning" || row["severity"] == "error")
                {
                    review.push(source_name.clone());
                }
                trees.insert(destination_name.clone(), migrated.tree);
                files.push(json!({"source": source_name, "output": destination_name, "diagnostics": diagnostics}));
            }
        }
        let broken = check_links(&temporary, &mut files, &assets, &trees)?;
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
    println!(
        "{files} file(s) converted; {assets} asset(s) copied; {review} file(s) need review; {skipped} entry(s) skipped. Report: {}",
        output.join("migration-report.json").display()
    );
    Ok(u8::from(check_loss && (review > 0 || skipped > 0)))
}
