//! `carve` CLI — reads Carve source from a file or stdin, writes the rendered
//! output (HTML by default, or Markdown / plain text / ANSI / Carve) to stdout.

use std::io::{self, Read, Write};
use std::process::ExitCode;

#[derive(Clone, Copy)]
enum OutputFormat {
    Html,
    Markdown,
    Plain,
    Ansi,
    Carve,
    Json,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    Render,
    Fmt,
}

/// The stamp modes answer a question about the document rather than rendering
/// it: report the provenance marker, and optionally fail when the document
/// predates this engine's spec version.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StampMode {
    Info,
    Check,
}

fn main() -> ExitCode {
    let raw_args = std::env::args().skip(1).collect::<Vec<_>>();
    if raw_args.first().map(String::as_str) == Some("merge") {
        return run_merge(&raw_args[1..]);
    }
    // Bundled interactive extensions, owned here so they outlive `options`
    // (which borrows them). Registered only when `--extensions` is passed, so
    // the default CLI behavior is unchanged. They are degradation-safe: in
    // `--static` they render their flattened form, in interactive their live
    // form, and a document not using them is unaffected.
    let details = carve::Details::new();
    let spoiler = carve::Spoiler::new();
    let code_callouts = carve::CodeCallouts::new();
    let color_swatch = carve::ColorSwatch::new();
    // Every FencedRender diagram preset (mermaid, plantuml, d2, dot/graphviz,
    // wavedrom, abc, vega-lite, chart), owned here so it outlives `options`.
    let fenced_presets = carve::FencedRender::presets();
    let math_block = carve::MathBlock::new();

    let mut options = carve::Options::new();
    let mut format = OutputFormat::Html;
    let mut command = Command::Render;
    let mut fmt_write = false;
    let mut fmt_check = false;
    let mut fmt_stamp = None;
    let mut stamp_mode: Option<StampMode> = None;
    let mut enable_extensions = false;
    let mut from_json = false;
    let mut input_paths: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "fmt" if command == Command::Render && input_paths.is_empty() => {
                command = Command::Fmt;
                format = OutputFormat::Carve;
            }
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            "-w" | "--write" if command == Command::Fmt => fmt_write = true,
            "--check" if command == Command::Fmt => fmt_check = true,
            "--stamp" if command == Command::Fmt => fmt_stamp = Some(carve::StampForm::Line),
            "--stamp-block" if command == Command::Fmt => {
                fmt_stamp = Some(carve::StampForm::Block);
            }
            "--stamp-info" => stamp_mode = Some(StampMode::Info),
            "--stamp-check" => stamp_mode = Some(StampMode::Check),
            "--mention-url" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --mention-url requires a template");
                    return ExitCode::FAILURE;
                };
                options = options.with_mention_url(value);
            }
            "--tag-url" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --tag-url requires a template");
                    return ExitCode::FAILURE;
                };
                options = options.with_tag_url(value);
            }
            "--symbol" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --symbol requires name=value");
                    return ExitCode::FAILURE;
                };
                let Some((name, glyph)) = value.split_once('=') else {
                    eprintln!("carve: --symbol requires name=value");
                    return ExitCode::FAILURE;
                };
                options = options.with_symbol(name, glyph);
            }
            "--profile" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --profile requires a name (full|article|comment|minimal)");
                    return ExitCode::FAILURE;
                };
                let profile = match value.as_str() {
                    "full" => carve::Profile::full(),
                    "article" => carve::Profile::article(),
                    "comment" => carve::Profile::comment(),
                    "minimal" => carve::Profile::minimal(),
                    other => {
                        eprintln!(
                            "carve: unknown profile: {other} (expected full|article|comment|minimal)"
                        );
                        return ExitCode::FAILURE;
                    }
                };
                options = options.with_profile(profile);
            }
            "--smart-typography" => {
                // The switch the spec documents as document-global
                // (divergence-from-djot section 12). Source mode is for
                // machine-facing output, which is exactly what a CLI pipes
                // into something else, so the flag belongs here rather than
                // only in the library API.
                let Some(value) = args.next() else {
                    eprintln!("carve: --smart-typography requires a mode (glyph|source)");
                    return ExitCode::FAILURE;
                };
                options.smart_typography = match value.as_str() {
                    "glyph" => carve::SmartTypographyMode::Glyph,
                    "source" => carve::SmartTypographyMode::Source,
                    other => {
                        eprintln!(
                            "carve: unknown smart typography mode: {other} (expected glyph|source)"
                        );
                        return ExitCode::FAILURE;
                    }
                };
            }
            "--profile-base-host" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --profile-base-host requires a host");
                    return ExitCode::FAILURE;
                };
                options = options.with_profile_base_host(value);
            }
            "--html" => format = OutputFormat::Html,
            "--markdown" | "--md" => format = OutputFormat::Markdown,
            "--plain" | "--plain-text" => format = OutputFormat::Plain,
            "--ansi" => format = OutputFormat::Ansi,
            "--carve" => format = OutputFormat::Carve,
            "--json" | "--ast" => format = OutputFormat::Json,
            "--from-json" => from_json = true,
            "--static" => options = options.with_mode(carve::Mode::Static),
            "--interactive" => options = options.with_mode(carve::Mode::Interactive),
            "--extensions" => enable_extensions = true,
            "--no-raw-html" | "--safe" => options = options.with_raw_html(false),
            "-" if command == Command::Render => input_paths.clear(),
            "-" if command == Command::Fmt => input_paths.push(arg),
            path if path.starts_with('-') => {
                eprintln!("carve: unknown option: {path}");
                return ExitCode::FAILURE;
            }
            path => {
                if command == Command::Render && !input_paths.is_empty() {
                    eprintln!("carve: multiple input files specified");
                    return ExitCode::FAILURE;
                }
                input_paths.push(path.to_string());
            }
        }
    }

    if command == Command::Fmt {
        return run_fmt(&input_paths, fmt_write, fmt_check, fmt_stamp);
    }

    if enable_extensions {
        options = options
            .with_extension(&details)
            .with_extension(&spoiler)
            .with_extension(&code_callouts)
            .with_extension(&color_swatch)
            .with_extension(&math_block);
        for preset in &fenced_presets {
            options = options.with_extension(preset);
        }
    }

    let source = match input_paths.first().map(String::as_str) {
        None | Some("-") => {
            let mut buf = String::new();
            if let Err(err) = io::stdin().read_to_string(&mut buf) {
                eprintln!("carve: cannot read stdin: {err}");
                return ExitCode::FAILURE;
            }
            buf
        }
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("carve: cannot read {path}: {err}");
                return ExitCode::FAILURE;
            }
        },
    };
    if let Some(mode) = stamp_mode {
        match carve::read_stamp(&source) {
            None => println!(
                "unstamped (spec version unknown; this engine targets {})",
                carve::SPEC_VERSION
            ),
            Some(stamp) => println!(
                "carve-version: {}\ngenerated-by: {}\nthis engine targets: {}",
                stamp.version,
                stamp.generated_by.as_deref().unwrap_or("(unrecorded)"),
                carve::SPEC_VERSION
            ),
        }

        if mode == StampMode::Check && carve::needs_review(&source, carve::SPEC_VERSION) {
            eprintln!(
                "Review the [behavior] changelog entries between that version and {}.",
                carve::SPEC_VERSION
            );
            return ExitCode::FAILURE;
        }

        return ExitCode::SUCCESS;
    }

    let output = if from_json {
        // A profile's max_length bounds UNTRUSTED INPUT, and here the untrusted
        // input is the JSON payload: it is what gets parsed, held and walked.
        // The document's own `srcByteLength` cannot stand in for it - that number
        // arrives inside the payload, so a hostile tree can claim 0 and render
        // anything. Measured on the payload, which is also the form a host
        // storing trees actually receives.
        if let Some(profile) = &options.profile {
            let max_length = profile.max_length();
            if max_length > 0 && source.len() > max_length {
                eprintln!(
                    "carve: encoded AST exceeds the profile's maximum length of {max_length} bytes ({} bytes of JSON given).",
                    source.len()
                );
                return ExitCode::FAILURE;
            }
        }
        let doc = match carve::from_json(&source) {
            Ok(doc) => doc,
            Err(err) => {
                eprintln!("carve: cannot decode JSON AST: {err}");
                return ExitCode::FAILURE;
            }
        };
        match render_document(doc, format, &options) {
            Ok(output) => output,
            Err(err) => {
                eprintln!("carve: {err}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        // Mention/tag URL templates are an HTML-link concern, so they only affect
        // HTML output. All formats share the same parse + profile pipeline.
        match format {
            OutputFormat::Html => carve::to_html_with_options(&source, &options),
            // Positions ON for the three targets that PRINT the footnote
            // definitions: §7 orders them by source position, and the map they
            // come from is a BTreeMap, so without spans they print in label
            // order (carve-rs#686). `--json` below asks for the same thing.
            OutputFormat::Markdown => {
                options = options.with_positions(true);
                carve::to_markdown_with_options(&source, &options)
            }
            OutputFormat::Plain => {
                options = options.with_positions(true);
                carve::to_plain_text_with_options(&source, &options)
            }
            OutputFormat::Ansi => {
                options = options.with_positions(true);
                carve::to_ansi_with_options(&source, &options)
            }
            OutputFormat::Carve => carve::to_carve(&source),
            OutputFormat::Json => {
                options = options.with_positions(true);
                carve::to_json_with_options(&source, &options)
            }
        }
    };
    let mut stdout = io::stdout().lock();
    if let Err(err) = stdout.write_all(output.as_bytes()) {
        eprintln!("carve: cannot write stdout: {err}");
        return ExitCode::FAILURE;
    }
    if !output.ends_with('\n') {
        let _ = stdout.write_all(b"\n");
    }
    ExitCode::SUCCESS
}

fn run_merge(args: &[String]) -> ExitCode {
    let json = args.iter().any(|arg| arg == "--json");
    let paths = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .collect::<Vec<_>>();
    if paths.len() != 3 {
        eprintln!("carve merge: takes exactly three files (base, ours, theirs)");
        return ExitCode::from(2);
    }
    let mut documents = Vec::new();
    for path in paths {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("carve merge: cannot read {path}: {error}");
                return ExitCode::from(2);
            }
        };
        documents.push(carve::parse(&source));
    }
    match carve::merge_ast(&documents[0], &documents[1], &documents[2]) {
        Ok(carve::MergeResult::Merged(document)) => {
            let output = if json {
                carve::to_json(&document)
            } else {
                match carve::render_carve(&document) {
                    Ok(output) => output,
                    Err(error) => {
                        eprintln!("carve merge: cannot serialize result: {error}");
                        return ExitCode::FAILURE;
                    }
                }
            };
            print!("{output}");
            if !output.ends_with('\n') {
                println!();
            }
            ExitCode::SUCCESS
        }
        Ok(carve::MergeResult::Conflicts(conflicts)) => {
            if json {
                let items = conflicts.iter().map(|item| format!(
                    "{{\"path\":{:?},\"reason\":{:?},\"base\":{},\"ours\":{},\"theirs\":{}}}",
                    item.path,
                    match item.reason { carve::MergeConflictReason::BothChanged => "both-changed", carve::MergeConflictReason::DeleteEdit => "delete-edit", carve::MergeConflictReason::ConcurrentSequenceEdit => "concurrent-sequence-edit" },
                    item.base.as_deref().unwrap_or("null"), item.ours.as_deref().unwrap_or("null"), item.theirs.as_deref().unwrap_or("null")
                )).collect::<Vec<_>>().join(",");
                println!("{{\"ok\":false,\"ast\":null,\"conflicts\":[{items}]}}");
            } else {
                for item in &conflicts {
                    eprintln!("conflict {:?} at {}", item.reason, item.path);
                }
                eprintln!("{} structural conflict(s)", conflicts.len());
            }
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("carve merge: {error}");
            ExitCode::from(2)
        }
    }
}

/// What can stop `--from-json` from producing output.
///
/// This path is the one where a renderer's §25 ceiling is reachable: the JSON
/// reader accepts trees deeper than the markup parser can build, so a decoded
/// document may exceed a bound the source path cannot. The refusal is reported
/// and exits non-zero, like every other CLI failure.
enum RenderError {
    Profile(carve::ProfileViolationError),
    Depth(carve::RenderDepthError),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Profile(err) => write!(f, "profile violation: {err}"),
            RenderError::Depth(err) => write!(f, "{err}"),
        }
    }
}

impl From<carve::ProfileViolationError> for RenderError {
    fn from(err: carve::ProfileViolationError) -> Self {
        RenderError::Profile(err)
    }
}

impl From<carve::RenderDepthError> for RenderError {
    fn from(err: carve::RenderDepthError) -> Self {
        RenderError::Depth(err)
    }
}

fn render_document(
    doc: carve::Document,
    format: OutputFormat,
    options: &carve::Options<'_>,
) -> Result<String, RenderError> {
    let (mode, target_is_html) = match format {
        OutputFormat::Html => (options.mode, true),
        _ => (carve::Mode::Interactive, false),
    };
    let doc = carve::prepare_document_for_render(doc, options, mode, target_is_html)?;
    Ok(match format {
        OutputFormat::Html => carve::render_html_with_options(&doc, options)?,
        OutputFormat::Markdown => carve::render_markdown_with_options(&doc, options)?,
        OutputFormat::Plain => carve::render_plain_text_with_options(&doc, options)?,
        OutputFormat::Ansi => carve::render_ansi_with_options(&doc, options)?,
        OutputFormat::Carve => carve::render_carve(&doc)?,
        OutputFormat::Json => carve::to_json(&doc),
    })
}

fn run_fmt(
    paths: &[String],
    write: bool,
    check: bool,
    stamp: Option<carve::StampForm>,
) -> ExitCode {
    if write && check {
        eprintln!("carve fmt: --write and --check are mutually exclusive");
        return ExitCode::FAILURE;
    }
    if paths.is_empty() || paths == ["-"] {
        if write || check {
            eprintln!("carve fmt: --write/--check require file paths");
            return ExitCode::FAILURE;
        }
        let mut source = String::new();
        if let Err(err) = io::stdin().read_to_string(&mut source) {
            eprintln!("carve fmt: cannot read stdin: {err}");
            return ExitCode::FAILURE;
        }
        return write_stdout(&format_carve(&source, stamp));
    }

    let mut changed = Vec::new();
    let mut stdout = String::new();
    for path in paths {
        if path == "-" {
            eprintln!("carve fmt: stdin cannot be mixed with file paths");
            return ExitCode::FAILURE;
        }
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("carve fmt: cannot read {path}: {err}");
                return ExitCode::FAILURE;
            }
        };
        let formatted = format_carve(&source, stamp);
        if formatted != source {
            changed.push(path.clone());
            if write {
                if let Err(err) = std::fs::write(path, formatted.as_bytes()) {
                    eprintln!("carve fmt: cannot write {path}: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
        if !write && !check {
            stdout.push_str(&formatted);
        }
    }
    if check && !changed.is_empty() {
        for path in changed {
            eprintln!("carve fmt: would reformat {path}");
        }
        return ExitCode::FAILURE;
    }
    if !stdout.is_empty() {
        return write_stdout(&stdout);
    }
    ExitCode::SUCCESS
}

fn format_carve(source: &str, stamp: Option<carve::StampForm>) -> String {
    let formatted = carve::to_carve(source);
    match stamp {
        Some(form) => {
            let generated_by = format!("carve-rs {}", env!("CARGO_PKG_VERSION"));
            carve::stamp_carve(&formatted, &generated_by, form)
        }
        None => formatted,
    }
}

fn write_stdout(output: &str) -> ExitCode {
    let mut stdout = io::stdout().lock();
    if let Err(err) = stdout.write_all(output.as_bytes()) {
        eprintln!("carve: cannot write stdout: {err}");
        return ExitCode::FAILURE;
    }
    if !output.ends_with('\n') {
        let _ = stdout.write_all(b"\n");
    }
    ExitCode::SUCCESS
}

fn print_usage() {
    println!(
        "carve — render Carve markup\n\n\
         Usage:\n  \
         carve [options] [file]      render file (or stdin when omitted or `-`)\n  \
         carve fmt [options] [files] format Carve source to stdout\n  \
         carve merge [--json] BASE OURS THEIRS\n  \
                                     merge independent structural edits\n  \
         carve -h                    show this help\n\n\
         Output format (default --html; last one wins):\n  \
         --html                      HTML\n  \
         --markdown, --md            Markdown\n  \
         --plain, --plain-text       plain text\n  \
         --ansi                      ANSI-colored terminal text\n  \
         --carve                     canonical Carve source\n\n\
         --json, --ast               the parsed AST as JSON\n  \
         --from-json                 read an encoded AST instead of Carve source\n\n\
         Format options:\n  \
         -w, --write                 write formatted output in place\n  \
         --check                     fail if any file is not formatted\n\n\
         --stamp                     append/update provenance marker\n  \
         --stamp-block               append/update provenance marker as block comment\n\n\
         Render mode (HTML only; default --interactive):\n  \
         --static                    self-contained HTML: flatten interactive\n                              \
         constructs, degrade diagrams/math to source\n  \
         --interactive               live HTML (default)\n\n\
         Options:\n  \
         --extensions                enable the bundled interactive extensions\n                              \
         (details, spoiler, code-callouts, color, math, and every diagram\n                              \
         preset: mermaid, plantuml, d2, graphviz, wavedrom, abc, vega-lite,\n                              \
         chart); needed for --static to flatten/degrade those constructs\n  \
         --mention-url TEMPLATE      render @mentions as links (HTML only)\n  \
         --tag-url TEMPLATE          render #tags as links (HTML only)\n  \
         --symbol NAME=VALUE         map :NAME: to VALUE (repeatable)\n  \
         --no-raw-html, --safe       escape =html raw blocks/spans instead of\n                              \
         emitting them (for untrusted input)\n  \
         --profile NAME              restrict features (full|article|comment|minimal)\n  \
         --profile-base-host HOST    base host for the profile link policy\n  \
         --smart-typography MODE     glyph (default) or source: emit the runs\n                              \
         the author typed instead of the resolved glyphs\n\n\
         Spec: https://markup-carve.github.io/carve/"
    );
}
