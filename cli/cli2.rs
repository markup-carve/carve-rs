//! The 0.2 command-line contract in spec/0.2/tooling.md.
use carve::api::{
    self, Carve, Entry, ImportOptions, LintOptions, LossOptions, Options, WriteOptions,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Default)]
struct Args {
    command: String,
    files: Vec<String>,
    values: BTreeMap<String, Value>,
    help: bool,
    version: bool,
}

fn fail(category: &str, message: impl std::fmt::Display) -> i32 {
    eprintln!("carve[{category}]: {message}");
    if matches!(
        category,
        "render-loss" | "writer-refused" | "import-tree-invalid"
    ) {
        1
    } else {
        2
    }
}
fn api_fail(error: api::Error) -> i32 {
    fail(error.category(), error)
}
fn usage(message: impl std::fmt::Display) -> i32 {
    fail("usage", message)
}
fn flag_rows() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("../spec-data/cli.json")).unwrap()["flags"]
        .as_array()
        .unwrap()
        .clone()
}
fn flag_row<'a>(rows: &'a [Value], command: &str, name: &str) -> Option<&'a Value> {
    rows.iter().find(|r| {
        r["name"] == name
            && r["commands"]
                .as_array()
                .is_some_and(|c| c.iter().any(|c| c == command))
    })
}
fn parse(arguments: &[String]) -> Result<Args, String> {
    let commands = [
        "render",
        "ast",
        "fmt",
        "diagnostics",
        "import",
        "migrate-ast",
        "lint",
    ];
    let command = arguments
        .first()
        .filter(|a| commands.contains(&a.as_str()))
        .map_or("render", String::as_str);
    let mut parsed = Args {
        command: command.into(),
        ..Default::default()
    };
    let rows = flag_rows();
    let mut at = usize::from(arguments.first().is_some_and(|a| a == command));
    while at < arguments.len() {
        match arguments[at].as_str() {
            "--" => {
                parsed.files.extend(arguments[at + 1..].iter().cloned());
                break;
            }
            "--help" | "-h" => parsed.help = true,
            "--version" | "-V" => parsed.version = true,
            "-w" if command == "fmt" => {
                parsed.values.insert("write".into(), json!(true));
            }
            arg if arg == "-" || !arg.starts_with('-') => parsed.files.push(arg.into()),
            _ => {
                at = read_flag(arguments, at, &rows, &mut parsed)?;
            }
        }
        at += 1;
    }
    if parsed.files.is_empty() {
        parsed.files.push("-".into());
    }
    if !parsed.help && !parsed.version {
        validate_args(&parsed)?;
    }
    Ok(parsed)
}
fn read_flag(
    arguments: &[String],
    at: usize,
    rows: &[Value],
    parsed: &mut Args,
) -> Result<usize, String> {
    let arg = &arguments[at];
    let (spelling, glued) = arg
        .split_once('=')
        .map_or((arg.as_str(), None), |(a, b)| (a, Some(b)));
    let name = spelling
        .strip_prefix("--")
        .ok_or_else(|| format!("unknown flag {spelling}"))?;
    let (base, negative) = name
        .strip_prefix("no-")
        .map_or((name, false), |base| (base, true));
    let row = find_flag(rows, &parsed.command, name, base)?;
    let kind = if name == "to" {
        "enum"
    } else {
        row["kind"].as_str().unwrap_or("")
    };
    if kind == "switch" {
        if glued.is_some() {
            return Err(format!("--{name} takes no value"));
        }
        parsed.values.insert(base.into(), json!(!negative));
        return Ok(at);
    }
    if negative {
        return Err(format!("unknown flag --{name} for {}", parsed.command));
    }
    let (input, next) = match glued {
        Some(input) => (input, at),
        None => (
            arguments
                .get(at + 1)
                .ok_or_else(|| format!("missing value for --{name}"))?
                .as_str(),
            at + 1,
        ),
    };
    let value = read_flag_value(kind, name, input, row)?;
    if row["repeatable"] == true {
        parsed
            .values
            .entry(base.into())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .push(value);
    } else {
        parsed.values.insert(base.into(), value);
    }
    Ok(next)
}
fn find_flag<'a>(
    rows: &'a [Value],
    command: &str,
    name: &str,
    base: &str,
) -> Result<&'a Value, String> {
    flag_row(rows, command, base)
        .or_else(|| flag_row(rows, command, name))
        .or_else(|| (command == "render" && name == "to").then_some(&Value::Null))
        .ok_or_else(|| format!("unknown flag --{name} for {command}"))
}
fn read_flag_value(kind: &str, name: &str, input: &str, row: &Value) -> Result<Value, String> {
    match kind {
        "count" => {
            if input.is_empty() || !input.bytes().all(|b| b.is_ascii_digit()) {
                return Err(format!("--{name} takes a count"));
            }
            Ok(json!(
                input
                    .parse::<usize>()
                    .map_err(|_| format!("--{name} takes a count"))?
            ))
        }
        "json" => serde_json::from_str(input).map_err(|_| format!("bad JSON for --{name}")),
        "enum" => {
            if !enum_flag_value(name, input, row) {
                return Err(format!("bad value for --{name}: {input}"));
            }
            Ok(json!(input))
        }
        _ => Ok(json!(input)),
    }
}
fn enum_flag_value(name: &str, input: &str, row: &Value) -> bool {
    if name == "to" {
        ["html", "carve", "markdown", "plain", "ansi"].contains(&input)
    } else {
        row["values"]
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value == input))
    }
}
fn validate_args(parsed: &Args) -> Result<(), String> {
    let yes = |name: &str| parsed.values.get(name) == Some(&json!(true));
    if parsed.command == "fmt"
        && ["check", "diff", "write"]
            .iter()
            .filter(|name| yes(name))
            .count()
            > 1
    {
        return Err("--check, --diff and --write cannot be combined".into());
    }
    if yes("normalize") && yes("from-ast") {
        return Err("--normalize needs source input".into());
    }
    if yes("write") && parsed.files.iter().any(|file| file == "-") {
        return Err("--write cannot replace standard input".into());
    }
    if parsed.command == "import" && !parsed.values.contains_key("from") {
        return Err("import requires --from".into());
    }
    if let (Some(a), Some(b)) = (
        str_value(parsed, "report-includes"),
        str_value(parsed, "report-losses"),
    ) {
        if a != "-" && b != "-" && absolute(a) == absolute(b) {
            return Err("two reports cannot write the same path".into());
        }
    }
    Ok(())
}

fn absolute(path: &str) -> std::path::PathBuf {
    let mut out = std::env::current_dir().unwrap_or_default();
    for part in Path::new(path).components() {
        use std::path::Component;
        match part {
            Component::RootDir => out = Path::new("/").to_path_buf(),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(p) => out.push(p),
            Component::Prefix(p) => out.push(p.as_os_str()),
        }
    }
    out
}
fn value<'a>(args: &'a Args, name: &str) -> Option<&'a Value> {
    args.values.get(name)
}
fn bool_value(args: &Args, name: &str, default: bool) -> bool {
    value(args, name)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}
fn str_value<'a>(args: &'a Args, name: &str) -> Option<&'a str> {
    value(args, name).and_then(Value::as_str)
}
fn read(path: &str) -> Result<Vec<u8>, i32> {
    let mut data = Vec::new();
    if path == "-" {
        std::io::stdin()
            .read_to_end(&mut data)
            .map_err(|e| fail("input-unreadable", e))?;
    } else {
        data = std::fs::read(path).map_err(|e| fail("input-unreadable", format!("{path}: {e}")))?;
    }
    Ok(data)
}
fn strict_text<'a>(path: &str, bytes: &'a [u8]) -> Result<&'a str, i32> {
    std::str::from_utf8(bytes).map_err(|_| {
        fail(
            "input-unreadable",
            format!("{path}: a tree in JSON is not UTF-8"),
        )
    })
}
fn output(text: &str) -> Result<(), i32> {
    std::io::stdout()
        .write_all(text.as_bytes())
        .map_err(|e| fail("output-unwritable", e))
}
fn report(path: &str, entries: &[Value]) -> Result<(), i32> {
    let content = format!("{}\n", json!({"files":entries}));
    if path == "-" {
        std::io::stderr()
            .write_all(content.as_bytes())
            .map_err(|e| fail("output-unwritable", e))
    } else {
        std::fs::write(path, content).map_err(|e| fail("output-unwritable", format!("{path}: {e}")))
    }
}
fn includes(path: &str, args: &Args, options: &mut Options) -> Result<(), i32> {
    if !bool_value(args, "includes", true) {
        return Ok(());
    }
    let root = str_value(args, "include-root")
        .map(Path::new)
        .map(std::path::Path::to_path_buf)
        .or_else(|| {
            (path != "-").then(|| {
                Path::new(path)
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or(Path::new("."))
                    .to_path_buf()
            })
        });
    let Some(root) = root else {
        return Ok(());
    };
    let root = std::fs::canonicalize(&root)
        .map_err(|e| usage(format!("the include root cannot be read: {e}")))?;
    let resolver = carve::include::fs::resolver(&root).map_err(|e| usage(format!("{e:?}")))?;
    let mut inc = carve::include::Includes::new(resolver);
    if path != "-" {
        inc.file = std::fs::canonicalize(path).ok().and_then(|p| {
            p.strip_prefix(&root)
                .ok()
                .map(|s| s.to_string_lossy().replace('\\', "/"))
        });
    }
    options.includes = Some(inc);
    Ok(())
}
fn options(args: &Args, path: &str) -> Result<Options, i32> {
    let mut result = Options::default();
    set_simple_options(args, &mut result)?;
    set_structure_options(args, &mut result)?;
    set_loss_options(args, &mut result)?;
    if matches!(
        args.command.as_str(),
        "render" | "ast" | "lint" | "diagnostics"
    ) && !bool_value(args, "from-ast", false)
    {
        includes(path, args, &mut result)?;
    }
    Ok(result)
}
fn set_simple_options(args: &Args, options: &mut Options) -> Result<(), i32> {
    options.raw_html = bool_value(args, "raw-html", options.raw_html);
    options.roundtrip = bool_value(args, "roundtrip", options.roundtrip);
    options.sections = bool_value(args, "sections", options.sections);
    options.smart_typography = bool_value(args, "smart-typography", options.smart_typography);
    options.source_lines = bool_value(args, "source-lines", options.source_lines);
    // The CLI applies the strict loss gate after the allow list.
    options.strict_losses = false;
    if let Some(name) = str_value(args, "heading-ids") {
        options.heading_ids =
            api::HeadingIds::from_name(name).ok_or_else(|| usage("bad heading ids"))?;
    }
    if let Some(name) = str_value(args, "profile") {
        options.profile = api::Profile::from_name(name).ok_or_else(|| usage("bad profile"))?;
    }
    if let Some(name) = str_value(args, "soft-break") {
        options.soft_break =
            api::SoftBreak::from_name(name).ok_or_else(|| usage("bad soft break"))?;
    }
    if let Some(name) = str_value(args, "mode") {
        options.mode = if name == "static" {
            api::Mode::Static
        } else {
            api::Mode::Interactive
        };
    }
    Ok(())
}
fn set_structure_options(args: &Args, options: &mut Options) -> Result<(), i32> {
    if let Some(value) = value(args, "labels") {
        options.labels = string_map(value)?;
    }
    if let Some(value) = value(args, "symbols") {
        options.symbols = string_map(value)?;
    }
    if let Some(value) = value(args, "social") {
        options.social = Some(api::Social {
            mention_url: value
                .get("mentionUrl")
                .and_then(Value::as_str)
                .map(str::to_owned),
            tag_url: value
                .get("tagUrl")
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
    }
    if let Some(value) = value(args, "url-policy") {
        options.url_policy = url_policy(value)?;
    }
    Ok(())
}
fn url_policy(value: &Value) -> Result<api::UrlPolicy, i32> {
    if let Some(allowed) = value.get("allow").and_then(Value::as_array) {
        return Ok(api::UrlPolicy::Allow(
            allowed
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        ));
    }
    if let Some(denied) = value.get("deny").and_then(Value::as_array) {
        return Ok(api::UrlPolicy::Deny(
            denied
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        ));
    }
    Err(usage("bad url policy"))
}
fn set_loss_options(args: &Args, options: &mut Options) -> Result<(), i32> {
    if let Some(codes) = value(args, "allow-loss").and_then(Value::as_array) {
        let rules: Value =
            serde_json::from_str(include_str!("../spec-data/lint-rules.json")).unwrap();
        for code in codes.iter().filter_map(Value::as_str) {
            if !rules["rules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|rule| rule["id"] == code && rule["channel"] == "render-loss")
            {
                return Err(usage(format!(
                    "{code} is no code of the render-loss channel"
                )));
            }
            options.allow_losses.push(code.into());
        }
    }
    if let Some(limit) = value(args, "max-render-losses").and_then(Value::as_u64) {
        options.max_losses =
            usize::try_from(limit).map_err(|_| usage("--max-render-losses is too large"))?;
    }
    Ok(())
}
fn string_map(v: &Value) -> Result<BTreeMap<String, String>, i32> {
    let Some(map) = v.as_object() else {
        return Err(usage("expected a JSON object"));
    };
    map.iter()
        .map(|(k, v)| {
            v.as_str()
                .map(|s| (k.clone(), s.into()))
                .ok_or_else(|| usage("expected text map values"))
        })
        .collect()
}
fn display(path: &str) -> &str {
    if path == "-" { "<stdin>" } else { path }
}
#[allow(clippy::naive_bytecount)] // Only line-feed bytes count as line breaks.
fn place(bytes: &[u8], offset: usize) -> (usize, usize) {
    let prefix = &bytes[..offset.min(bytes.len())];
    let start = prefix
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |p| p + 1);
    (
        prefix.iter().filter(|b| **b == b'\n').count() + 1,
        String::from_utf8_lossy(&prefix[start..]).chars().count() + 1,
    )
}
fn located(entries: &[Entry], bytes: &[u8]) -> Vec<Value> {
    entries
        .iter()
        .map(|entry| {
            let mut v = serde_json::to_value(entry).unwrap();
            if let Some(range) = entry.range {
                let (line, column) = place(bytes, range.start_offset);
                v["line"] = json!(line);
                v["column"] = json!(column);
            }
            v
        })
        .collect()
}
fn lines(path: &str, bytes: &[u8], entries: &[Entry], stderr: bool) -> Result<(), i32> {
    let mut text = String::new();
    for e in entries {
        let (line, column) = e.range.map_or((0, 0), |r| place(bytes, r.start_offset));
        let name = e.file.as_deref().unwrap_or(display(path));
        if line == 0 {
            let _ = writeln!(text, "{name}: {} {} {}", e.severity, e.code, e.message);
        } else {
            let _ = writeln!(
                text,
                "{name}:{line}:{column}: {} {} {}",
                e.severity, e.code, e.message
            );
        }
    }
    if stderr {
        std::io::stderr()
            .write_all(text.as_bytes())
            .map_err(|e| fail("output-unwritable", e))
    } else {
        output(&text)
    }
}
fn loss_report(path: &str, entries: &[Entry], bytes: &[u8], suppressed: usize) -> Value {
    json!({"file":display(path),"diagnostics":located(entries,bytes),"suppressed":suppressed})
}
fn include_report(path: &str, tree: &api::Tree) -> Value {
    let dependencies: Vec<Value> = api::include_dependencies(tree)
        .into_iter()
        .map(|d| {
            let mut row = json!({"path":d.path,"read":d.file.is_some()});
            if let Some(from) = d.from.filter(|from| from != display(path)) {
                row["from"] = json!(from);
            }
            if let Some(file) = d.file {
                row["file"] = json!(file);
            }
            row
        })
        .collect();
    json!({"file":display(path),"dependencies":dependencies})
}
fn first_change(path: &str, before: &str, after: &str) -> String {
    let old: Vec<&str> = before.split_inclusive('\n').collect();
    let new: Vec<&str> = after.split_inclusive('\n').collect();
    let at = (0..old.len().max(new.len()))
        .find(|&i| old.get(i) != new.get(i))
        .unwrap_or(0);
    let side = |prefix: &str, line: Option<&&str>| match line {
        Some(s) => format!("{prefix} {}", s.trim_end_matches(['\r', '\n'])),
        None => prefix.into(),
    };
    format!(
        "{}:{}\n{}\n{}\n",
        display(path),
        at + 1,
        side("-", old.get(at)),
        side("+", new.get(at))
    )
}
fn unified(path: &str, before: &str, after: &str) -> String {
    let old: Vec<&str> = before.trim_end_matches('\n').split('\n').collect();
    let new: Vec<&str> = after.trim_end_matches('\n').split('\n').collect();
    let mut result = format!(
        "--- a/{path}\n+++ b/{path}\n@@ -1,{} +1,{} @@\n",
        old.len(),
        new.len()
    );
    for line in old {
        let _ = writeln!(result, "-{line}");
    }
    for line in new {
        let _ = writeln!(result, "+{line}");
    }
    result
}
fn one(
    args: &Args,
    path: &str,
    bytes: &[u8],
    loss_reports: &mut Vec<Value>,
    include_reports: &mut Vec<Value>,
    diagnostics: &mut Vec<Value>,
) -> Result<i32, i32> {
    let host = Carve::new();
    let opts = options(args, path)?;
    match args.command.as_str() {
        "ast" => ast_file(args, path, bytes, &host, &opts, include_reports),
        "diagnostics" => diagnostics_file(
            args,
            path,
            bytes,
            &host,
            &opts,
            include_reports,
            diagnostics,
        ),
        "lint" => lint_file(args, path, bytes, &opts, diagnostics),
        "fmt" => fmt_file(args, path, bytes, &host),
        "import" => import_file(args, path, bytes, &host, &opts, loss_reports),
        "migrate-ast" => migrate_file(path, bytes),
        _ => render_file(
            args,
            path,
            bytes,
            &host,
            opts,
            loss_reports,
            include_reports,
        ),
    }
}

fn ast_file(
    args: &Args,
    path: &str,
    bytes: &[u8],
    host: &Carve,
    opts: &Options,
    include_reports: &mut Vec<Value>,
) -> Result<i32, i32> {
    let tree = host.parse(bytes, opts).map_err(api_fail)?;
    let positions = bool_value(args, "positions", true);
    let encoded_tree = api::encode_tree_with(&tree, api::EncodeOptions { positions });
    let encoded = if bool_value(args, "envelope", false) {
        if positions {
            api::encode_envelope(&tree)
        } else {
            let mut envelope: Value = serde_json::from_str(&api::encode_envelope(&tree)).unwrap();
            envelope["document"] = serde_json::from_str(&encoded_tree).unwrap();
            envelope.to_string()
        }
    } else {
        encoded_tree
    };
    output(&format!("{encoded}\n"))?;
    if str_value(args, "report-includes").is_some() {
        include_reports.push(include_report(path, &tree));
    }
    Ok(0)
}

fn diagnostics_file(
    args: &Args,
    path: &str,
    bytes: &[u8],
    host: &Carve,
    opts: &Options,
    include_reports: &mut Vec<Value>,
    diagnostics: &mut Vec<Value>,
) -> Result<i32, i32> {
    let parsed = host.parse_with_diagnostics(bytes, opts).map_err(api_fail)?;
    if bool_value(args, "json", false) {
        diagnostics.push(loss_report(
            path,
            &parsed.diagnostics,
            bytes,
            parsed.suppressed_diagnostics,
        ));
    } else {
        lines(path, bytes, &parsed.diagnostics, false)?;
    }
    if str_value(args, "report-includes").is_some() {
        include_reports.push(include_report(path, &parsed.tree));
    }
    Ok(0)
}

fn lint_file(
    args: &Args,
    path: &str,
    bytes: &[u8],
    opts: &Options,
    diagnostics: &mut Vec<Value>,
) -> Result<i32, i32> {
    let mut lint_opts = LintOptions {
        includes: opts.includes.clone(),
        heading_ids: opts.heading_ids,
        ..Default::default()
    };
    if let Some(map) = value(args, "rules").and_then(Value::as_object) {
        lint_opts.rules = map
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.into())))
            .collect();
    }
    if let Some(list) = value(args, "platforms").and_then(Value::as_array) {
        lint_opts.platforms = list
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
    }
    let entries = api::lint(bytes, &lint_opts).map_err(api_fail)?;
    if bool_value(args, "json", false) {
        diagnostics.push(loss_report(path, &entries, bytes, 0));
    } else {
        lines(path, bytes, &entries, false)?;
    }
    Ok(i32::from(
        entries
            .iter()
            .any(|e| e.severity == "warning" || e.severity == "error"),
    ))
}

fn fmt_file(args: &Args, path: &str, bytes: &[u8], host: &Carve) -> Result<i32, i32> {
    let (before, after) = formatted(args, path, bytes, host)?;
    fmt_output(args, path, &before, &after)
}
fn formatted(args: &Args, path: &str, bytes: &[u8], host: &Carve) -> Result<(String, String), i32> {
    let settings = WriteOptions {
        align_tables: bool_value(args, "align-tables", false),
        normalize: bool_value(args, "normalize", false),
        ..Default::default()
    };
    let before = if bool_value(args, "from-ast", false) {
        strict_text(path, bytes)?.to_string()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    let after = if bool_value(args, "from-ast", false) {
        let tree = api::decode_tree(&before).map_err(api_fail)?;
        host.write(&tree, &settings).map_err(api_fail)?
    } else {
        host.format(bytes, &settings).map_err(api_fail)?
    };
    Ok((before, after))
}
fn fmt_output(args: &Args, path: &str, before: &str, after: &str) -> Result<i32, i32> {
    let changed = before != after;
    let check = bool_value(args, "check", false);
    let diff = bool_value(args, "diff", false);
    let write = bool_value(args, "write", false);
    if !changed {
        if ![check, diff, write].contains(&true) {
            output(after)?;
        }
        return Ok(0);
    }
    if check {
        output(&first_change(path, before, after))?;
    } else if diff {
        output(&unified(display(path), before, after))?;
    } else if write {
        std::fs::write(path, after).map_err(|e| fail("output-unwritable", e))?;
    } else {
        output(after)?;
    }
    Ok(i32::from(check || diff))
}

fn import_file(
    args: &Args,
    path: &str,
    bytes: &[u8],
    host: &Carve,
    opts: &Options,
    loss_reports: &mut Vec<Value>,
) -> Result<i32, i32> {
    let input = strict_text(path, bytes)?;
    let format = str_value(args, "from")
        .and_then(api::ImportFormat::from_name)
        .ok_or_else(|| usage("bad import format"))?;
    let settings = ImportOptions {
        raw_html: bool_value(args, "raw-html", false),
        strict_losses: bool_value(args, "strict-losses", false),
    };
    let limits = LossOptions {
        allow_losses: opts.allow_losses.clone(),
        max_losses: opts.max_losses,
    };
    let imported = host
        .import_from(input, format, &settings, &limits)
        .map_err(api_fail)?;
    let retained: Vec<Entry> = imported
        .diagnostics
        .into_iter()
        .filter(|e| e.channel == "render-loss")
        .collect();
    lines(path, bytes, &retained, true)?;
    if str_value(args, "report-losses").is_some() {
        loss_reports.push(loss_report(
            path,
            &retained,
            bytes,
            imported.suppressed_diagnostics,
        ));
    }
    output(&imported.source)?;
    Ok(0)
}

fn migrate_file(path: &str, bytes: &[u8]) -> Result<i32, i32> {
    let source = strict_text(path, bytes)?;
    let value: Value = serde_json::from_str(source).map_err(|e| fail("ast-json-invalid", e))?;
    let migrated =
        api::migrate_tree_with(&value, &api::MigrateOptions::default()).map_err(api_fail)?;
    lines(path, bytes, &migrated.diagnostics, true)?;
    let encoded = migrated
        .envelope
        .unwrap_or_else(|| api::encode_tree(&migrated.tree));
    output(&format!("{encoded}\n"))?;
    Ok(0)
}

fn render_file(
    args: &Args,
    path: &str,
    bytes: &[u8],
    host: &Carve,
    opts: Options,
    loss_reports: &mut Vec<Value>,
    include_reports: &mut Vec<Value>,
) -> Result<i32, i32> {
    let (rendered, tree) = render_input(args, path, bytes, host, &opts)?;
    let parsed_tree = tree_if_needed(args, bytes, host, &opts, tree.as_ref())?;
    let resolved_tree = tree.as_ref().or(parsed_tree.as_ref());
    let retained: Vec<Entry> = rendered
        .diagnostics
        .into_iter()
        .filter(|e| e.channel == "render-loss")
        .collect();
    if str_value(args, "report-losses").is_some() {
        loss_reports.push(loss_report(
            path,
            &retained,
            bytes,
            rendered.suppressed_diagnostics,
        ));
    }
    if str_value(args, "report-includes").is_some() {
        include_reports.push(include_report(path, resolved_tree.unwrap()));
    }
    let strict = bool_value(args, "strict-losses", false) && !retained.is_empty();
    if strict {
        eprintln!(
            "carve[render-loss]: {}: {} render losses under --strict-losses",
            display(path),
            retained.len()
        );
    }
    lines(path, bytes, &retained, true)?;
    if strict {
        return Ok(1);
    }
    if let Some(target) = str_value(args, "to").filter(|s| *s != "html") {
        let out = host
            .write_to(
                target,
                resolved_tree.unwrap(),
                &WriteOptions {
                    render: opts,
                    ..Default::default()
                },
            )
            .map_err(api_fail)?;
        output(&out.output)?;
    } else {
        output(&format!("{}\n", rendered.html))?;
    }
    Ok(0)
}
fn render_input(
    args: &Args,
    path: &str,
    bytes: &[u8],
    host: &Carve,
    opts: &Options,
) -> Result<(api::Rendered, Option<api::Tree>), i32> {
    if bool_value(args, "from-ast", false) {
        let tree = api::decode_tree(strict_text(path, bytes)?).map_err(api_fail)?;
        Ok((
            api::render_tree_with_diagnostics(&tree, opts).map_err(api_fail)?,
            Some(tree),
        ))
    } else {
        Ok((
            host.render_with_diagnostics(bytes, opts)
                .map_err(api_fail)?,
            None,
        ))
    }
}
fn tree_if_needed(
    args: &Args,
    bytes: &[u8],
    host: &Carve,
    opts: &Options,
    existing: Option<&api::Tree>,
) -> Result<Option<api::Tree>, i32> {
    let needs_tree = str_value(args, "report-includes").is_some()
        || str_value(args, "to").is_some_and(|target| target != "html");
    if needs_tree && existing.is_none() {
        Ok(Some(host.parse(bytes, opts).map_err(api_fail)?))
    } else {
        Ok(None)
    }
}

pub fn run(arguments: &[String]) -> i32 {
    let args = match parse(arguments) {
        Ok(a) => a,
        Err(e) => return usage(e),
    };
    if args.version {
        println!("carve {}", api::VERSION);
        return 0;
    }
    if args.help {
        println!("Usage: carve {} [FLAG...] [FILE...]", args.command);
        return 0;
    }
    let mut status = 0;
    let mut losses = Vec::new();
    let mut includes = Vec::new();
    let mut diagnostics = Vec::new();
    for path in &args.files {
        let bytes = match read(path) {
            Ok(b) => b,
            Err(code) => return code,
        };
        match one(
            &args,
            path,
            &bytes,
            &mut losses,
            &mut includes,
            &mut diagnostics,
        ) {
            Ok(code) => status = status.max(code),
            Err(1) => status = status.max(1),
            Err(code) => return code,
        }
    }
    if bool_value(&args, "json", false) && matches!(args.command.as_str(), "lint" | "diagnostics") {
        if let Err(code) = output(&format!("{}\n", json!({"files":diagnostics}))) {
            return code;
        }
    }
    if let Some(path) = str_value(&args, "report-losses") {
        if let Err(code) = report(path, &losses) {
            return code;
        }
    }
    if let Some(path) = str_value(&args, "report-includes") {
        if let Err(code) = report(path, &includes) {
            return code;
        }
    }
    status
}
