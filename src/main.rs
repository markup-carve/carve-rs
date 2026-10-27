//! `carve` CLI — reads Carve source from a file or stdin, writes the rendered
//! output (HTML by default, or Markdown / plain text / ANSI / Carve) to stdout.

use carve::CarveExtension;
use std::io::{self, Read, Write};
use std::process::ExitCode;
mod migrate_directory;

fn read_stdin_lossy(out: &mut String) -> io::Result<()> {
    let mut bytes = Vec::new();
    io::stdin().read_to_end(&mut bytes)?;
    *out = String::from_utf8_lossy(&bytes).into_owned();
    Ok(())
}

fn read_path_lossy(path: &str) -> io::Result<String> {
    std::fs::read(path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

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

#[derive(Default, Clone, Copy)]
struct IncludeBudgets {
    depth: Option<usize>,
    bytes: Option<usize>,
    resolver_calls: Option<usize>,
    warnings: Option<usize>,
}

impl IncludeBudgets {
    fn apply<'a>(self, mut options: carve::IncludeOptions<'a>) -> carve::IncludeOptions<'a> {
        if let Some(value) = self.depth {
            options = options.with_max_depth(value);
        }
        if let Some(value) = self.bytes {
            options = options.with_max_bytes(value);
        }
        if let Some(value) = self.resolver_calls {
            options = options.with_max_resolver_calls(value);
        }
        if let Some(value) = self.warnings {
            options = options.with_max_warnings(value);
        }
        options
    }

    fn any(self) -> bool {
        self.depth.is_some()
            || self.bytes.is_some()
            || self.resolver_calls.is_some()
            || self.warnings.is_some()
    }

    fn set(&mut self, flag: &str, value: usize) {
        match flag {
            "--max-depth" => self.depth = Some(value),
            "--max-bytes" => self.bytes = Some(value),
            "--max-resolver-calls" => self.resolver_calls = Some(value),
            _ => self.warnings = Some(value),
        }
    }
}

fn parse_budget(value: Option<&String>, flag: &str) -> Result<usize, ExitCode> {
    value
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| cli_error("usage", format!("{flag} requires a non-negative integer")))
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

/// Every flag the render, `fmt` and `flatten` commands accept, as parsed.
struct CliArgs<'a> {
    options: carve::Options<'a>,
    format: OutputFormat,
    command: Command,
    fmt_write: bool,
    fmt_check: bool,
    fmt_align_tables: bool,
    fmt_stamp: Option<carve::StampForm>,
    fmt_migrate: bool,
    stamp_mode: Option<StampMode>,
    enable_extensions: bool,
    extension_keys: Vec<String>,
    tabs_mode: carve::TabsMode,
    tabs_mode_set: bool,
    citation_mode: carve::CitationMode,
    citation_mode_set: bool,
    selective_render_options: bool,
    quote_locale: Option<String>,
    from_json: bool,
    max_render_losses: usize,
    include_root: Option<String>,
    no_includes: bool,
    include_budgets: IncludeBudgets,
    input_paths: Vec<String>,
    reports: LossReporting,
}

/// What happens to render losses and include dependencies once rendering ran.
struct LossReporting {
    strict_losses: bool,
    report_losses: Option<String>,
    report_includes: Option<String>,
    allowed_render_losses: Vec<&'static str>,
}

/// Bundled interactive extensions, owned by `main` so they outlive `options`
/// (which borrows them). Registered only when `--extensions` is passed, so
/// the default CLI behavior is unchanged. They are degradation-safe: in
/// `--static` they render their flattened form, in interactive their live
/// form, and a document not using them is unaffected.
struct BundledExtensions {
    details: carve::Details,
    spoiler: carve::Spoiler,
    code_callouts: carve::CodeCallouts,
    color_swatch: carve::ColorSwatch,
    // Every FencedRender diagram preset (mermaid, plantuml, d2, dot/graphviz,
    // wavedrom, abc, vega-lite, chart).
    fenced_presets: Vec<carve::FencedRender>,
    math_block: carve::MathBlock,
}

impl BundledExtensions {
    fn new() -> Self {
        Self {
            details: carve::Details::new(),
            spoiler: carve::Spoiler::new(),
            code_callouts: carve::CodeCallouts::new(),
            color_swatch: carve::ColorSwatch::new(),
            fenced_presets: carve::FencedRender::presets(),
            math_block: carve::MathBlock::new(),
        }
    }

    fn register<'a>(&'a self, mut options: carve::Options<'a>) -> carve::Options<'a> {
        options = options
            .with_extension(&self.details)
            .with_extension(&self.spoiler)
            .with_extension(&self.code_callouts)
            .with_extension(&self.color_swatch)
            .with_extension(&self.math_block);
        for preset in &self.fenced_presets {
            options = options.with_extension(preset);
        }
        options
    }

    fn provides(&self, name: &str) -> bool {
        name == self.details.name()
            || name == self.spoiler.name()
            || name == self.code_callouts.name()
            || name == self.color_swatch.name()
            || name == self.math_block.name()
            || self
                .fenced_presets
                .iter()
                .any(|preset| name == preset.name())
    }
}

/// The rendered output plus the loss and include bookkeeping it produced.
struct RenderOutcome {
    output: String,
    losses: Vec<carve::RenderLoss>,
    total_losses: usize,
    truncated: bool,
    totals_by_code: std::collections::BTreeMap<&'static str, usize>,
    include_dependencies: Vec<carve::IncludeDependency>,
}

impl RenderOutcome {
    fn from_checked<E: Into<RenderError>>(
        checked: carve::RenderResult<Result<String, E>>,
    ) -> Result<Self, ExitCode> {
        let output = match checked.value {
            Ok(output) => output,
            Err(err) => {
                let err: RenderError = err.into();
                eprintln!("carve: {err}");
                return Err(ExitCode::FAILURE);
            }
        };
        Ok(Self {
            output,
            losses: checked.losses,
            total_losses: checked.total_losses,
            truncated: checked.truncated,
            totals_by_code: checked.totals_by_code,
            include_dependencies: Vec::new(),
        })
    }
}

#[cfg(feature = "fs")]
type CliResolver = carve::FileSystemResolver;
#[cfg(not(feature = "fs"))]
type CliResolver = NoResolver;

fn main() -> ExitCode {
    // `std::env::args` panics on an argument that is not valid UTF-8, which
    // surfaced as exit 101 with a backtrace hint instead of a usage error.
    let mut raw_args = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.into_string() {
            Ok(arg) => raw_args.push(arg),
            Err(arg) => {
                return cli_error(
                    "usage",
                    format!("argument is not valid UTF-8: {}", arg.to_string_lossy()),
                );
            }
        }
    }
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
    if raw_args.first().map(String::as_str) == Some("region") {
        return run_region(&raw_args[1..]);
    }
    let bundled = BundledExtensions::new();
    let cli = match parse_args(raw_args) {
        Ok(cli) => cli,
        Err(code) => return code,
    };

    if cli.command != Command::Render && cli.selective_render_options {
        eprintln!("carve: selective extension and render options apply only to rendering");
        return ExitCode::FAILURE;
    }

    if cli.command == Command::Fmt {
        if cli.include_budgets.any() {
            eprintln!("carve fmt: include limits do not apply to formatting");
            return ExitCode::from(2);
        }
        return run_fmt(
            &cli.input_paths,
            cli.fmt_write,
            cli.fmt_check,
            cli.fmt_stamp,
            cli.fmt_migrate,
            cli.fmt_align_tables,
        );
    }

    if cli.command == Command::Flatten {
        if cli.options.align_tables {
            eprintln!("carve flatten: --align-tables applies only to --carve or fmt");
            return ExitCode::from(2);
        }
        // Flatten exists to read includes, so a switch that forbids that is
        // refused rather than ignored (carve-js rejects both flags too).
        if cli.no_includes {
            eprintln!("carve flatten: --safe and --no-includes leave nothing to flatten");
            return ExitCode::from(2);
        }
        return run_flatten(
            cli.input_paths.first().map(String::as_str),
            cli.include_root.as_deref(),
            cli.include_budgets,
        );
    }

    run_render(cli, &bundled)
}

fn cli_error(category: &str, detail: impl std::fmt::Display) -> ExitCode {
    eprintln!("carve[{category}]: {detail}");
    ExitCode::from(2)
}

fn parse_args<'a>(raw_args: Vec<String>) -> Result<CliArgs<'a>, ExitCode> {
    let mut cli = CliArgs {
        options: carve::Options::new(),
        format: OutputFormat::Html,
        command: Command::Render,
        fmt_write: false,
        fmt_check: false,
        fmt_align_tables: false,
        fmt_stamp: None,
        fmt_migrate: false,
        stamp_mode: None,
        enable_extensions: false,
        extension_keys: Vec::new(),
        tabs_mode: carve::TabsMode::Css,
        tabs_mode_set: false,
        citation_mode: carve::CitationMode::Numbered,
        citation_mode_set: false,
        selective_render_options: false,
        quote_locale: None,
        from_json: false,
        max_render_losses: carve::DEFAULT_MAX_RENDER_LOSSES,
        include_root: None,
        no_includes: false,
        include_budgets: IncludeBudgets::default(),
        input_paths: Vec::new(),
        reports: LossReporting {
            strict_losses: false,
            report_losses: None,
            report_includes: None,
            allowed_render_losses: Vec::new(),
        },
    };
    let mut args = raw_args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "fmt" if cli.command == Command::Render && cli.input_paths.is_empty() => {
                cli.command = Command::Fmt;
                cli.format = OutputFormat::Carve;
                cli.fmt_align_tables = cli.options.align_tables;
            }
            "flatten" if cli.command == Command::Render && cli.input_paths.is_empty() => {
                cli.command = Command::Flatten;
                cli.format = OutputFormat::Carve;
            }
            "-h" | "--help" => {
                print_usage();
                return Err(ExitCode::SUCCESS);
            }
            "-V" | "--version" => {
                println!("carve-rs {}", env!("CARGO_PKG_VERSION"));
                return Err(ExitCode::SUCCESS);
            }
            "-w" | "--write" if cli.command == Command::Fmt => cli.fmt_write = true,
            "--check" if cli.command == Command::Fmt => cli.fmt_check = true,
            "--align-tables" if cli.command == Command::Fmt => cli.fmt_align_tables = true,
            "--align-tables" => cli.options = cli.options.with_align_tables(true),
            "--migrate" if cli.command == Command::Fmt => cli.fmt_migrate = true,
            "--stamp" if cli.command == Command::Fmt => {
                cli.fmt_stamp = Some(carve::StampForm::Line);
            }
            "--stamp-block" if cli.command == Command::Fmt => {
                cli.fmt_stamp = Some(carve::StampForm::Block);
            }
            "--stamp-info" => cli.stamp_mode = Some(StampMode::Info),
            "--stamp-check" => cli.stamp_mode = Some(StampMode::Check),
            "--mention-url" => {
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --mention-url requires a template");
                    return Err(ExitCode::from(2));
                };
                cli.options = cli.options.with_mention_url(value);
            }
            "--tag-url" => {
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --tag-url requires a template");
                    return Err(ExitCode::from(2));
                };
                cli.options = cli.options.with_tag_url(value);
            }
            "--symbol" => {
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --symbol requires name=value");
                    return Err(ExitCode::from(2));
                };
                let Some((name, glyph)) = value.split_once('=') else {
                    eprintln!("carve[usage]: --symbol requires name=value");
                    return Err(ExitCode::from(2));
                };
                cli.options = cli.options.with_symbol(name, glyph);
            }
            "--profile" => {
                let Some(value) = args.next() else {
                    eprintln!(
                        "carve[usage]: --profile requires a name (full|article|comment|minimal)"
                    );
                    return Err(ExitCode::from(2));
                };
                cli.options = cli.options.with_profile(parse_profile(&value)?);
            }
            "--smart-typography" => {
                // The switch the spec documents as document-global
                // (divergence-from-djot section 12). Source mode is for
                // machine-facing output, which is exactly what a CLI pipes
                // into something else, so the flag belongs here rather than
                // only in the library API.
                let Some(value) = args.next() else {
                    eprintln!(
                        "carve[usage]: --smart-typography requires a mode (glyph|source|quotes-source)"
                    );
                    return Err(ExitCode::from(2));
                };
                cli.options.smart_typography = parse_smart_typography(&value)?;
            }
            "--quote-locale" => {
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --quote-locale requires a locale");
                    return Err(ExitCode::from(2));
                };
                cli.quote_locale = Some(value);
            }
            "--profile-base-host" => {
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --profile-base-host requires a host");
                    return Err(ExitCode::from(2));
                };
                cli.options = cli.options.with_profile_base_host(value);
            }
            "--roundtrip" => {
                // PART 10 §13. The target decides the carrier, so this is
                // accepted for HTML and Markdown and refused elsewhere.
                cli.options.roundtrip = true;
            }
            "--html" => cli.format = OutputFormat::Html,
            "--markdown" | "--md" => cli.format = OutputFormat::Markdown,
            "--plain" | "--plain-text" => cli.format = OutputFormat::Plain,
            "--ansi" => cli.format = OutputFormat::Ansi,
            "--carve" => cli.format = OutputFormat::Carve,
            "--json" | "--ast" => cli.format = OutputFormat::Json,
            "--from-json" => cli.from_json = true,
            "--strict-losses" => cli.reports.strict_losses = true,
            "--report-losses" => {
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --report-losses requires a file or -");
                    return Err(ExitCode::from(2));
                };
                cli.reports.report_losses = Some(value);
            }
            "--report-includes" => {
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --report-includes requires a file or -");
                    return Err(ExitCode::from(2));
                };
                cli.reports.report_includes = Some(value);
            }
            "--allow-loss" => {
                let code = parse_allowed_loss(args.next().as_deref())?;
                cli.reports.allowed_render_losses.push(code);
            }
            "--max-render-losses" => {
                let Some(value) = args.next().and_then(|value| value.parse::<usize>().ok()) else {
                    eprintln!("carve[usage]: --max-render-losses requires a non-negative integer");
                    return Err(ExitCode::from(2));
                };
                cli.max_render_losses = value;
            }
            "--static" => cli.options = cli.options.with_mode(carve::Mode::Static),
            "--interactive" => cli.options = cli.options.with_mode(carve::Mode::Interactive),
            "--extensions" => cli.enable_extensions = true,
            "--extension" => {
                cli.selective_render_options = true;
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --extension requires a registry key");
                    return Err(ExitCode::from(2));
                };
                if !carve::extensions::registry::keys().any(|key| key == value) {
                    let known = carve::extensions::registry::keys()
                        .collect::<Vec<_>>()
                        .join(", ");
                    eprintln!(
                        "carve[usage]: unknown extension: {value} (expected one of: {known})"
                    );
                    return Err(ExitCode::from(2));
                }
                if !cli.extension_keys.contains(&value) {
                    cli.extension_keys.push(value);
                }
            }
            "--tabs-mode" => {
                cli.selective_render_options = true;
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --tabs-mode requires css or aria");
                    return Err(ExitCode::from(2));
                };
                cli.tabs_mode = parse_tabs_mode(&value)?;
                cli.tabs_mode_set = true;
            }
            "--citation-mode" => {
                cli.selective_render_options = true;
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --citation-mode requires numbered or author-date");
                    return Err(ExitCode::from(2));
                };
                cli.citation_mode = parse_citation_mode(&value)?;
                cli.citation_mode_set = true;
            }
            "--no-sections" => {
                cli.selective_render_options = true;
                cli.options = cli.options.with_sections(false);
            }
            "--source-lines" => {
                cli.selective_render_options = true;
                cli.options = cli.options.with_source_lines(true);
            }
            "--include-root" => {
                let Some(value) = args.next() else {
                    eprintln!("carve[usage]: --include-root requires a directory");
                    return Err(ExitCode::from(2));
                };
                cli.include_root = Some(include_root_flag(value)?);
            }
            "--max-depth" | "--max-bytes" | "--max-resolver-calls" | "--max-warnings" => {
                let value = parse_budget(args.next().as_ref(), &arg)?;
                cli.include_budgets.set(&arg, value);
            }
            "--no-raw-html" => cli.options = cli.options.with_raw_html(false),
            // Untrusted input: the resolver MUST be off (includes, "The host
            // resolver"), so --safe also leaves every directive literal.
            "--safe" => {
                cli.options = cli.options.with_raw_html(false);
                cli.no_includes = true;
            }
            "--no-includes" => cli.no_includes = true,
            "-" if cli.command == Command::Render => cli.input_paths.clear(),
            "-" if cli.command == Command::Fmt => cli.input_paths.push(arg),
            path if path.starts_with('-') => {
                return Err(cli_error("usage", format!("unknown option: {path}")));
            }
            path => {
                if cli.command == Command::Render && !cli.input_paths.is_empty() {
                    return Err(cli_error("usage", "multiple input files specified"));
                }
                cli.input_paths.push(path.to_string());
            }
        }
    }
    Ok(cli)
}

fn parse_profile(value: &str) -> Result<carve::Profile, ExitCode> {
    match value {
        "full" => Ok(carve::Profile::full()),
        "article" => Ok(carve::Profile::article()),
        "comment" => Ok(carve::Profile::comment()),
        "minimal" => Ok(carve::Profile::minimal()),
        other => {
            eprintln!(
                "carve[usage]: unknown profile: {other} (expected full|article|comment|minimal)"
            );
            Err(ExitCode::from(2))
        }
    }
}

fn parse_smart_typography(value: &str) -> Result<carve::SmartTypographyMode, ExitCode> {
    match value {
        "glyph" => Ok(carve::SmartTypographyMode::Glyph),
        "source" => Ok(carve::SmartTypographyMode::Source),
        "quotes-source" => Ok(carve::SmartTypographyMode::QuotesSource),
        other => {
            eprintln!(
                "carve[usage]: unknown smart typography mode: {other} (expected glyph|source|quotes-source)"
            );
            Err(ExitCode::from(2))
        }
    }
}

fn parse_allowed_loss(value: Option<&str>) -> Result<&'static str, ExitCode> {
    match value {
        Some("raw-format-dropped") => Ok("raw-format-dropped"),
        Some("ruby-flattened") => Ok("ruby-flattened"),
        Some("editorial-comment-flattened") => Ok("editorial-comment-flattened"),
        Some("carrier-attribute-dropped") => Ok("carrier-attribute-dropped"),
        Some("definition-list-flattened") => Ok("definition-list-flattened"),
        Some("table-span-flattened") => Ok("table-span-flattened"),
        Some("table-header-row-demoted") => Ok("table-header-row-demoted"),
        Some("table-header-row-hoisted") => Ok("table-header-row-hoisted"),
        Some("table-header-cell-demoted") => Ok("table-header-cell-demoted"),
        Some("list-style-dropped") => Ok("list-style-dropped"),
        Some("list-tightness-lost") => Ok("list-tightness-lost"),
        Some("list-delimiter-changed") => Ok("list-delimiter-changed"),
        Some("markdown-container-flattened") => Ok("markdown-container-flattened"),
        Some("markdown-caption-flattened") => Ok("markdown-caption-flattened"),
        Some("markdown-attributes-dropped") => Ok("markdown-attributes-dropped"),
        Some("fence-title-dropped") => Ok("fence-title-dropped"),
        Some("raw-html-escaped") => Ok("raw-html-escaped"),
        _ => {
            eprintln!("carve[usage]: --allow-loss expects raw-format-dropped, ruby-flattened, editorial-comment-flattened, carrier-attribute-dropped, definition-list-flattened, list-style-dropped, list-tightness-lost, list-delimiter-changed, table-span-flattened, table-header-row-demoted, table-header-row-hoisted or table-header-cell-demoted");
            Err(ExitCode::from(2))
        }
    }
}

fn parse_tabs_mode(value: &str) -> Result<carve::TabsMode, ExitCode> {
    match value {
        "css" => Ok(carve::TabsMode::Css),
        "aria" => Ok(carve::TabsMode::Aria),
        other => {
            eprintln!("carve[usage]: unknown tabs mode: {other} (expected css|aria)");
            Err(ExitCode::from(2))
        }
    }
}

fn parse_citation_mode(value: &str) -> Result<carve::CitationMode, ExitCode> {
    match value {
        "numbered" => Ok(carve::CitationMode::Numbered),
        "author-date" => Ok(carve::CitationMode::AuthorDate),
        other => {
            eprintln!(
                "carve[usage]: unknown citation mode: {other} (expected numbered|author-date)"
            );
            Err(ExitCode::from(2))
        }
    }
}

fn include_root_flag(value: String) -> Result<String, ExitCode> {
    // REFUSED, not ignored: a build without the resolver cannot
    // honour the flag, and silently rendering the directive as
    // literal text would look like the include simply failed.
    #[cfg(not(feature = "fs"))]
    {
        let _ = value;
        eprintln!(
            "carve[usage]: --include-root needs the `fs` feature, which this build does not have"
        );
        Err(ExitCode::from(2))
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
        Ok(absolute.unwrap_or(value))
    }
}

fn run_render<'a>(mut cli: CliArgs<'a>, bundled: &'a BundledExtensions) -> ExitCode {
    if cli.tabs_mode_set && !cli.extension_keys.iter().any(|key| key == "tabs") {
        eprintln!("carve: --tabs-mode requires --extension tabs");
        return ExitCode::FAILURE;
    }
    if cli.citation_mode_set && !cli.extension_keys.iter().any(|key| key == "citations") {
        eprintln!("carve: --citation-mode requires --extension citations");
        return ExitCode::FAILURE;
    }
    if cli.options.roundtrip && !matches!(cli.format, OutputFormat::Html | OutputFormat::Markdown) {
        eprintln!("carve: --roundtrip requires --html or --markdown");
        return ExitCode::FAILURE;
    }
    if cli.options.align_tables && cli.format != OutputFormat::Carve {
        eprintln!("carve: --align-tables requires --carve");
        return ExitCode::from(2);
    }
    if cli.enable_extensions {
        cli.options = bundled.register(cli.options);
    }
    let named_extensions = named_extensions(&cli, bundled);
    let mut options: carve::Options<'_> = cli.options;
    for extension in &named_extensions {
        options = options.with_extension(extension.as_ref());
    }
    let smart_quotes = cli.quote_locale.as_deref().map(carve::SmartQuotes::new);
    if let Some(extension) = &smart_quotes {
        options = options.with_extension(extension);
    }

    let source = match read_source(cli.input_paths.first().map(String::as_str)) {
        Ok(source) => source,
        Err(code) => return code,
    };
    if let Some(mode) = cli.stamp_mode {
        return run_stamp_query(mode, &source, cli.include_budgets);
    }

    let format = cli.format;
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
    let input_path = cli
        .input_paths
        .first()
        .filter(|p| p.as_str() != "-")
        .map(|p| {
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
    let root = cli.include_root.clone().or_else(|| {
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
    let want_includes = !cli.no_includes
        && root.is_some()
        && carve::expands_for_target(include_target)
        && (cli.include_root.is_some() || source.contains("{{"));
    let has_include_root = root.is_some();
    let resolver = match include_resolver(want_includes, root, cli.include_root.is_some()) {
        Ok(resolver) => resolver,
        Err(code) => return code,
    };

    if cli.include_budgets.any()
        && (cli.from_json
            || cli.no_includes
            || !carve::expands_for_target(include_target)
            || !has_include_root
            || !cfg!(feature = "fs")
            || (want_includes && resolver.is_none()))
    {
        eprintln!("carve: include limits require an available include resolver");
        return ExitCode::from(2);
    }

    let checked_options = carve::CheckedRenderOptions {
        strict: false,
        max_losses: cli.max_render_losses,
    };
    // Loss diagnostics promise source positions. Position tracking does not
    // alter rendered output, and the CLI is itself a reporting surface.
    options = options.with_positions(true);
    let outcome = if cli.from_json {
        render_from_json(&source, format, target, checked_options, &options)
    } else if let Some(resolver) = &resolver {
        let mut include_options = cli
            .include_budgets
            .apply(carve::IncludeOptions::new().with_resolver(resolver));
        if let Some(path) = &input_path {
            include_options = include_options.with_source_path(path.to_string_lossy().into_owned());
        }
        render_with_includes(
            &source,
            format,
            target,
            checked_options,
            &options,
            &include_options,
        )
    } else {
        render_source(&source, format, target, checked_options, options)
    };
    match outcome {
        Ok(outcome) => finish_render(outcome, &cli.input_paths, cli.reports),
        Err(code) => code,
    }
}

fn named_extensions(
    cli: &CliArgs<'_>,
    bundled: &BundledExtensions,
) -> Vec<Box<dyn carve::CarveExtension>> {
    cli.extension_keys
        .iter()
        .map(|key| match key.as_str() {
            "tabs" => Box::new(carve::Tabs::with_options(carve::TabsOptions {
                mode: cli.tabs_mode,
                ..carve::TabsOptions::default()
            })) as Box<dyn carve::CarveExtension>,
            "citations" => Box::new(carve::Citations::with_mode(cli.citation_mode)),
            _ => carve::extensions::registry::by_key(key)
                .expect("extension keys were validated while parsing arguments"),
        })
        .filter(|extension| !cli.enable_extensions || !bundled.provides(extension.name()))
        .filter(|extension| cli.quote_locale.is_none() || extension.name() != "smart-quotes")
        .collect()
}

fn read_source(input: Option<&str>) -> Result<String, ExitCode> {
    match input {
        None | Some("-") => {
            let mut buf = String::new();
            if let Err(err) = read_stdin_lossy(&mut buf) {
                return Err(cli_error(
                    "input-unreadable",
                    format!("cannot read stdin: {err}"),
                ));
            }
            Ok(buf)
        }
        Some(path) => match read_path_lossy(path) {
            Ok(s) => Ok(s),
            Err(err) => Err(cli_error(
                "input-unreadable",
                format!("cannot read {path}: {err}"),
            )),
        },
    }
}

fn run_stamp_query(mode: StampMode, source: &str, include_budgets: IncludeBudgets) -> ExitCode {
    if include_budgets.any() {
        eprintln!("carve: include limits do not apply to stamp queries");
        return ExitCode::from(2);
    }
    match carve::read_stamp(source) {
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

    if mode == StampMode::Check && carve::needs_review(source, carve::SPEC_VERSION) {
        eprintln!(
            "Review the [behavior] changelog entries between that version and {}.",
            carve::SPEC_VERSION
        );
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

// The CLI's include path IS the filesystem resolver, so it compiles out with
// it. Without the feature the directive stays literal, which is the core
// behavior, not a degraded one.
#[cfg(feature = "fs")]
fn include_resolver(
    want_includes: bool,
    root: Option<String>,
    explicit_root: bool,
) -> Result<Option<CliResolver>, ExitCode> {
    if !want_includes {
        return Ok(None);
    }
    let root = root.expect("guarded by want_includes");
    match carve::FileSystemResolver::new(&root) {
        Ok(resolver) => Ok(Some(resolver)),
        Err(err) => {
            // An explicit root is a user request, so a bad one is fatal; an
            // inferred one silently falls back to no includes rather than
            // failing a render the user never asked to change.
            if explicit_root {
                eprintln!("carve: cannot use include root {root}: {err}");
                return Err(ExitCode::FAILURE);
            }
            Ok(None)
        }
    }
}

#[cfg(not(feature = "fs"))]
fn include_resolver(
    want_includes: bool,
    root: Option<String>,
    explicit_root: bool,
) -> Result<Option<CliResolver>, ExitCode> {
    let _ = (want_includes, root, explicit_root);
    Ok(None)
}

fn render_from_json(
    source: &str,
    format: OutputFormat,
    target: carve::RenderTarget,
    checked_options: carve::CheckedRenderOptions,
    options: &carve::Options<'_>,
) -> Result<RenderOutcome, ExitCode> {
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
            return Err(ExitCode::FAILURE);
        }
    }
    let doc = match carve::from_json(source) {
        Ok(doc) => doc,
        Err(err) => {
            return Err(cli_error(
                "ast-json-invalid",
                format!("cannot decode JSON AST: {err}"),
            ));
        }
    };
    let checked = carve::with_render_loss_report(target, checked_options, || {
        render_document(doc, format, options)
    })
    .expect("non-strict collection cannot fail");
    RenderOutcome::from_checked(checked)
}

fn render_with_includes(
    source: &str,
    format: OutputFormat,
    target: carve::RenderTarget,
    checked_options: carve::CheckedRenderOptions,
    options: &carve::Options<'_>,
    include_options: &carve::IncludeOptions<'_>,
) -> Result<RenderOutcome, ExitCode> {
    // INCLUDES TAKE THE DOCUMENT PATH, not the source facades below: the
    // pass works on a parsed tree, which is also what carries the file
    // identity each included node keeps.
    let target_is_html = matches!(format, OutputFormat::Html);
    let prepared = match carve::prepare_doc_with_includes(
        source,
        options,
        include_options,
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
            return Err(ExitCode::FAILURE);
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
        render_document(prepared.doc.clone(), format, options)
    })
    .expect("non-strict collection cannot fail");
    let mut outcome = RenderOutcome::from_checked(checked)?;
    outcome.include_dependencies = prepared.dependencies;
    Ok(outcome)
}

fn render_source(
    source: &str,
    format: OutputFormat,
    target: carve::RenderTarget,
    checked_options: carve::CheckedRenderOptions,
    mut options: carve::Options<'_>,
) -> Result<RenderOutcome, ExitCode> {
    let checked = carve::with_render_loss_report(target, checked_options, || match format {
        OutputFormat::Html => {
            carve::try_to_html_with_options(source, &options).map_err(carve::CarveWriteError::from)
        }
        // Positions ON for the three targets that PRINT the footnote
        // definitions: §7 orders them by source position, and the map they
        // come from is a BTreeMap, so without spans they print in label
        // order (carve-rs#686). `--json` below asks for the same thing.
        OutputFormat::Markdown => {
            options = options.with_positions(true);
            carve::try_to_markdown_with_options(source, &options)
                .map_err(carve::CarveWriteError::from)
        }
        OutputFormat::Plain => {
            options = options.with_positions(true);
            carve::try_to_plain_text_with_options(source, &options)
                .map_err(carve::CarveWriteError::from)
        }
        OutputFormat::Ansi => {
            options = options.with_positions(true);
            carve::try_to_ansi_with_options(source, &options).map_err(carve::CarveWriteError::from)
        }
        // The options-taking sibling. `to_carve` carries no `Options`, so
        // the profile was never even asked about on this target: an
        // over-cap document was re-serialized in full at exit 0, and a raw
        // HTML block a `minimal` profile removes from every other output
        // came straight back out (carve-rs#1191).
        OutputFormat::Carve => carve::try_to_carve_with_options(source, &options),
        OutputFormat::Json => {
            options = options.with_positions(true);
            carve::try_to_json_with_options(source, &options)
                .map(add_ast_version)
                .map_err(carve::CarveWriteError::from)
        }
    })
    .expect("non-strict collection cannot fail");
    RenderOutcome::from_checked(checked)
}

fn finish_render(
    mut outcome: RenderOutcome,
    input_paths: &[String],
    reports: LossReporting,
) -> ExitCode {
    if outcome.total_losses > 0
        && outcome
            .totals_by_code
            .keys()
            .all(|code| reports.allowed_render_losses.contains(code))
    {
        outcome.losses.clear();
        outcome.total_losses = 0;
        outcome.truncated = false;
    }
    let total_losses = outcome.total_losses;
    let file = input_paths.first().map(String::as_str).unwrap_or("<stdin>");
    if total_losses > 0 {
        for loss in &outcome.losses {
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
    if let Some(path) = reports.report_losses {
        let json = render_loss_json(&outcome.losses, total_losses, outcome.truncated);
        if path == "-" {
            eprintln!("{json}");
        } else if let Err(error) = std::fs::write(&path, format!("{json}\n")) {
            eprintln!("carve: cannot write render-loss report {path}: {error}");
            return ExitCode::from(2);
        }
    }
    if let Some(path) = reports.report_includes {
        let json = include_dependency_json(&outcome.include_dependencies);
        if path == "-" {
            eprintln!("{json}");
        } else if let Err(error) = std::fs::write(&path, format!("{json}\n")) {
            eprintln!("carve: cannot write include-dependency report {path}: {error}");
            return ExitCode::from(2);
        }
    }
    if reports.strict_losses && total_losses > 0 {
        return ExitCode::FAILURE;
    }
    let output = outcome.output;
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
    let mut mode_set = false;
    let mut adapter_set = false;
    let mut report_path: Option<&str> = None;
    let mut check_loss = false;
    let mut list_table = false;
    let mut input: Option<&str> = None;
    let mut from: Option<String> = None;
    let mut directory: Option<String> = None;
    let mut output: Option<String> = None;
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
                mode_set = true;
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
                adapter_set = true;
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
                if report_path.is_none() {
                    eprintln!("carve migrate: --report requires a value");
                    return ExitCode::from(2);
                }
            }
            "--directory" => {
                i += 1;
                directory = args.get(i).cloned();
                if directory.is_none() {
                    eprintln!("carve migrate: --directory requires a value");
                    return ExitCode::from(2);
                }
            }
            "--output" => {
                i += 1;
                output = args.get(i).cloned();
                if output.is_none() {
                    eprintln!("carve migrate: --output requires a value");
                    return ExitCode::from(2);
                }
            }
            "--check-loss" => check_loss = true,
            "--list-table" => list_table = true,
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
            return cli_error("usage", format!("migrate: unknown source format {other}"));
        }
        None => {
            eprintln!("carve migrate: --from html, markdown, djot or bbcode is required");
            return ExitCode::from(2);
        }
    };
    if directory.is_some() || output.is_some() {
        if !matches!(from.as_str(), "markdown" | "md")
            || directory.is_none()
            || output.is_none()
            || input.is_some()
            || report_path.is_some()
            || list_table
            || mode_set
            || adapter_set
        {
            eprintln!("carve migrate: folder mode requires --from markdown --directory SOURCE --output TARGET; single-file options are not allowed");
            return ExitCode::from(2);
        }
        return match migrate_directory::run(
            directory.as_deref().unwrap(),
            output.as_deref().unwrap(),
            check_loss,
        ) {
            Ok(code) => ExitCode::from(code),
            Err(error) => {
                eprintln!("carve migrate: {error}");
                ExitCode::from(2)
            }
        };
    }
    let source = match input {
        None | Some("-") => {
            let mut s = String::new();
            if let Err(err) = read_stdin_lossy(&mut s) {
                return cli_error(
                    "input-unreadable",
                    format!("migrate: cannot read stdin: {err}"),
                );
            }
            s
        }
        Some(path) => match read_path_lossy(path) {
            Ok(s) => s,
            Err(e) => {
                return cli_error(
                    "input-unreadable",
                    format!("migrate: cannot read {path}: {e}"),
                );
            }
        },
    };
    let result = match from.as_str() {
        "html" => match carve::migrate_html(
            &source,
            &carve::HtmlImportOptions {
                mode,
                adapter,
                list_table_for_block_cells: list_table,
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
        _ => match carve::try_migrate_markdown(&source) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("carve migrate: {error}");
                return ExitCode::from(2);
            }
        },
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
        let source = match read_path_lossy(path) {
            Ok(source) => source,
            Err(error) => {
                return cli_error(
                    "input-unreadable",
                    format!("merge: cannot read {path}: {error}"),
                );
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
        let attribute_field = loss.attribute.as_ref().map(|attribute| format!(",\"attribute\":{}", json_string(attribute))).unwrap_or_default();
        let style_field = loss.style.map(|style| format!(",\"style\":{}", json_string(style))).unwrap_or_default();
        let delim_field = loss.delim.map(|delim| format!(",\"delim\":{}", json_string(&delim.to_string()))).unwrap_or_default();
        let emitted_field = loss.emitted_delim.map(|delim| format!(",\"emittedDelim\":{}", json_string(&delim.to_string()))).unwrap_or_default();
        let colspan_field = loss.colspan.map(|count| format!(",\"colspan\":{count}")).unwrap_or_default();
        let rowspan_field = loss.rowspan.map(|count| format!(",\"rowspan\":{count}")).unwrap_or_default();
        let format_field = loss.format.as_ref().map(|format| format!(",\"format\":{}", json_string(format))).unwrap_or_default();
        format!(
            "{{\"code\":{}{format_field}{attribute_field}{style_field}{delim_field}{emitted_field}{colspan_field}{rowspan_field},\"target\":{},\"nodeType\":{},\"message\":{}{pos}}}",
            json_string(loss.code), json_string(loss.target.as_str()),
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

impl From<carve::CarveWriteError> for RenderError {
    fn from(err: carve::CarveWriteError) -> Self {
        match err {
            carve::CarveWriteError::Profile(err) => RenderError::Profile(err),
            carve::CarveWriteError::Render(err) => RenderError::Carve(err),
        }
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
        OutputFormat::Carve => carve::render_carve_with_options(
            &doc,
            &carve::CarveWriterOptions::default().with_align_tables(options.align_tables),
        )?,
        OutputFormat::Json => add_ast_version(carve::to_json(&doc)),
    })
}

/// The bare tree `--json` prints carries the contract version on its root (CARVE-P12-056).
fn add_ast_version(json: String) -> String {
    let trimmed = json.trim_end();
    let Some(body) = trimmed.strip_suffix('}') else {
        return json;
    };
    format!(
        "{body},\"astVersion\":\"{}\"}}{}",
        carve::AST_CONTRACT_VERSION,
        &json[trimmed.len()..]
    )
}

fn run_region(args: &[String]) -> ExitCode {
    let mut mode = carve::IncludeRegionMode::Carve;
    let mut operands = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--code" => mode = carve::IncludeRegionMode::Code,
            "--help" | "-h" => {
                println!("Usage: carve region [--code] NAME [file]\nPrint the region's 1-based inclusive content lines as JSON.");
                return ExitCode::SUCCESS;
            }
            value if value.starts_with('-') && value != "-" => {
                eprintln!("carve region: unknown option: {value}");
                return ExitCode::from(2);
            }
            _ => operands.push(arg.as_str()),
        }
    }
    if operands.is_empty() || operands.len() > 2 {
        eprintln!("carve region: expected NAME [file]");
        return ExitCode::from(2);
    }
    let mut source = String::new();
    match operands.get(1).copied().unwrap_or("-") {
        "-" => {
            if let Err(err) = read_stdin_lossy(&mut source) {
                return cli_error(
                    "input-unreadable",
                    format!("region: cannot read stdin: {err}"),
                );
            }
        }
        path => match read_path_lossy(path) {
            Ok(value) => source = value,
            Err(err) => {
                return cli_error(
                    "input-unreadable",
                    format!("region: cannot read {path}: {err}"),
                );
            }
        },
    }
    match carve::select_include_region(&source, operands[0], mode) {
        Ok(region) => {
            println!(
                "{}",
                serde_json::json!({"start": region.start, "end": region.end})
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("carve region: {}", err.rule());
            ExitCode::from(1)
        }
    }
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
    let mut enable_citations = false;
    let mut include_root: Option<String> = None;
    let mut json = false;
    let mut budgets = IncludeBudgets::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--json" => json = true,
            "--max-depth" | "--max-bytes" | "--max-resolver-calls" | "--max-warnings" => {
                index += 1;
                let value = match parse_budget(args.get(index), arg) {
                    Ok(value) => value,
                    Err(code) => return code,
                };
                match arg.as_str() {
                    "--max-depth" => budgets.depth = Some(value),
                    "--max-bytes" => budgets.bytes = Some(value),
                    "--max-resolver-calls" => budgets.resolver_calls = Some(value),
                    _ => budgets.warnings = Some(value),
                }
            }
            "--extensions" => enable_extensions = true,
            "--include-root" => {
                index += 1;
                match args.get(index) {
                    Some(root) => include_root = Some(root.clone()),
                    None => {
                        eprintln!("carve lint: --include-root requires a directory");
                        return ExitCode::from(2);
                    }
                }
            }
            other if other.starts_with("--include-root=") => {
                include_root = Some(other[15..].to_string())
            }
            "--extension" => {
                index += 1;
                match args.get(index).map(String::as_str) {
                    Some("citations") => enable_citations = true,
                    Some(other) => {
                        eprintln!("carve lint: unknown extension: {other} (known: citations)");
                        return ExitCode::from(2);
                    }
                    None => {
                        eprintln!("carve lint: --extension requires a name");
                        return ExitCode::from(2);
                    }
                }
            }
            other if other.starts_with("--extension=") => {
                if other == "--extension=citations" {
                    enable_citations = true;
                } else {
                    eprintln!(
                        "carve lint: unknown extension: {} (known: citations)",
                        &other["--extension=".len()..]
                    );
                    return ExitCode::from(2);
                }
            }
            "-h" | "--help" => {
                println!(
                    "carve lint - report constructs that parse but do not reach the page\n\n\
                     Usage:\n  \
                     carve lint [--json] [--extensions] [--extension citations] [--include-root DIR] [files]\n\n\
                     Reads stdin when no file is given, or when the file is `-`.\n\n\
                     Options:\n  \
                     --extensions           enable the bundled extensions\n  \
                     --json                 print one JSON warning per line\n  \
                     --extension citations  enable citation placement lint\n  \
                     --include-root DIR     expand and lint includes within DIR;\n                         \
                     without it no include target is read\n  \
                     --max-depth N          include depth limit\n  \
                     --max-bytes N          expanded include byte limit\n  \
                     --max-resolver-calls N resolver call limit\n  \
                     --max-warnings N       retained include warning limit\n\n\
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
                eprintln!("carve lint: unknown option: {other}");
                return ExitCode::from(2);
            }
            path => paths.push(path),
        }
        index += 1;
    }
    if include_root.is_none()
        && (budgets.depth.is_some()
            || budgets.bytes.is_some()
            || budgets.resolver_calls.is_some()
            || budgets.warnings.is_some())
    {
        eprintln!("carve lint: include limits require --include-root");
        return ExitCode::from(2);
    }
    run_lint_paths(
        &paths,
        enable_extensions,
        enable_citations,
        include_root.as_deref(),
        json,
        budgets,
    )
}

fn run_lint_paths(
    paths: &[&str],
    enable_extensions: bool,
    enable_citations: bool,
    include_root: Option<&str>,
    json: bool,
    budgets: IncludeBudgets,
) -> ExitCode {
    // Only `options.extensions` is read by the linter - `lint_carve_with_options`
    // documents that - so nothing else from the render path is plumbed through
    // here. Owned locally so they outlive the borrow in `options`.
    let details = carve::Details::new();
    let spoiler = carve::Spoiler::new();
    let code_callouts = carve::CodeCallouts::new();
    let color_swatch = carve::ColorSwatch::new();
    let fenced_presets = carve::FencedRender::presets();
    let math_block = carve::MathBlock::new();
    let citations = carve::Citations::new();
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
    if enable_citations {
        options = options.with_extension(&citations);
    }

    #[cfg(feature = "fs")]
    let resolver = match include_root.map(|root| {
        let absolute = if std::path::Path::new(root).is_absolute() {
            std::path::PathBuf::from(root)
        } else {
            std::env::current_dir()?.join(root)
        };
        carve::FileSystemResolver::new(absolute)
    }) {
        Some(Ok(resolver)) => Some(resolver),
        Some(Err(err)) => {
            eprintln!(
                "carve lint: cannot use include root {}: {err}",
                include_root.unwrap_or_default()
            );
            return ExitCode::from(2);
        }
        None => None,
    };
    #[cfg(not(feature = "fs"))]
    let resolver: Option<NoResolver> = {
        if include_root.is_some() {
            eprintln!(
                "carve lint: cannot use include root {}",
                include_root.unwrap_or_default()
            );
            return ExitCode::from(2);
        }
        None
    };
    let root_path = include_root.and_then(|root| std::fs::canonicalize(root).ok());
    let display = |file: &str| -> String {
        root_path
            .as_ref()
            .and_then(|root| std::path::Path::new(file).strip_prefix(root).ok())
            .map_or_else(
                || file.to_string(),
                |path| path.to_string_lossy().replace('\\', "/"),
            )
    };
    let mut findings = 0usize;
    let mut unreadable = false;

    let mut lint_source = |source: &str, label: &str| {
        let mut lint_options = carve::Options::new();
        lint_options.extensions = options.extensions.clone();
        if let Some(resolver) = resolver.as_ref() {
            lint_options.lint_includes = Some(
                budgets
                    .apply(carve::IncludeOptions::new())
                    .with_resolver(resolver)
                    .with_source_path(
                        std::fs::canonicalize(label)
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_else(|_| label.to_string()),
                    )
                    .collecting_included(),
            );
        }
        for warning in carve::lint_carve_with_options(source, &lint_options) {
            let file = warning
                .file
                .as_deref()
                .map_or_else(|| label.to_string(), display);
            let chain = if warning.included_by.is_empty() {
                String::new()
            } else {
                format!(
                    " (included from {})",
                    warning
                        .included_by
                        .iter()
                        .enumerate()
                        .map(|(index, site)| {
                            let site_file = if index == 0 {
                                label.to_string()
                            } else {
                                display(&site.file)
                            };
                            format!("{site_file}:{}:{}", site.line, site.column)
                        })
                        .collect::<Vec<_>>()
                        .join(" > ")
                )
            };
            if json {
                let included_by = warning.included_by.iter().enumerate().map(|(index, site)| {
                    serde_json::json!({"file": if index == 0 { label.to_string() } else { display(&site.file) }, "line": site.line, "column": site.column})
                }).collect::<Vec<_>>();
                println!(
                    "{}",
                    serde_json::json!({"file": file, "includedBy": included_by,
                    "line": warning.line, "column": warning.column, "rule": warning.rule,
                    "message": warning.message, "start": warning.start, "end": warning.end})
                );
            } else {
                println!(
                    "{file}:{}:{} {} — {}{chain}",
                    warning.line, warning.column, warning.rule, warning.message
                );
            }
            findings += 1;
        }
    };

    if paths.is_empty() || paths == ["-"] {
        let mut buf = String::new();
        if let Err(err) = read_stdin_lossy(&mut buf) {
            return cli_error(
                "input-unreadable",
                format!("lint: cannot read stdin: {err}"),
            );
        }
        lint_source(&buf, "<stdin>");
    } else {
        for path in paths {
            if *path == "-" {
                let mut buf = String::new();
                if let Err(err) = read_stdin_lossy(&mut buf) {
                    cli_error(
                        "input-unreadable",
                        format!("lint: cannot read stdin: {err}"),
                    );
                    unreadable = true;
                    continue;
                }
                lint_source(&buf, "<stdin>");
                continue;
            }
            match read_path_lossy(path) {
                Ok(source) => lint_source(&source, path),
                Err(err) => {
                    // Reported and skipped rather than fatal, so one bad path in
                    // a glob still lets every other document be checked - the
                    // whole point of running this over a tree.
                    cli_error(
                        "input-unreadable",
                        format!("lint: cannot read {path}: {err}"),
                    );
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
fn run_flatten(
    path: Option<&str>,
    include_root: Option<&str>,
    budgets: IncludeBudgets,
) -> ExitCode {
    let (source, source_path) = match path {
        Some(path) if path != "-" => match read_path_lossy(path) {
            Ok(source) => (source, Some(path.to_string())),
            Err(err) => {
                return cli_error(
                    "input-unreadable",
                    format!("flatten: cannot read {path}: {err}"),
                );
            }
        },
        _ => {
            let mut source = String::new();
            if let Err(err) = read_stdin_lossy(&mut source) {
                return cli_error(
                    "input-unreadable",
                    format!("flatten: cannot read stdin: {err}"),
                );
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

    let mut options = budgets.apply(carve::IncludeOptions::new().with_resolver(&resolver));
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
fn run_flatten(
    _path: Option<&str>,
    _include_root: Option<&str>,
    _budgets: IncludeBudgets,
) -> ExitCode {
    eprintln!("carve flatten: needs the `fs` feature, which this build does not have");
    ExitCode::FAILURE
}

fn run_fmt(
    paths: &[String],
    write: bool,
    check: bool,
    stamp: Option<carve::StampForm>,
    migrate: bool,
    align_tables: bool,
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
        if let Err(err) = read_stdin_lossy(&mut source) {
            return cli_error("input-unreadable", format!("fmt: cannot read stdin: {err}"));
        }
        return match format_carve(&source, stamp, migrate, align_tables) {
            Ok(formatted) => write_stdout(&formatted),
            Err(err) => {
                eprintln!("carve fmt: {err}");
                ExitCode::FAILURE
            }
        };
    }

    let mut changed = Vec::new();
    let mut stdout = String::new();
    for path in paths {
        if path == "-" {
            eprintln!("carve fmt: stdin cannot be mixed with file paths");
            return ExitCode::FAILURE;
        }
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) => {
                return cli_error(
                    "input-unreadable",
                    format!("fmt: cannot read {path}: {err}"),
                );
            }
        };
        // Printing replaces ill-formed input like every other read; --write and
        // --check refuse it so a file is never rewritten with U+FFFD.
        let source = match String::from_utf8(bytes) {
            Ok(source) => source,
            Err(_) if write || check => {
                return cli_error(
                    "input-unreadable",
                    format!("fmt: cannot read {path}: stream did not contain valid UTF-8"),
                );
            }
            Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
        };
        let formatted = match format_carve(&source, stamp, migrate, align_tables) {
            Ok(formatted) => formatted,
            Err(err) => {
                eprintln!("carve fmt: {path}: {err}");
                return ExitCode::FAILURE;
            }
        };
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

/// FALLIBLE, because the writer can refuse (carve-rs#2326). `to_carve` leaves
/// the source as authored when it cannot spell the tree, which is the right
/// answer for a library caller formatting in place and the wrong one here: a
/// formatter that silently emits its input looks like a document already in
/// canonical form. `fmt` says so and exits non-zero instead.
fn format_carve(
    source: &str,
    stamp: Option<carve::StampForm>,
    migrate: bool,
    align_tables: bool,
) -> Result<String, carve::CarveWriteError> {
    let options = carve::Options::default().with_align_tables(align_tables);
    let formatted = if migrate {
        carve::try_to_carve_with_options(
            &carve::migrate_case_only_references(source, &options),
            &options,
        )?
    } else {
        carve::try_to_carve_with_options(source, &options)?
    };
    Ok(match stamp {
        Some(form) => {
            let generated_by = format!("carve-rs {}", env!("CARGO_PKG_VERSION"));
            carve::stamp_carve(&formatted, &generated_by, form)
        }
        None => formatted,
    })
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
         carve lint [--json] [--include-root DIR] [files]\n                              \
         report constructs that render wrong\n                              \
         (exit 1 on findings, 2 if a file cannot be read)\n  \
         carve region [--code] NAME [file]\n                              \
         find a named include region (JSON start/end lines)\n  \
         carve merge [--json] BASE OURS THEIRS\n                              \
         merge independent structural edits\n  \
         carve migrate --from FORMAT [options] [file]\n                              \
         convert html, markdown (md), djot or bbcode to Carve.\n                              \
         Folder: --from markdown --directory SOURCE --output TARGET.\n                              \
         Folder mode checks local links and copies assets.\n                              \
         --mode/--adapter/--list-table apply to html (--list-table\n                              \
         writes a table whose cells hold blocks as ::: list-table);\n                              \
         --report/--check-loss\n                              \
         apply to all importers and fail closed when fidelity is unknown\n                              \
         (exit 1 when --check-loss finds loss, unresolved links\n                              \
         or skipped entries; 2 on a\n                              \
         usage error or an unreadable file)\n  \
         carve -h                    show this help\n  \
         carve -V, --version         print the version\n\n\
         Output format (default --html; last one wins):\n  \
         --html                      HTML\n  \
         --markdown, --md            Markdown\n  \
         --roundtrip                 add preservation carriers to HTML or Markdown\n  \
         --plain, --plain-text       plain text\n  \
         --ansi                      ANSI-colored terminal text\n  \
         --carve                     canonical Carve source\n\n  \
         --json, --ast               the parsed AST as JSON\n  \
         --from-json                 read an encoded AST instead of Carve source\n\n\
         Format options:\n  \
         -w, --write                 write formatted output in place\n  \
         --check                     fail if any file is not formatted\n  \
         --align-tables              align table columns in Carve source\n  \
         --migrate                   rewrite a reference that misses its only\n                              \
         target by case to the target's exact spelling\n\n  \
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
         --no-raw-html               escape =html raw blocks/spans instead of\n                              \
         emitting them\n  \
         --safe                      for untrusted input: --no-raw-html and\n                              \
         --no-includes together\n  \
         --max-depth N               include depth limit\n  \
         --max-bytes N               expanded include byte limit\n  \
         --max-resolver-calls N      resolver call limit\n  \
         --max-warnings N            retained include warning limit\n  \
         --profile NAME              restrict features (full|article|comment|minimal)\n  \
         --profile-base-host HOST    base host for the profile link policy\n  \
         --smart-typography MODE     glyph (default), source (all source runs),\n                              \
         quotes-source (quotes only); every presentation target\n  \
         --quote-locale LOCALE       use locale-specific opening/closing quotes\n\n  \
         --strict-losses             refuse output when rendering loses content or structure\n  \
         --report-losses FILE        write JSON loss report (`-` for stderr)\n  \
         --allow-loss CODE           accept raw-format-dropped, ruby-flattened,\n                              \
         editorial-comment-flattened, carrier-attribute-dropped\n                              \
         definition-list-flattened, list-style-dropped,\n                              \
         list-tightness-lost,\n                              \
         list-delimiter-changed, table-span-flattened,\n                              \
         table-header-row-demoted, table-header-row-hoisted\n                              \
         table-header-cell-demoted, markdown-container-flattened,\n                              \
         markdown-caption-flattened, markdown-attributes-dropped,\n                              \
         fence-title-dropped\n                              \
         or raw-html-escaped\n  \
         --max-render-losses N       bound detailed losses (default 100)\n\n  \
         --include-root DIR          containment root for {{{{ path }}}} includes.\n                              \
         Render defaults to the input file's directory; pass this\n                              \
         to widen or narrow it, or to enable includes on stdin.\n                              \
         carve lint has no default: it expands and lints\n                              \
         included content only within this root\n  \
         --no-includes               leave include directives literal\n  \
         --report-includes FILE      write the JSON include-dependency list\n                              \
         (`-` for stderr)\n\n\
         Spec: https://markup-carve.github.io/carve/"
    );
}
