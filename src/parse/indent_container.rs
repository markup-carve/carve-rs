use super::*;

/// Physical-line admission shared by item, description, and footnote bodies.
/// Each caller still handles its own block and definition side effects.
#[derive(Clone, Copy)]
pub(super) struct IndentContainer {
    pub marker_column: usize,
    pub content_column: usize,
    pub owner: continuation_marker::Kind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ContainerLine {
    Blank,
    Content,
    OwnedContinuation,
    Below,
    Outside,
}

#[derive(Clone, Copy)]
pub(super) struct ContainerAdmission {
    pub blank: bool,
    pub preceding_blanks: usize,
}

macro_rules! run_indented_body {
    ($source:expr, $index:expr, $container:ident, |$line:ident, $admission:ident| $body:block) => {{
        let source = $source;
        let mut preceding_blanks = 0usize;
        let mut unobserved = $index;
        let mut previous = None;
        while $index < source.len() {
            debug_assert!(previous.map_or(true, |at| $index > at));
            for at in unobserved..$index {
                preceding_blanks = if is_blank_line(source.at(at)) {
                    preceding_blanks + 1
                } else {
                    0
                };
            }
            let $line = source.at($index);
            let $admission = ContainerAdmission {
                blank: is_blank_line($line),
                preceding_blanks,
            };
            debug_assert!($admission.preceding_blanks <= $index);
            unobserved = $index;
            previous = Some($index);
            $body
        }
    }};
}
pub(super) use run_indented_body;

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
            && decision.is_some_and(|decision| decision.owner == Some(self.owner))
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
        } else if indent_columns(line) >= self.marker_column {
            ContainerLine::Below
        } else {
            ContainerLine::Outside
        }
    }
}
