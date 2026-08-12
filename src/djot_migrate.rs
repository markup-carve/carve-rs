//! Migrate Djot source to Carve source.
//!
//! Port of carve-php `DjotToCarve`, which is itself the converter form of the
//! carve-js `djot-migrate` linter - the canonical list of Djot/Carve delimiter
//! collisions.
//!
//! Several inline delimiters mean different things in the two languages, so a
//! Djot document fed to a Carve processor renders WRONG WITH NO ERROR. This
//! rewrites exactly those:
//!
//! | Djot | Carve | why |
//! |---|---|---|
//! | `_x_` | `/x/` | Djot emphasis is underline in Carve |
//! | `~x~` | `{,x,}` | Djot subscript is strikethrough in Carve |
//! | `^x^` | `{^x^}` | Carve has no bare superscript; the braced form is the only one |
//! | `**x**` | `*x*` | Markdown bold; Carve bold is a single `*` |
//! | `~~x~~` | `~x~` | Markdown strikethrough; Carve strike is a single `~` |
//! | `{=x=}` | `{=x=}` | already the same braced form |
//!
//! Constructs that mean the same in both languages - `$math$`, `{+ins+}`,
//! `{-del-}`, reference links - are left alone. Delimiters inside code, fenced
//! or inline, and inside link destinations are never rewritten.
//!
//! ```
//! assert_eq!(carve::djot_to_carve("_em_ and ~sub~"), "/em/ and {,sub,}");
//! ```

/// Convert Djot source to Carve source.
pub fn djot_to_carve(djot: &str) -> String {
    let source = djot.replace("\r\n", "\n").replace('\r', "\n");
    let masked = mask_code_and_destinations(&source);

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut taken: Vec<(char, usize, usize)> = Vec::new();

    for rule in RULES {
        for (start, end, inner_start, inner_end) in find_pairs(&masked, rule) {
            // One delimiter run belongs to one rule. A `~~x~~` claimed by the
            // strikethrough rule must not be re-read as two subscripts.
            if taken
                .iter()
                .any(|(family, s, e)| *family == rule.family && start < *e && *s < end)
            {
                continue;
            }
            taken.push((rule.family, start, end));
            // Only the DELIMITERS are replaced, never the inner text, so a
            // construct of another family nested inside this one is still
            // rewritten by its own rule rather than swallowed whole.
            edits.push((start, inner_start, rule.open.to_string()));
            edits.push((inner_end, end, rule.close.to_string()));
        }
    }

    if edits.is_empty() {
        return source;
    }

    edits.sort_by_key(|(start, _, _)| *start);

    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;
    for (start, end, replacement) in edits {
        if start < cursor {
            continue;
        }
        out.push_str(&source[cursor..start]);
        out.push_str(&replacement);
        cursor = end;
    }
    out.push_str(&source[cursor..]);

    out
}

struct Rule {
    /// The delimiter run that opens and closes the construct.
    delimiter: &'static str,
    /// Which delimiter character owns the range, so two rules over the same
    /// character cannot both claim it.
    family: char,
    open: &'static str,
    close: &'static str,
    /// `_` only opens and closes at a word boundary. See INTRAWORD below.
    word_bounded: bool,
}

/// Longest run first within a family: `~~x~~` is strikethrough, and only what
/// is left over is read as a subscript.
const RULES: &[Rule] = &[
    Rule {
        delimiter: "**",
        family: '*',
        open: "*",
        close: "*",
        word_bounded: false,
    },
    Rule {
        delimiter: "~~",
        family: '~',
        open: "~",
        close: "~",
        word_bounded: false,
    },
    Rule {
        delimiter: "~",
        family: '~',
        open: "{,",
        close: ",}",
        word_bounded: false,
    },
    Rule {
        delimiter: "^",
        family: '^',
        open: "{^",
        close: "^}",
        word_bounded: false,
    },
    Rule {
        delimiter: "_",
        family: '_',
        open: "/",
        close: "/",
        word_bounded: true,
    },
    Rule {
        delimiter: "{=",
        family: '{',
        open: "{=",
        close: "=}",
        word_bounded: false,
    },
];

/// INTRAWORD UNDERSCORES ARE DELIBERATELY LEFT ALONE, and this is a choice
/// about what the author MEANT rather than about what Djot says.
///
/// A strict Djot reader emphasizes them: pandoc's Djot reader turns
/// `snake_case_name` into `snake<em>case</em>name`. This converter does not,
/// because the documents it exists for - notes, READMEs, generated docs - are
/// full of `snake_case` identifiers that no author intended as emphasis, and
/// Carve itself leaves an intraword `_` literal for exactly that reason.
///
/// So the migration is faithful to intent, not to a strict reading, and the
/// cost is real: a Djot document that DID mean emphasis there loses it
/// silently. Documented rather than left as a surprise in the pattern.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Is the byte at `index` escaped by a backslash? An odd run of backslashes
/// before it escapes it; an even run is literal backslashes and the character
/// still counts. `\_not em\_` is text, not emphasis.
fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let mut backslashes = 0;
    let mut k = index;
    while k > 0 && bytes[k - 1] == b'\\' {
        backslashes += 1;
        k -= 1;
    }

    backslashes % 2 == 1
}

/// Every `(start, end, inner_start, inner_end)` this rule matches, scanning
/// left to right and never overlapping itself.
fn find_pairs(masked: &str, rule: &Rule) -> Vec<(usize, usize, usize, usize)> {
    let bytes = masked.as_bytes();
    let delimiter = rule.delimiter.as_bytes();
    let width = delimiter.len();
    let mut found = Vec::new();
    let mut i = 0;

    while i + width * 2 <= bytes.len() {
        if !bytes[i..].starts_with(delimiter) {
            i += 1;
            continue;
        }

        // An escaped delimiter is literal text and opens nothing.
        if is_escaped(bytes, i) {
            i += width;
            continue;
        }

        // A `~~` opener must not be read as the `~` rule's opener plus content,
        // so a longer run of the same character is not an opener for the
        // shorter rule.
        if width == 1 && bytes.get(i + 1) == Some(&delimiter[0]) {
            i += 2;
            continue;
        }

        if rule.word_bounded && i > 0 {
            let before = masked[..i].chars().next_back();
            if before.is_some_and(is_word_char) {
                i += width;
                continue;
            }
        }

        let inner_start = i + width;
        // An opener is not one when whitespace follows it.
        if bytes
            .get(inner_start)
            .is_some_and(|b| b.is_ascii_whitespace())
        {
            i += width;
            continue;
        }

        match find_closer(masked, inner_start, rule) {
            Some(inner_end) => {
                found.push((i, inner_end + width, inner_start, inner_end));
                i = inner_end + width;
            }
            None => i += width,
        }
    }

    found
}

fn find_closer(masked: &str, from: usize, rule: &Rule) -> Option<usize> {
    let bytes = masked.as_bytes();
    let delimiter = rule.delimiter.as_bytes();
    let width = delimiter.len();
    let mut j = from;

    while j + width <= bytes.len() {
        // A construct never spans a blank line: that is a paragraph break, and
        // a delimiter on the far side of one closes nothing.
        if bytes[j] == b'\n' && blank_line_follows(bytes, j) {
            return None;
        }

        if bytes[j..].starts_with(delimiter) {
            if is_escaped(bytes, j) {
                j += width;
                continue;
            }

            // A closer is not one when whitespace precedes it, and an empty
            // pair is not a construct.
            let preceded_by_space = j > from && bytes[j - 1].is_ascii_whitespace();
            if j == from || preceded_by_space {
                j += 1;
                continue;
            }

            if rule.word_bounded {
                let after = masked[j + width..].chars().next();
                if after.is_some_and(is_word_char) {
                    j += width;
                    continue;
                }
            }

            // For a single-character rule the closer must not be part of a
            // longer run, which belongs to the longer rule.
            if width == 1 && bytes.get(j + 1) == Some(&delimiter[0]) {
                j += 2;
                continue;
            }

            return Some(j);
        }

        j += 1;
    }

    None
}

/// Is the newline at `index` followed by a line holding nothing but spaces and
/// tabs, i.e. a paragraph break?
fn blank_line_follows(bytes: &[u8], index: usize) -> bool {
    let mut k = index + 1;
    while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
        k += 1;
    }

    k >= bytes.len() || bytes[k] == b'\n'
}

/// Replace every byte of code and every link destination with a space, so the
/// scan above cannot see a delimiter that is not one. Offsets are preserved, so
/// a match in the mask splices into the original unchanged.
fn mask_code_and_destinations(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut mask: Vec<u8> = bytes.to_vec();
    let mut i = 0;

    while i < bytes.len() {
        // A fenced block: everything to the closing fence is verbatim.
        if at_line_start(bytes, i) {
            if let Some((fence_char, run)) = fence_at(bytes, i) {
                let body = i + run;
                let end = find_fence_close(bytes, body, fence_char, run);
                blank_out(&mut mask, i, end);
                i = end;
                continue;
            }
        }

        // An inline code span, delimited by a matching backtick run.
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
            if let Some(close) = find_backtick_close(bytes, i + run, run) {
                blank_out(&mut mask, i, close + run);
                i = close + run;
                continue;
            }
        }

        // A link or image destination: `](...)`.
        if bytes[i] == b']' && bytes.get(i + 1) == Some(&b'(') {
            if let Some(close) = bytes[i + 2..].iter().position(|b| *b == b')') {
                let end = i + 2 + close + 1;
                blank_out(&mut mask, i + 2, end - 1);
                i = end;
                continue;
            }
        }

        i += 1;
    }

    String::from_utf8(mask).unwrap_or_else(|_| source.to_string())
}

fn blank_out(mask: &mut [u8], from: usize, to: usize) {
    let end = to.min(mask.len());
    for byte in &mut mask[from..end] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

fn at_line_start(bytes: &[u8], index: usize) -> bool {
    index == 0 || bytes[index - 1] == b'\n'
}

fn fence_at(bytes: &[u8], index: usize) -> Option<(u8, usize)> {
    let fence_char = bytes[index];
    if fence_char != b'`' && fence_char != b'~' {
        return None;
    }
    let run = bytes[index..]
        .iter()
        .take_while(|b| **b == fence_char)
        .count();

    (run >= 3).then_some((fence_char, run))
}

fn find_fence_close(bytes: &[u8], from: usize, fence_char: u8, open_run: usize) -> usize {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let line_start = i + 1;
            let mut k = line_start;
            while k < bytes.len() && bytes[k] == fence_char {
                k += 1;
            }
            if k - line_start >= open_run {
                return k;
            }
        }
        i += 1;
    }

    bytes.len()
}

fn find_backtick_close(bytes: &[u8], from: usize, run: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let here = bytes[i..].iter().take_while(|b| **b == b'`').count();
            if here == run {
                return Some(i);
            }
            i += here;
            continue;
        }
        i += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasis_becomes_the_carve_spelling() {
        assert_eq!(djot_to_carve("_em_ text"), "/em/ text");
    }

    #[test]
    fn subscript_and_superscript_take_the_braced_form() {
        assert_eq!(djot_to_carve("H~2~O"), "H{,2,}O");
        assert_eq!(djot_to_carve("E=mc^2^"), "E=mc{^2^}");
    }

    #[test]
    fn the_markdown_habits_collapse_to_one_delimiter() {
        assert_eq!(djot_to_carve("**strong**"), "*strong*");
        assert_eq!(djot_to_carve("~~gone~~"), "~gone~");
    }

    #[test]
    fn highlight_is_already_the_same_form() {
        assert_eq!(djot_to_carve("{=marked=}"), "{=marked=}");
    }

    /// The documented intent choice: an identifier is not emphasis.
    #[test]
    fn an_intraword_underscore_is_left_alone() {
        assert_eq!(
            djot_to_carve("snake_case_name stays"),
            "snake_case_name stays"
        );
    }

    #[test]
    fn code_is_never_rewritten() {
        assert_eq!(
            djot_to_carve("`_no_ **no**` yes _yes_"),
            "`_no_ **no**` yes /yes/"
        );
        assert_eq!(
            djot_to_carve("``` js\n_no_\n```\n\n_yes_"),
            "``` js\n_no_\n```\n\n/yes/"
        );
    }

    #[test]
    fn a_destination_is_never_rewritten() {
        assert_eq!(
            djot_to_carve("[t](http://e.com/_a_/b) and _yes_"),
            "[t](http://e.com/_a_/b) and /yes/"
        );
    }

    #[test]
    fn a_pair_never_spans_a_paragraph_break() {
        assert_eq!(djot_to_carve("_a\n\nb_"), "_a\n\nb_");
        assert_eq!(djot_to_carve("_a\nb_"), "/a\nb/");
    }

    #[test]
    fn an_escaped_delimiter_is_literal_text() {
        assert_eq!(djot_to_carve("\\_not em\\_"), "\\_not em\\_");
    }

    #[test]
    fn constructs_that_mean_the_same_are_untouched() {
        assert_eq!(djot_to_carve("{+add+} and {-cut-}"), "{+add+} and {-cut-}");
    }

    #[test]
    fn nesting_of_different_families_composes() {
        assert_eq!(djot_to_carve("_a **b** c_"), "/a *b* c/");
    }
}
