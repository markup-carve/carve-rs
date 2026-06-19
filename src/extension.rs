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
}

impl<'a> RenderContext<'a> {
    pub(crate) fn new(options: &'a Options<'a>) -> Self {
        Self { options }
    }

    pub fn render_inlines(&self, nodes: &[InlineNode]) -> String {
        crate::render::render_inlines_with_options(nodes, self.options)
    }

    pub fn render_blocks(&self, nodes: &[BlockNode]) -> String {
        crate::render::render_blocks_with_options(nodes, self.options)
    }

    pub fn escape_html(&self, input: &str) -> String {
        escape_text(input)
    }

    pub fn escape_attr(&self, input: &str) -> String {
        escape_attr(input)
    }
}
