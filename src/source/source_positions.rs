//! Codepoint line indexes shared by parsing and lint diagnostics.

/// Line starts in codepoints, kept distinct from byte-indexed source maps.
pub(crate) struct CodepointLineStarts(Vec<usize>);

impl CodepointLineStarts {
    /// Index original input, treating CRLF and lone CR as newlines and retaining
    /// the leading BOM's width before the first content character.
    pub(crate) fn original(source: &str) -> Self {
        let mut chars = source.chars().peekable();
        let mut starts = Vec::new();
        let mut count = 0usize;
        if chars.peek() == Some(&'\u{feff}') {
            chars.next();
            count += 1;
        }
        starts.push(count);
        while let Some(ch) = chars.next() {
            count += 1;
            if ch == '\r' {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                    count += 1;
                }
                starts.push(count);
            } else if ch == '\n' {
                starts.push(count);
            }
        }

        Self(starts)
    }

    pub(crate) fn get(&self, line_index: usize) -> Option<&usize> {
        self.0.get(line_index)
    }

    pub(crate) fn has_leading_bom(&self) -> bool {
        self.0.first() == Some(&1)
    }

    pub(crate) fn last(&self) -> Option<&usize> {
        self.0.last()
    }
}
