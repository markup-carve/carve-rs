//! `carve` CLI — reads Carve source from a file or stdin, writes the rendered
//! output (HTML by default, or Markdown / plain text / ANSI / Carve) to stdout.

use std::io::{self, Read, Write};
use std::process::ExitCode;

use carve::CarveExtension;

#[derive(Clone, Copy, PartialEq, Eq)]
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
    Flatten,
}

/// The stamp modes answer a question about the document rather than rendering
/// it: report the provenance marker, and optionally fail when the document
/// predates this engine's spec version.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StampMode {
    Info,
    Check,
}

/// Stands in for the filesystem resolver when the `fs` feature is off, so the
/// render path below compiles unchanged while carrying no code that opens a
/// file. It resolves nothing, which is what leaves the directive literal.
#[cfg(not(feature = "fs"))]
struct NoResolver;

#[cfg(not(feature = "fs"))]
impl carve::IncludeResolver for NoResolver {
    fn resolve(
        &self,
        _path: &str,
        _ctx: &carve::IncludeContext<'_>,
    ) -> Result<carve::IncludeResolved, carve::IncludeDenial> {
        Err(carve::IncludeDenial::Unresolved)
    }
}

fn main() -> ExitCode {
    let raw_args = std::env::args().skip(1).collect::<Vec<_>>();
    if raw_args.first().map(String::as_str) == Some("merge") {
        return run_merge(&raw_args[1..]);
    }
    if raw_args.first().map(String::as_str) == Some("migrate") {
        return run_migrate(&raw_args[1..]);
    }
    // Parsed here, not in the render loop below, so `lint` cannot inherit the
    // render flag surface. Sharing that loop meant `carve lint --static` was
    // accepted and silently ignored, and `carve lint --bogus` exited 1 - which
    // a CI gate reads as "found problems" rather than "could not run".
    if raw_args.first().map(String::as_str) == Some("lint") {
        return run_lint(&raw_args[1..]);
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
    let mut quote_locale: Option<String> = None;

    let mut options = carve::Options::new();
    let mut format = OutputFormat::Html;
    let mut command = Command::Render;
    let mut fmt_write = false;
    let mut fmt_check = false;
    let mut fmt_stamp = None;
    let mut stamp_mode: Option<StampMode> = None;
    let mut enable_extensions = false;
    let mut extension_keys: Vec<String> = Vec::new();
    let mut tabs_mode = carve::TabsMode::Css;
    let mut tabs_mode_set = false;
    let mut citation_mode = carve::CitationMode::Numbered;
    let mut citation_mode_set = false;
    let mut selective_render_options = false;
    let mut from_json = false;
    let mut strict_losses = false;
    let mut report_losses: Option<String> = None;
    let mut report_includes: Option<String> = None;
    let mut allow_render_loss = false;
    let mut max_render_losses = carve::DEFAULT_MAX_RENDER_LOSSES;
    // `mut` only where the flag can be honoured: without the `fs` feature the
    // flag is refused at the parse site and this never moves.
    #[cfg_attr(not(feature = "fs"), allow(unused_mut))]
    let mut include_root: Option<String> = None;
    let mut input_paths: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "fmt" if command == Command::Render && input_paths.is_empty() => {
                command = Command::Fmt;
                format = OutputFormat::Carve;
            }
            "flatten" if command == Command::Render && input_paths.is_empty() => {
                command = Command::Flatten;
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
            "--quote-locale" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --quote-locale requires a locale");
                    return ExitCode::FAILURE;
                };
                quote_locale = Some(value);
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
            "--strict-losses" => strict_losses = true,
            "--report-losses" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --report-losses requires a file or -");
                    return ExitCode::from(2);
                };
                report_losses = Some(value);
            }
            "--report-includes" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --report-includes requires a file or -");
                    return ExitCode::from(2);
                };
                report_includes = Some(value);
            }
            "--allow-loss" => match args.next().as_deref() {
                Some("raw-format-dropped") => allow_render_loss = true,
                _ => {
                    eprintln!("carve: --allow-loss expects raw-format-dropped");
                    return ExitCode::from(2);
                }
            },
            "--max-render-losses" => {
                let Some(value) = args.next().and_then(|value| value.parse::<usize>().ok()) else {
                    eprintln!("carve: --max-render-losses requires a non-negative integer");
                    return ExitCode::from(2);
                };
                max_render_losses = value;
            }
            "--static" => options = options.with_mode(carve::Mode::Static),
            "--interactive" => options = options.with_mode(carve::Mode::Interactive),
            "--extensions" => enable_extensions = true,
            "--extension" => {
                selective_render_options = true;
                let Some(value) = args.next() else {
                    eprintln!("carve: --extension requires a registry key");
                    return ExitCode::FAILURE;
                };
                if !carve::extensions::registry::keys().any(|key| key == value) {
                    let known = carve::extensions::registry::keys()
                        .collect::<Vec<_>>()
                        .join(", ");
                    eprintln!("carve: unknown extension: {value} (expected one of: {known})");
                    return ExitCode::FAILURE;
                }
                if !extension_keys.contains(&value) {
                    extension_keys.push(value);
                }
            }
            "--tabs-mode" => {
                selective_render_options = true;
                let Some(value) = args.next() else {
                    eprintln!("carve: --tabs-mode requires css or aria");
                    return ExitCode::FAILURE;
                };
                tabs_mode = match value.as_str() {
                    "css" => carve::TabsMode::Css,
                    "aria" => carve::TabsMode::Aria,
                    other => {
                        eprintln!("carve: unknown tabs mode: {other} (expected css|aria)");
                        return ExitCode::FAILURE;
                    }
                };
                tabs_mode_set = true;
            }
            "--citation-mode" => {
                selective_render_options = true;
                let Some(value) = args.next() else {
                    eprintln!("carve: --citation-mode requires numbered or author-date");
                    return ExitCode::FAILURE;
                };
                citation_mode = match value.as_str() {
                    "numbered" => carve::CitationMode::Numbered,
                    "author-date" => carve::CitationMode::AuthorDate,
                    other => {
                        eprintln!(
                            "carve: unknown citation mode: {other} (expected numbered|author-date)"
                        );
                        return ExitCode::FAILURE;
                    }
                };
                citation_mode_set = true;
            }
            "--no-sections" => {
                selective_render_options = true;
                options = options.with_sections(false);
            }
            "--source-lines" => {
                selective_render_options = true;
                options = options.with_source_lines(true);
            }
            "--include-root" => {
                let Some(value) = args.next() else {
                    eprintln!("carve: --include-root requires a directory");
                    return ExitCode::FAILURE;
                };
                // REFUSED, not ignored: a build without the resolver cannot
                // honour the flag, and silently rendering the directive as
                // literal text would look like the include simply failed.
                #[cfg(not(feature = "fs"))]
                {
                    let _ = value;
                    eprintln!("carve: --include-root needs the `fs` feature, which this build does not have");
                    return ExitCode::FAILURE;
                }
                #[cfg(feature = "fs")]
                {
                    // The resolver refuses a relative root (§19), so the flag is
                    // absolutized HERE - that is where `--include-root .` stays
                    // a convenience instead of becoming a cwd-rooted resolver.
                    let path = std::path::Path::new(&value);
                    let absolute = (!path.is_absolute()).then(|| {
                        match std::fs::canonicalize(path) {
                            Ok(real) => real,
                            // Keep the cwd-joined spelling so the failure the
                            // user sees names the directory they meant.
                            Err(_) => std::env::current_dir()
                                .map(|cwd| cwd.join(path))
                                .unwrap_or_else(|_| path.to_path_buf()),
                        }
                        .to_string_lossy()
                        .into_owned()
                    });
                    include_root = Some(absolute.unwrap_or(value));
                }
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

    if command != Command::Render && selective_render_options {
        eprintln!("carve: selective extension and render options apply only to rendering");
        return ExitCode::FAILURE;
    }

    if command == Command::Fmt {
        return run_fmt(&input_paths, fmt_write, fmt_check, fmt_stamp);
    }

    if command == Command::Flatten {
        return run_flatten(
            input_paths.first().map(String::as_str),
            include_root.as_deref(),
        );
    }

    if tabs_mode_set && !extension_keys.iter().any(|key| key == "tabs") {
        eprintln!("carve: --tabs-mode requires --extension tabs");
        return ExitCode::FAILURE;
    }
    if citation_mode_set && !extension_keys.iter().any(|key| key == "citations") {
        eprintln!("carve: --citation-mode requires --extension citations");
        return ExitCode::FAILURE;
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
    let named_extensions: Vec<Box<dyn carve::CarveExtension>> = extension_keys
        .iter()
        .map(|key| match key.as_str() {
            "tabs" => Box::new(carve::Tabs::with_options(carve::TabsOptions {
                mode: tabs_mode,
                ..carve::TabsOptions::default()
            })) as Box<dyn carve::CarveExtension>,
            "citations" => Box::new(carve::Citations::with_mode(citation_mode)),
            _ => carve::extensions::registry::by_key(key)
                .expect("extension keys were validated while parsing arguments"),
        })
        .filter(|extension| {
            !enable_extensions
                || (extension.name() != details.name()
                    && extension.name() != spoiler.name()
                    && extension.name() != code_callouts.name()
                    && extension.name() != color_swatch.name()
                    && extension.name() != math_block.name()
                    && !fenced_presets
                        .iter()
                        .any(|preset| extension.name() == preset.name()))
        })
        .filter(|extension| quote_locale.is_none() || extension.name() != "smart-quotes")
        .collect();
    for extension in &named_extensions {
        options = options.with_extension(extension.as_ref());
    }
    let smart_quotes = quote_locale.as_deref().map(carve::SmartQuotes::new);
    if let Some(extension) = &smart_quotes {
        options = options.with_extension(extension);
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

    let target = match format {
        OutputFormat::Html => carve::RenderTarget::Html,
        OutputFormat::Markdown => carve::RenderTarget::Markdown,
        OutputFormat::Plain => carve::RenderTarget::Plain,
        OutputFormat::Ansi => carve::RenderTarget::Ansi,
        OutputFormat::Carve | OutputFormat::Json => carve::RenderTarget::Carve,
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
    // contain a directive opener. `carve fmt` / --carve is excluded by I15 -
    // the formatter round-trips SOURCE, and inlining files into it would
    // rewrite the author's document rather than format it.
    //
    // NOT `target`, which collapses `--json` onto `RenderTarget::Carve` for
    // loss-reporting purposes. That collapse answers a different question: the
    // AST dump publishes a TREE, not Carve source, so it expands (carve-js has
    // `json` as its own target and expands for it too).
    let include_target = if format == OutputFormat::Carve {
        carve::RenderTarget::Carve
    } else {
        carve::RenderTarget::Html
    };
    let want_includes = root.is_some()
        && carve::expands_for_target(include_target)
        && (include_root.is_some() || source.contains("{{"));

    // The CLI's include path IS the filesystem resolver, so it compiles out with
    // it. Without the feature the directive stays literal, which is the core
    // behavior, not a degraded one.
    #[cfg(feature = "fs")]
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
    #[cfg(not(feature = "fs"))]
    let resolver: Option<NoResolver> = {
        let _ = want_includes;
        None
    };

    let checked_options = carve::CheckedRenderOptions {
        strict: false,
        max_losses: max_render_losses,
    };
    // Loss diagnostics promise source positions. Position tracking does not
    // alter rendered output, and the CLI is itself a reporting surface.
    options = options.with_positions(true);
    let mut include_dependencies: Vec<carve::IncludeDependency> = Vec::new();
    let (output, (mut losses, mut total_losses, mut truncated)) = if from_json {
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
        let checked = carve::with_render_loss_report(target, checked_options, || {
            render_document(doc, format, &options)
        })
        .expect("non-strict collection cannot fail");
        let output = match checked.value {
            Ok(output) => output,
            Err(err) => {
                eprintln!("carve: {err}");
                return ExitCode::FAILURE;
            }
        };
        (
            output,
            (checked.losses, checked.total_losses, checked.truncated),
        )
    } else if let Some(resolver) = &resolver {
        // INCLUDES TAKE THE DOCUMENT PATH, not the source facades below: the
        // pass works on a parsed tree, which is also what carries the file
        // identity each included node keeps.
        let mut include_options = carve::IncludeOptions::new().with_resolver(resolver);
        if let Some(path) = &input_path {
            include_options = include_options.with_source_path(path.to_string_lossy().into_owned());
        }
        let target_is_html = matches!(format, OutputFormat::Html);
        let prepared = match carve::prepare_doc_with_includes(
            &source,
            &options,
            &include_options,
            // The same mode the HTML target renders under; every other target
            // is interactive, which is what `render_document` also assumes.
            if target_is_html {
                options.mode
            } else {
                carve::Mode::Interactive
            },
            target_is_html,
        ) {
            Ok(prepared) => prepared,
            Err(err) => {
                eprintln!("carve: {err}");
                return ExitCode::FAILURE;
            }
        };
        for warning in &prepared.warnings {
            // The rule id is machine-readable and stable across engines, and
            // the file names the document the reader has to edit. The
            // resolver's own error text is NOT printed: it routinely carries
            // absolute host paths, and spec I7 keeps it off the report.
            eprintln!(
                "carve: {}: {} [{}]",
                warning.file.as_deref().unwrap_or("<stdin>"),
                warning.message,
                warning.rule
            );
        }
        if prepared.suppressed_warnings > 0 {
            eprintln!(
                "carve: {} additional include warning(s) suppressed",
                prepared.suppressed_warnings
            );
        }
        let checked = carve::with_render_loss_report(target, checked_options, || {
            render_document(prepared.doc.clone(), format, &options)
        })
        .expect("non-strict collection cannot fail");
        let output = match checked.value {
            Ok(output) => output,
            Err(err) => {
                eprintln!("carve: {err}");
                return ExitCode::FAILURE;
            }
        };
        include_dependencies = prepared.dependencies;
        (
            output,
            (checked.losses, checked.total_losses, checked.truncated),
        )
    } else {
        let checked = carve::with_render_loss_report(target, checked_options, || match format {
            OutputFormat::Html => carve::try_to_html_with_options(&source, &options),
            // Positions ON for the three targets that PRINT the footnote
            // definitions: §7 orders them by source position, and the map they
            // come from is a BTreeMap, so without spans they print in label
            // order (carve-rs#686). `--json` below asks for the same thing.
            OutputFormat::Markdown => {
                options = options.with_positions(true);
                carve::try_to_markdown_with_options(&source, &options)
            }
            OutputFormat::Plain => {
                options = options.with_positions(true);
                carve::try_to_plain_text_with_options(&source, &options)
            }
            OutputFormat::Ansi => {
                options = options.with_positions(true);
                carve::try_to_ansi_with_options(&source, &options)
            }
            // The options-taking sibling. `to_carve` carries no `Options`, so
            // the profile was never even asked about on this target: an
            // over-cap document was re-serialized in full at exit 0, and a raw
            // HTML block a `minimal` profile removes from every other output
            // came straight back out (carve-rs#1191).
            OutputFormat::Carve => carve::try_to_carve_with_options(&source, &options),
            OutputFormat::Json => {
                options = options.with_positions(true);
                carve::try_to_json_with_options(&source, &options)
            }
        })
        .expect("non-strict collection cannot fail");
        let output = match checked.value {
            Ok(output) => output,
            Err(err) => {
                let err = RenderError::from(err);
                eprintln!("carve: {err}");
                return ExitCode::FAILURE;
            }
        };
        (
            output,
            (checked.losses, checked.total_losses, checked.truncated),
        )
    };
    if allow_render_loss {
        losses.clear();
        total_losses = 0;
        truncated = false;
    }
    let file = input_paths.first().map(String::as_str).unwrap_or("<stdin>");
    if total_losses > 0 {
        for loss in &losses {
            let at = loss
                .pos
                .as_ref()
                .map(|pos| format!(":{}:{}", pos.start_line, pos.start_column))
                .unwrap_or_default();
            eprintln!("{file}{at} {} - {}", loss.code, loss.message);
        }
        eprintln!(
            "{file}: {total_losses} render loss{}",
            if total_losses == 1 { "" } else { "es" }
        );
    }
    if let Some(path) = report_losses {
        let json = render_loss_json(&losses, total_losses, truncated);
        if path == "-" {
            eprintln!("{json}");
        } else if let Err(error) = std::fs::write(&path, format!("{json}\n")) {
            eprintln!("carve: cannot write render-loss report {path}: {error}");
            return ExitCode::from(2);
        }
    }
    if let Some(path) = report_includes {
        let json = include_dependency_json(&include_dependencies);
        if path == "-" {
            eprintln!("{json}");
        } else if let Err(error) = std::fs::write(&path, format!("{json}\n")) {
            eprintln!("carve: cannot write include-dependency report {path}: {error}");
            return ExitCode::from(2);
        }
    }
    if strict_losses && total_losses > 0 {
        return ExitCode::FAILURE;
    }
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

/// `carve migrate`, whose exit codes separate "the conversion never ran" from
/// "the conversion ran and lost something".
///
/// EXIT 1 MEANS EXACTLY ONE THING HERE: `--check-loss` found loss. Every other
/// non-zero exit is 2 - a bad flag, a missing `--from`, an unreadable input, a
/// report that could not be written. They all used to be 1 as well, so
///
/// ```text
/// carve migrate --from html --check-loss in.html > out.crv || echo "content was dropped"
/// ```
///
/// printed "content was dropped" when the real problem was a typo in a flag
/// name, and the operator had no signal that the conversion never ran at all.
/// carve-js and carve-php both exit 2 for these, and all three already agree on
/// 1 for loss, so only the usage half was out of step (#1276).
///
/// `run_lint` and `run_merge` in this file already draw the line in the same
/// place, and the comment above `run_lint` records why: `carve lint --bogus`
/// exiting 1 "a CI gate reads as 'found problems' rather than 'could not run'".
fn run_migrate(args: &[String]) -> ExitCode {
    let mut mode = carve::HtmlImportMode::Safe;
    let mut adapter = carve::HtmlImportAdapter::Generic;
    let mut report_path: Option<&str> = None;
    let mut check_loss = false;
    let mut input: Option<&str> = None;
    let mut from: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--from" => {
                i += 1;
                from = args.get(i).cloned();
            }
            // Both flags read the SAME vocabulary the report writes back out,
            // so a mode the CLI takes is always a mode the schema admits.
            "--mode" => {
                i += 1;
                mode = match args.get(i).map(String::as_str) {
                    Some(value) => match carve::HtmlImportMode::from_name(value) {
                        Some(mode) => mode,
                        None => {
                            eprintln!("carve migrate: unknown mode {value}");
                            return ExitCode::from(2);
                        }
                    },
                    None => {
                        eprintln!("carve migrate: --mode requires a value");
                        return ExitCode::from(2);
                    }
                };
            }
            "--adapter" => {
                i += 1;
                adapter = match args.get(i).map(String::as_str) {
                    Some(value) => match carve::HtmlImportAdapter::from_name(value) {
                        Some(adapter) => adapter,
                        None => {
                            eprintln!("carve migrate: unknown adapter {value}");
                            return ExitCode::from(2);
                        }
                    },
                    None => {
                        eprintln!("carve migrate: --adapter requires a value");
                        return ExitCode::from(2);
                    }
                };
            }
            "--report" => {
                i += 1;
                report_path = args.get(i).map(String::as_str);
            }
            "--check-loss" => check_loss = true,
            "-" => input = Some("-"),
            value if value.starts_with('-') => {
                eprintln!("carve migrate: unknown option {value}");
                return ExitCode::from(2);
            }
            value => {
                if input.is_some() {
                    eprintln!("carve migrate: takes at most one input file");
                    return ExitCode::from(2);
                }
                input = Some(value);
            }
        }
        i += 1;
    }
    let from = match from.as_deref() {
        Some("html") | Some("markdown") | Some("md") | Some("djot") | Some("bbcode") => {
            from.clone().unwrap_or_default()
        }
        Some(other) => {
            eprintln!("carve migrate: unknown source format {other}");
            return ExitCode::from(2);
        }
        None => {
            eprintln!("carve migrate: --from html, markdown, djot or bbcode is required");
            return ExitCode::from(2);
        }
    };
    let source = match input {
        None | Some("-") => {
            let mut s = String::new();
            if io::stdin().read_to_string(&mut s).is_err() {
                return ExitCode::from(2);
            }
            s
        }
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("carve migrate: cannot read {path}: {e}");
                return ExitCode::from(2);
            }
        },
    };
    let result = match from.as_str() {
        "html" => match carve::migrate_html(
            &source,
            &carve::HtmlImportOptions {
                mode,
                adapter,
                ..Default::default()
            },
        ) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("carve migrate: {error:?}");
                return ExitCode::from(2);
            }
        },
        "djot" => carve::migrate_djot(&source),
        "bbcode" => match carve::migrate_bbcode(&source) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("carve migrate: {error}");
                return ExitCode::from(2);
            }
        },
        _ => carve::migrate_markdown(&source),
    };
    print!("{}", result.value);
    let report = migration_report_json(&result.report);
    if let Some(path) = report_path {
        if path == "-" {
            eprintln!("{report}");
        } else if let Err(e) = std::fs::write(path, format!("{report}\n")) {
            eprintln!("carve migrate: cannot write report {path}: {e}");
            return ExitCode::from(2);
        }
    }
    if check_loss
        && result.report.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.fidelity,
                carve::MigrationFidelity::Degraded | carve::MigrationFidelity::Dropped
            )
        })
    {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The shared migration report as JSON.
///
/// Every spelling comes from the vocabulary's own `as_str`, never from a copy
/// kept here: `tests/the_report_answers_to_the_published_schema.rs` holds
/// those to `resources/html-import-schema.json`, and a second table in this
/// file would be outside what that test can see.
fn migration_report_json(report: &carve::MigrationReport) -> String {
    let diagnostics = report
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let mut value = serde_json::json!({
                "code": diagnostic.code,
                "message": diagnostic.message,
                "severity": diagnostic.severity.as_str(),
                "fidelity": diagnostic.fidelity.as_str(),
                "confidence": diagnostic.confidence.as_str(),
                "path": diagnostic.path,
            });
            if diagnostic.path.is_none() {
                value
                    .as_object_mut()
                    .expect("diagnostic object")
                    .remove("path");
            }
            value
        })
        .collect::<Vec<_>>();
    let mut value = serde_json::json!({
        "schemaVersion": report.schema_version,
        "sourceFormat": report.source_format.as_str(),
        "mode": report.mode.map(|value| value.as_str()),
        "adapter": report.adapter.map(|value| value.as_str()),
        "diagnostics": diagnostics,
    });
    if report.mode.is_none() {
        value.as_object_mut().expect("report object").remove("mode");
    }
    if report.adapter.is_none() {
        value
            .as_object_mut()
            .expect("report object")
            .remove("adapter");
    }
    value.to_string()
}

fn run_merge(args: &[String]) -> ExitCode {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("usage: carve merge [--json] BASE OURS THEIRS");
        return ExitCode::SUCCESS;
    }
    if let Some(option) = args
        .iter()
        .find(|arg| arg.starts_with('-') && arg.as_str() != "--json")
    {
        eprintln!("carve merge: unknown option: {option}");
        return ExitCode::from(2);
    }
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
                format!(
                    "{{\"ok\":true,\"ast\":{},\"conflicts\":[]}}",
                    carve::to_json(&document)
                )
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
                let items = conflicts
                    .iter()
                    .map(|item| {
                        let deleted = if item.base.is_none()
                            || item.ours.is_none()
                            || item.theirs.is_none()
                        {
                            format!(
                                ",\"deleted\":{{\"base\":{},\"ours\":{},\"theirs\":{}}}",
                                item.base.is_none(),
                                item.ours.is_none(),
                                item.theirs.is_none(),
                            )
                        } else {
                            String::new()
                        };
                        format!(
                            "{{\"path\":{},\"reason\":{},\"base\":{},\"ours\":{},\"theirs\":{}{deleted}}}",
                            json_string(&item.path),
                            json_string(merge_reason(item.reason)),
                            item.base.as_deref().unwrap_or("null"),
                            item.ours.as_deref().unwrap_or("null"),
                            item.theirs.as_deref().unwrap_or("null")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                println!("{{\"ok\":false,\"ast\":null,\"conflicts\":[{items}]}}");
            } else {
                for item in &conflicts {
                    eprintln!("conflict {} at {}", merge_reason(item.reason), item.path);
                }
                eprintln!(
                    "{} structural conflict{}",
                    conflicts.len(),
                    if conflicts.len() == 1 { "" } else { "s" }
                );
            }
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("carve merge: {error}");
            ExitCode::from(2)
        }
    }
}

fn merge_reason(reason: carve::MergeConflictReason) -> &'static str {
    match reason {
        carve::MergeConflictReason::BothChanged => "both-changed",
        carve::MergeConflictReason::DeleteEdit => "delete-edit",
        carve::MergeConflictReason::ConcurrentSequenceEdit => "concurrent-sequence-edit",
    }
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '\"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character <= '\u{1f}' => {
                use std::fmt::Write as _;
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output.push('\"');
    output
}

fn render_loss_json(losses: &[carve::RenderLoss], total: usize, truncated: bool) -> String {
    let rows = losses.iter().map(|loss| {
        let pos = loss.pos.as_ref().map(|pos| format!(
            ",\"pos\":{{\"startLine\":{},\"endLine\":{},\"startColumn\":{},\"endColumn\":{},\"startOffset\":{},\"endOffset\":{}}}",
            pos.start_line, pos.end_line, pos.start_column, pos.end_column, pos.start_offset, pos.end_offset,
        )).unwrap_or_default();
        format!(
            "{{\"code\":{},\"format\":{},\"target\":{},\"nodeType\":{},\"message\":{}{pos}}}",
            json_string(loss.code), json_string(&loss.format), json_string(loss.target.as_str()),
            json_string(loss.node_type.as_str()), json_string(&loss.message),
        )
    }).collect::<Vec<_>>().join(",");
    format!("{{\"losses\":[{rows}],\"totalLosses\":{total},\"truncated\":{truncated}}}")
}

/// The spec I11 dependency list, in the order expansion first reached each
/// target.
///
/// Unlike the warnings, this list is NOT capped: a host past
/// `DEFAULT_MAX_WARNINGS` refusals can still tell a complete list from a
/// truncated one, which reconstructing it from the warnings cannot.
fn include_dependency_json(dependencies: &[carve::IncludeDependency]) -> String {
    let rows = dependencies
        .iter()
        .map(|dependency| {
            let denial = dependency
                .denial
                .map(|denial| format!(",\"denial\":{}", json_string(denial.as_str())))
                .unwrap_or_default();
            format!(
                "{{\"id\":{},\"resolved\":{}{denial}}}",
                json_string(&dependency.id),
                dependency.resolved,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"dependencies\":[{rows}]}}")
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
    Carve(carve::RenderCarveError),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Profile(err) => write!(f, "profile violation: {err}"),
            RenderError::Depth(err) => write!(f, "{err}"),
            RenderError::Carve(err) => write!(f, "{err}"),
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

impl From<carve::RenderCarveError> for RenderError {
    fn from(err: carve::RenderCarveError) -> Self {
        RenderError::Carve(err)
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

/// Report constructs that parse and render, but not the way the author meant.
///
/// The defect class here is the silent one: the document parses, the renderer
/// emits something, and what the author wrote never reaches the page. An
/// unattached block attribute is the clearest case - `{#id .cls}` above a blank
/// line attaches to nothing, so the id and the class vanish and nothing says so.
///
/// The output line and the exit codes match carve-js's `carve lint` exactly,
/// because the two are used interchangeably as a CI gate and a script that
/// parses one must parse the other:
///
/// ```text
/// path.crv:3:1 unattached-block-attribute — This block attribute reaches no block: ...
/// ```
///
/// Exit codes, and the three-way split is the point: **0** clean, **1** findings,
/// **2** could not run. A gate that collapsed 2 into 1 would report an unreadable
/// file as a lint failure, and one that collapsed it into 0 would pass a build
/// whose documents were never read.
fn run_lint(args: &[String]) -> ExitCode {
    let mut paths: Vec<&str> = Vec::new();
    let mut enable_extensions = false;
    for arg in args {
        match arg.as_str() {
            "--extensions" => enable_extensions = true,
            "-h" | "--help" => {
                println!(
                    "carve lint - report constructs that parse but do not reach the page\n\n\
                     Usage:\n  \
                     carve lint [--extensions] [files]\n\n\
                     Reads stdin when no file is given, or when the file is `-`.\n\n\
                     Options:\n  \
                     --extensions   enable the bundled extensions (the only render\n                 \
                     option the linter reads)\n\n\
                     Exit codes:\n  \
                     0   no findings\n  \
                     1   findings reported\n  \
                     2   a file could not be read, or an option was not understood"
                );
                return ExitCode::SUCCESS;
            }
            // Every other flag is REFUSED rather than ignored. The render loop
            // would have accepted `--static` or `--profile` here and quietly
            // dropped them, which reads as a clean lint of the wrong thing.
            other if other.starts_with('-') && other != "-" => {
                eprintln!(
                    "carve lint: unknown option: {other} (only --extensions applies to lint)"
                );
                return ExitCode::from(2);
            }
            path => paths.push(path),
        }
    }
    run_lint_paths(&paths, enable_extensions)
}

fn run_lint_paths(paths: &[&str], enable_extensions: bool) -> ExitCode {
    // Only `options.extensions` is read by the linter - `lint_carve_with_options`
    // documents that - so nothing else from the render path is plumbed through
    // here. Owned locally so they outlive the borrow in `options`.
    let details = carve::Details::new();
    let spoiler = carve::Spoiler::new();
    let code_callouts = carve::CodeCallouts::new();
    let color_swatch = carve::ColorSwatch::new();
    let fenced_presets = carve::FencedRender::presets();
    let math_block = carve::MathBlock::new();
    let mut options = carve::Options::new();
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

    let mut findings = 0usize;
    let mut unreadable = false;

    let mut lint_source = |source: &str, label: &str| {
        for warning in carve::lint_carve_with_options(source, &options) {
            println!(
                "{label}:{}:{} {} — {}",
                warning.line, warning.column, warning.rule, warning.message
            );
            findings += 1;
        }
    };

    if paths.is_empty() || paths == ["-"] {
        let mut buf = String::new();
        if let Err(err) = io::stdin().read_to_string(&mut buf) {
            eprintln!("carve lint: cannot read stdin: {err}");
            return ExitCode::from(2);
        }
        lint_source(&buf, "<stdin>");
    } else {
        for path in paths {
            if *path == "-" {
                let mut buf = String::new();
                if let Err(err) = io::stdin().read_to_string(&mut buf) {
                    eprintln!("carve lint: cannot read stdin: {err}");
                    unreadable = true;
                    continue;
                }
                lint_source(&buf, "<stdin>");
                continue;
            }
            match std::fs::read_to_string(path) {
                Ok(source) => lint_source(&source, path),
                Err(err) => {
                    // Reported and skipped rather than fatal, so one bad path in
                    // a glob still lets every other document be checked - the
                    // whole point of running this over a tree.
                    eprintln!("carve lint: cannot read {path}: {err}");
                    unreadable = true;
                }
            }
        }
    }

    if unreadable {
        return ExitCode::from(2);
    }
    if findings > 0 {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// `carve flatten` - write the document back as ONE self-contained Carve file,
/// with every include expanded in place.
///
/// The deliberate opposite of `carve fmt`, and the reason both exist. Formatting
/// round-trips the author's document, so it leaves directives alone (I15);
/// flattening is an explicit request for the OTHER document - the one with the
/// children merged in - because that is what can be pasted somewhere with no
/// filesystem behind it.
///
/// TWO THINGS THIS CHANGES beyond inlining, both worth reporting rather than
/// letting someone find them in a published page:
///
///   - The output is CANONICAL Carve. Parent and children alike go through the
///     writer, so formatting is normalized, not preserved.
///   - Colliding heading ids and footnote labels are RENAMED (I5), because two
///     files that were never in one document together can each define `intro`.
///     The rename warnings print like any other.
///
/// Requires a containment root, so stdin is refused unless `--include-root`
/// names one: "flatten this" with nothing to resolve against is a request that
/// cannot be honoured, and silently writing the document back unchanged would
/// look like it had no includes.
#[cfg(feature = "fs")]
fn run_flatten(path: Option<&str>, include_root: Option<&str>) -> ExitCode {
    let (source, source_path) = match path {
        Some(path) if path != "-" => match std::fs::read_to_string(path) {
            Ok(source) => (source, Some(path.to_string())),
            Err(err) => {
                eprintln!("carve flatten: cannot read {path}: {err}");
                return ExitCode::FAILURE;
            }
        },
        _ => {
            let mut source = String::new();
            if let Err(err) = io::stdin().read_to_string(&mut source) {
                eprintln!("carve flatten: cannot read stdin: {err}");
                return ExitCode::FAILURE;
            }
            (source, None)
        }
    };

    let root = match include_root.map(str::to_string).or_else(|| {
        source_path
            .as_deref()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()))
    }) {
        Some(root) => root,
        None => {
            eprintln!(
                "carve flatten: stdin has no directory to resolve includes against; \
                 pass --include-root DIR"
            );
            return ExitCode::FAILURE;
        }
    };

    let resolver = match carve::FileSystemResolver::new(&root) {
        Ok(resolver) => resolver,
        Err(err) => {
            eprintln!("carve flatten: cannot use include root {root}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut options = carve::IncludeOptions::new().with_resolver(&resolver);
    if let Some(path) = source_path.as_deref() {
        let absolute =
            std::fs::canonicalize(path).unwrap_or_else(|_| std::path::PathBuf::from(path));
        options = options.with_source_path(absolute.to_string_lossy().into_owned());
    }
    // POSITIONS ON. The writer orders collected definitions by source position,
    // and after a merge that ordering is the only thing that keeps a child's
    // footnote definitions in the order the document reads. Parsed without
    // them, every merged definition reported no position at all and the order
    // fell back to the LABEL - so `[^m]` from the second include was published
    // above `[^n]` from the first.
    let parse_options = carve::Options::default().with_positions(true);
    let result = carve::expand_includes(
        carve::parse_with_options(&source, &parse_options),
        &source,
        &options,
    );
    for warning in &result.warnings {
        eprintln!("carve: {} [{}]", warning.message, warning.rule);
    }
    if result.suppressed_warnings > 0 {
        eprintln!(
            "carve: {} further include warning(s) suppressed",
            result.suppressed_warnings
        );
    }

    match carve::render_carve(&result.doc) {
        Ok(output) => write_stdout(&output),
        Err(err) => {
            eprintln!("carve flatten: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Without the filesystem resolver there is nothing to flatten AGAINST, so the
/// command refuses rather than writing the document back unchanged.
#[cfg(not(feature = "fs"))]
fn run_flatten(_path: Option<&str>, _include_root: Option<&str>) -> ExitCode {
    eprintln!("carve flatten: needs the `fs` feature, which this build does not have");
    ExitCode::FAILURE
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
         carve flatten [file]        write the document as ONE self-contained\n                              \
         file, every include expanded in place (the\n                              \
         opposite of fmt, which leaves them alone)\n  \
         carve lint [files]          report constructs that render wrong\n                              \
         (exit 1 on findings, 2 if a file cannot be read)\n  \
         carve merge [--json] BASE OURS THEIRS\n  \
                                     merge independent structural edits\n  \
         carve migrate --from FORMAT [options] [file]\n                              \
         convert html, markdown (md), djot or bbcode to Carve.\n                              \
         --mode/--adapter apply to html; --report/--check-loss\n                              \
         apply to all importers and fail closed when fidelity is unknown\n                              \
         (exit 1 only when --check-loss finds loss, 2 on a\n                              \
         usage error or an unreadable file)\n  \
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
         --extension KEY             enable a registry extension (repeatable;\n                              \
                                     an unknown key prints the accepted list)\n  \
         --tabs-mode MODE            css (default) or aria for --extension tabs\n  \
         --citation-mode MODE        numbered (default) or author-date\n  \
         --no-sections               render headings without section wrappers\n  \
         --source-lines              emit data-source-line annotations\n  \
         --mention-url TEMPLATE      render @mentions as links (HTML only)\n  \
         --tag-url TEMPLATE          render #tags as links (HTML only)\n  \
         --symbol NAME=VALUE         map :NAME: to VALUE (repeatable)\n  \
         --no-raw-html, --safe       escape =html raw blocks/spans instead of\n                              \
         emitting them (for untrusted input)\n  \
         --profile NAME              restrict features (full|article|comment|minimal)\n  \
         --profile-base-host HOST    base host for the profile link policy\n  \
         --smart-typography MODE     glyph (default) or source: emit the runs\n                              \
         the author typed instead of the resolved glyphs\n  \
         --quote-locale LOCALE       use locale-specific opening/closing quotes\n\n\
         --strict-losses             refuse output when raw formats are dropped\n  \
         --report-losses FILE        write JSON loss report (`-` for stderr)\n  \
         --allow-loss raw-format-dropped\n                              \
                                     accept intentional target filtering\n  \
         --max-render-losses N       bound detailed losses (default 100)\n\n\
         --include-root DIR          containment root for {{ path }} includes.\n                              \
         Defaults to the input file's directory; pass this to widen\n                              \
         or narrow it, or to enable includes on stdin\n  \
         --report-includes FILE      write the JSON include-dependency list\n                              \
         (`-` for stderr)\n\n\
         Spec: https://markup-carve.github.io/carve/"
    );
}
