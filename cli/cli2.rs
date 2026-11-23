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
    // Only the flags: the other sections of cli.json are skipped, not built.
    #[derive(serde::Deserialize)]
    struct Rows {
        flags: Vec<Value>,
    }
    serde_json::from_str::<Rows>(include_str!("../spec-data/cli.json"))
        .unwrap()
        .flags
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
        "fix",
        "flatten",
    ];
    let command = arguments
        .first()
        .filter(|a| commands.contains(&a.as_str()))
        .map_or("render", String::as_str);
    let mut parsed = Args {
        command: command.into(),
        ..Default::default()
    };
    let mut rows: Option<Vec<Value>> = None;
    let mut at = usize::from(arguments.first().is_some_and(|a| a == command));
    while at < arguments.len() {
        match arguments[at].as_str() {
            "--" => {
                parsed.files.extend(arguments[at + 1..].iter().cloned());
                break;
            }
            "--help" | "-h" => parsed.help = true,
            "--version" | "-V" => parsed.version = true,
            "-o" if ["render", "fmt", "import", "flatten"].contains(&command) => {
                at += 1;
                let path = arguments
                    .get(at)
                    .ok_or_else(|| "missing value for -o".to_string())?;
                parsed.values.insert("output".into(), json!(path));
            }
            "-w" if ["fmt", "lint", "fix"].contains(&command) => {
                parsed.values.insert("write".into(), json!(true));
            }
            "--directory" if command == "import" => {
                at += 1;
                let path = arguments.get(at).ok_or("missing value for --directory")?;
                parsed.values.insert("directory".into(), json!(path));
            }
            arg if command == "import" && arg.starts_with("--directory=") => {
                parsed.values.insert("directory".into(), json!(&arg[12..]));
            }
            arg if arg == "-" || !arg.starts_with('-') => parsed.files.push(arg.into()),
            _ => {
                let rows = rows.get_or_insert_with(flag_rows);
                at = read_flag(arguments, at, rows, &mut parsed)?;
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
    flag_row(rows, if command == "fix" { "lint" } else { command }, base)
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
    } else if name == "from" {
        ["html", "markdown", "gfm", "djot"].contains(&input)
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
    if parsed.command == "lint" || parsed.command == "fix" {
        let fixed = parsed.command == "fix" || yes("fix");
        if yes("write") && !fixed {
            return Err("--write needs --fix".into());
        }
        if yes("json") && fixed && !yes("write") {
            return Err("--json with --fix needs --write".into());
        }
    }
    if parsed.command == "fmt"
        && parsed.values.contains_key("output")
        && (yes("write") || yes("check") || yes("diff"))
    {
        return Err("--output cannot combine with --write, --check or --diff".into());
    }
    if let Some(out) = str_value(parsed, "output") {
        for report in ["report-includes", "report-losses"] {
            if let Some(path) = str_value(parsed, report) {
                if path != "-" && absolute(out) == absolute(path) {
                    return Err("output and report cannot share a path".into());
                }
            }
        }
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
    if parsed.command == "import" && parsed.values.contains_key("directory") {
        if str_value(parsed, "from") != Some("gfm")
            || !parsed.values.contains_key("output")
            || parsed.files != ["-"]
        {
            return Err(
                "directory import requires --from gfm --directory SOURCE --output TARGET".into(),
            );
        }
        for name in parsed.values.keys() {
            if !["from", "directory", "output", "strict-losses"].contains(&name.as_str()) {
                return Err(format!("--{name} is not supported for directory import"));
            }
        }
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
fn product(args: &Args, text: &str) -> Result<(), i32> {
    if let Some(path) = str_value(args, "output") {
        use std::fs::OpenOptions;
        OpenOptions::new()
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(text.as_bytes()))
            .map_err(|e| fail("output-unwritable", format!("{path}: {e}")))
    } else {
        output(text)
    }
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
        "render" | "ast" | "lint" | "fix" | "flatten" | "diagnostics"
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
    set_heading_shift(args, options)
}
/// C2-H-208, C2-H-209: `--heading-shift` up to the maximum of its option
/// row, refused for the plain-text and ANSI exports.
fn set_heading_shift(args: &Args, options: &mut Options) -> Result<(), i32> {
    let Some(shift) = value(args, "heading-shift").and_then(Value::as_u64) else {
        return Ok(());
    };
    if let Some(target) = str_value(args, "to").filter(|t| matches!(*t, "plain" | "ansi")) {
        return Err(usage(format!(
            "--heading-shift does not apply to --to {target}"
        )));
    }
    let rows: Value = serde_json::from_str(include_str!("../spec-data/options.json")).unwrap();
    let max = rows["options"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["name"] == "headingShift"))
        .and_then(|row| row["maximum"].as_u64())
        .unwrap_or(0);
    options.heading_shift = u8::try_from(shift)
        .ok()
        .filter(|n| u64::from(*n) <= max)
        .ok_or_else(|| usage(format!("--heading-shift takes a count from 0 to {max}")))?;
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
const POSITION_STEP: usize = 256;

struct Positions<'a> {
    bytes: &'a [u8],
    line_starts: Vec<usize>,
    marks: Vec<(usize, usize)>,
}

impl<'a> Positions<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let mut line_starts = vec![0];
        for (at, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                line_starts.push(at + 1);
            }
        }
        if bytes.is_ascii() {
            return Self {
                bytes,
                line_starts,
                marks: Vec::new(),
            };
        }
        let mut marks = vec![(0, 0)];
        let mut at = 0;
        let mut count = 0;
        for chunk in bytes.utf8_chunks() {
            for ch in chunk.valid().chars() {
                at += ch.len_utf8();
                count += 1;
                while marks.len() * POSITION_STEP <= at {
                    marks.push((at, count));
                }
            }
            if !chunk.invalid().is_empty() {
                at += chunk.invalid().len();
                count += 1;
                while marks.len() * POSITION_STEP <= at {
                    marks.push((at, count));
                }
            }
        }
        Self {
            bytes,
            line_starts,
            marks,
        }
    }

    fn chars_before(&self, offset: usize) -> usize {
        if self.marks.is_empty() {
            return offset;
        }
        let mark = self.marks.partition_point(|(at, _)| *at <= offset) - 1;
        let (at, count) = self.marks[mark];
        count
            + String::from_utf8_lossy(&self.bytes[at..offset])
                .chars()
                .count()
    }

    fn place(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.bytes.len());
        let line = self.line_starts.partition_point(|start| *start <= offset);
        let start = self.line_starts[line - 1];
        (
            line,
            self.chars_before(offset) - self.chars_before(start) + 1,
        )
    }
}
fn located(entries: &[Entry], bytes: &[u8]) -> Vec<Value> {
    if entries.is_empty() {
        return Vec::new();
    }
    let positions = Positions::new(bytes);
    entries
        .iter()
        .map(|entry| {
            let mut v = serde_json::to_value(entry).unwrap();
            if let Some(range) = entry.range {
                let (line, column) = positions.place(range.start_offset);
                v["line"] = json!(line);
                v["column"] = json!(column);
            }
            v
        })
        .collect()
}
fn lines(path: &str, bytes: &[u8], entries: &[Entry], stderr: bool) -> Result<(), i32> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut text = String::new();
    let positions = Positions::new(bytes);
    for e in entries {
        let (line, column) = e.range.map_or((0, 0), |r| positions.place(r.start_offset));
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
/// C2-T-010: the standard extensions `--extensions` names, with the renderer
/// options `--extension-options` gives them, checked against
/// `data/extensions.json`; every refusal is a usage error.
struct StandardExtensions {
    names: Vec<String>,
    host: Carve,
}
impl StandardExtensions {
    fn from_args(args: &Args) -> Result<Self, i32> {
        let rows: Value = serde_json::from_str(include_str!("../spec-data/extensions.json"))
            .unwrap_or(Value::Null);
        let rows = rows["extensions"].as_array().cloned().unwrap_or_default();
        let row = |name: &str| rows.iter().find(|r| r["name"] == name);
        let mut names = Vec::new();
        if let Some(list) = value(args, "extensions") {
            let list = list.as_array().ok_or_else(|| {
                usage("--extensions takes a JSON list of standard extension names")
            })?;
            for item in list {
                let name = item.as_str().ok_or_else(|| {
                    usage("--extensions takes a JSON list of standard extension names")
                })?;
                if row(name).is_none() {
                    return Err(usage(format!("{name} is not a standard extension")));
                }
                if !names.iter().any(|n| n == name) {
                    names.push(name.to_string());
                }
            }
        }
        let empty = serde_json::Map::new();
        let options = match value(args, "extension-options") {
            Some(v) => v
                .as_object()
                .ok_or_else(|| usage("--extension-options takes a JSON object"))?,
            None => &empty,
        };
        for (name, settings) in options {
            let Some(declared) = row(name) else {
                return Err(usage(format!("{name} is not a standard extension")));
            };
            // The text leaves open options for an extension that is not enabled.
            if !names.iter().any(|n| n == name) {
                return Err(usage(format!("{name} is not among --extensions")));
            }
            let settings = settings
                .as_object()
                .ok_or_else(|| usage(format!("the options of {name} are not a JSON object")))?;
            for (option, setting) in settings {
                let values = &declared["rendererOptions"][option]["values"];
                if values.is_null() {
                    return Err(usage(format!("{name} has no renderer option {option}")));
                }
                if !values.as_array().is_some_and(|v| v.contains(setting)) {
                    return Err(usage(format!("bad value for the {name} option {option}")));
                }
            }
        }
        let render = args.command == "render";
        let mut host = Carve::new();
        for name in &names {
            // `carve.tab` has no matcher: the matcher of its parent claims it.
            if row(name).is_some_and(|r| r.get("matcher").is_none()) {
                continue;
            }
            let mode = options
                .get(name)
                .and_then(|o| o.get("mode"))
                .and_then(Value::as_str);
            let enabled = match (name.as_str(), mode) {
                ("carve.tabs", Some(mode)) if render => host.enable_standard_tabs_renderer(mode),
                // An extension with no standard renderer (C2-X-058) keeps its fallback.
                _ if render => host
                    .enable_standard(name, true)
                    .or_else(|_| host.enable_standard(name, false)),
                _ => host.enable_standard(name, false),
            };
            enabled.map_err(|e| usage(format!("{name}: {e}")))?;
        }
        Ok(Self { names, host })
    }
    /// The names the Carve writer models claims for (`writer.md`).
    fn writer(&self) -> Vec<String> {
        self.names
            .iter()
            .filter(|n| matches!(n.as_str(), "carve.tabs" | "carve.tab"))
            .cloned()
            .collect()
    }
}
fn one(
    args: &Args,
    standard: &StandardExtensions,
    path: &str,
    bytes: &[u8],
    loss_reports: &mut Vec<Value>,
    include_reports: &mut Vec<Value>,
    diagnostics: &mut Vec<Value>,
) -> Result<i32, i32> {
    let host = &standard.host;
    let opts = options(args, path)?;
    match args.command.as_str() {
        "ast" => ast_file(args, path, bytes, host, &opts, include_reports),
        "diagnostics" => {
            diagnostics_file(args, path, bytes, host, &opts, include_reports, diagnostics)
        }
        "lint" | "fix" => lint_file(args, standard, path, bytes, &opts, diagnostics),
        "flatten" => flatten_file(args, path, bytes, host, &opts),
        "fmt" => fmt_file(args, standard, path, bytes),
        "import" => import_file(args, path, bytes, host, &opts, loss_reports),
        "migrate-ast" => migrate_file(path, bytes),
        _ => render_file(args, path, bytes, host, opts, loss_reports, include_reports),
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

fn fix_fragment(code: &str, text: &str) -> Option<String> {
    let paired = |open: &str, close: &str, left: &str, right: &str| {
        let start = text.find(open)?;
        let end = text.rfind(close)?;
        (end >= start + open.len()).then(|| {
            format!(
                "{}{}{}{}{}",
                &text[..start],
                left,
                &text[start + open.len()..end],
                right,
                &text[end + close.len()..]
            )
        })
    };
    match code {
        "markdown-strong-double-star" => paired("**", "**", "*", "*"),
        "markdown-strikethrough-double-tilde" => paired("~~", "~~", "{~", "~}"),
        "deprecated-bare-strike" => paired("~", "~", "{~", "~}"),
        "djot-subscript-tilde" => paired("~", "~", "{,", ",}"),
        "deprecated-bare-highlight" => paired("=", "=", "{=", "=}"),
        "djot-emphasis-underscore" => paired("_", "_", "/", "/"),
        "djot-intraword-underscore" => paired("_", "_", "{/", "/}"),
        // C2-T-049 permits the edit only if its replacement parses as the
        // construct it promises. Braces inside a pair need this check.
        "djot-superscript-caret" => paired("^", "^", "{^", "^}").filter(|replacement| {
            let tree = carve::tree_json(replacement, &carve::Options::default());
            serde_json::from_str::<Value>(&tree)
                .ok()
                .is_some_and(|tree| tree["children"][0]["children"][0]["type"] == "superscript")
        }),
        "heading-closing-hashes" => Some(String::new()),
        "thematic-break-spelling" => Some("***".into()),
        "djot-plus-bullet" => text
            .find('+')
            .map(|at| format!("{}-{}", &text[..at], &text[at + 1..])),
        _ => None,
    }
}

fn apply_fixes(bytes: &[u8], options: &LintOptions) -> Result<Vec<u8>, i32> {
    const MAX_PASSES: usize = 10;
    let mut source = String::from_utf8_lossy(bytes).into_owned();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..MAX_PASSES {
        if !seen.insert(source.clone()) {
            return Ok(source.into_bytes());
        }
        let entries = api::lint(source.as_bytes(), options).map_err(api_fail)?;
        let mut candidates = BTreeMap::<(usize, usize), Vec<String>>::new();
        for entry in entries {
            let Some(range) = entry.range else {
                continue;
            };
            let Some(slice) = source.get(range.start_offset..range.end_offset) else {
                continue;
            };
            if let Some(replacement) = fix_fragment(&entry.code, slice) {
                if replacement != slice {
                    let start = if entry.code == "heading-closing-hashes"
                        && range.start_offset > 0
                        && source.as_bytes()[range.start_offset - 1] == b' '
                    {
                        range.start_offset - 1
                    } else {
                        range.start_offset
                    };
                    candidates
                        .entry((start, range.end_offset))
                        .or_default()
                        .push(replacement);
                }
            }
        }
        let mut changes: Vec<_> = candidates
            .into_iter()
            .filter_map(|(range, replacements)| {
                (replacements.iter().all(|r| r == &replacements[0]))
                    .then(|| (range, replacements[0].clone()))
            })
            .collect();
        changes.sort_by_key(|(range, _)| std::cmp::Reverse(range.0));
        let mut boundary = source.len();
        let mut applied = false;
        for ((start, end), replacement) in changes {
            if end > boundary {
                continue;
            }
            source.replace_range(start..end, &replacement);
            boundary = start;
            applied = true;
        }
        if !applied {
            return Ok(source.into_bytes());
        }
    }
    Ok(source.into_bytes())
}

fn leading_frontmatter(node: &Value) -> bool {
    node.get("type").and_then(Value::as_str) == Some("frontmatter")
        && node.pointer("/pos/startLine").and_then(Value::as_u64) == Some(1)
}

fn expand_include_nodes(value: &mut Value) {
    match value {
        Value::Array(items) => {
            let mut expanded = Vec::new();
            for mut item in std::mem::take(items) {
                if item.get("type").and_then(Value::as_str) == Some("include") {
                    if let Some(children) = item
                        .get_mut("children")
                        .and_then(Value::as_array_mut)
                        .map(std::mem::take)
                    {
                        let mut children = children;
                        // C2-T-047: the included file's leading frontmatter is not written.
                        if children.first().is_some_and(leading_frontmatter) {
                            children.remove(0);
                        }
                        let mut nested = Value::Array(children);
                        expand_include_nodes(&mut nested);
                        expanded.extend(
                            nested
                                .as_array_mut()
                                .map(std::mem::take)
                                .unwrap_or_default(),
                        );
                    }
                } else {
                    expand_include_nodes(&mut item);
                    expanded.push(item);
                }
            }
            *items = expanded;
        }
        Value::Object(map) => {
            for child in map.values_mut() {
                expand_include_nodes(child);
            }
        }
        _ => {}
    }
}

fn flatten_file(
    args: &Args,
    path: &str,
    bytes: &[u8],
    host: &Carve,
    opts: &Options,
) -> Result<i32, i32> {
    let parsed = host.parse_with_diagnostics(bytes, opts).map_err(api_fail)?;
    let causes: Vec<_> = parsed
        .diagnostics
        .iter()
        .filter(|e| e.code.starts_with("include-"))
        .cloned()
        .collect();
    if !causes.is_empty() {
        eprintln!("carve[include-unresolved]: {path}: an include did not expand");
        lines(path, bytes, &causes, true)?;
        return Ok(1);
    }
    let mut expanded: Value = serde_json::from_str(&api::encode_tree(&parsed.tree))
        .map_err(|e| fail("ast-json-invalid", e))?;
    expand_include_nodes(&mut expanded);
    let flat_tree = api::decode_tree(&expanded.to_string()).map_err(api_fail)?;
    let flattened = host
        .write(&flat_tree, &WriteOptions::default())
        .map_err(api_fail)?;
    let expanded_html = host
        .render_tree(&parsed.tree, &Options::default())
        .map_err(api_fail)?;
    let flat_html = host
        .render(flattened.as_str(), &Options::default())
        .map_err(api_fail)?;
    if expanded_html != flat_html {
        eprintln!("carve[flatten-changed]: {path}: flattened HTML differs");
        return Ok(1);
    }
    product(args, &flattened)?;
    Ok(0)
}

fn lint_file(
    args: &Args,
    standard: &StandardExtensions,
    path: &str,
    bytes: &[u8],
    opts: &Options,
    diagnostics: &mut Vec<Value>,
) -> Result<i32, i32> {
    let mut lint_opts = LintOptions {
        includes: opts.includes.clone(),
        heading_ids: opts.heading_ids,
        raw_html: opts.raw_html,
        extensions: standard.names.clone(),
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
    lint_opts.upgrade = bool_value(args, "upgrade", false);
    let fixed = args.command == "fix" || bool_value(args, "fix", false);
    if fixed {
        for rule in serde_json::from_str::<Value>(include_str!("../spec-data/lint-rules.json"))
            .unwrap()["rules"]
            .as_array()
            .unwrap()
        {
            if rule.get("fix").is_some()
                && !lint_opts
                    .rules
                    .contains_key(rule["id"].as_str().unwrap_or(""))
            {
                lint_opts
                    .rules
                    .insert(rule["id"].as_str().unwrap().into(), "on".into());
            }
        }
    }
    let source = if fixed {
        apply_fixes(bytes, &lint_opts)?
    } else {
        bytes.to_vec()
    };
    let entries: Vec<Entry> = api::lint(source.as_slice(), &lint_opts)
        .map_err(api_fail)?
        .into_iter()
        .filter(|e| {
            !value(args, "ignore")
                .and_then(Value::as_array)
                .is_some_and(|files| {
                    files
                        .iter()
                        .any(|f| f.as_str() == Some(path) || f.as_str() == e.file.as_deref())
                })
        })
        .collect();
    if fixed {
        if bool_value(args, "write", false) {
            if source != bytes {
                std::fs::write(path, &source).map_err(|e| fail("output-unwritable", e))?;
            }
        } else {
            output(&String::from_utf8_lossy(&source))?;
        }
    }
    if bool_value(args, "json", false) {
        diagnostics.push(loss_report(path, &entries, &source, 0));
    } else {
        lines(path, &source, &entries, fixed)?;
    }
    Ok(i32::from(entries.iter().any(|e| {
        bool_value(args, "strict", false) || e.severity == "warning" || e.severity == "error"
    })))
}

fn fmt_file(
    args: &Args,
    standard: &StandardExtensions,
    path: &str,
    bytes: &[u8],
) -> Result<i32, i32> {
    let (before, after) = formatted(args, path, bytes, &standard.host, &standard.writer())?;
    fmt_output(args, path, &before, &after)
}
fn formatted(
    args: &Args,
    path: &str,
    bytes: &[u8],
    host: &Carve,
    extensions: &[String],
) -> Result<(String, String), i32> {
    let settings = WriteOptions {
        align_tables: bool_value(args, "align-tables", false),
        normalize: bool_value(args, "normalize", false),
        extensions: extensions.to_vec(),
        ..Default::default()
    };
    let before = if bool_value(args, "from-ast", false) {
        strict_text(path, bytes)?.to_string()
    } else {
        std::str::from_utf8(bytes).map_or_else(
            |_| String::from_utf8_lossy(bytes).into_owned(),
            str::to_string,
        )
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
            product(args, after)?;
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
        product(args, after)?;
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
        adapter: str_value(args, "adapter")
            .and_then(api::ImportAdapter::from_name)
            .unwrap_or_default(),
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
    product(args, &imported.source)?;
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
    let strict_entry = bool_value(args, "strict", false) && !rendered.diagnostics.is_empty();
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
        product(args, &out.output)?;
    } else {
        product(args, &format!("{}\n", rendered.html))?;
    }
    Ok(i32::from(strict_entry))
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
            host.render_tree_with_diagnostics(&tree, opts)
                .map_err(api_fail)?,
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

/// The usage line and the flags `data/cli.json` gives the command.
fn help(command: &str) -> String {
    let mut out = format!("Usage: carve {command} [FLAG...] [FILE...]\n");
    let command = if command == "fix" { "lint" } else { command };
    for row in flag_rows() {
        if flag_row(
            std::slice::from_ref(&row),
            command,
            row["name"].as_str().unwrap_or(""),
        )
        .is_none()
        {
            continue;
        }
        let kind = match row["values"].as_array() {
            Some(values) => values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("|"),
            None => row["kind"].as_str().unwrap_or("").to_string(),
        };
        let _ = writeln!(out, "  --{} {kind}", row["name"].as_str().unwrap_or(""));
    }
    if command == "render" {
        out.push_str("  --to html|carve|markdown|plain|ansi\n");
    }
    out
}

#[allow(clippy::cognitive_complexity)]
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
        print!("{}", help(&args.command));
        return 0;
    }
    if let Some(source) = str_value(&args, "directory") {
        return match crate::migrate_directory::run(
            source,
            str_value(&args, "output").unwrap_or(""),
            bool_value(&args, "strict-losses", false),
        ) {
            Ok(status) => i32::from(status),
            Err(error) => fail("import-input-invalid", error),
        };
    }
    // Before `-o` truncates its file.
    let standard = match StandardExtensions::from_args(&args) {
        Ok(standard) => standard,
        Err(code) => return code,
    };
    let mut staged = vec![None; args.files.len()];
    if let Some(path) = str_value(&args, "output") {
        for (index, input) in args.files.iter().enumerate() {
            if input != "-" && absolute(input) == absolute(path) {
                staged[index] = Some(match read(input) {
                    Ok(bytes) => bytes,
                    Err(code) => return code,
                });
            }
        }
        if let Err(e) = std::fs::write(path, "") {
            return fail("output-unwritable", e);
        }
    }
    let mut status = 0;
    let mut losses = Vec::new();
    let mut includes = Vec::new();
    let mut diagnostics = Vec::new();
    for (path, staged_bytes) in args.files.iter().zip(staged) {
        let bytes = match staged_bytes {
            Some(bytes) => bytes,
            None => match read(path) {
                Ok(bytes) => bytes,
                Err(code) => return code,
            },
        };
        match one(
            &args,
            &standard,
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
    if bool_value(&args, "json", false)
        && matches!(args.command.as_str(), "lint" | "fix" | "diagnostics")
    {
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

pub fn region_command(args: &[String]) -> i32 {
    let mut code = false;
    let mut operands = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--code" => code = true,
            "--help" | "-h" => {
                println!(
                    "Usage: carve region [--code] NAME [file]\nPrint the region's 1-based inclusive content lines as JSON."
                );
                return 0;
            }
            flag if flag.starts_with('-') && flag != "-" => {
                eprintln!("carve region: unknown option: {flag}");
                return 2;
            }
            _ => operands.push(arg.as_str()),
        }
    }
    if operands.is_empty() || operands.len() > 2 {
        eprintln!("carve region: expected NAME [file]");
        return 2;
    }
    let name = operands[0];
    let file = operands.get(1).copied().unwrap_or("-");
    let bytes = match read(file) {
        Ok(bytes) => bytes,
        Err(code) => return code,
    };
    let source = String::from_utf8_lossy(&bytes);
    if let Ok((start, end)) = carve::include::region_lines(&source, name, code) {
        output(&format!("{}\n", json!({"start":start,"end":end}))).map_or_else(|code| code, |()| 0)
    } else {
        eprintln!("carve region: include-region-invalid");
        1
    }
}

#[cfg(test)]
mod position_tests {
    use super::Positions;

    #[test]
    fn dense_positions_on_one_line_and_many_lines() {
        for source in ["_a_ ".repeat(32_768), "_a_\n".repeat(32_768)] {
            let bytes = source.as_bytes();
            let positions = Positions::new(bytes);
            assert!(positions.marks.is_empty());
            for (number, offset) in (0..bytes.len()).step_by(4).enumerate() {
                let expected = if bytes[offset + 3] == b'\n' {
                    (number + 1, 1)
                } else {
                    (1, offset + 1)
                };
                assert_eq!(positions.place(offset), expected);
            }
        }
    }

    #[test]
    fn positions_count_code_points_and_replacement_characters() {
        let bytes = b"a\xc3\xa9\xff\n\xce\xb2";
        let positions = Positions::new(bytes);
        assert_eq!(positions.place(0), (1, 1));
        assert_eq!(positions.place(2), (1, 3));
        assert_eq!(positions.place(3), (1, 3));
        assert_eq!(positions.place(4), (1, 4));
        assert_eq!(positions.place(5), (2, 1));
        assert_eq!(positions.place(7), (2, 2));
    }

    #[test]
    fn sparse_character_marks_do_not_grow_per_byte() {
        let source = "é".repeat(65_536);
        let positions = Positions::new(source.as_bytes());
        assert!(positions.marks.len() < source.len() / 128);
        assert_eq!(positions.place(source.len()), (1, 65_537));
    }
}
