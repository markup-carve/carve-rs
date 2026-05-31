//! Extension contracts for opt-in Carve behavior.
//!
//! The model mirrors the language-level extension lifecycle:
//! parse matchers, `after_parse`, `before_render`, then renderer hooks.
//! Implementations register extensions through [`Options`].

use std::collections::BTreeMap;

use crate::ast::{BlockExtension, BlockNode, Document, InlineExtension, InlineNode};
use crate::escape::{escape_attr, escape_text};
use crate::parse::{parse_blocks_with_options, parse_inline_with_options};

#[derive(Default)]
pub struct Options<'a> {
    pub extensions: Vec<&'a dyn CarveExtension>,
    pub mention_url: Option<String>,
    pub tag_url: Option<String>,
    pub emoji: BTreeMap<String, String>,
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
