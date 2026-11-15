//! The HTML renderer (C2-H): a resolved tree to an HTML fragment.

use crate::ast::Document;
use crate::options::Options;

/// Renders a resolved tree (C2-H-001).
#[must_use]
pub fn render(doc: &Document, opts: &Options) -> String {
    let _ = (doc, opts);
    String::new()
}
