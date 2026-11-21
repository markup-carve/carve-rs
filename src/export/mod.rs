//! The exports of a Carve 0.2 tree: Markdown, plain text and text styled
//! with ANSI escape sequences for a terminal. Each format has one call that
//! returns the text and the losses of the run; the writers of the targets
//! `markdown`, `plain` and `ansi` put the same calls behind the writer
//! registry (`carve render --to markdown|plain|ansi`).
//!
//! ```
//! use carve::api::{self, Options};
//! use carve::export::{self, ExportOptions};
//!
//! let tree = api::parse("= Notes\n\nSee *this* and </#Notes>.\n", &Options::default())?;
//! let md = export::to_markdown(&tree, &ExportOptions::default())?;
//! assert_eq!(md.output, "# Notes\n\nSee **this** and [Notes](#notes).\n");
//! let plain = export::to_plain_text(&tree, &ExportOptions::default())?;
//! assert_eq!(plain.output, "Notes\n\nSee this and Notes.\n");
//! assert!(export::to_ansi(&tree, &ExportOptions::default())?.output.contains("\x1b[1mthis\x1b[0m"));
//! # Ok::<(), api::Error>(())
//! ```

mod common;
mod markdown;
mod terminal;
mod text;

use crate::api::{Error, Tree, WriteOptions, Writer, Written};
use crate::options::UrlPolicy;

/// The options every export reads.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Writes the glyph of smart punctuation (the default), or the characters as typed when false.
    pub smart_typography: bool,
    /// Refuses with [`Error::Export`] instead of returning a result that lost something.
    pub strict_losses: bool,
    /// The schemes a link or an image may use; a denied destination is written empty and reported.
    pub url_policy: UrlPolicy,
    /// `headingShift` (C2-H-209): read by the Markdown export, which writes a
    /// level above 6 as 6; plain text and ANSI refuse any value, 0 included.
    pub heading_shift: Option<u8>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            smart_typography: true,
            strict_losses: false,
            url_policy: UrlPolicy::default(),
            heading_shift: None,
        }
    }
}

impl ExportOptions {
    /// The export options of the render options a writer receives.
    #[must_use]
    pub fn from_render(opts: &crate::options::Options) -> Self {
        Self {
            smart_typography: opts.smart_typography,
            strict_losses: opts.strict_losses,
            url_policy: opts.url_policy.clone(),
            heading_shift: (opts.heading_shift != 0).then_some(opts.heading_shift),
        }
    }
}

/// An export target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Markdown,
    Plain,
    Ansi,
}

impl Target {
    /// The target name on the writer registry.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Plain => "plain",
            Self::Ansi => "ansi",
        }
    }

    /// The target of a name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        EXPORT_TARGETS.into_iter().find(|t| t.name() == name)
    }
}

/// C2-H-208: the largest `headingShift`.
const MAX_HEADING_SHIFT: u64 = 5;

/// The targets of the exports, in the order the writer registry lists them.
pub const EXPORT_TARGETS: [Target; 3] = [Target::Markdown, Target::Plain, Target::Ansi];

/// The loss codes only the exports report, beside the render-loss codes of
/// the catalog they share with HTML.
pub const EXPORT_LOSS_CODES: [&str; 11] = [
    "markdown-container-flattened",
    "markdown-caption-flattened",
    "markdown-attributes-dropped",
    "fence-title-dropped",
    "raw-html-escaped",
    "table-header-row-demoted",
    "table-header-cell-demoted",
    "list-style-dropped",
    "table-span-flattened",
    "ruby-flattened",
    "editorial-comment-flattened",
];

/// The Markdown (`CommonMark` with GFM tables, task lists, strikethrough and footnotes) of a tree.
///
/// # Errors
///
/// [`Error::Export`] under `strict_losses` where the run lost something;
/// [`Error::Option`] for a supplied tree nested too deep.
pub fn to_markdown(tree: &Tree, options: &ExportOptions) -> Result<Written, Error> {
    export_to(Target::Markdown, tree, options)
}

/// The plain text of a tree.
///
/// # Errors
///
/// As [`to_markdown`].
pub fn to_plain_text(tree: &Tree, options: &ExportOptions) -> Result<Written, Error> {
    export_to(Target::Plain, tree, options)
}

/// The text of a tree styled with ANSI escape sequences for a terminal.
///
/// # Errors
///
/// As [`to_markdown`].
pub fn to_ansi(tree: &Tree, options: &ExportOptions) -> Result<Written, Error> {
    export_to(Target::Ansi, tree, options)
}

/// The export of `tree` for `target`.
///
/// # Errors
///
/// As [`to_markdown`].
pub fn export_to(target: Target, tree: &Tree, options: &ExportOptions) -> Result<Written, Error> {
    if target != Target::Markdown && options.heading_shift.is_some() {
        return Err(Error::option(
            "headingShift",
            format!("the {} export does not read it", target.name()),
        ));
    }
    if options
        .heading_shift
        .is_some_and(|n| u64::from(n) > MAX_HEADING_SHIFT)
    {
        return Err(Error::option(
            "headingShift",
            "the value is not a count from 0 to 5",
        ));
    }
    // The tree is resolved again on a copy, as a render does: a supplied tree's results are no input.
    let arena = crate::Arena::new();
    let mut doc = tree.document().in_arena(&arena);
    let shift = options.heading_shift.unwrap_or(0);
    crate::heading_shift::apply(doc.children, shift);
    let resolve_opts = crate::options::Options::default();
    let _ = crate::resolve_tree_unflattened(&mut doc, &resolve_opts, &arena);
    let xref_text = crate::html::xref_targets(&doc, &resolve_opts)
        .into_iter()
        .collect();
    let value = crate::api::writer::tree_value(&crate::to_json(&doc))?;
    let mut x = common::Exporter::new(target, &value, xref_text, options);
    let output = match target {
        Target::Markdown => markdown::export_markdown(&mut x)?,
        Target::Plain => text::export_text(&mut x, false)?,
        Target::Ansi => text::export_text(&mut x, true)?,
    };
    let diagnostics = x.diagnostics();
    if options.strict_losses && !diagnostics.is_empty() {
        return Err(Error::Export { diagnostics });
    }
    Ok(Written {
        output,
        diagnostics,
    })
}

/// The writer of one export target, registered on [`crate::api::Writers`]
/// by default. It reads `smart_typography`, `strict_losses` and `url_policy`
/// of the render options.
#[derive(Debug, Clone, Copy)]
pub struct ExportWriter(pub Target);

impl Writer for ExportWriter {
    fn target(&self) -> &str {
        self.0.name()
    }

    fn write(&self, tree: &Tree, opts: &WriteOptions) -> Result<Written, Error> {
        export_to(self.0, tree, &ExportOptions::from_render(&opts.render))
    }
}
