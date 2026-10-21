use super::*;

/// Physical-line admission shared by item, description, and footnote bodies.
/// Each caller still handles its own block and definition side effects.
#[derive(Clone, Copy)]
pub(super) struct IndentContainer {
    pub content_column: usize,
    pub owner: continuation_marker::Kind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ContainerLine {
    Blank,
    Content,
    OwnedContinuation,
    Outside,
}

impl IndentContainer {
    pub(super) fn blank_run_end(self, lines: CursorLines<'_>, start: usize) -> usize {
        let mut end = start;
        while end < lines.len() && is_blank_line(lines.at(end)) {
            end += 1;
        }
        end
    }

    pub(super) fn owns_continuation(
        self,
        line: &str,
        decision: Option<continuation_marker::Decision>,
    ) -> bool {
        is_plus_marker(trim_ascii_start(line))
            && decision.is_some_and(|decision| {
                decision.owner == Some(self.owner) && decision.refusal.is_none()
            })
    }

    pub(super) fn classify(
        self,
        line: &str,
        decision: impl FnOnce() -> Option<continuation_marker::Decision>,
    ) -> ContainerLine {
        if is_blank_line(line) {
            ContainerLine::Blank
        } else if indent_columns(line) >= self.content_column {
            ContainerLine::Content
        } else if is_plus_marker(trim_ascii_start(line)) && self.owns_continuation(line, decision())
        {
            ContainerLine::OwnedContinuation
        } else {
            ContainerLine::Outside
        }
    }
}
