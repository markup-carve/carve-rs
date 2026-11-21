//! The 0.1 streaming HTML entry point over the 0.2 renderer.

use super::Options;

/// Result of [`try_render_html_streaming`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamOutcome {
    /// The sink received the complete HTML output.
    Complete,
    /// The sink was not called; the caller must use the regular renderer,
    /// which reports why the render gave no output.
    NeedsAst,
}

const CHUNK: usize = 4096;

/// Renders `source` and hands the HTML to `sink` in UTF-8 chunks of at most
/// 4096 bytes (one empty chunk for an empty render). The concatenated chunks equal [`super::render`]. A render
/// that [`super::render`] refuses returns [`StreamOutcome::NeedsAst`]
/// without calling the sink.
pub fn try_render_html_streaming(
    source: &str,
    options: &Options,
    mut sink: impl FnMut(&str),
) -> StreamOutcome {
    let Ok(html) = super::render(source, options) else {
        return StreamOutcome::NeedsAst;
    };
    let mut rest = html.as_str();
    if rest.is_empty() {
        sink("");
    }
    while !rest.is_empty() {
        let mut end = rest.len().min(CHUNK);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (chunk, tail) = rest.split_at(end);
        sink(chunk);
        rest = tail;
    }
    StreamOutcome::Complete
}
