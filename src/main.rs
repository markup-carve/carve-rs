//! `carve` CLI — reads Carve source from a file or stdin, writes the rendered
//! output (HTML by default, or Markdown / plain text / ANSI / Carve) to stdout.

use std::io::{self, Read, Write};
use std::process::ExitCode;

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Html,
    Markdown,
    Plain,
    Ansi,
    Carve,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    Render,
    Fmt,
}

fn main() -> ExitCode {
    // Bundled interactive extensions, owned here so they outlive `options`
    // (which borrows them). Registered only when `--extensions` is passed, so
    // the default CLI behavior is unchanged. They are degradation-safe: in
    // `--static` they render their flattened form, in interactive their live
    // form, and a document not using them is unaffected.
    let details = carve::Details::new();
    let spoiler = carve::Spoiler::new();
    let code_callouts = carve::CodeCallouts::new();
    let color_swatch = carve::ColorSwatch::new();
    let mermaid = carve::FencedRender::mermaid();
    let chart = carve::FencedRender::chart();
    let math_block = carve::MathBlock::new();

    let mut options = carve::Options::new();
    let mut format = OutputFormat::Html;
    let mut command = Command::Render;
    let mut fmt_write = false;
    let mut fmt_check = false;
    let mut fmt_stamp = None;
    let mut enable_extensions = false;
    let mut include_root: Option<String> = None;
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
            "--static" => options = options.with_mode(carve::Mode::Static),
            "--interactive" => options = options.with_mode(carve::Mode::Interactive),
            "--extensions" => enable_extensions = true,
            "--include-root" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --include-root requires a directory");
                    return ExitCode::FAILURE;
                };
                include_root = Some(value);
            }
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
            .with_extension(&mermaid)
            .with_extension(&chart)
            .with_extension(&math_block);
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
    // Containment root (spec I10): an explicit --include-root wins, otherwise a
    // file input defaults to the DIRECTORY OF THE DOCUMENT. Never the process
    // working directory, which is arbitrary with respect to the document and
    // may be `/` or a home directory. Stdin has no path context and therefore
    // no inferable root, so directives stay literal unless --include-root says
    // otherwise.
    //
    // The document path is ABSOLUTIZED first. The resolver looks a nested
    // relative include up from `root.join(parent)`, so a relative input like
    // `book/main.crv` (root `book`) would otherwise re-prefix the root and
    // search `book/book/child.crv`. carve-js absolutizes here for the same
    // reason.
    let input_path = input_paths.first().filter(|p| p.as_str() != "-").map(|p| {
        let path = std::path::Path::new(p);
        match std::fs::canonicalize(path) {
            Ok(real) => real,
            // The file was read successfully above, so this is unreachable
            // in practice; fall back to cwd-joining rather than to a
            // relative path, which would reintroduce the re-prefix bug.
            Err(_) => std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf()),
        }
    });
    let root = include_root.clone().or_else(|| {
        input_path.as_deref().and_then(|p| {
            p.parent()
                .filter(|d| !d.as_os_str().is_empty())
                .map(|d| d.to_string_lossy().into_owned())
        })
    });
    // Only pay for the expansion pass when includes could matter: an explicit
    // --include-root is a user request, otherwise the source must actually
    // contain a directive opener. `carve fmt` / --carve is excluded on
    // purpose - the formatter round-trips SOURCE, and inlining files into it
    // would rewrite the author's document rather than format it.
    let want_includes = root.is_some()
        && format != OutputFormat::Carve
        && (include_root.is_some() || source.contains("{{"));

    let resolver = if want_includes {
        let root = root.expect("guarded by want_includes");
        match carve::FileSystemResolver::new(&root) {
            Ok(resolver) => Some(resolver),
            Err(err) => {
                // An explicit root is a user request, so a bad one is fatal; an
                // inferred one silently falls back to no includes rather than
                // failing a render the user never asked to change.
                if include_root.is_some() {
                    eprintln!("carve: cannot use include root {root}: {err}");
                    return ExitCode::FAILURE;
                }
                None
            }
        }
    } else {
        None
    };

    let output = if let Some(resolver) = &resolver {
        let mut include_options = carve::IncludeOptions::new().with_resolver(resolver);
        if let Some(path) = &input_path {
            include_options = include_options.with_source_path(path.to_string_lossy());
        }
        let mode = match format {
            OutputFormat::Html => options.mode,
            _ => carve::Mode::Interactive,
        };
        match carve::prepare_doc_with_includes(&source, &options, &include_options, mode) {
            Ok(prepared) => {
                for warning in &prepared.warnings {
                    match &warning.file {
                        Some(file) => {
                            eprintln!("carve: {file}: {} ({})", warning.message, warning.rule)
                        }
                        None => eprintln!("carve: {} ({})", warning.message, warning.rule),
                    }
                }
                match format {
                    OutputFormat::Html => carve::render_html_with_options(&prepared.doc, &options),
                    OutputFormat::Markdown => {
                        carve::render_markdown_with_options(&prepared.doc, &options)
                    }
                    OutputFormat::Plain => {
                        carve::render_plain_text_with_options(&prepared.doc, &options)
                    }
                    OutputFormat::Ansi => carve::render_ansi_with_options(&prepared.doc, &options),
                    OutputFormat::Carve => unreachable!("excluded by want_includes"),
                }
            }
            // Matches the infallible `to_*_with_options` entry points: a
            // profile violation renders an empty safe output.
            Err(_) => String::new(),
        }
    } else {
        // Mention/tag URL templates are an HTML-link concern, so they only
        // affect HTML output. All formats share the same parse + profile
        // pipeline.
        match format {
            OutputFormat::Html => carve::to_html_with_options(&source, &options),
            OutputFormat::Markdown => carve::to_markdown_with_options(&source, &options),
            OutputFormat::Plain => carve::to_plain_text_with_options(&source, &options),
            OutputFormat::Ansi => carve::to_ansi_with_options(&source, &options),
            OutputFormat::Carve => carve::to_carve(&source),
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
         carve -h                    show this help\n\n\
         Output format (default --html; last one wins):\n  \
         --html                      HTML\n  \
         --markdown, --md            Markdown\n  \
         --plain, --plain-text       plain text\n  \
         --ansi                      ANSI-colored terminal text\n  \
         --carve                     canonical Carve source\n\n\
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
         (details, spoiler, code-callouts, color, mermaid, chart, math);\n                              \
         needed for --static to flatten/degrade those constructs\n  \
         --mention-url TEMPLATE      render @mentions as links (HTML only)\n  \
         --tag-url TEMPLATE          render #tags as links (HTML only)\n  \
         --symbol NAME=VALUE         map :NAME: to VALUE (repeatable)\n  \
         --no-raw-html, --safe       escape =html raw blocks/spans instead of\n                              \
         emitting them (for untrusted input)\n  \
         --profile NAME              restrict features (full|article|comment|minimal)\n  \
         --profile-base-host HOST    base host for the profile link policy\n  \
         --include-root DIR          containment root for {{ path }} includes.\n                              \
         Defaults to the input file's directory; pass this to widen\n                              \
         or narrow it, or to enable includes on stdin\n\n\
         Spec: https://markup-carve.github.io/carve/"
    );
}
