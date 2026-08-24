use crate::{parse::try_layout_html, Options};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamOutcome {
    Complete,
    NeedsAst,
}

/// Try the borrowed-layout render path without silently falling back.
///
/// The sink is not called unless the fast path accepted the complete document,
/// so a caller can safely run the AST renderer after `NeedsAst`. The first
/// implementation emits one complete chunk; later event and chunk extraction
/// can refine delivery without changing the fallback contract.
pub fn try_render_html_streaming(
    source: &str,
    options: &Options<'_>,
    mut sink: impl FnMut(&str),
) -> StreamOutcome {
    let Some(html) = try_layout_html(source, options) else {
        return StreamOutcome::NeedsAst;
    };
    sink(&html);
    StreamOutcome::Complete
}
