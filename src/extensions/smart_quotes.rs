//! Locale-specific smart quote glyphs.

use crate::ast::{BlockNode, Document, FigureTarget, InlineNode};
use crate::extension::CarveExtension;

pub type QuoteCharacters = [&'static str; 4];

pub const SMART_QUOTE_LOCALES: &[(&str, QuoteCharacters)] = &[
    ("en", ["“", "”", "‘", "’"]),
    ("de", ["„", "“", "‚", "‘"]),
    ("de-CH", ["«", "»", "‹", "›"]),
    ("fr", ["«\u{00a0}", "\u{00a0}»", "‹\u{00a0}", "\u{00a0}›"]),
    ("pl", ["„", "”", "‚", "’"]),
    ("ru", ["«", "»", "„", "“"]),
    ("ja", ["「", "」", "『", "』"]),
    ("zh", ["「", "」", "『", "』"]),
    ("sv", ["”", "”", "’", "’"]),
    ("da", ["„", "“", "‚", "‘"]),
    ("fi", ["”", "”", "’", "’"]),
    ("cs", ["„", "“", "‚", "‘"]),
    ("hu", ["„", "”", "‚", "’"]),
    ("it", ["«", "»", "“", "”"]),
    ("es", ["«", "»", "“", "”"]),
    ("pt", ["«", "»", "“", "”"]),
    ("nl", ["“", "”", "‘", "’"]),
    ("nb", ["«", "»", "‘", "’"]),
    ("nn", ["«", "»", "‘", "’"]),
    ("uk", ["«", "»", "„", "“"]),
];

pub struct SmartQuotes {
    quotes: [String; 4],
}

impl SmartQuotes {
    pub fn new(locale: &str) -> Self {
        Self {
            quotes: resolve_locale(locale).map(str::to_owned),
        }
    }

    pub fn with_quotes(
        open_double: impl Into<String>,
        close_double: impl Into<String>,
        open_single: impl Into<String>,
        close_single: impl Into<String>,
    ) -> Self {
        Self {
            quotes: [
                open_double.into(),
                close_double.into(),
                open_single.into(),
                close_single.into(),
            ],
        }
    }

    pub fn with_open_double_quote(mut self, quote: impl Into<String>) -> Self {
        self.quotes[0] = quote.into();
        self
    }

    pub fn with_close_double_quote(mut self, quote: impl Into<String>) -> Self {
        self.quotes[1] = quote.into();
        self
    }

    pub fn with_open_single_quote(mut self, quote: impl Into<String>) -> Self {
        self.quotes[2] = quote.into();
        self
    }

    pub fn with_close_single_quote(mut self, quote: impl Into<String>) -> Self {
        self.quotes[3] = quote.into();
        self
    }

    pub fn supported_locales() -> impl Iterator<Item = &'static str> {
        SMART_QUOTE_LOCALES.iter().map(|(locale, _)| *locale)
    }
    pub fn is_locale_supported(locale: &str) -> bool {
        lookup_locale(locale).is_some()
    }

    fn visit_blocks(&self, blocks: &mut [BlockNode]) {
        for block in blocks {
            match block {
                BlockNode::Heading(h) => self.visit_inlines(&mut h.children),
                BlockNode::Paragraph(p) => self.visit_inlines(&mut p.children),
                BlockNode::CitationDefinition(d) => self.visit_inlines(&mut d.children),
                BlockNode::List(l) => {
                    for item in &mut l.items {
                        self.visit_blocks(&mut item.children);
                    }
                }
                BlockNode::BlockQuote(b) => {
                    self.visit_blocks(&mut b.children);
                }
                BlockNode::Table(t) => {
                    if let Some(c) = &mut t.caption {
                        self.visit_inlines(c);
                    }
                    for r in &mut t.rows {
                        for c in &mut r.cells {
                            self.visit_inlines(&mut c.children);
                        }
                    }
                }
                BlockNode::Admonition(a) => {
                    if let Some(t) = &mut a.title {
                        self.visit_inlines(t);
                    }
                    self.visit_blocks(&mut a.children);
                }
                BlockNode::Div(d) => self.visit_blocks(&mut d.children),
                BlockNode::LineBlock(l) => self.visit_blocks(&mut l.children),
                BlockNode::DefinitionList(d) => {
                    for item in &mut d.items {
                        for term in &mut item.terms {
                            self.visit_inlines(term);
                        }
                        for def in &mut item.definitions {
                            self.visit_blocks(def);
                        }
                    }
                }
                BlockNode::Figure(f) => {
                    self.visit_inlines(&mut f.caption);
                    match &mut f.target {
                        FigureTarget::BlockQuote(b) => self.visit_blocks(&mut b.children),
                        FigureTarget::Table(t) => {
                            if let Some(c) = &mut t.caption {
                                self.visit_inlines(c);
                            }
                            for r in &mut t.rows {
                                for c in &mut r.cells {
                                    self.visit_inlines(&mut c.children);
                                }
                            }
                        }
                        FigureTarget::Paragraph(p) => self.visit_inlines(&mut p.children),
                        FigureTarget::CodeBlock(_) | FigureTarget::Image(_) => {}
                    }
                }
                BlockNode::FigureGroup(g) => {
                    if let Some(caption) = &mut g.caption {
                        self.visit_inlines(caption);
                    }
                    self.visit_blocks(&mut g.children);
                }
                BlockNode::Extension(e) => self.visit_blocks(&mut e.children),
                BlockNode::CodeBlock(_)
                | BlockNode::AbbreviationDef(_)
                | BlockNode::LinkReferenceDefinition(_)
                | BlockNode::RawBlock(_)
                | BlockNode::Comment(_)
                | BlockNode::BlockImage(_)
                | BlockNode::ThematicBreak(_) => {}
            }
        }
    }

    fn visit_inlines(&self, nodes: &mut [InlineNode]) {
        let apostrophes: Vec<bool> = nodes
            .iter()
            .enumerate()
            .map(|(index, node)| match node {
                InlineNode::SmartPunctuation(s) if s.kind == "right_single_quote" => {
                    s.value == "{'}"
                        || matches!(nodes.get(index + 1), Some(InlineNode::Text(t)) if t.value.chars().next().is_some_and(char::is_alphanumeric))
                }
                _ => false,
            })
            .collect();
        for (index, node) in nodes.iter_mut().enumerate() {
            match node {
                InlineNode::SmartPunctuation(s) => match s.kind.as_str() {
                    "left_double_quote" => s.glyph = Some(self.quotes[0].clone()),
                    "right_double_quote" => s.glyph = Some(self.quotes[1].clone()),
                    "left_single_quote" => s.glyph = Some(self.quotes[2].clone()),
                    "right_single_quote" if !apostrophes[index] => {
                        s.glyph = Some(self.quotes[3].clone())
                    }
                    _ => {}
                },
                InlineNode::Emphasis(e) => self.visit_inlines(&mut e.children),
                InlineNode::Link(l) => self.visit_inlines(&mut l.children),
                InlineNode::Span(s) => self.visit_inlines(&mut s.children),
                InlineNode::Extension(e) => self.visit_inlines(&mut e.children),
                InlineNode::CriticInsert(c) => self.visit_inlines(&mut c.children),
                InlineNode::CriticDelete(c) => self.visit_inlines(&mut c.children),
                InlineNode::Footnote(f) => {
                    if let Some(i) = &mut f.inline {
                        self.visit_inlines(i);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Default for SmartQuotes {
    fn default() -> Self {
        Self::new("en")
    }
}
impl CarveExtension for SmartQuotes {
    fn name(&self) -> &'static str {
        "smart-quotes"
    }
    fn after_parse(&self, mut doc: Document) -> Document {
        self.visit_blocks(&mut doc.children);
        for body in doc.footnote_defs.values_mut() {
            self.visit_blocks(body);
        }
        doc
    }
}

fn lookup_locale(locale: &str) -> Option<QuoteCharacters> {
    let normalized = locale.replace('_', "-");
    SMART_QUOTE_LOCALES
        .iter()
        .find_map(|(k, v)| (*k == normalized).then_some(*v))
        .or_else(|| {
            let lang = normalized.split('-').next().unwrap_or("en");
            SMART_QUOTE_LOCALES
                .iter()
                .find_map(|(k, v)| (*k == lang).then_some(*v))
        })
}
fn resolve_locale(locale: &str) -> QuoteCharacters {
    lookup_locale(locale).unwrap_or(SMART_QUOTE_LOCALES[0].1)
}
