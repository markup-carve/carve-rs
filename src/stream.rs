use crate::{parse::try_layout_stream, Options};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamOutcome {
    Complete,
    NeedsAst,
}

/// Try the borrowed-layout render path without silently falling back.
///
/// The sink is not called unless the fast path accepted the complete document,
/// so a caller can safely run the AST renderer after `NeedsAst`.
/// Validation discards output; a second pass emits UTF-8 chunks of at most 4096 bytes.
pub fn try_render_html_streaming(
    source: &str,
    options: &Options<'_>,
    mut sink: impl FnMut(&str),
) -> StreamOutcome {
    if try_layout_stream(source, options, &mut sink) {
        StreamOutcome::Complete
    } else {
        StreamOutcome::NeedsAst
    }
}
