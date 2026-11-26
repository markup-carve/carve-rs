//! The physical lines of an included file (C2-N-034) and its region markers
//! (C2-N-037 to C2-N-041).

/// Line `i` is `starts[i]..ends[i]` without its line ending; `nexts[i]` is
/// where the line after it starts.
pub(super) struct FileLines {
    pub starts: Vec<usize>,
    pub ends: Vec<usize>,
    pub nexts: Vec<usize>,
}

impl FileLines {
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    pub fn line<'t>(&self, text: &'t str, i: usize) -> &'t str {
        match (self.starts.get(i), self.ends.get(i)) {
            (Some(&s), Some(&e)) => text.get(s..e).unwrap_or(""),
            _ => "",
        }
    }
}

/// C2-B-010, C2-N-034: a final line ending begins no further line, and a
/// leading U+FEFF is not content.
pub(super) fn file_lines(text: &str) -> FileLines {
    let mut out = FileLines {
        starts: Vec::new(),
        ends: Vec::new(),
        nexts: Vec::new(),
    };
    let mut start = if text.starts_with('\u{FEFF}') { 3 } else { 0 };
    while start < text.len() {
        let lf = text
            .get(start..)
            .and_then(|r| r.find('\n'))
            .map(|i| start + i);
        let next = lf.map_or(text.len(), |i| i + 1);
        let mut end = lf.unwrap_or(text.len());
        if lf.is_some() && end > start && text.as_bytes().get(end - 1) == Some(&b'\r') {
            end -= 1;
        }
        out.starts.push(start);
        out.ends.push(end);
        out.nexts.push(next);
        start = next;
    }
    out
}

/// Lines by index, `from` included and `to` excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LineRange {
    pub from: usize,
    pub to: usize,
}

/// C2-B-205: the bounds of a range value the parse found in shape.
pub(super) fn bounds(value: &str) -> Option<(usize, usize)> {
    let (a, b) = value.split_once('-')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// C2-N-035: the lines of `@lines:N-M`, clipped at the last line; `None`
/// when the range starts past it.
pub(super) fn line_range(value: &str, count: usize) -> Option<LineRange> {
    let (first, last) = bounds(value)?;
    (first >= 1 && first <= count).then(|| LineRange {
        from: first - 1,
        to: last.min(count),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    None,
    Malformed,
    Opener,
    Closer,
}

fn skip_blanks(line: &str, at: usize) -> usize {
    at + line
        .get(at..)
        .unwrap_or("")
        .bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .count()
}

/// C2-N-038, C2-N-039: where the keyword may start behind a comment opener,
/// and the closer the line must end with.
fn marker_body(line: &str, at: usize, code: bool) -> Option<(usize, &'static str)> {
    let rest = line.get(at..).unwrap_or("");
    if let Some(after) = rest.strip_prefix("%%") {
        return (!after.starts_with('%')).then_some((at + 2, ""));
    }
    if !code {
        return None;
    }
    if rest.starts_with("//") || rest.starts_with("--") {
        Some((at + 2, ""))
    } else if rest.starts_with("/*") {
        Some((at + 2, "*/"))
    } else if rest.starts_with("<!--") {
        Some((at + 4, "-->"))
    } else {
        rest.starts_with('#').then_some((at, ""))
    }
}

fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// The marker after its keyword: blanks, an optional name, blanks, the closer, blanks, nothing else.
fn marker_tail<'l>(line: &'l str, at: usize, closer: &str, kind: Kind) -> (Kind, &'l str) {
    let bytes = line.as_bytes();
    let mut p = skip_blanks(line, at);
    let mut name = "";
    if p > at
        && bytes
            .get(p)
            .is_some_and(|&b| b.is_ascii_alphabetic() || b == b'_')
    {
        let start = p;
        while bytes.get(p).is_some_and(|&b| is_name_char(b)) {
            p += 1;
        }
        name = line.get(start..p).unwrap_or("");
        p = skip_blanks(line, p);
    }
    if !closer.is_empty() {
        if !line.get(p..).is_some_and(|r| r.starts_with(closer)) {
            return (Kind::Malformed, "");
        }
        p = skip_blanks(line, p + closer.len());
    }
    if p != line.len() || (kind == Kind::Opener && name.is_empty()) {
        return (Kind::Malformed, "");
    }
    (kind, name)
}

/// C2-N-038 to C2-N-040: the region marker on the line.
pub(super) fn read_marker(line: &str, code: bool) -> (Kind, &str) {
    if !possible_marker(line, code) {
        return (Kind::None, "");
    }
    let Some((body, closer)) = marker_body(line, skip_blanks(line, 0), code) else {
        return (Kind::None, "");
    };
    let at = skip_blanks(line, body);
    let rest = line.get(at..).unwrap_or("");
    let (end, kind) = if rest.starts_with("#endregion") {
        (at + 10, Kind::Closer)
    } else if rest.starts_with("#region") {
        (at + 7, Kind::Opener)
    } else {
        return (Kind::None, "");
    };
    if line
        .as_bytes()
        .get(end)
        .is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return (Kind::None, "");
    }
    marker_tail(line, end, closer, kind)
}

pub(super) fn possible_marker(line: &str, code: bool) -> bool {
    let first = line.as_bytes().first().copied();
    matches!(first, Some(b'%' | b' ' | b'\t'))
        || code && matches!(first, Some(b'/' | b'-' | b'<' | b'#'))
}

pub(crate) fn is_carve_region_marker(line: &str) -> bool {
    matches!(read_marker(line, false).0, Kind::Opener | Kind::Closer)
}

/// C2-N-046: a well-formed marker line of a code include.
pub(super) fn is_marker_line(line: &str) -> bool {
    matches!(read_marker(line, true).0, Kind::Opener | Kind::Closer)
}

pub(super) const REGION_MALFORMED: &str = "has a malformed or unmatched marker in the file";
pub(super) const REGION_MISSING: &str = "is not opened in the file";

/// C2-N-037, C2-N-041: the lines strictly between the markers of `name`.
/// `skip` names the lines no marker is read on (inside a quoted field, C2-N-046).
pub(super) fn region_range(
    text: &str,
    lines: &FileLines,
    name: &str,
    code: bool,
    skip: &dyn Fn(usize) -> bool,
) -> Result<LineRange, &'static str> {
    region_scan(
        (0..lines.len()).map(|i| (i, lines.line(text, i))),
        name,
        code,
        skip,
    )
}

pub(super) fn region_scan<'a>(
    lines: impl Iterator<Item = (usize, &'a str)>,
    name: &str,
    code: bool,
    skip: impl Fn(usize) -> bool,
) -> Result<LineRange, &'static str> {
    let mut stack: Vec<(usize, &str)> = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut range = None;
    for (i, line) in lines {
        if skip(i) {
            continue;
        }
        match read_marker(line, code) {
            (Kind::None, _) => {}
            (Kind::Malformed, _) => return Err(REGION_MALFORMED),
            (Kind::Opener, n) => {
                if !seen.insert(n) {
                    return Err(REGION_MALFORMED);
                }
                stack.push((i, n));
            }
            (Kind::Closer, n) => {
                let open = stack.pop().ok_or(REGION_MALFORMED)?;
                if !n.is_empty() && n != open.1 {
                    return Err(REGION_MALFORMED);
                }
                if open.1 == name {
                    range = Some(LineRange {
                        from: open.0 + 1,
                        to: i,
                    });
                }
            }
        }
    }
    if !stack.is_empty() {
        return Err(REGION_MALFORMED);
    }
    range.ok_or(REGION_MISSING)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c2_n_034_lines_skip_bom_and_final_ending() {
        let l = file_lines("\u{FEFF}a\r\nb\n");
        assert_eq!((l.starts.clone(), l.ends.clone()), (vec![3, 6], vec![4, 7]));
        assert_eq!(file_lines("\u{FEFF}").len(), 0);
    }

    #[test]
    fn c2_n_040_keyword_run_on() {
        assert!(read_marker("%% #regionx a", false).0 == Kind::None);
        assert!(read_marker("%% #region-a", false).0 == Kind::Malformed);
        assert!(read_marker("%%% #region a", false).0 == Kind::None);
        assert!(read_marker("#region a", false).0 == Kind::None);
        assert!(read_marker("/* #region a */", true).0 == Kind::Opener);
        assert!(read_marker("<!-- #endregion -->", true).0 == Kind::Closer);
    }

    #[test]
    fn c2_n_041_nested_and_repeated() {
        let t = "%% #region a\n%% #region b\nx\n%% #endregion\n%% #endregion a\n";
        let l = file_lines(t);
        assert_eq!(
            region_range(t, &l, "b", false, &|_| false),
            Ok(LineRange { from: 2, to: 3 })
        );
        assert_eq!(
            region_range(t, &l, "a", false, &|_| false),
            Ok(LineRange { from: 1, to: 4 })
        );
        let t = "%% #region a\n%% #endregion\n%% #region a\n%% #endregion\n";
        assert_eq!(
            region_range(t, &file_lines(t), "a", false, &|_| false),
            Err(REGION_MALFORMED)
        );
    }
}
