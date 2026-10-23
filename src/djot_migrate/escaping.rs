use super::*;

/// Escape Carve inline syntax that is ORDINARY TEXT in Djot.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HandledDelimiters<'a> {
    /// Braced runs (`{X…X}`) the caller's language spells too.
    pub braced: &'a str,
    /// Bare runs (`X…X`) the caller's language spells too.
    pub bare: &'a str,
}

impl HandledDelimiters<'_> {
    /// Djot: the language of `djot_to_carve`.
    pub(crate) const DJOT: HandledDelimiters<'static> = HandledDelimiters {
        braced: "=+-*_^~",
        bare: "~*_",
    };

    /// A language that owns none of these delimiters: HTML and BBCode text.
    ///
    /// The BBCode importer passes this after protecting its tag and literal
    /// spans, because BBCode owns none of Carve's inline delimiters.
    pub(crate) const PLAIN: HandledDelimiters<'static> = HandledDelimiters {
        braced: "",
        bare: "",
    };

    /// Markdown: the `markdown` profile of the shared escaper corpus.
    #[cfg(test)]
    pub(crate) const MARKDOWN: HandledDelimiters<'static> = HandledDelimiters {
        braced: "*_",
        bare: "*_~",
    };

    fn owns_braced(&self, delim: u8) -> bool {
        self.braced.as_bytes().contains(&delim)
    }

    fn owns_bare(&self, delim: u8) -> bool {
        self.bare.as_bytes().contains(&delim)
    }
}

/// Every braced run Carve spells. A caller's handled set is subtracted from
/// this; what is left is what gets frozen.
pub(super) const BRACED_DELIMITERS: &[u8] = b",/#=+-*_^~";

/// Every bare run Carve spells (PART 4: `/ * _ ~ =`).
pub(super) const BARE_DELIMITERS: &[u8] = b"/=~*_";

/// Freeze the Carve constructs in `source` that the caller's language leaves as
/// literal text, given the delimiters that language HANDLES itself.
pub(crate) fn escape_plain_carve_syntax(source: &str, handled: HandledDelimiters<'_>) -> String {
    let _paragraph_cache = ParagraphCacheScope::new(source);
    let masked = mask_code_and_destinations(source);
    escape_plain_carve_syntax_masked(source, handled, &masked)
}

pub(super) fn escape_plain_carve_syntax_masked(
    source: &str,
    handled: HandledDelimiters<'_>,
    masked: &str,
) -> String {
    let mask = masked.as_bytes();
    let mut at: Vec<usize> = Vec::new();

    // `%%` opens a comment at the start of a line or after whitespace, and a
    // longer run opens on its FIRST two: a third sign neither closes the
    // comment nor cancels it, so `a %%%c b` loses everything after the run.
    // One escape on the first sign makes the whole run literal, because the
    // rest then sit inside a word (carve-rs#2451).
    let mut i = 0;
    while i + 1 < mask.len() {
        if mask[i] == b'%'
            && mask[i + 1] == b'%'
            && !is_escaped(mask, i)
            && (i == 0 || matches!(mask[i - 1], b' ' | b'\t' | b'\n'))
        {
            at.push(i);
            i += 2;
            continue;
        }
        i += 1;
    }

    // Braced forms whose delimiter this converter does not own.
    for &delim in BRACED_DELIMITERS {
        if handled.owns_braced(delim) {
            continue;
        }
        let mut i = 0;
        while i + 3 < mask.len() {
            if mask[i] != b'{' || mask[i + 1] != delim || is_escaped(mask, i) {
                i += 1;
                continue;
            }
            match find_braced_close(mask, i + 2, delim) {
                Some(end) => {
                    at.push(i);
                    i = end + 2;
                }
                None => {
                    // AN UNCLOSED BRACED OPENER STILL FREEZES. The escaper's
                    // unit is a LINE, but a braced run is not: `a {^x` here and
                    // `y^} b` on the next line render one `<sup>x\ny</sup>`. An
                    // opener left bare therefore lets the NEXT line close it and
                    // turns two lines of literal text into markup, which is the
                    // one failure a line-oriented escaper cannot see from inside
                    // its own line (corpus case `braced-unclosed`). A bare pair
                    // is deliberately NOT treated this way - `bare-unclosed`
                    // pins it unchanged under every profile.
                    if braced_run_opens(mask, i + 2) {
                        at.push(i);
                    }
                    i += 1;
                }
            }
        }
    }

    // Bare pairs: the ones the caller's language does not spell are the author's
    // literal text. Under the Djot profile that is `/` (Carve emphasis) and `=`
    // (Carve highlight), neither of which is Djot syntax.
    for &delim in BARE_DELIMITERS {
        if handled.owns_bare(delim) {
            continue;
        }
        let mut closer = None;
        let mut line_is_blank = false;
        for i in (0..mask.len()).rev() {
            let byte = mask[i];
            if byte == b'\n' {
                if line_is_blank {
                    closer = None;
                }
                line_is_blank = true;
                continue;
            }
            if !matches!(byte, b' ' | b'\t' | b'\r') {
                line_is_blank = false;
            }
            if byte != delim || is_escaped(mask, i) {
                continue;
            }
            let before = i.checked_sub(1).map(|at| mask[at]);
            let after = mask.get(i + 1).copied();
            let whitespace = |byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n');
            let brace_protects =
                handled.owns_braced(delim) && before == Some(b'{') && !is_escaped(mask, i - 1);
            let opens = before != Some(delim)
                && !before.is_some_and(|byte| byte.is_ascii_alphanumeric())
                && !brace_protects
                && after.is_some_and(|byte| byte != delim && !whitespace(byte));
            if opens && closer.is_some_and(|at| at > i + 1) {
                at.push(i);
            }
            if before.is_some_and(|byte| !whitespace(byte))
                && !after.is_some_and(|byte| byte.is_ascii_alphanumeric())
            {
                closer = Some(i);
            }
        }
    }

    let mut i = 0;
    while i < mask.len() {
        if mask[i] != b'#' || is_escaped(mask, i) {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !mask[i - 1].is_ascii_alphanumeric();
        let opens_tag = mask
            .get(i + 1)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-');
        if before_ok && opens_tag {
            at.push(i);
        }
        i += 1;
    }

    // A MENTION is the tag's sibling and needs the same rule for the same
    // reason: it opens on its own, so nothing downstream neutralizes it.
    // Ported from carve-php#1381, which fixed the same gap there. Djot has no
    // mention either, so prose quoting a framework directive came back as a
    // span that existed nowhere in the source.
    //
    // Mirrors `parse_mention` rather than approximating it: a mention opens on
    // an `@` NOT preceded by an alphanumeric or `_` and followed by a name
    // character. The preceding-character test is what leaves an email address
    // alone, since `foo@bar` has a letter before the `@`.
    let mut i = 0;
    while i < mask.len() {
        if mask[i] != b'@' || is_escaped(mask, i) {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !(mask[i - 1].is_ascii_alphanumeric() || mask[i - 1] == b'_');
        let opens_mention = mask
            .get(i + 1)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-');
        if before_ok && opens_mention {
            at.push(i);
        }
        i += 1;
    }

    let mut i = 0;
    while i < mask.len() {
        if mask[i] != b':' || is_escaped(mask, i) {
            i += 1;
            continue;
        }
        if i > 0 && (mask[i - 1].is_ascii_alphanumeric() || mask[i - 1] == b'_') {
            i += 1;
            continue;
        }
        let Some(&first) = mask.get(i + 1) else {
            break;
        };
        if !first.is_ascii_alphanumeric() && first != b'+' && first != b'-' {
            i += 1;
            continue;
        }
        let mut len = 1;
        while let Some(&b) = mask.get(i + 1 + len) {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-' {
                len += 1;
            } else {
                break;
            }
        }
        if mask.get(i + 1 + len) == Some(&b':') {
            at.push(i);
            // The whole shortcode is consumed, the way the parser consumes it.
            i += len + 2;
        } else {
            i += 1;
        }
    }

    for (i, byte) in source.bytes().enumerate() {
        if byte == b'!'
            && source.as_bytes().get(i + 1) == Some(&b'`')
            && mask[i] == b'!'
            && !is_escaped(source.as_bytes(), i)
        {
            at.push(i);
        }
    }
    if at.is_empty() {
        return source.to_string();
    }

    at.sort_unstable();
    at.dedup();
    let bytes = source.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + at.len());
    let mut cursor = 0;
    for point in at {
        out.extend_from_slice(&bytes[cursor..point]);
        out.push(b'\\');
        cursor = point;
    }
    out.extend_from_slice(&bytes[cursor..]);

    String::from_utf8(out).expect("inserting an ASCII backslash preserves UTF-8")
}

/// Whether a `{X` whose content starts at `from` opens a braced run at all.
///
/// The parser needs non-space content against the delimiter, so `{ ^x^ }` opens
/// nothing and is ordinary text. This is what separates "does not open" from
/// "opens and is never closed": only the second freezes.
pub(super) fn braced_run_opens(mask: &[u8], from: usize) -> bool {
    mask.get(from).is_some_and(|b| !b.is_ascii_whitespace())
}

/// The offset of the `X}` that closes a `{X` opened before `from`, on the same
/// line, with non-space content between.
pub(super) fn find_braced_close(mask: &[u8], from: usize, delim: u8) -> Option<usize> {
    if !braced_run_opens(mask, from) {
        return None;
    }
    let mut j = from;
    while j + 1 < mask.len() {
        if mask[j] == b'\n' {
            return None;
        }
        if mask[j] == delim && mask[j + 1] == b'}' && j > from && !mask[j - 1].is_ascii_whitespace()
        {
            return Some(j);
        }
        j += 1;
    }
    None
}
