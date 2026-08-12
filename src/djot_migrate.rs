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
    // Before anything else, and deliberately as a same-length rewrite: `+` and
    // `-` are one byte each, so every offset the mask and the rules below
    // compute stays valid. Doing it afterwards would mean re-masking.
    let source = normalize_plus_bullets(&source);
    // Escaping inserts backslashes, so the mask the delimiter rules scan is
    // taken AFTER it rather than before.
    let source = escape_plain_carve_syntax(&source);
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
    /// The delimiter run that opens the construct.
    delimiter: &'static str,
    /// The delimiter run that closes it. Equal to `delimiter` for a symmetric
    /// construct such as `~x~`; different for a braced one, where `{~` opens
    /// and `~}` closes. Searching for the OPENER as the closer is why the
    /// `{=x=}` rule below could never fire - it looked correct only because
    /// its conversion is the identity, so a rule that never matched and a rule
    /// that matched and changed nothing produce the same output.
    closer: &'static str,
    /// Which delimiter character owns the range, so two rules over the same
    /// character cannot both claim it.
    family: char,
    open: &'static str,
    close: &'static str,
    /// `_` only opens and closes at a word boundary. See INTRAWORD below.
    word_bounded: bool,
    /// The inverse: this rule matches ONLY between word characters, which is
    /// the case the word-bounded rule above deliberately declines.
    intraword: bool,
}

/// Longest run first within a family: `~~x~~` is strikethrough, and only what
/// is left over is read as a subscript.
const RULES: &[Rule] = &[
    Rule {
        delimiter: "**",
        closer: "**",
        family: '*',
        open: "*",
        close: "*",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "~~",
        closer: "~~",
        family: '~',
        open: "~",
        close: "~",
        word_bounded: false,
        intraword: false,
    },
    // Djot spells subscript braced as well as bare and means the same by each.
    // The braced form is listed BEFORE the bare one and shares its family, so
    // it claims the range first and the bare rule's match inside the braces is
    // rejected by the overlap check. Converting it as one edit is what keeps
    // the source's own braces from being left behind around a replacement that
    // supplies its own.
    Rule {
        delimiter: "{~",
        closer: "~}",
        family: '~',
        open: "{,",
        close: ",}",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "~",
        closer: "~",
        family: '~',
        open: "{,",
        close: ",}",
        word_bounded: false,
        intraword: false,
    },
    // Braced superscript is spelled identically in both languages, so this is
    // the identity. It still needs a rule: claiming the range is what stops the
    // bare rule below from matching the `^x^` inside the braces and wrapping it
    // a second time into `{{^x^}}`.
    Rule {
        delimiter: "{^",
        closer: "^}",
        family: '^',
        open: "{^",
        close: "^}",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "^",
        closer: "^",
        family: '^',
        open: "{^",
        close: "^}",
        word_bounded: false,
        intraword: false,
    },
    Rule {
        delimiter: "_",
        closer: "_",
        family: '_',
        open: "/",
        close: "/",
        word_bounded: true,
        intraword: false,
    },
    // The complement of the rule above, and it CONVERTS rather than leaving the
    // run literal. The input is a DJOT document: Djot emphasizes an intraword
    // `_`, and an author who wanted the literal characters had to escape them.
    // `snake\_case\_name` renders as `snake_case_name` in Djot and arrives here
    // already escaped, so an UNESCAPED `snake_case_name` is emphasis the author
    // saw in their own renderer and kept.
    //
    // The braced form is required, not stylistic: a bare `/` is literal
    // intraword in Carve, so only `snake{/case/}name` renders as
    // `snake<em>case</em>name`.
    Rule {
        delimiter: "_",
        closer: "_",
        family: '_',
        open: "{/",
        close: "/}",
        word_bounded: false,
        intraword: true,
    },
    Rule {
        delimiter: "{=",
        closer: "=}",
        family: '{',
        open: "{=",
        close: "=}",
        word_bounded: false,
        intraword: false,
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
    let closer_width = rule.closer.len();
    let mut found = Vec::new();
    let mut i = 0;

    while i + width + closer_width <= bytes.len() {
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

        if rule.intraword {
            let before = if i > 0 {
                masked[..i].chars().next_back()
            } else {
                None
            };
            if !before.is_some_and(is_word_char) {
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
                found.push((i, inner_end + closer_width, inner_start, inner_end));
                i = inner_end + closer_width;
            }
            None => i += width,
        }
    }

    found
}

fn find_closer(masked: &str, from: usize, rule: &Rule) -> Option<usize> {
    let bytes = masked.as_bytes();
    let delimiter = rule.closer.as_bytes();
    let width = delimiter.len();
    // Only a symmetric construct can mistake a longer run of its own delimiter
    // for a closer; `~}` cannot be part of a `~~` run.
    let symmetric = rule.closer == rule.delimiter;
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

            if rule.intraword {
                let after = masked[j + width..].chars().next();
                if !after.is_some_and(is_word_char) {
                    j += width;
                    continue;
                }
            }

            // For a single-character rule the closer must not be part of a
            // longer run, which belongs to the longer rule.
            if symmetric && width == 1 && bytes.get(j + 1) == Some(&delimiter[0]) {
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

/// Rewrite a Djot `+` bullet marker to `-`.
///
/// Djot allows `-`, `*` and `+` as bullets. Carve has no `+` bullet: `+` is the
/// list-continuation marker, so a Djot `+` list degrades to a paragraph - the
/// line stops being a list item at all, which is a structural change and not a
/// delimiter one.
///
/// A LONE `+` is left alone. That is the Carve continuation marker itself, and
/// a marker with no content is exactly the form that means it, so rewriting it
/// would break the construct this rule exists to protect.
///
/// Lines inside a fenced block are skipped via the code mask. The rewrite is
/// one byte for one byte, so it runs before the mask is taken for the inline
/// rules and leaves every later offset valid.
fn normalize_plus_bullets(source: &str) -> String {
    let masked = mask_code_and_destinations(source);
    let mask = masked.as_bytes();
    let mut out = source.as_bytes().to_vec();

    let mut line_start = 0;
    while line_start <= out.len() {
        let line_end = mask[line_start..]
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| line_start + n)
            .unwrap_or(out.len());

        // A masked line is code: its `+` is content, not a marker.
        let mut i = line_start;
        while i < line_end && (mask[i] == b' ' || mask[i] == b'\t') {
            i += 1;
        }

        if i < line_end && mask[i] == b'+' {
            // `+ content` is a bullet; a bare `+` (or `+` then only spaces) is
            // the continuation marker and stays.
            let mut j = i + 1;
            let mut spaced = false;
            while j < line_end && (mask[j] == b' ' || mask[j] == b'\t') {
                spaced = true;
                j += 1;
            }
            if spaced && j < line_end {
                out[i] = b'-';
            }
        }

        if line_end >= out.len() {
            break;
        }
        line_start = line_end + 1;
    }

    String::from_utf8(out).expect("one-byte substitution preserves UTF-8")
}

/// Escape Carve inline syntax that is ORDINARY TEXT in Djot.
///
/// The converter's job has two halves and this is the half the delimiter rules
/// cannot do. `/x/`, `=x=`, `%% c` and `{,x,}` carry no meaning in Djot, so an
/// author writes them as text - but each one IS markup in Carve, so passing
/// them through unchanged renders something the source never said. `%% c` is
/// the worst of them: Carve reads it as a line comment and the line disappears
/// from the output entirely.
///
/// Only delimiters this converter does NOT itself rewrite are escaped. `~`,
/// `*` and `_` are Djot's own, so they are converted rather than escaped, and
/// `{^…^}`, `{=…=}`, `{+…+}` and `{-…-}` mean the same in both languages.
///
/// THE BRACE IS NOT ESCAPED ALONE. Escaping only the `{` of `{/y/}` leaves the
/// `/y/` inside it bare, and a bare `/` is Carve emphasis, so the "literal"
/// text still renders as `{<em>y</em>}`. The bare pass below therefore runs
/// over the escaped brace as well - it has no `{` exclusion - which is what
/// produces `\{\/y/}` and renders the text the author wrote. A delimiter with
/// no bare Carve form, such as `,`, needs only the brace.
fn escape_plain_carve_syntax(source: &str) -> String {
    let masked = mask_code_and_destinations(source);
    let mask = masked.as_bytes();
    let mut at: Vec<usize> = Vec::new();

    // `%%` opens a comment at the start of a line or after whitespace. `%%%` is
    // not a comment opener, so it is left alone.
    let mut i = 0;
    while i + 1 < mask.len() {
        if mask[i] == b'%'
            && mask[i + 1] == b'%'
            && mask.get(i + 2) != Some(&b'%')
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
    for delim in *b",/#" {
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
                None => i += 1,
            }
        }
    }

    // Bare pairs. `/` is Carve emphasis and `=` is Carve highlight; neither is
    // Djot syntax, so both are the author's literal text.
    for delim in *b"/=" {
        let mut i = 0;
        while i < mask.len() {
            if mask[i] != delim || is_escaped(mask, i) {
                i += 1;
                continue;
            }
            // `{` is excluded for `=` and NOT for `/`, and the asymmetry is the
            // point rather than an oversight. `{=x=}` is a highlight in both
            // languages, so the inner `=` is markup that must survive; there is
            // no `{/x/}` that means the same in both, so the `/` inside an
            // escaped brace is literal text and still needs escaping - escaping
            // the brace alone leaves it free to open Carve emphasis and renders
            // `{<em>x</em>}`.
            let brace_protects = delim == b'=';
            let before_ok = i == 0
                || !(mask[i - 1].is_ascii_alphanumeric()
                    || mask[i - 1] == delim
                    || (brace_protects && mask[i - 1] == b'{'));
            let after = mask.get(i + 1).copied().unwrap_or(b' ');
            if !before_ok || after.is_ascii_whitespace() || after == delim {
                i += 1;
                continue;
            }
            match find_bare_close(mask, i + 1, delim) {
                Some(end) => {
                    at.push(i);
                    i = end + 1;
                }
                None => i += 1,
            }
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

/// The offset of the `X}` that closes a `{X` opened before `from`, on the same
/// line, with non-space content between.
fn find_braced_close(mask: &[u8], from: usize, delim: u8) -> Option<usize> {
    if mask.get(from).is_some_and(|b| b.is_ascii_whitespace()) {
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

/// The offset of the delimiter closing a bare pair opened before `from`, using
/// the same word boundaries the Carve parser opens on.
fn find_bare_close(mask: &[u8], from: usize, delim: u8) -> Option<usize> {
    let mut j = from;
    while j < mask.len() {
        if mask[j] == b'\n' {
            return None;
        }
        if mask[j] == delim {
            let preceded_by_space = mask[j - 1].is_ascii_whitespace();
            let after = mask.get(j + 1).copied().unwrap_or(b' ');
            if j > from && !preceded_by_space && !after.is_ascii_alphanumeric() && after != delim {
                return Some(j);
            }
            return None;
        }
        j += 1;
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

    /// An intraword `_x_` CONVERTS, to the braced form. Djot emphasizes it, and
    /// an author who wanted the literal characters had to escape them, so an
    /// unescaped run is emphasis the source states rather than an identifier
    /// the converter should protect.
    #[test]
    fn an_intraword_underscore_converts_to_the_braced_form() {
        assert_eq!(
            djot_to_carve("snake_case_name stays"),
            "snake{/case/}name stays"
        );
        assert_eq!(djot_to_carve("MAX_BUFFER_SIZE"), "MAX{/BUFFER/}SIZE");
        assert_eq!(djot_to_carve("a _x_ and y_z_w"), "a /x/ and y{/z/}w");
    }

    /// The other side, and what makes it safe: the escape survives, so an
    /// author who did mean the identifier keeps it.
    #[test]
    fn an_escaped_intraword_underscore_is_left_alone() {
        assert_eq!(djot_to_carve("snake\\_case\\_name"), "snake\\_case\\_name");
    }

    /// BOUND: the word-bounded rule still emits the BARE form, and shapes that
    /// are not an intraword pair at all are untouched. Removing the intraword
    /// rule leaves every row here passing.
    #[test]
    fn the_surrounding_underscore_shapes_are_unchanged() {
        assert_eq!(djot_to_carve("a _x_ b"), "a /x/ b");
        assert_eq!(djot_to_carve("__init__"), "__init__");
        assert_eq!(djot_to_carve("_leading"), "_leading");
        assert_eq!(djot_to_carve("[t](/a_b_c)"), "[t](/a_b_c)");
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

    /// Djot spells subscript braced as well as bare and means the same by each,
    /// so the braced form converts too. It previously fell through untouched
    /// and stayed a Carve STRIKETHROUGH, which is a different word.
    #[test]
    fn the_braced_subscript_converts_like_the_bare_one() {
        assert_eq!(djot_to_carve("{~y~} a"), "{,y,} a");
        assert_eq!(djot_to_carve("a{~b~}c"), "a{,b,}c");
        assert_eq!(djot_to_carve("{~y~}"), djot_to_carve("~y~"));
    }

    /// The braced superscript is spelled identically in both languages, so the
    /// conversion is the identity. Without a rule claiming the range the bare
    /// rule matched the `^x^` INSIDE the braces and wrapped it again, into
    /// `{{^x^}}`.
    #[test]
    fn the_braced_superscript_is_not_wrapped_twice() {
        assert_eq!(djot_to_carve("{^x^} a"), "{^x^} a");
        // The inner `{,b,}` is literal text in Djot and a SUBSCRIPT in Carve,
        // so it is escaped rather than passed through. Rendering the result
        // gives Djot's `<sup>a{,b,}c</sup>` back.
        assert_eq!(djot_to_carve("{^a{,b,}c^} x"), "{^a\\{,b,}c^} x");
    }

    /// A `+` bullet is a list item in Djot and the CONTINUATION MARKER in
    /// Carve, so leaving it turns the list into a paragraph.
    #[test]
    fn the_plus_bullet_becomes_a_dash() {
        assert_eq!(djot_to_carve("+ one\n+ two\n"), "- one\n- two\n");
        assert_eq!(djot_to_carve("+ a\n  + b\n"), "- a\n  - b\n");
    }

    /// BOUND: a lone `+` IS the Carve continuation marker, and mid-line text is
    /// not a marker at all. Neither moves under this change.
    #[test]
    fn a_lone_plus_and_an_inline_plus_are_left_alone() {
        assert_eq!(djot_to_carve("+\n"), "+\n");
        assert_eq!(djot_to_carve("a + b\n"), "a + b\n");
    }

    /// Carve syntax that is ordinary text in Djot has to be escaped or the
    /// conversion renders something the source never said. `%%` is the sharpest
    /// case: Carve reads it as a line comment and the line DISAPPEARS.
    #[test]
    fn carve_syntax_that_is_plain_djot_text_is_escaped() {
        assert_eq!(djot_to_carve("%% not a comment\n"), "\\%% not a comment\n");
        assert_eq!(djot_to_carve("/slashes/ x\n"), "\\/slashes/ x\n");
        assert_eq!(djot_to_carve("=marked= x\n"), "\\=marked= x\n");
        assert_eq!(djot_to_carve("{,sub,} x\n"), "\\{,sub,} x\n");
    }

    /// Escaping the brace ALONE is not enough when the delimiter inside it has
    /// a bare Carve form: `\\{/y/}` still renders `{<em>y</em>}`.
    #[test]
    fn a_braced_delimiter_with_a_bare_form_escapes_both() {
        assert_eq!(djot_to_carve("{/y/} x\n"), "\\{\\/y/} x\n");
    }

    /// BOUND, and the reason the rule above is delimiter-specific: `{=x=}` is a
    /// highlight in BOTH languages, so its inner `=` is markup that must
    /// survive. Escaping it would break the one braced form that already works.
    #[test]
    fn the_braced_highlight_keeps_its_inner_delimiter() {
        assert_eq!(djot_to_carve("{=marked=} x"), "{=marked=} x");
    }

    /// BOUND: escaping never reaches code, a destination, or an unpaired
    /// delimiter. None of these move under any of the rules above.
    #[test]
    fn code_destinations_and_unpaired_delimiters_are_untouched() {
        assert_eq!(djot_to_carve("a `_x_` b\n"), "a `_x_` b\n");
        assert_eq!(djot_to_carve("ftp://x/ y\n"), "ftp://x/ y\n");
        assert_eq!(djot_to_carve("a/b/c\n"), "a/b/c\n");
        assert_eq!(djot_to_carve("%%% not\n"), "%%% not\n");
        assert_eq!(djot_to_carve("```\n/code/\n```\n"), "```\n/code/\n```\n");
    }
}
