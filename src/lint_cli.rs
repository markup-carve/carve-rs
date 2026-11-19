//! C2-W-109 lint command and exit codes.

use carve::diagnostic::Diagnostic;
use carve::lint::{LintOptions, lint};
use serde_json::{Value, json};
use std::io::Read;

struct Cli {
    options: LintOptions,
    paths: Vec<String>,
    ignored: Vec<String>,
    json_output: bool,
    include_root: Option<String>,
}

pub(crate) fn run(args: &[String]) -> i32 {
    let cli = match parse_args(args) {
        Ok(Some(cli)) => cli,
        Ok(None) => return 0,
        Err(code) => return code,
    };
    run_files(cli)
}

fn parse_args(args: &[String]) -> Result<Option<Cli>, i32> {
    let mut cli = Cli {
        options: LintOptions::default(),
        paths: Vec::new(),
        ignored: Vec::new(),
        json_output: false,
        include_root: None,
    };
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if let Some(done) = simple_arg(arg, &mut cli) {
            if done {
                return Ok(None);
            }
            index += 1;
            continue;
        }
        if let Some(consumed) = value_arg(args, index, &mut cli)? {
            index += consumed;
            continue;
        }
        if arg.starts_with('-') && arg != "-" {
            return Err(option_error(&format!("unknown option: {arg}")));
        }
        cli.paths.push(arg.clone());
        index += 1;
    }
    if cli.paths.is_empty() {
        cli.paths.push("-".into());
    }
    Ok(Some(cli))
}

fn simple_arg(arg: &str, cli: &mut Cli) -> Option<bool> {
    match arg {
        "--json" => cli.json_output = true,
        "--caption-hints" => enable(&mut cli.options, "caption-looking-paragraph"),
        "--from-djot" => {
            for code in DJOT_RULES {
                enable(&mut cli.options, code);
            }
        }
        "--portable" => {}
        "-h" | "--help" => {
            print_help();
            return Some(true);
        }
        "-V" | "--version" => {
            println!("carve {}", env!("CARGO_PKG_VERSION"));
            return Some(true);
        }
        flag if flag.starts_with("--include-root=") => {
            cli.include_root = Some(flag[15..].to_string());
        }
        _ => return None,
    }
    Some(false)
}

fn value_arg(args: &[String], index: usize, cli: &mut Cli) -> Result<Option<usize>, i32> {
    let Some(flag) = args.get(index).map(String::as_str) else {
        return Ok(None);
    };
    if !matches!(
        flag,
        "--rule" | "--ignore" | "--platform" | "--extension" | "--include-root"
    ) {
        return Ok(None);
    }
    let Some(value) = args.get(index + 1) else {
        return Err(option_error("option needs a value"));
    };
    match flag {
        "--rule" => {
            if !known(value) {
                return Err(option_error("unknown rule"));
            }
            enable(&mut cli.options, value);
        }
        "--ignore" => cli.ignored.push(value.clone()),
        "--platform" if value == "github" => cli.options.platforms.push(value.clone()),
        "--extension" if value == "citations" => {}
        "--include-root" => cli.include_root = Some(value.clone()),
        _ => return Err(option_error("unknown platform or extension")),
    }
    Ok(Some(2))
}

const DJOT_RULES: &[&str] = &[
    "djot-emphasis-underscore",
    "djot-heading-continuation",
    "djot-highlight-braces",
    "djot-intraword-underscore",
    "djot-plus-bullet",
    "djot-subscript-tilde",
    "djot-subscript-tilde-braced",
    "djot-superscript-caret",
];

fn enable(options: &mut LintOptions, code: &str) {
    options.rules.insert(code.to_string(), "on".into());
}

fn print_help() {
    println!(
        "Usage: carve lint [--json] [--caption-hints] [--rule ID] [--ignore ID] \
        [--from-djot] [--portable] [--platform github] [--extension citations] \
        [--include-root DIR] [files]\nExit: 0 clean, 1 findings, 2 input or option error."
    );
}

fn run_files(mut cli: Cli) -> i32 {
    let root = cli
        .include_root
        .as_deref()
        .and_then(|path| std::fs::canonicalize(path).ok());
    if cli.include_root.is_some() && root.is_none() {
        return option_error("cannot read include root");
    }
    if let Some(root) = &root {
        let Ok(resolver) = carve::include::fs::resolver(root) else {
            return option_error("invalid include root");
        };
        cli.options.includes = Some(carve::include::Includes::new(resolver));
    }
    let mut findings = Vec::new();
    let mut failed = false;
    for path in &cli.paths {
        if cli.ignored.contains(path) {
            continue;
        }
        let Ok(content) = read(path) else {
            eprintln!("carve lint: cannot read {path}");
            failed = true;
            continue;
        };
        if let (Some(root), Some(includes)) = (&root, cli.options.includes.as_mut()) {
            includes.file = std::fs::canonicalize(path).ok().and_then(|file| {
                file.strip_prefix(root)
                    .ok()
                    .map(|name| name.to_string_lossy().replace('\\', "/"))
            });
        }
        for diagnostic in lint(&content, &cli.options) {
            if diagnostic
                .file
                .as_ref()
                .is_some_and(|file| cli.ignored.contains(file))
            {
                continue;
            }
            emit_finding(&diagnostic, path, cli.json_output, &mut findings);
        }
    }
    if cli.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&findings).unwrap_or_default()
        );
    }
    if failed {
        2
    } else {
        i32::from(!findings.is_empty())
    }
}

fn emit_finding(diagnostic: &Diagnostic, path: &str, json_output: bool, findings: &mut Vec<Value>) {
    let Some(pos) = diagnostic.pos else { return };
    let entry = if path == "-" { "<stdin>" } else { path };
    let label = diagnostic.file.as_deref().unwrap_or(entry);
    if json_output {
        let mut finding = json!({"file": label, "line": pos.start_line, "column": pos.start_column,
                    "rule": diagnostic.code, "message": diagnostic.message,
                    "start": pos.start_offset, "end": pos.end_offset});
        if !diagnostic.included_by.is_empty() {
            let chain: Vec<Value> = diagnostic.included_by.iter().map(|site| json!({
                        "file": site.file.as_deref().unwrap_or(entry), "line": site.line, "column": site.column
                    })).collect();
            if let Some(object) = finding.as_object_mut() {
                object.insert("includedBy".into(), json!(chain));
            }
        }
        findings.push(finding);
    } else {
        let chain = diagnostic
            .included_by
            .iter()
            .map(|site| {
                format!(
                    "{}:{}:{}",
                    site.file.as_deref().unwrap_or(entry),
                    site.line,
                    site.column
                )
            })
            .collect::<Vec<_>>()
            .join(" > ");
        let suffix = if chain.is_empty() {
            String::new()
        } else {
            format!(" (included from {chain})")
        };
        println!(
            "{label}:{}:{} {} — {}{suffix}",
            pos.start_line, pos.start_column, diagnostic.code, diagnostic.message,
        );
        findings.push(Value::Null);
    }
}

fn read(path: &str) -> std::io::Result<String> {
    if path == "-" {
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    } else {
        std::fs::read(path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }
}

fn known(code: &str) -> bool {
    let rules: Value =
        serde_json::from_str(include_str!("../spec-data/lint-rules.json")).unwrap_or(Value::Null);
    rules["rules"]
        .as_array()
        .is_some_and(|list| list.iter().any(|row| row["id"] == code))
}

fn option_error(message: &str) -> i32 {
    eprintln!("carve lint: {message}");
    2
}
