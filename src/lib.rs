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
mod document_ids;
mod escape;
mod extension;
pub mod extensions;
mod index_budget;
mod parse;
pub mod profile;
pub mod profile_filter;
mod render;
mod render_ansi;
mod render_carve;
mod render_markdown;
mod render_plain;
mod render_text;
mod stamp;
mod unicode_nfc;

/// Private-use sentinel for a parser/renderer-GENERATED non-breaking space
/// (an escaped space `\ ` or line-block leading indent). It is distinct from a
/// LITERAL U+00A0 typed in the source: HTML folds both to `&nbsp;`, but the
/// plain/ANSI renderers turn this placeholder back into an ASCII space while
/// preserving literal U+00A0. Using a real char would conflate the two.
pub(crate) const NBSP_PLACEHOLDER: char = '\u{e001}';
pub(crate) const ESCAPED_CARET_PLACEHOLDER: char = '\u{e002}';
pub const SPEC_VERSION: &str = "0.1";

pub use ast::*;
pub use citations::{
    parse_locator, CitationMode, Citations, CslDate, CslEntry, CslName, ParsedLocator,
};
pub use extension::{
    BeforeRenderContext, BlockMatch, CarveExtension, InlineMatch, MatcherContext, Mode, Options,
    RenderContext, SmartTypographyMode, StaticRenderers,
};
pub use extensions::{
    sanitize_svg, Autolink, AutolinkOptions, CodeCallouts, ColorSwatch, ContentMode, CrossrefStyle,
    Details, ExternalLinks, ExternalLinksOptions, FencedRender, FencedRenderOptions, Glossary,
    HeadingNumbers, HeadingNumbersOptions, HeadingPermalinks, HeadingPermalinksOptions, ImgFence,
    Index, ListTable, ListType, MathBlock, MathBlockOptions, Position, SanitizeResult,
    SanitizeSvgOptions, Spoiler, SwatchPosition, SwatchShape, TabNormalize, TableOfContents,
    TableOfContentsOptions, TocPlacement, UrlGenerator, Wikilinks, WikilinksOptions,
};
pub use parse::{parse, parse_with_options};
pub use profile::{DisallowedAction, LinkPolicy, Profile, ProfileViolation, ProfileViolationError};
pub use profile_filter::{apply_profile, ProfileFilterResult};
pub use render::{render_html, render_html_with_options};
pub use render_ansi::{render_ansi, render_ansi_with_options};
pub use render_carve::render_carve;
pub use render_markdown::{render_markdown, render_markdown_with_options};
pub use render_plain::{render_plain_text, render_plain_text_with_options};
pub use stamp::{needs_review, read_stamp, stamp_carve, Stamp, StampForm};

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

/// Parse a Carve source string and render canonical Carve source in one call.
///
/// This formatter is intentionally parse-only: it does not run extension hooks,
/// profile filtering, heading-id enrichment, or other render-time transforms.
pub fn to_carve(source: &str) -> String {
    let (frontmatter, _) = raw_frontmatter(source);
    let mut doc = parse::parse_for_carve(source);
    if frontmatter.is_some() {
        doc.frontmatter.clear();
    }
    let body = restore_inline_comments(source, &render_carve(&doc));
    match frontmatter {
        Some(frontmatter) if body.trim().is_empty() => format!("{frontmatter}\n"),
        Some(frontmatter) => format!("{frontmatter}\n\n{body}"),
        None => body,
    }
}

fn restore_inline_comments(source: &str, formatted: &str) -> String {
    let mut lines = formatted.lines().map(str::to_string).collect::<Vec<_>>();
    // Formatting preserves block order, so match comment-bearing source lines to
    // formatted lines in order: advance a cursor and consume each match so a
    // repeated line cannot pull a later comment onto an earlier duplicate.
    let mut cursor = 0;
    for source_line in source.lines() {
        let Some((before, comment)) = split_inline_comment(source_line) else {
            continue;
        };
        if before.trim().is_empty() {
            continue;
        }
        let marker = render_carve(&parse::parse_for_carve(before));
        let marker = marker.trim_end();
        if marker.is_empty() {
            continue;
        }
        if let Some(offset) = lines[cursor..]
            .iter()
            .position(|line| line.as_str() == marker)
        {
            let idx = cursor + offset;
            lines[idx].push(' ');
            lines[idx].push_str(comment);
            cursor = idx + 1;
        }
    }
    format!("{}\n", lines.join("\n"))
}

fn split_inline_comment(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut in_code = false;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'`' {
            in_code = !in_code;
            i += 1;
            continue;
        }
        if !in_code
            && bytes[i] == b'%'
            && bytes[i + 1] == b'%'
            && (i == 0 || matches!(bytes[i - 1], b' ' | b'\t'))
        {
            return Some((line[..i].trim_end(), &line[i..]));
        }
        i += 1;
    }
    None
}

fn raw_frontmatter(source: &str) -> (Option<String>, &str) {
    if !source.starts_with("---") {
        return (None, source);
    }
    let Some(first_nl) = source.find('\n') else {
        return (None, source);
    };
    let kind = source[3..first_nl].trim();
    if !kind.is_empty() && !kind.chars().all(|c| c.is_ascii_alphanumeric()) {
        return (None, source);
    }
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
    let open = if kind.is_empty() {
        "---".to_string()
    } else {
        format!("---{kind}")
    };
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
        &prepare_doc(source, options, options.mode, true)?,
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
        &prepare_doc(source, options, Mode::Interactive, false)?,
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
        &prepare_doc(source, options, Mode::Interactive, false)?,
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
        &prepare_doc(source, options, Mode::Interactive, false)?,
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
