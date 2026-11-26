//! The two shapes of named snippets: the definition marker `{$name}:`
//! (C2-B-236) and the use `{{ $name }}` (C2-I-128, C2-B-237). The block pass,
//! the inline scan, the writer and lint all read them here.

pub(crate) mod expand;

use crate::attr::{AttrBlock, parse_block};

/// The parts of a definition line.
#[derive(Debug, Clone)]
pub(crate) struct Marker {
    pub name_end: usize,
    /// Behind the `:`.
    pub end: usize,
    pub block: AttrBlock,
    /// A marker that holds an id makes no definition.
    pub has_id: bool,
}

const fn name_first(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// C2-B-236: the end of the snippet name at `pos`.
#[must_use]
pub(crate) fn name_end(s: &str, pos: usize) -> Option<usize> {
    let b = s.as_bytes();
    if !b.get(pos).copied().is_some_and(name_first) {
        return None;
    }
    let mut p = pos + 1;
    while b.get(p).is_some_and(|&c| name_first(c) || c == b'-') {
        p += 1;
    }
    Some(p)
}

/// C2-B-236: whether `text` is a snippet name.
#[must_use]
pub fn is_name(text: &str) -> bool {
    name_end(text, 0) == Some(text.len())
}

/// C2-B-236: the snippet marker at the start of `line`: `{$`, a name,
/// optionally spaces and attribute parts, `}`, glued attribute blocks and `:`.
#[must_use]
pub(crate) fn marker(line: &str) -> Option<Marker> {
    let rest = line.strip_prefix("{$")?;
    let name_end = name_end(rest, 0)? + 2;
    let b = line.as_bytes();
    let mut block = AttrBlock::default();
    let mut p = match b.get(name_end) {
        // The parts after the name read as the inside of a block whose brace is the last name character.
        Some(b' ') => {
            let inner = format!("{{{}", line.get(name_end..)?);
            let (parsed, end) = parse_block(&inner, 0)?;
            block = parsed;
            name_end + end - 1
        }
        Some(b'}') => name_end + 1,
        _ => return None,
    };
    while b.get(p) == Some(&b'{') {
        let (later, end) = parse_block(line, p)?;
        block.merge(later);
        p = end;
    }
    if b.get(p) != Some(&b':') {
        return None;
    }
    let has_id = block.attrs.id.is_some()
        || block
            .attrs
            .key_values
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("id"));
    Some(Marker {
        name_end,
        end: p + 1,
        block,
        has_id,
    })
}

/// C2-B-236: the start of the value of the definition `line`: a marker
/// without an id, one space, and a value that holds a character other than a
/// space or a tab.
#[must_use]
pub(crate) fn value_start(line: &str, m: &Marker) -> Option<usize> {
    if m.has_id || line.as_bytes().get(m.end) != Some(&b' ') {
        return None;
    }
    let value = line.get(m.end + 1..)?;
    value
        .bytes()
        .any(|c| !matches!(c, b' ' | b'\t' | b'\n' | b'\r'))
        .then_some(m.end + 1)
}

/// C2-I-128: the offset behind the use at `pos`, `{{`, spaces, `$`, a name,
/// spaces and `}}`, with the range of the name.
#[must_use]
pub(crate) fn use_at(s: &str, pos: usize) -> Option<(usize, usize, usize)> {
    let b = s.as_bytes();
    if b.get(pos) != Some(&b'{') || b.get(pos + 1) != Some(&b'{') {
        return None;
    }
    let mut p = pos + 2;
    while b.get(p) == Some(&b' ') {
        p += 1;
    }
    if p == pos + 2 || b.get(p) != Some(&b'$') {
        return None;
    }
    let start = p + 1;
    let end = name_end(s, start)?;
    p = end;
    while b.get(p) == Some(&b' ') {
        p += 1;
    }
    if p == end || b.get(p) != Some(&b'}') || b.get(p + 1) != Some(&b'}') {
        return None;
    }
    Some((p + 2, start, end))
}

/// C2-B-237: the name of a block use, a use with only spaces and tabs after it.
#[must_use]
pub(crate) fn block_use(line: &str) -> Option<&str> {
    let (after, start, end) = use_at(line, 0)?;
    line.get(after..)?
        .bytes()
        .all(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r'))
        .then(|| line.get(start..end))
        .flatten()
}

/// The source spelling of a use that was not expanded (C2-N-093).
#[must_use]
pub(crate) fn spelling(name: &str) -> String {
    format!("{{{{ ${name} }}}}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        assert!(marker("{$v}: X").is_some_and(|m| value_start("{$v}: X", &m) == Some(6)));
        assert!(marker("{$v #i}: X").is_some_and(|m| m.has_id));
        assert!(marker("{$v}{.c}: X").is_some_and(|m| m.block.attrs.classes == ["c"]));
        assert!(marker("{$v}:  ").is_some_and(|m| value_start("{$v}:  ", &m).is_none()));
        assert_eq!(use_at("{{  $a-b   }}", 0), Some((13, 5, 8)));
        assert_eq!(use_at("{{$a}}", 0), None);
        assert_eq!(block_use("{{ $v }} \t"), Some("v"));
        assert_eq!(spelling("v"), "{{ $v }}");
    }
}
