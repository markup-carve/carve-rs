//! Carve — a parser + HTML renderer for the [Carve](https://github.com/markup-carve/carve)
//! markup language.
//!
//! ## Quick start
//!
//! ```
//! let html = carve::to_html("# Hello\n\n/italic/ and *bold*.");
//! assert!(html.contains("<h1>Hello</h1>"));
//! assert!(html.contains("<em>italic</em>"));
//! ```
//!
//! Implementation status: passes every `.crv` / `.html` pair currently
//! checked into this crate's `tests/spec` submodule, including tables,
//! captions / figures, admonitions, abbreviations, mentions, tags,
//! inline extensions, attributes, and frontmatter.

mod abbr_budget;
pub mod ast;
pub mod ast_json;
pub mod ast_merge;
pub mod ast_patch;
mod citations;
pub mod djot_migrate;
mod document_ids;
mod escape;
mod extension;
pub mod extensions;
pub mod html_import;
mod index_budget;
pub mod lint;
pub mod markdown_import;
mod parse;
pub mod profile;
pub mod profile_filter;
pub mod prosemirror;
mod render;
mod render_ansi;
mod render_carve;
mod render_carve_error;
mod render_depth;
mod render_markdown;
mod render_plain;
mod render_text;
mod source_layout;
mod stamp;
mod unicode_nfc;
mod wire_fields;

/// Private-use sentinel for a parser/renderer-GENERATED non-breaking space
/// (an escaped space `\ ` or line-block leading indent). It is distinct from a
/// LITERAL U+00A0 typed in the source: HTML folds both to `&nbsp;`, but the
/// plain/ANSI renderers turn this placeholder back into an ASCII space while
/// preserving literal U+00A0. Using a real char would conflate the two, and
/// `fmt` could no longer tell `a\ b` from a typed no-break space.
///
/// U+E000 because this value is PUBLISHED - it reaches a consumer in a text
/// node - and the reference implementation publishes U+E000 for it. Two engines
/// spelling the same resolved space with different private-use characters is not
/// something a consumer can be expected to absorb (carve-rs#404). The writer's
/// own staging markers moved to U+E010.. to free it.
pub(crate) const NBSP_PLACEHOLDER: char = '\u{e000}';
/// The Carve specification version this engine implements.
///
/// `carve fmt --stamp` writes it into a document and [`needs_review`] compares
/// an existing stamp against it, so a stale value tells a reader their document
/// is current when it is not. It is not kept correct by hand: the test
/// `the_version_a_build_reports_is_the_one_that_shipped` compares it against the
/// `Version:` field of the vendored grammar on every run.
///
/// The version of the crate itself is `CARGO_PKG_VERSION` - derived from the
/// manifest, never written out a second time here.
pub const SPEC_VERSION: &str = "0.1";

pub use ast::*;
pub use ast_json::{from_json, to_json, try_to_json, AstJsonError};
pub use ast_merge::{
    merge_ast, merge_ast_with_resolver, MergeConflict, MergeConflictReason, MergeResolution,
    MergeResult,
};
pub use ast_patch::{
    apply_ast_patch, ast_patch_from_json, ast_patch_to_json, create_ast_patch, AstPatchError,
    AstPatchOperation,
};
pub use citations::{
    parse_locator, CitationMode, Citations, CslDate, CslEntry, CslName, ParsedLocator,
};
pub use djot_migrate::djot_to_carve;
pub use extension::{
    BeforeRenderContext, BlockMatch, CarveExtension, InlineMatch, MatcherContext, Mode, Options,
    RenderContext, SmartTypographyMode, StaticRenderers,
};
pub use extensions::{
    sanitize_svg, Autolink, AutolinkOptions, CodeCallouts, CodeGroup, CodeGroupOptions,
    ColorSwatch, ContentMode, CrossrefStyle, Details, ExternalLinks, ExternalLinksOptions,
    FencedRender, FencedRenderOptions, Glossary, HeadingLevelShift, HeadingLevelShiftOptions,
    HeadingNumbers, HeadingNumbersOptions, HeadingPermalinks, HeadingPermalinksOptions,
    HeadingReference, HeadingReferenceOptions, ImgFence, Index, ListTable, ListType, MathBlock,
    MathBlockOptions, Position, QuoteCharacters, SanitizeResult, SanitizeSvgOptions, SmartQuotes,
    Spoiler, SwatchPosition, SwatchShape, TabNormalize, TableOfContents, TableOfContentsOptions,
    Tabs, TabsMode, TabsOptions, TocPlacement, UrlGenerator, Wikilinks, WikilinksOptions,
    SMART_QUOTE_LOCALES,
};
pub use html_import::{
    html_to_ast, html_to_carve, HtmlImportAdapter, HtmlImportDiagnostic, HtmlImportDiagnosticCode,
    HtmlImportError, HtmlImportMode, HtmlImportOptions, HtmlImportReport, HtmlImportResult,
    HtmlImportSeverity,
};
pub use lint::{lint_carve, lint_carve_with_options, LintWarning};
pub use markdown_import::{markdown_to_ast, markdown_to_carve};
pub use parse::{parse, parse_with_options};
pub use profile::{DisallowedAction, LinkPolicy, Profile, ProfileViolation, ProfileViolationError};
pub use profile_filter::{apply_profile, apply_profile_with_typography, ProfileFilterResult};
pub use prosemirror::{from_prosemirror, to_prosemirror, ProseMirrorDoc, ProseMirrorError};
pub use render::{render_html, render_html_with_options, MAX_RENDER_DEPTH};
pub use render_ansi::{render_ansi, render_ansi_with_options};
pub use render_carve::render_carve;
pub use render_carve_error::{RenderCarveError, SourceUnspellable};
pub use render_depth::RenderDepthError;
pub use render_markdown::{render_markdown, render_markdown_with_options};
pub use render_plain::{render_plain_text, render_plain_text_with_options};
pub use source_layout::{parse_with_source_layout, to_source_layout_json};
pub use stamp::{needs_review, read_stamp, stamp_carve, Stamp, StampForm};

/// Parse a Carve source string and render it as HTML in one call.
///
/// Infallible: the parser caps nesting at its own bound, which sits BELOW the
/// renderers' ceiling, so the §25 refusal is unreachable from a source string
/// (see [`RenderDepthError`], which the tree-taking renderers return).
pub fn to_html(source: &str) -> String {
    render_html(&parse(source))
        .expect("the parse cap sits below the render ceiling, so a parsed tree never reaches it")
}

/// Parse a Carve source string and render it as Markdown in one call.
///
/// Infallible: the parser caps nesting at its own bound, which sits BELOW the
/// renderers' ceiling, so the §25 refusal is unreachable from a source string
/// (see [`RenderDepthError`], which the tree-taking renderers return).
/// Positions are ON for this convenience wrapper: §7 orders collected
/// definitions by source position, and the footnote map is a BTreeMap, so
/// without spans the definitions print in LABEL order (carve-rs#686). A caller
/// that builds its own document and calls the tree-taking renderer keeps
/// whatever it parsed with; label order is the only order available there.
pub fn to_markdown(source: &str) -> String {
    render_markdown(&parse_with_options(
        source,
        &Options::default().with_positions(true),
    ))
    .expect("the parse cap sits below the render ceiling, so a parsed tree never reaches it")
}

/// Parse a Carve source string and render it as plain text in one call.
///
/// Infallible: the parser caps nesting at its own bound, which sits BELOW the
/// renderers' ceiling, so the §25 refusal is unreachable from a source string
/// (see [`RenderDepthError`], which the tree-taking renderers return).
/// Positions are ON for this convenience wrapper: §7 orders collected
/// definitions by source position, and the footnote map is a BTreeMap, so
/// without spans the definitions print in LABEL order (carve-rs#686). A caller
/// that builds its own document and calls the tree-taking renderer keeps
/// whatever it parsed with; label order is the only order available there.
pub fn to_plain_text(source: &str) -> String {
    render_plain_text(&parse_with_options(
        source,
        &Options::default().with_positions(true),
    ))
    .expect("the parse cap sits below the render ceiling, so a parsed tree never reaches it")
}

/// Parse a Carve source string and render it as ANSI-styled text in one call.
///
/// Infallible: the parser caps nesting at its own bound, which sits BELOW the
/// renderers' ceiling, so the §25 refusal is unreachable from a source string
/// (see [`RenderDepthError`], which the tree-taking renderers return).
/// Positions are ON for this convenience wrapper: §7 orders collected
/// definitions by source position, and the footnote map is a BTreeMap, so
/// without spans the definitions print in LABEL order (carve-rs#686). A caller
/// that builds its own document and calls the tree-taking renderer keeps
/// whatever it parsed with; label order is the only order available there.
pub fn to_ansi(source: &str) -> String {
    render_ansi(&parse_with_options(
        source,
        &Options::default().with_positions(true),
    ))
    .expect("the parse cap sits below the render ceiling, so a parsed tree never reaches it")
}

/// Parse a Carve source string and render canonical Carve source in one call.
///
/// This formatter is intentionally parse-only: it does not run extension hooks,
/// profile filtering, heading-id enrichment, or other render-time transforms.
pub fn to_carve(source: &str) -> String {
    // The SAME text the parser reads. `raw_frontmatter` scans for the block's
    // closing `---` line on the RAW string while `parse_for_carve` ran on a
    // normalized copy. On CRLF input the closer scan (`\n---\n`) missed, so `to_carve`
    // concluded there was no frontmatter while the parser had already found it
    // - and the document fell through to `render_frontmatter`, which rebuilds
    // the block from the parsed key/value map. That map has no format token, so
    // `---toml` came back `---`; and a format the map cannot represent parses
    // into an EMPTY map, so the whole block was dropped. A lone CR and a leading
    // BOM reached the same fall-through (carve-rs#732).
    let normalized = parse::normalize_source(source);
    let source = normalized.as_ref();
    let (frontmatter, _) = raw_frontmatter(source);
    let mut doc = parse::parse_for_carve(source);
    if frontmatter.is_some() {
        doc.frontmatter.clear();
    }
    let rendered = render_carve(&doc)
        .expect("the parse cap sits below the render ceiling, so a parsed tree never reaches it");
    // The writer's own output, unedited. `restore_inline_comments` used to walk
    // the SOURCE lines here and graft each trailing `%%` back onto the first
    // formatted line equal to the part before it. It could not repair what it
    // was written for: `render_carve` emits `InlineNode::Comment` itself, so a
    // line the writer carried the comment onto already ENDS in the marker and is
    // therefore not equal to the rendering of the part before it. The graft
    // landed on some OTHER equal line or matched nothing - inert or harmful,
    // never corrective - and on `a` over `a %%` inside a line block it wrote the
    // marker onto both (carve-rs#1076). Only the trailing newline it also
    // guaranteed is kept.
    let body = if rendered.ends_with('\n') {
        rendered
    } else {
        format!("{rendered}\n")
    };
    match frontmatter {
        Some(frontmatter) if body.trim().is_empty() => format!("{frontmatter}\n"),
        Some(frontmatter) => format!("{frontmatter}\n\n{body}"),
        None => body,
    }
}

fn raw_frontmatter(source: &str) -> (Option<String>, &str) {
    if !source.starts_with("---") {
        return (None, source);
    }
    let Some(first_nl) = source.find('\n') else {
        return (None, source);
    };
    // The same opener test the parser applies, from the same helper rather than
    // a second copy of it: a `fmt` that disagrees with the parser about what a
    // frontmatter block is would rewrite an ordinary line into one. This copy
    // read `source[3..first_nl].trim()` and admitted a tab in the padding slot
    // exactly as the parser's did (carve-rs#725).
    let Some(kind) = parse::frontmatter_format_token(&source[3..first_nl]) else {
        return (None, source);
    };
    let rest = &source[first_nl + 1..];
    let (content_len, after) = if rest == "---" {
        (0, rest.len())
    } else if let Some(r) = rest.strip_prefix("---\n") {
        (0, rest.len() - r.len())
    } else if let Some(close) = rest.find("\n---\n") {
        (close, close + 5)
    } else if let Some(close) = rest.strip_suffix("\n---").map(str::len) {
        (close, rest.len())
    } else {
        return (None, source);
    };
    // The opening delimiter carries the format token for EVERY format, the
    // default one included: PART 11 section 6b says frontmatter in `yaml` comes
    // back as `---yaml` and never as a bare `---`. An untyped opener is the case
    // that clause was written for, not one it forgot - `---` and `---yaml` open
    // the same block, so nothing in the tree tells them apart, and a writer that
    // omitted the token would be special-casing a value `frontmatter_format`
    // distinguishes nowhere except in the parser's leniency rule. A reader's
    // leniency is not a writer's license.
    //
    // The default is the parser's own, from the same constant that fills
    // `Frontmatter::format` for a bare fence, so the written source and the
    // published tree cannot disagree about which format the document is in.
    let open = format!(
        "---{}",
        if kind.is_empty() {
            parse::DEFAULT_FRONTMATTER_FORMAT
        } else {
            kind
        }
    );
    let content = &rest[..content_len];
    let body = &rest[after..];
    (Some(format!("{open}\n{content}\n---")), body)
}

/// Parse the source, run `before_render` extension hooks, then apply the
/// feature-restriction profile (if any) as an AST transform. Enforces the
/// profile's `max_length` on the source bytes (pre-render) and returns a
/// [`ProfileViolationError`] when the profile's action is
/// [`DisallowedAction::Error`] and a disallowed node is found.
///
/// This is the shared pipeline position (after parse, before render) used by
/// every `try_to_*_with_options` entry point, so the profile holds identically
/// across the HTML, Markdown, plain-text and ANSI renderers.
fn prepare_doc(
    source: &str,
    options: &Options<'_>,
    effective_mode: Mode,
    target_is_html: bool,
) -> Result<ast::Document, ProfileViolationError> {
    let Some(profile) = &options.profile else {
        return Ok(parsed_doc_with_hooks(
            source,
            options,
            effective_mode,
            target_is_html,
        ));
    };
    let max_length = profile.max_length();
    if max_length > 0 && source.len() > max_length {
        // Match carve-php / carve-js: an over-length input is a profile
        // violation surfaced as an error regardless of the configured action.
        let violation = ProfileViolation {
            node_type: "document".to_string(),
            reason: "max_length_exceeded".to_string(),
            reason_description: Some(format!(
                "Input exceeds the profile's maximum length of {max_length} bytes ({} given).",
                source.len()
            )),
        };
        return Err(ProfileViolationError {
            violations: vec![violation],
        });
    }
    let doc = parsed_doc_with_hooks(source, options, effective_mode, target_is_html);
    let base_host = options.profile_base_host.as_deref();
    Ok(apply_profile_with_typography(doc, profile, base_host, options.smart_typography)?.doc)
}

/// Run render-time extension hooks and the configured feature profile on an
/// already-built document. Used by `--from-json`, where parsing has happened in
/// another process but render restrictions must still apply.
pub fn prepare_document_for_render(
    mut doc: ast::Document,
    options: &Options<'_>,
    effective_mode: Mode,
    target_is_html: bool,
) -> Result<ast::Document, ProfileViolationError> {
    // FIRST, ahead of the hooks. A profile's `max_length` bounds UNTRUSTED
    // INPUT, so it has to be answered before anything walks that input:
    // `before_render` hooks are where the table of contents and the index
    // traverse and allocate from the tree, and refusing afterwards means the
    // work the cap exists to prevent has already been done.
    //
    // It also puts the number out of a hook's reach. `before_render` takes the
    // document by value and hands one back, so a hook could return a document
    // whose `ingest_payload_len` it had cleared - and a cap read after the
    // hooks would be a cap whose input the pipeline could rewrite.
    if let Some(profile) = &options.profile {
        // Measured, not claimed. On the ingest path the untrusted input is the
        // payload: it is what got parsed, held and walked. `source_len` cannot
        // stand in for it there, because that number arrives inside the payload
        // - a hostile tree claims 0 and renders anything, which is exactly what
        // `Profile::minimal()` used to accept: an 80 KB payload against a
        // 10,000 byte cap. `main.rs` already measures the payload for
        // `--from-json` and says why; this helper is documented as the same
        // path for a host that has already decoded, so it has to reach the same
        // answer.
        let max_length = profile.max_length();
        let input_len = doc.untrusted_input_len();
        if max_length > 0 && input_len > max_length {
            let violation = ProfileViolation {
                node_type: "document".to_string(),
                reason: "max_length_exceeded".to_string(),
                reason_description: Some(format!(
                    "Input exceeds the profile's maximum length of {max_length} bytes ({input_len} given)."
                )),
            };
            return Err(ProfileViolationError {
                violations: vec![violation],
            });
        }
    }

    let ctx = extension::BeforeRenderContext::new(options, effective_mode, target_is_html);
    for ext in &options.extensions {
        doc = ext.before_render(doc, &ctx);
    }
    let Some(profile) = &options.profile else {
        return Ok(doc);
    };
    let base_host = options.profile_base_host.as_deref();
    Ok(apply_profile_with_typography(doc, profile, base_host, options.smart_typography)?.doc)
}

/// Parse, run extension hooks, apply the profile, and render to HTML.
/// Returns an error only when the profile's action is
/// [`DisallowedAction::Error`] (or `max_length` is exceeded).
pub fn try_to_html_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    // HTML honors the configured mode (interactive / static).
    Ok(
        render_html_with_options(&prepare_doc(source, options, options.mode, true)?, options)
            .expect(
                "the parse cap sits below the render ceiling, so a parsed tree never reaches it",
            ),
    )
}

/// Parse, run extension/profile transforms, and serialize the AST as JSON.
pub fn try_to_json_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    Ok(to_json(&prepare_doc(
        source,
        options,
        Mode::Interactive,
        false,
    )?))
}

pub fn to_json_with_options(source: &str, options: &Options<'_>) -> String {
    try_to_json_with_options(source, options).unwrap_or_default()
}

/// Parse, run extension hooks, apply the profile, and render to Markdown.
/// Markdown is inherently static, so the render mode is forced to
/// [`Mode::Interactive`] in the hooks (the HTML-only static path never runs);
/// the Markdown renderer flattens containers on its own.
pub fn try_to_markdown_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    Ok(render_markdown_with_options(
        &prepare_doc(source, options, Mode::Interactive, false)?,
        options,
    )
    .expect("the parse cap sits below the render ceiling, so a parsed tree never reaches it"))
}

/// Parse, run extension hooks, apply the profile, and render to plain text.
/// Plain text is inherently static; see [`try_to_markdown_with_options`] for
/// why the mode is forced to [`Mode::Interactive`] in the hooks.
pub fn try_to_plain_text_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    Ok(render_plain_text_with_options(
        &prepare_doc(source, options, Mode::Interactive, false)?,
        options,
    )
    .expect("the parse cap sits below the render ceiling, so a parsed tree never reaches it"))
}

/// Parse, run extension hooks, apply the profile, and render to ANSI text.
/// ANSI is inherently static; see [`try_to_markdown_with_options`] for why the
/// mode is forced to [`Mode::Interactive`] in the hooks.
pub fn try_to_ansi_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    Ok(render_ansi_with_options(
        &prepare_doc(source, options, Mode::Interactive, false)?,
        options,
    )
    .expect("the parse cap sits below the render ceiling, so a parsed tree never reaches it"))
}

/// Parse and run `before_render` extension hooks, WITHOUT applying the profile.
/// `effective_mode` is the resolved render mode for the target format: the HTML
/// renderer passes `Options::mode`, the non-HTML renderers force
/// [`Mode::Interactive`] (static rendering is HTML-only).
fn parsed_doc_with_hooks(
    source: &str,
    options: &Options<'_>,
    effective_mode: Mode,
    target_is_html: bool,
) -> ast::Document {
    let mut doc = parse_with_options(source, options);
    let ctx = extension::BeforeRenderContext::new(options, effective_mode, target_is_html);
    for ext in &options.extensions {
        doc = ext.before_render(doc, &ctx);
    }
    doc
}

/// Infallible HTML entry point. Identical to [`try_to_html_with_options`]
/// except that profile errors render an empty safe output instead of returning
/// an error. Callers that need to surface violations should use
/// [`try_to_html_with_options`].
pub fn to_html_with_options(source: &str, options: &Options<'_>) -> String {
    try_to_html_with_options(source, options).unwrap_or_default()
}

/// Infallible Markdown entry point. See [`to_html_with_options`] for the
/// error-action fallback behavior.
pub fn to_markdown_with_options(source: &str, options: &Options<'_>) -> String {
    try_to_markdown_with_options(source, options).unwrap_or_default()
}

/// Infallible plain-text entry point. See [`to_html_with_options`].
pub fn to_plain_text_with_options(source: &str, options: &Options<'_>) -> String {
    try_to_plain_text_with_options(source, options).unwrap_or_default()
}

/// Infallible ANSI entry point. See [`to_html_with_options`].
pub fn to_ansi_with_options(source: &str, options: &Options<'_>) -> String {
    try_to_ansi_with_options(source, options).unwrap_or_default()
}
