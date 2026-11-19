//! `carve`: reads Carve source on stdin, writes HTML, or the tree with `--tree`,
//! or another output format with `render --to markdown|plain|ansi|carve`.

use std::io::{Read, Write};

mod cli2;
mod lint_cli;

/// The entry file and the include settings of the command line (C2-N-004).
#[derive(Default)]
struct Entry {
    file: Option<String>,
    root: Option<String>,
    no_includes: bool,
    /// The output format of `--to`; HTML when absent.
    to: Option<String>,
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let legacy = matches!(
        arguments.first().map(String::as_str),
        Some("call" | "--batch")
    ) || arguments.first().map(String::as_str) == Some("render")
        && arguments.get(1).map(String::as_str) == Some("--batch");
    if !legacy {
        std::process::exit(cli2::run(&arguments));
    }
    if arguments.first().map(String::as_str) == Some("lint") {
        std::process::exit(lint_cli::run(&arguments[1..]));
    }
    if arguments.first().map(String::as_str) == Some("call") {
        std::process::exit(call());
    }
    let mut opts = carve::Options::default();
    let mut entry = Entry::default();
    let Some(tree) = parse_args(&mut opts, &mut entry) else {
        return;
    };
    let bytes = if let Some(f) = &entry.file {
        std::fs::read(f).unwrap_or_else(|_| {
            eprintln!("carve: cannot read the entry file");
            std::process::exit(1)
        })
    } else {
        let mut bytes = Vec::new();
        if std::io::stdin().read_to_end(&mut bytes).is_err() {
            std::process::exit(1);
        }
        bytes
    };
    includes(&entry, &mut opts);
    let text = std::str::from_utf8(&bytes)
        .ok()
        .filter(|_| opts.includes.is_some());
    let out = if let Some(target) = entry.to.as_deref().filter(|t| *t != "html") {
        write_to(target, &bytes, &opts)
    } else if let (true, Some(text)) = (tree, text) {
        carve::tree_json(text, &opts)
    } else if tree {
        match carve::try_parse_resolved_bytes_in(&bytes, &opts, &carve::Arena::new()) {
            Ok(doc) => carve::to_json(&doc),
            Err(e) => {
                eprintln!("carve: {e}");
                std::process::exit(1);
            }
        }
    } else {
        match carve::try_render_bytes(&bytes, &opts) {
            Ok(html) => html,
            Err(e) => {
                eprintln!("carve: {e}");
                std::process::exit(1);
            }
        }
    };
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.write_all(b"\n");
}

/// `render --to TARGET`: the tree written by the writer of TARGET.
fn write_to(target: &str, bytes: &[u8], opts: &carve::Options) -> String {
    let host = carve::api::Carve::new();
    let written = carve::api::parse(bytes, opts).and_then(|tree| {
        let write = carve::api::WriteOptions {
            render: opts.clone(),
            ..Default::default()
        };
        host.write_to(target, &tree, &write)
    });
    match written {
        Ok(w) => w.output.strip_suffix('\n').unwrap_or(&w.output).to_string(),
        Err(e) => {
            eprintln!("carve: {e}");
            std::process::exit(e.status())
        }
    }
}

/// `carve call`: one JSON request of `carve::api::json` on standard
/// input, its response on standard output; the status of its error.
fn call() -> i32 {
    let mut request = String::new();
    if std::io::stdin().read_to_string(&mut request).is_err() {
        return 2;
    }
    let response = carve::api::json::call(&request);
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(response.as_bytes());
    let _ = stdout.write_all(b"\n");
    carve::api::json::status(&response)
}

/// C2-N-004, C2-N-010, C2-N-014: an entry file expands against its own
/// directory unless another root is named; standard input expands only
/// against a named root.
fn includes(entry: &Entry, opts: &mut carve::Options) {
    if entry.no_includes {
        return;
    }
    let file = entry
        .file
        .as_deref()
        .and_then(|f| std::fs::canonicalize(f).ok());
    let root = match (&entry.root, &file) {
        (Some(r), _) => std::fs::canonicalize(r).ok(),
        (None, Some(f)) => f.parent().map(std::path::Path::to_path_buf),
        (None, None) => None,
    };
    let Some(root) = root else { return };
    let Ok(resolver) = carve::include::fs::resolver(&root) else {
        eprintln!("carve: the include root is refused");
        std::process::exit(2)
    };
    let mut inc = carve::include::Includes::new(resolver);
    inc.file = file
        .as_deref()
        .and_then(|f| f.strip_prefix(&root).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"));
    opts.includes = Some(inc);
}

fn parse_args(opts: &mut carve::Options, entry: &mut Entry) -> Option<bool> {
    let mut tree = false;
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("render") {
        args.remove(0);
    }
    let args = entry_args(args, entry);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--tree" => tree = true,
            "--batch" => {
                batch(it.next(), it.next());
                return None;
            }
            "--roundtrip" => opts.roundtrip = true,
            "--raw-html" => opts.raw_html = true,
            "--no-sections" => opts.sections = false,
            "--no-smart-typography" => opts.smart_typography = false,
            "--source-lines" => opts.source_lines = true,
            "--strict-losses" => opts.strict_losses = true,
            "--mention-url" | "--tag-url" | "--heading-ids" | "--profile" => {
                option_value(opts, a, it.next());
            }
            _ => {
                eprintln!(
                    "{}",
                    concat!(
                        "usage: carve [render] [--to html|carve|markdown|plain|ansi] [--tree] [--roundtrip] [--raw-html] [--no-sections] ",
                        "[--no-smart-typography] [--source-lines] [--strict-losses] ",
                        "[--mention-url URL] [--tag-url URL] [--heading-ids off|keep-case|lower|ascii] ",
                        "[--profile full|article|comment|minimal] [--include-root DIR] [--no-includes] ",
                        "[FILE | < input]"
                    )
                );
                std::process::exit(2);
            }
        }
    }
    Some(tree)
}

/// Takes the entry file and the include flags out of the arguments.
fn entry_args(args: Vec<String>, entry: &mut Entry) -> Vec<String> {
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--no-includes" => entry.no_includes = true,
            "--to" => entry.to = it.next(),
            "--include-root" => entry.root = it.next(),
            "--batch" => {
                rest.push(a);
                rest.extend(it.by_ref());
            }
            f if !f.starts_with("--")
                && entry.file.is_none()
                && !rest.last().is_some_and(|p| takes_value(p)) =>
            {
                entry.file = Some(a);
            }
            _ => rest.push(a),
        }
    }
    rest
}

fn takes_value(flag: &str) -> bool {
    matches!(
        flag,
        "--mention-url" | "--tag-url" | "--heading-ids" | "--profile"
    )
}

fn option_value(opts: &mut carve::Options, name: &str, value: Option<&String>) {
    match name {
        "--mention-url" | "--tag-url" => {
            let social = opts.social.get_or_insert_with(Default::default);
            let v = value.cloned();
            if name == "--mention-url" {
                social.mention_url = v;
            } else {
                social.tag_url = v;
            }
        }
        "--heading-ids" => {
            if let Some(v) = value.and_then(|v| carve::options::HeadingIds::from_name(v)) {
                opts.heading_ids = v;
            }
        }
        "--profile" => {
            if let Some(v) = value.and_then(|v| carve::options::Profile::from_name(v)) {
                opts.profile = v;
            }
        }
        _ => {}
    }
}

/// `--batch IN OUT`: IN is a JSON list of `{"input", "opts"}`, OUT gets a
/// list of `{"html", "tree", "ns"}`, for the differential runs.
fn batch(input: Option<&String>, output: Option<&String>) {
    let (Some(input), Some(output)) = (input, output) else {
        std::process::exit(2)
    };
    let text = std::fs::read_to_string(input).unwrap_or_default();
    let cases: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    let mut out = Vec::new();
    for c in cases.as_array().into_iter().flatten() {
        let src = c
            .get("input")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let opts = options_from_json(c.get("opts"));
        let t0 = std::time::Instant::now();
        let html = carve::try_render(src, &opts);
        let ns = t0.elapsed().as_nanos();
        let tree = carve::parse_resolved_in(src, &opts, &carve::Arena::new())
            .map_or_else(|e| e.to_string(), |doc| carve::to_json(&doc));
        let lint: Vec<_> = carve::lint::lint(src, &carve::lint::LintOptions::default())
            .into_iter()
            .map(|finding| {
                serde_json::json!({
                    "code": finding.code,
                    "severity": finding.severity,
                    "start": finding.pos.map(|pos| pos.start_offset),
                    "end": finding.pos.map(|pos| pos.end_offset),
                    "message": finding.message,
                })
            })
            .collect();
        let ns = u64::try_from(ns).unwrap_or(u64::MAX);
        out.push(match html {
            Ok(html) => serde_json::json!({ "html": html, "tree": tree, "lint": lint, "ns": ns }),
            Err(e) => serde_json::json!({ "htmlError": e.to_string(), "tree": tree, "lint": lint, "ns": ns }),
        });
    }
    let _ = std::fs::write(output, serde_json::Value::Array(out).to_string());
}

/// Render options in the oracle's camelCase spelling.
fn options_from_json(v: Option<&serde_json::Value>) -> carve::Options {
    let mut o = carve::Options::default();
    let Some(v) = v else { return o };
    let flag = |k: &str, d: bool| v.get(k).and_then(serde_json::Value::as_bool).unwrap_or(d);
    o.roundtrip = flag("roundtrip", o.roundtrip);
    o.raw_html = flag("rawHtml", o.raw_html);
    o.sections = flag("sections", o.sections);
    o.smart_typography = flag("smartTypography", o.smart_typography);
    o.source_lines = flag("sourceLines", o.source_lines);
    o.strict_losses = flag("strictLosses", o.strict_losses);
    if let Some(s) = v.get("social") {
        let url = |k: &str| {
            s.get(k)
                .and_then(serde_json::Value::as_str)
                .map(String::from)
        };
        o.social = Some(carve::options::Social {
            mention_url: url("mentionUrl"),
            tag_url: url("tagUrl"),
        });
    }
    if let Some(h) = v
        .get("headingIds")
        .and_then(serde_json::Value::as_str)
        .and_then(carve::options::HeadingIds::from_name)
    {
        o.heading_ids = h;
    }
    if let Some(p) = v
        .get("profile")
        .and_then(serde_json::Value::as_str)
        .and_then(carve::options::Profile::from_name)
    {
        o.profile = p;
    }
    if v.get("mode").and_then(serde_json::Value::as_str) == Some("static") {
        o.mode = carve::options::Mode::Static;
    }
    for (key, map) in [("labels", &mut o.labels), ("symbols", &mut o.symbols)] {
        if let Some(m) = v.get(key).and_then(serde_json::Value::as_object) {
            for (k, val) in m {
                if let Some(t) = val.as_str() {
                    map.insert(k.clone(), t.to_string());
                }
            }
        }
    }
    if let Some(p) = v.get("urlPolicy") {
        let list = |k: &str| {
            p.get(k).and_then(serde_json::Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
        };
        if let Some(a) = list("allow") {
            o.url_policy = carve::options::UrlPolicy::Allow(a);
        } else if let Some(d) = list("deny") {
            o.url_policy = carve::options::UrlPolicy::Deny(d);
        }
    }
    o
}
