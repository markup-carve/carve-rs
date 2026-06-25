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
mod citations;
mod escape;
mod extension;
pub mod extensions;
mod index_budget;
mod parse;
pub mod profile;
pub mod profile_filter;
mod render;
mod render_ansi;
mod render_markdown;
mod render_plain;
mod render_text;

/// Private-use sentinel for a parser/renderer-GENERATED non-breaking space
/// (an escaped space `\ ` or line-block leading indent). It is distinct from a
/// LITERAL U+00A0 typed in the source: HTML folds both to `&nbsp;`, but the
/// plain/ANSI renderers turn this placeholder back into an ASCII space while
/// preserving literal U+00A0. Using a real char would conflate the two.
pub(crate) const NBSP_PLACEHOLDER: char = '\u{e001}';

pub use ast::*;
pub use citations::{CitationMode, Citations, CslDate, CslEntry, CslName};
pub use extension::{
    BeforeRenderContext, BlockMatch, CarveExtension, InlineMatch, MatcherContext, Mode, Options,
    RenderContext, StaticRenderers,
};
pub use extensions::{
    Autolink, AutolinkOptions, ColorSwatch, ContentMode, CrossrefStyle, Details, ExternalLinks,
    ExternalLinksOptions, FencedRender, FencedRenderOptions, Glossary, HeadingNumbers,
    HeadingNumbersOptions, HeadingPermalinks, HeadingPermalinksOptions, Index, ListTable, ListType,
    MathBlock, MathBlockOptions, Position, Spoiler, SwatchPosition, SwatchShape, TabNormalize,
    TableOfContents, TableOfContentsOptions, UrlGenerator, Wikilinks, WikilinksOptions,
};
pub use parse::{parse, parse_with_options};
pub use profile::{DisallowedAction, LinkPolicy, Profile, ProfileViolation, ProfileViolationError};
pub use profile_filter::{apply_profile, ProfileFilterResult};
pub use render::{render_html, render_html_with_options};
pub use render_ansi::{render_ansi, render_ansi_with_options};
pub use render_markdown::{render_markdown, render_markdown_with_options};
pub use render_plain::{render_plain_text, render_plain_text_with_options};

/// Parse a Carve source string and render it as HTML in one call.
pub fn to_html(source: &str) -> String {
    render_html(&parse(source))
}

/// Parse a Carve source string and render it as Markdown in one call.
pub fn to_markdown(source: &str) -> String {
    render_markdown(&parse(source))
}

/// Parse a Carve source string and render it as plain text in one call.
pub fn to_plain_text(source: &str) -> String {
    render_plain_text(&parse(source))
}

/// Parse a Carve source string and render it as ANSI-styled text in one call.
pub fn to_ansi(source: &str) -> String {
    render_ansi(&parse(source))
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
) -> Result<ast::Document, ProfileViolationError> {
    let Some(profile) = &options.profile else {
        return Ok(parsed_doc_with_hooks(source, options, effective_mode));
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
    let doc = parsed_doc_with_hooks(source, options, effective_mode);
    let base_host = options.profile_base_host.as_deref();
    Ok(apply_profile(doc, profile, base_host)?.doc)
}

/// Parse, run extension hooks, apply the profile, and render to HTML.
/// Returns an error only when the profile's action is
/// [`DisallowedAction::Error`] (or `max_length` is exceeded).
pub fn try_to_html_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    // HTML honors the configured mode (interactive / static).
    Ok(render_html_with_options(
        &prepare_doc(source, options, options.mode)?,
        options,
    ))
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
        &prepare_doc(source, options, Mode::Interactive)?,
        options,
    ))
}

/// Parse, run extension hooks, apply the profile, and render to plain text.
/// Plain text is inherently static; see [`try_to_markdown_with_options`] for
/// why the mode is forced to [`Mode::Interactive`] in the hooks.
pub fn try_to_plain_text_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    Ok(render_plain_text_with_options(
        &prepare_doc(source, options, Mode::Interactive)?,
        options,
    ))
}

/// Parse, run extension hooks, apply the profile, and render to ANSI text.
/// ANSI is inherently static; see [`try_to_markdown_with_options`] for why the
/// mode is forced to [`Mode::Interactive`] in the hooks.
pub fn try_to_ansi_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    Ok(render_ansi_with_options(
        &prepare_doc(source, options, Mode::Interactive)?,
        options,
    ))
}

/// Parse and run `before_render` extension hooks, WITHOUT applying the profile.
/// `effective_mode` is the resolved render mode for the target format: the HTML
/// renderer passes `Options::mode`, the non-HTML renderers force
/// [`Mode::Interactive`] (static rendering is HTML-only).
fn parsed_doc_with_hooks(
    source: &str,
    options: &Options<'_>,
    effective_mode: Mode,
) -> ast::Document {
    let mut doc = parse_with_options(source, options);
    let ctx = extension::BeforeRenderContext::new(options, effective_mode);
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
