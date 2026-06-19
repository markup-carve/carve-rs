//! Extension contracts for opt-in Carve behavior.
//!
//! The model mirrors the language-level extension lifecycle:
//! parse matchers, `after_parse`, `before_render`, then renderer hooks.
//! Implementations register extensions through [`Options`].

use std::collections::BTreeMap;

use crate::ast::{BlockExtension, BlockNode, Document, InlineExtension, InlineNode};
use crate::escape::{escape_attr, escape_text};
use crate::parse::{parse_blocks_with_options, parse_inline_with_options};
use crate::profile::Profile;

#[derive(Default)]
pub struct Options<'a> {
    pub extensions: Vec<&'a dyn CarveExtension>,
    pub mention_url: Option<String>,
    pub tag_url: Option<String>,
    pub emoji: BTreeMap<String, String>,
    /// When `true`, lowercase the kept characters of an auto-generated heading
    /// id per code point (`char::to_lowercase`). Default `false`: heading ids
    /// are CASE-PRESERVING (`# Getting Started` -> `Getting-Started`), matching
    /// carve-js / carve-php. carve-rs has no ASCII transliterator, so
    /// ascii-folding is intentionally unsupported here; only `lowercase` is.
    pub lowercase_heading_ids: bool,
    /// Optional feature-restriction profile. When set, disallowed nodes are
    /// converted to text / stripped / error'd per the profile's action,
    /// link/image URLs are gated by its link policy, and `max_nesting` /
    /// `max_length` are enforced. The transform runs on the parsed document
    /// before rendering, so it holds for every renderer. See [`Profile`].
    pub profile: Option<Profile>,
    /// Current document host, used by the profile's link policy to tell
    /// internal from external links.
    pub profile_base_host: Option<String>,
}

impl<'a> Options<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_extension(mut self, extension: &'a dyn CarveExtension) -> Self {
        self.extensions.push(extension);
        self
    }

    pub fn with_mention_url(mut self, template: impl Into<String>) -> Self {
        self.mention_url = Some(template.into());
        self
    }

    pub fn with_tag_url(mut self, template: impl Into<String>) -> Self {
        self.tag_url = Some(template.into());
        self
    }

    pub fn with_emoji(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.emoji.insert(name.into(), value.into());
        self
    }

    /// Opt in to lowercasing auto-generated heading ids (default is
    /// case-preserving). See [`Options::lowercase_heading_ids`].
    pub fn with_lowercase_heading_ids(mut self, lowercase: bool) -> Self {
        self.lowercase_heading_ids = lowercase;
        self
    }

    /// Apply a feature-restriction [`Profile`] before rendering.
    pub fn with_profile(mut self, profile: Profile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Set the base host for the profile's link policy (internal/external
    /// link detection).
    pub fn with_profile_base_host(mut self, host: impl Into<String>) -> Self {
        self.profile_base_host = Some(host.into());
        self
    }
}

pub trait CarveExtension {
    fn name(&self) -> &'static str;

    fn match_inline(
        &self,
        _text: &str,
        _pos: usize,
        _ctx: &MatcherContext<'_>,
    ) -> Option<InlineMatch> {
        None
    }

    fn match_block(
        &self,
        _lines: &[&str],
        _start: usize,
        _ctx: &MatcherContext<'_>,
    ) -> Option<BlockMatch> {
        None
    }

    fn after_parse(&self, doc: Document) -> Document {
        doc
    }

    fn before_render(&self, doc: Document) -> Document {
        doc
    }

    fn render_inline_extension(
        &self,
        _node: &InlineExtension,
        _ctx: &RenderContext<'_>,
    ) -> Option<String> {
        None
    }

    fn render_block_extension(
        &self,
        _node: &BlockExtension,
        _ctx: &RenderContext<'_>,
    ) -> Option<String> {
        None
    }
}

pub struct InlineMatch {
    pub node: InlineNode,
    pub end: usize,
}

pub struct BlockMatch {
    pub node: BlockNode,
    pub lines_consumed: usize,
}

pub struct MatcherContext<'a> {
    options: &'a Options<'a>,
}

impl<'a> MatcherContext<'a> {
    pub(crate) fn new(options: &'a Options<'a>) -> Self {
        Self { options }
    }

    pub fn parse_inlines(&self, text: &str) -> Vec<InlineNode> {
        parse_inline_with_options(text, self.options)
    }

    pub fn parse_blocks(&self, source: &str) -> Vec<BlockNode> {
        parse_blocks_with_options(source, self.options)
    }
}

pub struct RenderContext<'a> {
    pub(crate) options: &'a Options<'a>,
    /// Indentation level of the node currently being rendered. Zero on the
    /// inline path (inline extensions never indent); set to the block node's
    /// level when a block extension is rendered, so a block-extension renderer
    /// can emit nesting-aware indentation (mirrors carve-js
    /// `BlockExtensionRenderContext.level`).
    level: usize,
    /// The live document render state (heading-id counter) when rendering a
    /// block extension, so [`RenderContext::render_blocks_at`] continues the
    /// document's heading numbering across the extension boundary. `None` on
    /// the inline path and at level-0 entry points.
    state: Option<&'a std::cell::RefCell<&'a mut crate::render::RenderState>>,
}

impl<'a> RenderContext<'a> {
    pub(crate) fn new(options: &'a Options<'a>) -> Self {
        Self {
            options,
            level: 0,
            state: None,
        }
    }

    pub(crate) fn with_level_and_state(
        options: &'a Options<'a>,
        level: usize,
        state: &'a std::cell::RefCell<&'a mut crate::render::RenderState>,
    ) -> Self {
        Self {
            options,
            level,
            state: Some(state),
        }
    }

    pub fn render_inlines(&self, nodes: &[InlineNode]) -> String {
        crate::render::render_inlines_with_options(nodes, self.options)
    }

    /// Render block nodes at level 0 (no leading indentation). Use
    /// [`RenderContext::render_blocks_at`] to render at a specific nesting
    /// level.
    pub fn render_blocks(&self, nodes: &[BlockNode]) -> String {
        crate::render::render_blocks_with_options(nodes, self.options)
    }

    /// Render block nodes indented to `level`, matching the core renderer's
    /// two-space-per-level layout. A block-extension renderer uses this to
    /// place its children at `ctx.level() + 1` so a details/disclosure block
    /// nests identically wherever it sits (top level, list item, blockquote).
    /// When this context carries the live document state, the heading-id
    /// counter continues across the extension boundary (a duplicate slug gets
    /// its numeric suffix); otherwise it falls back to a fresh counter.
    pub fn render_blocks_at(&self, nodes: &[BlockNode], level: usize) -> String {
        match self.state {
            Some(cell) => {
                let mut state = cell.borrow_mut();
                crate::render::render_blocks_at_with_state(nodes, self.options, level, &mut state)
            }
            None => crate::render::render_blocks_at_with_options(nodes, self.options, level),
        }
    }

    /// The indentation level of the block node being rendered. Zero on the
    /// inline path.
    pub fn level(&self) -> usize {
        self.level
    }

    /// The two-space-per-level indent string for `level`.
    pub fn indent(&self, level: usize) -> String {
        "  ".repeat(level)
    }

    pub fn escape_html(&self, input: &str) -> String {
        escape_text(input)
    }

    pub fn escape_attr(&self, input: &str) -> String {
        escape_attr(input)
    }
}
