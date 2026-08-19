//! Shift every heading level down by a fixed offset.
//!
//! Ported from carve-js `heading-level-shift` and carve-php
//! `HeadingLevelShiftExtension`. Useful when `h1` is reserved for the page
//! title and the document's own headings should start at `h2` or lower.
//!
//! ```
//! use carve::{HeadingLevelShift, Options};
//! let ext = HeadingLevelShift::new();
//! let opts = Options::new().with_extension(&ext);
//! let html = carve::to_html_with_options("# Title", &opts);
//! assert!(html.contains("<h2"));
//! ```

use crate::ast::{BlockNode, Document, FigureTarget};
use crate::extension::{BeforeRenderContext, CarveExtension};

/// Options for [`HeadingLevelShift`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadingLevelShiftOptions {
    /// Levels to shift every heading down (`h1` -> `h2`, and so on). Clamped
    /// to `0..=5`, matching carve-js and carve-php: a shift wide enough to push
    /// every heading past `h6` would erase the document's structure rather than
    /// re-base it.
    pub shift: u8,
}

impl Default for HeadingLevelShiftOptions {
    fn default() -> Self {
        Self { shift: 1 }
    }
}

/// The extension itself. Register it through [`crate::Options::with_extension`].
#[derive(Debug, Clone, Copy, Default)]
pub struct HeadingLevelShift {
    opts: HeadingLevelShiftOptions,
}

impl HeadingLevelShift {
    /// Shift by one level, the default the other two engines use.
    pub fn new() -> Self {
        Self::default()
    }

    /// Shift by a caller-chosen number of levels, clamped to `0..=5`.
    pub fn with_options(opts: HeadingLevelShiftOptions) -> Self {
        Self {
            opts: HeadingLevelShiftOptions {
                shift: opts.shift.min(5),
            },
        }
    }
}

impl CarveExtension for HeadingLevelShift {
    fn name(&self) -> &'static str {
        "heading-level-shift"
    }

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
        if self.opts.shift == 0 {
            return doc;
        }

        shift_blocks(&mut doc.children, self.opts.shift);

        doc
    }
}

fn shift_blocks(blocks: &mut [BlockNode], shift: u8) {
    for block in blocks {
        shift_block(block, shift);
    }
}

/// Every container that can hold a heading is walked. A heading inside a
/// blockquote or a list item is still a heading of the document.
fn shift_block(block: &mut BlockNode, shift: u8) {
    match block {
        // Capped at `h6`: there is no `h7`, and the HTML writer would have to
        // invent one. Matches carve-php's `Heading::setLevel` clamp.
        BlockNode::Heading(heading) => heading.level = (heading.level + shift).min(6),
        BlockNode::BlockQuote(quote) => shift_blocks(&mut quote.children, shift),
        BlockNode::Div(div) => shift_blocks(&mut div.children, shift),
        BlockNode::Admonition(admonition) => shift_blocks(&mut admonition.children, shift),
        BlockNode::List(list) => {
            for item in &mut list.items {
                shift_blocks(&mut item.children, shift);
            }
        }
        BlockNode::DefinitionList(definition_list) => {
            for item in &mut definition_list.items {
                for definition in &mut item.definitions {
                    shift_blocks(&mut definition.children, shift);
                }
            }
        }
        // Only the blockquote target can hold one; an image, a table, a code
        // block and a paragraph cannot.
        BlockNode::Figure(figure) => {
            if let FigureTarget::BlockQuote(quote) = &mut *figure.target {
                shift_blocks(&mut quote.children, shift);
            }
        }
        BlockNode::FigureGroup(group) => shift_blocks(&mut group.children, shift),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Options;

    fn html(source: &str, ext: &HeadingLevelShift) -> String {
        let opts = Options::new().with_extension(ext);
        crate::to_html_with_options(source, &opts)
    }

    #[test]
    fn shifts_by_one_by_default() {
        let out = html("# Title", &HeadingLevelShift::new());
        assert!(out.contains("<h2"), "{out}");
        assert!(!out.contains("<h1"), "{out}");
    }

    #[test]
    fn caps_at_h6() {
        let ext = HeadingLevelShift::with_options(HeadingLevelShiftOptions { shift: 5 });
        let out = html("##### Five\n\n###### Six", &ext);
        assert!(out.contains("<h6"), "{out}");
        assert!(!out.contains("<h7"), "{out}");
    }

    #[test]
    fn a_shift_of_zero_leaves_the_document_alone() {
        let ext = HeadingLevelShift::with_options(HeadingLevelShiftOptions { shift: 0 });
        let out = html("# Title", &ext);
        assert!(out.contains("<h1"), "{out}");
    }

    #[test]
    fn an_over_wide_shift_is_clamped_rather_than_wrapping() {
        let ext = HeadingLevelShift::with_options(HeadingLevelShiftOptions { shift: 200 });
        let out = html("# Title", &ext);
        assert!(out.contains("<h6"), "{out}");
    }

    #[test]
    fn a_heading_inside_a_container_is_shifted_too() {
        let out = html("> # Quoted", &HeadingLevelShift::new());
        assert!(out.contains("<h2"), "{out}");
    }

    #[test]
    fn a_heading_inside_a_list_item_is_shifted_too() {
        let out = html("- # Item\n", &HeadingLevelShift::new());
        assert!(out.contains("<h2"), "{out}");
    }
}
