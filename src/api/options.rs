//! Option validation (C2-H-137): option values from JSON, as the JSON
//! boundary and the bindings pass them, and the lint options (C2-W-109).

use super::{Error, LintOptions, Options, WriteOptions};
use crate::options::{HeadingIds, Mode, Profile, Social, SoftBreak, UrlPolicy};
use serde_json::{Map, Value};
use std::sync::OnceLock;

/// The stages of `read_by` in `data/options.json` that each call runs.
pub(crate) fn stages(call: &str) -> Option<&'static [&'static str]> {
    Some(match call {
        "parse" | "parseWithDiagnostics" | "parseWithLayout" => &["parse", "resolve"],
        "render" | "renderWithDiagnostics" => &["parse", "resolve", "render"],
        "format" | "formatWithDiagnostics" => &["format", "parse"],
        "write" | "writeWithDiagnostics" => &["format"],
        "lint" => &["lint", "parse"],
        "importDocument" => &["import"],
        "decodeTree" | "validateTree" => &["tree"],
        "includeDependencies" | "encodeTree" | "encodeEnvelope" | "migrateTree" => &[],
        _ => return None,
    })
}

fn option_rows() -> &'static [Value] {
    static ROWS: OnceLock<Vec<Value>> = OnceLock::new();
    ROWS.get_or_init(|| {
        serde_json::from_str::<Value>(include_str!("../../spec-data/options.json"))
            .ok()
            .and_then(|v| v.get("options").and_then(Value::as_array).cloned())
            .unwrap_or_default()
    })
}

fn rule_ids() -> &'static [String] {
    static IDS: OnceLock<Vec<String>> = OnceLock::new();
    IDS.get_or_init(|| {
        serde_json::from_str::<Value>(include_str!("../../spec-data/lint-rules.json"))
            .ok()
            .and_then(|v| v.get("rules").and_then(Value::as_array).cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
            .collect()
    })
}

/// C2-H-204: the codes of the render-loss channel.
fn loss_code_ids() -> &'static [String] {
    static IDS: OnceLock<Vec<String>> = OnceLock::new();
    IDS.get_or_init(|| {
        serde_json::from_str::<Value>(include_str!("../../spec-data/lint-rules.json"))
            .ok()
            .and_then(|v| v.get("rules").and_then(Value::as_array).cloned())
            .unwrap_or_default()
            .iter()
            .filter(|r| r.get("channel").and_then(Value::as_str) == Some("render-loss"))
            .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
            .collect()
    })
}

fn loss_codes(v: &Value) -> Result<Vec<String>, Error> {
    let codes = string_list("allowLosses", v)?;
    match codes.iter().find(|c| !loss_code_ids().contains(c)) {
        Some(c) => Err(Error::option(
            "allowLosses",
            format!("{c} is no code of the render-loss channel"),
        )),
        None => Ok(codes),
    }
}

/// The options a JSON request sets, for every stage at once.
#[derive(Debug, Clone, Default)]
pub(crate) struct CallOptions {
    pub options: Options,
    pub lint: LintOptions,
    pub write: WriteOptions,
    /// `extensions` by name: the standard extensions to enable.
    pub extensions: Vec<String>,
    /// `renderers`: the extensions whose standard renderer is registered.
    pub renderers: Vec<String>,
}

/// Reads the options of a call from a JSON object, refusing an unknown
/// option, an option no stage of the call reads, and a refused value.
pub(crate) fn from_json(call: &str, value: Option<&Value>) -> Result<CallOptions, Error> {
    let Some(call_stages) = stages(call) else {
        return Err(Error::option("call", format!("unknown call {call}")));
    };
    let mut out = CallOptions::default();
    let Some(value) = value else {
        return Ok(out);
    };
    let Some(map) = value.as_object() else {
        return Err(Error::option("options", "the options are not an object"));
    };
    for (name, v) in map {
        let Some(row) = option_rows().iter().find(|r| r["name"] == *name) else {
            return Err(Error::option(name, "unknown option"));
        };
        if !read_by(row).iter().any(|s| call_stages.contains(s)) {
            return Err(Error::option(
                name,
                format!("the call {call} does not read it"),
            ));
        }
        set(&mut out, name, v)?;
    }
    out.lint.includes.clone_from(&out.options.includes);
    out.write.render = out.options.clone();
    out.write.extensions.clone_from(&out.extensions);
    Ok(out)
}

fn read_by(row: &Value) -> Vec<&str> {
    match &row["read_by"] {
        Value::String(s) => vec![s.as_str()],
        Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

fn set(out: &mut CallOptions, name: &str, v: &Value) -> Result<(), Error> {
    if let Some(slot) = boolean_slot(out, name) {
        *slot = boolean(name, v)?;
        return Ok(());
    }
    match name {
        "rules" | "platforms" => lint_option(&mut out.lint, name, v),
        "extensions" => {
            out.extensions = standard_names(v)?;
            Ok(())
        }
        "renderers" => {
            out.renderers = standard_renderers(v)?;
            Ok(())
        }
        "labels" | "symbols" | "social" | "urlPolicy" | "includes" => {
            structure(&mut out.options, name, v)
        }
        _ => enumerated(&mut out.options, name, v),
    }
}

fn enumerated(o: &mut Options, name: &str, v: &Value) -> Result<(), Error> {
    match name {
        "headingIds" => o.heading_ids = enum_value(name, v, HeadingIds::from_name)?,
        "mode" => o.mode = enum_value(name, v, mode)?,
        "profile" => o.profile = enum_value(name, v, Profile::from_name)?,
        "softBreak" => o.soft_break = enum_value(name, v, SoftBreak::from_name)?,
        "allowLosses" => o.allow_losses = loss_codes(v)?,
        "maxLosses" => {
            o.max_losses = v
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| Error::option(name, "the value is not a count"))?;
        }
        _ => return Err(Error::option(name, "this engine does not read it")),
    }
    Ok(())
}

fn structure(o: &mut Options, name: &str, v: &Value) -> Result<(), Error> {
    match name {
        "labels" => labels(o, v)?,
        "symbols" => o.symbols = string_map(name, v)?.into_iter().collect(),
        "social" => o.social = Some(social(v)?),
        "urlPolicy" => o.url_policy = url_policy(v)?,
        _ => o.includes = includes(v)?,
    }
    Ok(())
}

fn boolean_slot<'o>(out: &'o mut CallOptions, name: &str) -> Option<&'o mut bool> {
    let o = &mut out.options;
    Some(match name {
        "rawHtml" => &mut o.raw_html,
        "roundtrip" => &mut o.roundtrip,
        "sections" => &mut o.sections,
        "smartTypography" => &mut o.smart_typography,
        "sourceLines" => &mut o.source_lines,
        "strictLosses" => &mut o.strict_losses,
        "alignTables" => &mut out.write.align_tables,
        "normalize" => &mut out.write.normalize,
        _ => return None,
    })
}

fn lint_option(lint: &mut LintOptions, name: &str, v: &Value) -> Result<(), Error> {
    if name == "rules" {
        lint.rules = string_map(name, v)?.into_iter().collect();
    } else {
        lint.platforms = string_list(name, v)?;
    }
    check_lint(lint)
}

fn mode(s: &str) -> Option<Mode> {
    match s {
        "interactive" => Some(Mode::Interactive),
        "static" => Some(Mode::Static),
        _ => None,
    }
}

fn enum_value<T>(name: &str, v: &Value, read: fn(&str) -> Option<T>) -> Result<T, Error> {
    v.as_str()
        .and_then(read)
        .ok_or_else(|| Error::option(name, format!("{v} is not a value of the option")))
}

fn boolean(name: &str, v: &Value) -> Result<bool, Error> {
    v.as_bool()
        .ok_or_else(|| Error::option(name, "the value is not a boolean"))
}

fn object<'v>(name: &str, v: &'v Value) -> Result<&'v Map<String, Value>, Error> {
    v.as_object()
        .ok_or_else(|| Error::option(name, "the value is not an object"))
}

fn string_map(name: &str, v: &Value) -> Result<Vec<(String, String)>, Error> {
    object(name, v)?
        .iter()
        .map(|(k, item)| {
            item.as_str()
                .map(|s| (k.clone(), s.to_string()))
                .ok_or_else(|| Error::option(name, format!("the value of {k} is not a string")))
        })
        .collect()
}

fn string_list(name: &str, v: &Value) -> Result<Vec<String>, Error> {
    let Some(items) = v.as_array() else {
        return Err(Error::option(name, "the value is not a list"));
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| Error::option(name, "an item is not a string"))
        })
        .collect()
}

/// C2-H-125: a key left out keeps its English default; an unknown key is refused.
fn labels(o: &mut Options, v: &Value) -> Result<(), Error> {
    for (k, text) in string_map("labels", v)? {
        if !crate::tables::DEFAULT_LABELS
            .iter()
            .any(|(key, _)| *key == k)
        {
            return Err(Error::option("labels", format!("unknown label key {k}")));
        }
        o.labels.insert(k, text);
    }
    Ok(())
}

/// C2-H-126: URL templates; the host functions of the option have no JSON form.
fn social(v: &Value) -> Result<Social, Error> {
    let mut out = Social::default();
    for (k, item) in object("social", v)? {
        let template = item
            .as_str()
            .ok_or_else(|| Error::option("social", format!("{k} is not a string")))?;
        check_template(template)?;
        match k.as_str() {
            "mentionUrl" => out.mention_url = Some(template.to_string()),
            "tagUrl" => out.tag_url = Some(template.to_string()),
            _ => {
                return Err(Error::option(
                    "social",
                    format!("{k} takes host code or is unknown"),
                ));
            }
        }
    }
    Ok(out)
}

/// A placeholder other than `{name}` is an option error.
fn check_template(template: &str) -> Result<(), Error> {
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = rest.get(open + 1..).unwrap_or("");
        let Some(close) = after.find('}') else { break };
        let inner = after.get(..close).unwrap_or("");
        let placeholder = !inner.is_empty()
            && inner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_');
        if placeholder && inner != "name" {
            return Err(Error::option(
                "social",
                format!("unknown placeholder {{{inner}}}"),
            ));
        }
        rest = after;
    }
    Ok(())
}

fn url_policy(v: &Value) -> Result<UrlPolicy, Error> {
    let map = object("urlPolicy", v)?;
    match (map.get("deny"), map.get("allow"), map.len()) {
        (Some(list), None, 1) => Ok(UrlPolicy::Deny(string_list("urlPolicy", list)?)),
        (None, Some(list), 1) => Ok(UrlPolicy::Allow(string_list("urlPolicy", list)?)),
        _ => Err(Error::option(
            "urlPolicy",
            "the value is an object of exactly one of deny or allow",
        )),
    }
}

/// `includes` in JSON: `null`, or `{"root": DIR}` with optional `file` (the
/// id of the top-level document), `depth` and `bytes` (C2-N-003). The root
/// reads the file system.
fn includes(v: &Value) -> Result<Option<crate::include::Includes>, Error> {
    if v.is_null() {
        return Ok(None);
    }
    let map = object("includes", v)?;
    let Some(root) = map.get("root").and_then(Value::as_str) else {
        return Err(Error::option("includes", "root is missing or not a string"));
    };
    let mut inc = crate::include::Includes::new(root_resolver(root)?);
    for (k, item) in map {
        match (k.as_str(), item) {
            ("root", _) => {}
            ("file", Value::String(s)) => inc.file = Some(s.clone()),
            ("depth", n) => inc.depth = limit(n)?,
            ("bytes", n) => inc.bytes = limit(n)?,
            _ => {
                return Err(Error::option(
                    "includes",
                    format!("unknown or invalid field {k}"),
                ));
            }
        }
    }
    Ok(Some(inc))
}

#[cfg(feature = "fs")]
fn root_resolver(root: &str) -> Result<crate::include::Resolver, Error> {
    crate::include::fs::resolver(std::path::Path::new(root))
        .map_err(|_| Error::option("includes", "the include root cannot be read"))
}

// Without `fs` the library opens no file, so a root on the JSON boundary is refused.
#[cfg(not(feature = "fs"))]
fn root_resolver(_root: &str) -> Result<crate::include::Resolver, Error> {
    Err(Error::option(
        "includes",
        "this build reads no files (the fs feature is off)",
    ))
}

fn limit(v: &Value) -> Result<usize, Error> {
    v.as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| Error::option("includes", "a limit is not a count"))
}

/// `extensions` in JSON: the names of standard extensions (C2-X-039). Host
/// extensions take host code (C2-T-010) and register in Rust.
fn standard_names(v: &Value) -> Result<Vec<String>, Error> {
    v.as_array()
        .and_then(|items| {
            items
                .iter()
                .map(|i| i.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| Error::option("extensions", "not a list of standard extension names"))
}

/// `renderers` in JSON: `{NAME: "standard"}`, the standard renderer of a
/// standard extension; a host renderer takes host code (C2-T-010).
fn standard_renderers(v: &Value) -> Result<Vec<String>, Error> {
    let Some(map) = v.as_object() else {
        return Err(Error::option("renderers", "not an object"));
    };
    map.iter()
        .map(|(name, r)| {
            if r == "standard" {
                Ok(name.clone())
            } else {
                Err(Error::option(
                    "renderers",
                    format!("{name}: only \"standard\" has a JSON form"),
                ))
            }
        })
        .collect()
}

/// C2-W-109: rule ids of `data/lint-rules.json` set to `on` or `off`, and
/// platform names of `data/options.json`.
pub(crate) fn check_lint(opts: &LintOptions) -> Result<(), Error> {
    for (id, setting) in &opts.rules {
        if !rule_ids().iter().any(|known| known == id) {
            return Err(Error::option("rules", format!("unknown rule {id}")));
        }
        if setting != "on" && setting != "off" {
            return Err(Error::option(
                "rules",
                format!("{id} is neither on nor off"),
            ));
        }
    }
    let platforms = option_rows()
        .iter()
        .find(|r| r["name"] == "platforms")
        .and_then(|r| r["values"].as_array());
    for p in &opts.platforms {
        if !platforms.is_some_and(|known| known.iter().any(|k| k == p.as_str())) {
            return Err(Error::option("platforms", format!("unknown platform {p}")));
        }
    }
    Ok(())
}
