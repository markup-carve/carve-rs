//! Resolve `[[Heading Text]]` references to intra-document heading links.
//!
//! Port of carve-js `heading-reference.ts` and carve-php
//! `HeadingReferenceExtension`. A reference names a heading by its PLAIN TEXT
//! rather than by a guessed id, so an author never has to know the slug rules;
//! `[[Heading Text|click here]]` sets its own display text.
//!
//! A reference to a heading that does not exist - or to text that appears on
//! more than one heading, where no choice would be right - falls back to its
//! literal `[[…]]` source, so nothing is silently swallowed.
//!
//! Like carve-php, this shares the `[[…]]` syntax with [`crate::Wikilinks`];
//! enable one or the other on a render, not both.
//!
//! ```
//! use carve::{HeadingReference, Options};
//! let ext = HeadingReference::new();
//! let opts = Options::new().with_extension(&ext);
//! let html = carve::to_html_with_options("See [[Getting Started]].\n\n# Getting Started", &opts);
//! assert!(html.contains("class=\"heading-ref\""));
//! ```

use std::collections::BTreeMap;

use crate::ast::{AttrSlot, Attrs, BlockNode, Document, InlineNode, Link, Text};
use crate::extension::{BeforeRenderContext, CarveExtension, InlineMatch, MatcherContext};

/// Key-value marker the matcher writes so the resolution pass can find its own
/// links without inspecting every link in the document.
const MARK: &str = "data-heading-ref";

/// Options for [`HeadingReference`].
#[derive(Debug, Clone)]
pub struct HeadingReferenceOptions {
    /// Class(es) on the resolved anchor, space separated. Default `heading-ref`.
    pub css_class: String,
}

impl Default for HeadingReferenceOptions {
    fn default() -> Self {
        Self {
            css_class: "heading-ref".to_string(),
        }
    }
}

/// Resolve `[[Heading Text]]` references to intra-document heading links.
#[derive(Debug, Clone, Default)]
pub struct HeadingReference {
    opts: HeadingReferenceOptions,
}

impl HeadingReference {
    /// A heading-reference extension with the default anchor class.
    pub fn new() -> Self {
        Self::default()
    }

    /// A heading-reference extension with a caller-chosen anchor class.
    pub fn with_options(opts: HeadingReferenceOptions) -> Self {
        Self { opts }
    }

    fn classes(&self) -> Vec<String> {
        self.opts
            .css_class
            .split(' ')
            .filter(|class| !class.is_empty())
            .map(str::to_string)
            .collect()
    }
}

impl CarveExtension for HeadingReference {
    fn name(&self) -> &'static str {
        "heading-reference"
    }

    fn match_inline(
        &self,
        text: &str,
        pos: usize,
        _ctx: &MatcherContext<'_>,
    ) -> Option<InlineMatch> {
        let rest = text.get(pos..)?;
        let inner = rest.strip_prefix("[[")?;

        // A leading `]`, `|` or `#` is not a reference: the first two are empty
        // targets and the third is a tag, which core owns.
        let first = inner.chars().next()?;
        if first == ']' || first == '|' || first == '#' {
            return None;
        }

        let close_rel = inner.find("]]")?;
        let body = &inner[..close_rel];
        let (raw_target, display) = match body.find('|') {
            Some(bar) => (&body[..bar], Some(body[bar + 1..].trim())),
            None => (body, None),
        };

        // The target may not carry a `]` of its own before the close.
        if raw_target.contains(']') {
            return None;
        }

        let target = raw_target.trim();
        if target.is_empty() {
            return None;
        }

        let display = display.filter(|text| !text.is_empty()).unwrap_or(target);
        let mut key_values = BTreeMap::new();
        key_values.insert(MARK.to_string(), target.to_string());

        let link = Link {
            attrs: Some(Attrs {
                id: None,
                classes: self.classes(),
                key_values,
                order: vec![AttrSlot::Class, AttrSlot::Key(MARK.to_string())],
            }),
            href: String::new(),
            title: None,
            children: vec![InlineNode::Text(Text {
                value: display.to_string(),
                pos: None,
            })],
            ref_label: None,
            raw_ref: None,
            from_crossref: false,
            // Core resolves the spec's `[Heading][]` reference form (PART 11
            // R1) and marks those links with this flag so the canonical writer
            // can reproduce the authored spelling. A `[[…]]` reference is a
            // different construct with its own source text, so it is not one
            // of those and must not claim to be.
            from_heading_reference: false,
            pos: None,
        };

        Some(InlineMatch {
            node: InlineNode::Link(link),
            end: pos + 2 + close_rel + 2,
        })
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        // Heading ids are assigned by now, so the map is built from the
        // resolved document rather than from a slug guess.
        let mut targets: BTreeMap<String, String> = BTreeMap::new();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();

        collect_headings(&doc.children, &mut targets, &mut counts);
        resolve_blocks(&mut doc.children, &targets, &counts);

        for blocks in doc.footnote_defs.values_mut() {
            resolve_blocks(blocks, &targets, &counts);
        }

        doc
    }
}

/// Straight and curly quotes name the same heading. A reference typed with
/// straight quotes has to match a heading the smart-typography pass curled.
fn normalize_quotes(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{201C}' | '\u{201D}' => '"',
            '\u{2018}' | '\u{2019}' => '\'',
            other => other,
        })
        .collect()
}

fn collect_headings(
    blocks: &[BlockNode],
    targets: &mut BTreeMap<String, String>,
    counts: &mut BTreeMap<String, usize>,
) {
    for block in blocks {
        if let BlockNode::Heading(heading) = block {
            let Some(id) = heading.attrs.as_ref().and_then(|attrs| attrs.id.as_ref()) else {
                continue;
            };
            let text = normalize_quotes(inline_text(&heading.children).trim());
            if text.is_empty() {
                continue;
            }
            *counts.entry(text.clone()).or_insert(0) += 1;
            targets.entry(text).or_insert_with(|| id.clone());
        }
    }
}

fn resolve_blocks(
    blocks: &mut [BlockNode],
    targets: &BTreeMap<String, String>,
    counts: &BTreeMap<String, usize>,
) {
    for block in blocks {
        match block {
            BlockNode::Heading(heading) => resolve_inlines(&mut heading.children, targets, counts),
            BlockNode::Paragraph(paragraph) => {
                resolve_inlines(&mut paragraph.children, targets, counts)
            }
            BlockNode::BlockQuote(quote) => resolve_blocks(&mut quote.children, targets, counts),
            BlockNode::Div(div) => resolve_blocks(&mut div.children, targets, counts),
            BlockNode::Admonition(admonition) => {
                resolve_blocks(&mut admonition.children, targets, counts)
            }
            BlockNode::Extension(extension) => {
                resolve_blocks(&mut extension.children, targets, counts)
            }
            BlockNode::List(list) => {
                for item in &mut list.items {
                    resolve_blocks(&mut item.children, targets, counts);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        resolve_inlines(&mut cell.children, targets, counts);
                    }
                }
            }
            BlockNode::DefinitionList(definition_list) => {
                for item in &mut definition_list.items {
                    for term in &mut item.terms {
                        resolve_inlines(&mut term.children, targets, counts);
                    }
                    for definition in &mut item.definitions {
                        resolve_blocks(&mut definition.children, targets, counts);
                    }
                }
            }
            _ => {}
        }
    }
}

fn resolve_inlines(
    nodes: &mut [InlineNode],
    targets: &BTreeMap<String, String>,
    counts: &BTreeMap<String, usize>,
) {
    for node in nodes.iter_mut() {
        if let InlineNode::Link(link) = node {
            let marked = link
                .attrs
                .as_ref()
                .and_then(|attrs| attrs.key_values.get(MARK))
                .cloned();

            if let Some(target) = marked {
                let display = inline_text(&link.children);
                let normalized = normalize_quotes(&target);

                match targets.get(&normalized) {
                    // Exactly one heading carries this text: point at it and
                    // drop the marker, which was only ever ours.
                    Some(id) if counts.get(&normalized) == Some(&1) => {
                        link.href = format!("#{id}");
                        if let Some(attrs) = link.attrs.as_mut() {
                            attrs.key_values.remove(MARK);
                            attrs
                                .order
                                .retain(|slot| !matches!(slot, AttrSlot::Key(key) if key == MARK));
                        }
                    }
                    // Missing, or ambiguous across several headings: there is
                    // no right answer, so the author's source text stands.
                    _ => {
                        let literal = if target == display {
                            format!("[[{target}]]")
                        } else {
                            format!("[[{target}|{display}]]")
                        };
                        *node = InlineNode::Text(Text {
                            value: literal,
                            pos: None,
                        });
                    }
                }

                continue;
            }
        }

        match node {
            InlineNode::Emphasis(emphasis) => {
                resolve_inlines(&mut emphasis.children, targets, counts)
            }
            InlineNode::Link(link) => resolve_inlines(&mut link.children, targets, counts),
            InlineNode::Span(span) => resolve_inlines(&mut span.children, targets, counts),
            InlineNode::Extension(extension) => {
                resolve_inlines(&mut extension.children, targets, counts)
            }
            _ => {}
        }
    }
}

fn inline_text(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(text) => out.push_str(&text.value),
            InlineNode::Code(code) => out.push_str(&code.value),
            InlineNode::LiteralInline(literal) => out.push_str(&literal.content),
            InlineNode::Emphasis(emphasis) => out.push_str(&inline_text(&emphasis.children)),
            InlineNode::Link(link) => out.push_str(&inline_text(&link.children)),
            InlineNode::Span(span) => out.push_str(&inline_text(&span.children)),
            InlineNode::Extension(extension) => out.push_str(&inline_text(&extension.children)),
            _ => {}
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Options;

    fn html(source: &str) -> String {
        let ext = HeadingReference::new();
        let opts = Options::new().with_extension(&ext);
        crate::to_html_with_options(source, &opts)
    }

    #[test]
    fn a_reference_resolves_to_the_headings_id() {
        let out = html("See [[Getting Started]].\n\n# Getting Started");
        assert!(out.contains("class=\"heading-ref\""), "{out}");
        assert!(out.contains("href=\"#"), "{out}");
        assert!(!out.contains("[["), "{out}");
    }

    #[test]
    fn a_bar_sets_the_display_text() {
        let out = html("See [[Getting Started|start here]].\n\n# Getting Started");
        assert!(out.contains(">start here</a>"), "{out}");
    }

    #[test]
    fn a_missing_heading_falls_back_to_the_source_text() {
        let out = html("See [[Nowhere]].");
        assert!(out.contains("[[Nowhere]]"), "{out}");
        assert!(!out.contains("heading-ref"), "{out}");
    }

    #[test]
    fn an_ambiguous_heading_falls_back_rather_than_guessing() {
        let out = html("See [[Notes]].\n\n# Notes\n\n## Notes");
        assert!(out.contains("[[Notes]]"), "{out}");
    }

    #[test]
    fn a_fallback_keeps_the_authors_display_text() {
        let out = html("See [[Nowhere|over there]].");
        assert!(out.contains("[[Nowhere|over there]]"), "{out}");
    }

    #[test]
    fn a_reference_inside_a_container_resolves_too() {
        let out = html("> See [[Getting Started]].\n\n# Getting Started");
        assert!(out.contains("class=\"heading-ref\""), "{out}");
    }

    #[test]
    fn a_tag_is_left_to_core() {
        let out = html("[[#tag]]");
        assert!(!out.contains("heading-ref"), "{out}");
    }

    #[test]
    fn an_empty_target_is_not_a_reference() {
        let out = html("[[]] and [[ ]]");
        assert!(!out.contains("heading-ref"), "{out}");
    }
}
