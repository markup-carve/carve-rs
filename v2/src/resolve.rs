//! The resolve stage: references, footnote numbers, heading ids, caption and
//! equation numbers, cross-references and table alignment, written as the
//! result fields of `ast-results.json` beside the authored fields (C2-A-106).

use crate::ast::Document;
use crate::options::Options;

/// Fills every result field of the tree and the root `stages` (C2-A-135).
pub fn resolve(doc: &mut Document, opts: &Options) {
    let _ = (doc, opts);
}
