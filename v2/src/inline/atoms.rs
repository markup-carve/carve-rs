//! Shape tests of the atoms (C2-I-005): each takes the content text and an
//! offset and returns what it matched, without touching the token list.

/// A code span (C2-I-014 to C2-I-017).
pub struct CodeSpan {
    pub end: usize,
    pub value: String,
    pub closed: bool,
}

fn run_len(b: &[u8], i: usize, c: u8) -> usize {
    b.get(i..)
        .map_or(0, |s| s.iter().take_while(|&&x| x == c).count())
}

/// Reads a code span whose backtick run starts at `i`.
#[must_use]
pub fn code_span(t: &str, i: usize) -> CodeSpan {
    let b = t.as_bytes();
    let n = run_len(b, i, b'`');
    let start = i + n;
    let mut j = start;
    while j < b.len() {
        if b.get(j) == Some(&b'`') {
            let m = run_len(b, j, b'`');
            if m == n {
                let value = trim_code(t.get(start..j).unwrap_or(""));
                return CodeSpan {
                    end: j + m,
                    value,
                    closed: true,
                };
            }
            j += m;
        } else {
            j += 1;
        }
    }
    // C2-I-017: unclosed, it runs to the end and trailing whitespace is dropped.
    let value = t
        .get(start..)
        .unwrap_or("")
        .trim_end_matches([' ', '\t', '\n'])
        .to_string();
    CodeSpan {
        end: b.len(),
        value,
        closed: false,
    }
}

/// C2-I-016: one space off each end when both are spaces and not all is space.
fn trim_code(s: &str) -> String {
    if s.len() >= 2 && s.starts_with(' ') && s.ends_with(' ') && s.bytes().any(|c| c != b' ') {
        s.get(1..s.len() - 1).unwrap_or(s).to_string()
    } else {
        s.to_string()
    }
}

/// A raw marker `{=name}` at `i` (C2-I-021). Returns the name and the end.
#[must_use]
pub fn raw_marker(t: &str, i: usize) -> Option<(String, usize)> {
    let b = t.as_bytes();
    if b.get(i) != Some(&b'{') || b.get(i + 1) != Some(&b'=') {
        return None;
    }
    let first = *b.get(i + 2)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut j = i + 3;
    while b
        .get(j)
        .is_some_and(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        j += 1;
    }
    (b.get(j) == Some(&b'}')).then(|| (t.get(i + 2..j).unwrap_or("").to_string(), j + 1))
}

/// C2-I-122: a URL character.
fn url_char(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+,;=%".contains(c)
    } else {
        !crate::chars::is_flank_space(c)
            && !c.is_whitespace()
            && !crate::chars::is_control_or_format(c)
    }
}

fn is_url(body: &str) -> bool {
    let Some((scheme, rest)) = body.split_once(':') else {
        return false;
    };
    let mut sc = scheme.chars();
    sc.next().is_some_and(|c| c.is_ascii_alphabetic())
        && sc.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && !rest.is_empty()
        && rest.chars().all(url_char)
}

const fn email_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_' | b'+')
}

/// C2-I-123: local part, at-sign, domain, a dot and ASCII letters.
fn is_email(body: &str) -> bool {
    let Some((local, domain)) = body.split_once('@') else {
        return false;
    };
    let Some((host, last)) = domain.rsplit_once('.') else {
        return false;
    };
    !local.is_empty()
        && local.bytes().all(email_char)
        && !host.is_empty()
        && host.bytes().all(email_char)
        && !last.is_empty()
        && last.bytes().all(|c| c.is_ascii_alphabetic())
}

/// What an angle bracket starts.
pub enum Angle {
    Xref {
        key: String,
        end: usize,
    },
    Autolink {
        href: String,
        body: String,
        end: usize,
    },
}

/// An autolink or a cross-reference at `i` (C2-I-073, C2-I-096, C2-I-124).
#[must_use]
pub fn angle(t: &str, i: usize) -> Option<Angle> {
    let rest = t.get(i + 1..)?;
    if let Some(id) = rest.strip_prefix("/#") {
        let len = id.find(['>', ' ', '\n']).unwrap_or(id.len());
        if len > 0 && id.as_bytes().get(len) == Some(&b'>') {
            let key = id.get(..len).unwrap_or("").to_string();
            return Some(Angle::Xref {
                key,
                end: i + 3 + len + 1,
            });
        }
    }
    let len = rest.find(|c: char| c == '>' || c == '<' || c.is_whitespace())?;
    if rest.as_bytes().get(len) != Some(&b'>') {
        return None;
    }
    let body = rest.get(..len)?;
    let end = i + 1 + len + 1;
    if is_url(body) {
        Some(Angle::Autolink {
            href: body.to_string(),
            body: body.to_string(),
            end,
        })
    } else if is_email(body) {
        Some(Angle::Autolink {
            href: format!("mailto:{body}"),
            body: body.to_string(),
            end,
        })
    } else {
        None
    }
}

const fn name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

/// The name of a mention or a tag at `i` (C2-I-092). Returns its end.
#[must_use]
pub fn sigil_name(t: &str, i: usize) -> Option<usize> {
    let b = t.as_bytes();
    let mut j = i;
    loop {
        match b.get(j) {
            Some(&c) if name_char(c) => j += 1,
            Some(b'.') if j > i && b.get(j + 1).copied().is_some_and(name_char) => j += 1,
            _ => break,
        }
    }
    while j > i && matches!(b.get(j - 1), Some(b'-' | b'.')) {
        j -= 1;
    }
    let name = b.get(i..j)?;
    name.iter().any(u8::is_ascii_alphabetic).then_some(j)
}

/// A symbol `:name:` whose colon is at `i` (C2-I-093). Returns the name end.
#[must_use]
pub fn symbol(t: &str, i: usize) -> Option<usize> {
    let b = t.as_bytes();
    let first = *b.get(i + 1)?;
    if !(first.is_ascii_alphanumeric() || first == b'+' || first == b'-') {
        return None;
    }
    let mut j = i + 2;
    while b
        .get(j)
        .is_some_and(|&c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'+' | b'-'))
    {
        j += 1;
    }
    (b.get(j) == Some(&b':') && b.get(j - 1) != Some(&b'-')).then_some(j)
}

/// An inline extension name `:name[` whose colon is at `i` (C2-I-094).
/// Returns the offset of the bracket.
#[must_use]
pub fn extension(t: &str, i: usize) -> Option<usize> {
    let b = t.as_bytes();
    let first = *b.get(i + 1)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut j = i + 2;
    while b.get(j).copied().is_some_and(name_char) {
        j += 1;
    }
    (b.get(j) == Some(&b'[')).then_some(j)
}

/// A link tail (C2-I-066 to C2-I-069).
pub struct Tail {
    pub dest: String,
    pub title: Option<String>,
    pub end: usize,
}

/// Reads a link tail at `i`, which holds `(`. `cell` reads `\|` as `|` (C2-B-224).
#[must_use]
pub fn link_tail(t: &str, i: usize, cell: bool) -> Option<Tail> {
    let b = t.as_bytes();
    let mut dest = String::new();
    let mut depth = 0usize;
    let mut j = i + 1;
    loop {
        let c = t.get(j..)?.chars().next()?;
        match c {
            ' ' | '\t' | '\n' => break,
            '\\' => {
                let n = b.get(j + 1).copied();
                if matches!(n, Some(b'(' | b')' | b'\\')) || (cell && n == Some(b'|')) {
                    dest.push(char::from(n.unwrap_or(b'\\')));
                    j += 2;
                    continue;
                }
                dest.push('\\');
            }
            '(' => {
                depth += 1;
                dest.push('(');
            }
            ')' if depth == 0 => {
                return Some(Tail {
                    dest,
                    title: None,
                    end: j + 1,
                });
            }
            ')' => {
                depth -= 1;
                dest.push(')');
            }
            c => dest.push(c),
        }
        j += c.len_utf8();
    }
    if b.get(j) != Some(&b' ') {
        return None;
    }
    let q = *b.get(j + 1).filter(|&&q| q == b'"' || q == b'\'')?;
    let (title, mut k) = quoted(t, j + 2, q)?;
    while b.get(k) == Some(&b' ') {
        k += 1;
    }
    (b.get(k) == Some(&b')')).then(|| Tail {
        dest,
        title: Some(title),
        end: k + 1,
    })
}

/// A quoted title body from `i` up to the closing `q` (C2-I-068, escapes read).
fn quoted(t: &str, mut i: usize, q: u8) -> Option<(String, usize)> {
    let mut out = String::new();
    loop {
        let c = t.get(i..)?.chars().next()?;
        match c {
            '\n' => return None,
            '\\' => match t.get(i + 1..).and_then(|s| s.chars().next()) {
                Some(n) if n.is_ascii_punctuation() => {
                    out.push(n);
                    i += 2;
                    continue;
                }
                _ => out.push('\\'),
            },
            c if c as u32 == u32::from(q) => return Some((out, i + 1)),
            c => out.push(c),
        }
        i += c.len_utf8();
    }
}

/// A reference or footnote label.
pub enum Label {
    /// The raw label source between the brackets and the end behind `]`.
    Ok(usize, usize),
    Unclosed,
}

/// C2-I-023: `%%` at `j` starts a line comment.
#[must_use]
pub fn comment_start(t: &str, j: usize) -> bool {
    let b = t.as_bytes();
    if b.get(j) != Some(&b'%') || b.get(j + 1) != Some(&b'%') {
        return false;
    }
    if t.get(j + 2..)
        .and_then(|s| s.chars().next())
        .is_some_and(crate::chars::is_joining)
    {
        return false;
    }
    j == 0
        || t.get(..j)
            .and_then(|s| s.chars().next_back())
            .is_some_and(crate::chars::is_flank_space)
}

/// Reads a label from `i` (behind its `[`) to the next unescaped `]`
/// (C2-I-076, C2-I-077: a line comment in it leaves it unclosed).
#[must_use]
pub fn label(t: &str, i: usize, comments: bool) -> Label {
    let b = t.as_bytes();
    let mut j = i;
    while let Some(&c) = b.get(j) {
        match c {
            b'\\' => j += 2,
            b']' => return Label::Ok(j, j + 1),
            b'%' if comments && comment_start(t, j) => return Label::Unclosed,
            _ => j += 1,
        }
    }
    Label::Unclosed
}
