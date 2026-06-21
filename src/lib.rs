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

pub mod ast;
mod citations;
mod escape;
mod extension;
pub mod extensions;
mod parse;
pub mod profile;
pub mod profile_filter;
mod render;
mod render_ansi;
mod render_markdown;
mod render_plain;
mod render_text;

pub use ast::*;
pub use citations::{CitationMode, Citations};
pub use extension::{
    BlockMatch, CarveExtension, InlineMatch, MatcherContext, Options, RenderContext,
};
pub use extensions::{
    Autolink, AutolinkOptions, ContentMode, Details, ExternalLinks, ExternalLinksOptions,
    FencedRender, FencedRenderOptions, HeadingPermalinks, HeadingPermalinksOptions, ListTable,
    ListType, MathBlock, MathBlockOptions, Position, TabNormalize, TableOfContents,
    TableOfContentsOptions, UrlGenerator, Wikilinks, WikilinksOptions,
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
) -> Result<ast::Document, ProfileViolationError> {
    let doc = parsed_doc_with_hooks(source, options);
    let Some(profile) = &options.profile else {
        return Ok(doc);
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
    Ok(render_html_with_options(
        &prepare_doc(source, options)?,
        options,
    ))
}

/// Parse, run extension hooks, apply the profile, and render to Markdown.
pub fn try_to_markdown_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    Ok(render_markdown_with_options(
        &prepare_doc(source, options)?,
        options,
    ))
}

/// Parse, run extension hooks, apply the profile, and render to plain text.
pub fn try_to_plain_text_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    Ok(render_plain_text_with_options(
        &prepare_doc(source, options)?,
        options,
    ))
}

/// Parse, run extension hooks, apply the profile, and render to ANSI text.
pub fn try_to_ansi_with_options(
    source: &str,
    options: &Options<'_>,
) -> Result<String, ProfileViolationError> {
    Ok(render_ansi_with_options(
        &prepare_doc(source, options)?,
        options,
    ))
}

/// Parse and run `before_render` extension hooks, WITHOUT applying the
/// profile. Used by the infallible entry points as the error-action /
/// `max_length` fallback so the hooks still run on that path (matching the
/// normal pipeline in [`prepare_doc`]).
fn parsed_doc_with_hooks(source: &str, options: &Options<'_>) -> ast::Document {
    let mut doc = parse_with_options(source, options);
    for ext in &options.extensions {
        doc = ext.before_render(doc);
    }
    doc
}

/// Infallible HTML entry point. Identical to [`try_to_html_with_options`]
/// except that, for the [`DisallowedAction::Error`] action (or an exceeded
/// `max_length`), it falls back to rendering the unfiltered document rather
/// than returning an error. Callers that need to surface violations should use
/// [`try_to_html_with_options`].
pub fn to_html_with_options(source: &str, options: &Options<'_>) -> String {
    match try_to_html_with_options(source, options) {
        Ok(out) => out,
        Err(_) => render_html_with_options(&parsed_doc_with_hooks(source, options), options),
    }
}

/// Infallible Markdown entry point. See [`to_html_with_options`] for the
/// error-action fallback behavior.
pub fn to_markdown_with_options(source: &str, options: &Options<'_>) -> String {
    match try_to_markdown_with_options(source, options) {
        Ok(out) => out,
        Err(_) => render_markdown_with_options(&parsed_doc_with_hooks(source, options), options),
    }
}

/// Infallible plain-text entry point. See [`to_html_with_options`].
pub fn to_plain_text_with_options(source: &str, options: &Options<'_>) -> String {
    match try_to_plain_text_with_options(source, options) {
        Ok(out) => out,
        Err(_) => render_plain_text_with_options(&parsed_doc_with_hooks(source, options), options),
    }
}

/// Infallible ANSI entry point. See [`to_html_with_options`].
pub fn to_ansi_with_options(source: &str, options: &Options<'_>) -> String {
    match try_to_ansi_with_options(source, options) {
        Ok(out) => out,
        Err(_) => render_ansi_with_options(&parsed_doc_with_hooks(source, options), options),
    }
}
