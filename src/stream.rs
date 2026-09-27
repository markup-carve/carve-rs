use crate::{parse::try_layout_html, Options};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamOutcome {
    Complete,
    NeedsAst,
}

/// Try the borrowed-layout render path without silently falling back.
///
/// The sink is not called unless the fast path accepted the complete document,
/// so a caller can safely run the AST renderer after `NeedsAst`.
/// HTML is buffered before delivery in newline-terminated chunks.
pub fn try_render_html_streaming(
    source: &str,
    options: &Options<'_>,
    mut sink: impl FnMut(&str),
) -> StreamOutcome {
    let Some(html) = try_layout_html(source, options) else {
        return StreamOutcome::NeedsAst;
    };
    for chunk in html.split_inclusive('\n') {
        sink(chunk);
    }
    if html.is_empty() {
        sink("");
    }
    StreamOutcome::Complete
}
