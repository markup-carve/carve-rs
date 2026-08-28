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

use std::collections::HashSet;

use crate::ast::{BlockNode, FigureTarget};

/// Convert Djot source to Carve source.
pub fn djot_to_carve(djot: &str) -> String {
    let source = djot.replace("\r\n", "\n").replace('\r', "\n");
    // Before anything else, and deliberately as a same-length rewrite: `+` and
    // `-` are one byte each, so every offset the mask and the rules below
    // compute stays valid. Doing it afterwards would mean re-masking.
    let source = normalize_plus_bullets(&source);
    // Layout, not a delimiter, and it runs before the escape pass because it
    // works on whole lines: a blank-line run Djot reads as nothing is a list
    // boundary in Carve.
    let source = collapse_false_list_boundaries(&source);
    // Escaping inserts backslashes, so the mask the delimiter rules scan is
    // taken AFTER it rather than before.
    let source = escape_plain_carve_syntax(&source, HandledDelimiters::DJOT);
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

/// Collapse a blank-line run that Carve alone would read as a list boundary.
///
/// Djot reads ANY run of blank lines between two compatible sibling markers as
/// one list. Carve reads a run of THREE OR MORE as a hard boundary (PART 9 §11
/// N1a) and opens a second list after it. So passing the author's run through
/// splits a list the source never split, and it does it silently: the halves
/// render as `</ul><ul>`, which shows nothing at all for a bullet list, and on
/// an ordered list restarts the numbering.
///
/// A run of three or more blank lines before a list-marker line therefore
/// collapses to ONE blank line, which is how Carve spells what the Djot source
/// said - one loose list. Other runs are left alone: between two paragraphs the
/// count means nothing in either language, and rewriting it would edit layout
/// the author chose for no gain.
///
/// The run must also FOLLOW a list, so the rewrite fires only where the two
/// languages disagree. Blank lines inside a fenced block are that block's
/// content and are skipped.
fn collapse_false_list_boundaries(source: &str) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let fenced = fenced_lines(&lines);

    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let run_start = i;
        let depth = quoted(lines[i]).0;
        while i < lines.len() && !fenced[i] && is_blank(lines[i]) && quoted(lines[i]).0 == depth {
            i += 1;
        }
        let run = i - run_start;
        let above = out.iter().rev().find(|line| !is_blank(line));
        if run >= 3
            && i < lines.len()
            && at_the_same_depth(lines[i], depth, opens_a_list_item)
            && above.is_some_and(|line| at_the_same_depth(line, depth, continues_a_list))
        {
            // The run's own first line, so a quoted run keeps its `>` prefix
            // and an unquoted one stays the empty line it already was.
            out.push(lines[run_start]);
            continue;
        }
        if run > 0 {
            out.extend_from_slice(&lines[run_start..i]);
            continue;
        }
        out.push(lines[i]);
        i += 1;
    }

    out.join("\n")
}

/// A line's block-quote depth and what it holds inside the quote markers. The
/// boundary applies inside a quote as much as outside it, and there a "blank"
/// line is written `>` -- so both the run and the markers around it are read
/// through the prefix rather than off the raw line.
fn quoted(line: &str) -> (usize, &str) {
    let mut rest = line;
    let mut depth = 0;
    loop {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        match trimmed.strip_prefix('>') {
            Some(after) => {
                depth += 1;
                rest = after.strip_prefix(' ').unwrap_or(after);
            }
            // The remainder is NOT trimmed: an item's own indentation is what
            // says the line continues the item, and inside a quote it sits
            // right after the marker.
            None => return (depth, rest),
        }
    }
}

/// Whether a line carries no content, inside whatever quote holds it.
fn is_blank(line: &str) -> bool {
    quoted(line).1.trim().is_empty()
}

/// Whether a line sits at `depth` and its content answers `test`. A marker one
/// quote level away from the run separates nothing the run could join.
fn at_the_same_depth(line: &str, depth: usize, test: fn(&str) -> bool) -> bool {
    let (line_depth, content) = quoted(line);

    line_depth == depth && test(content)
}

/// Which lines sit inside a fenced block, whose blank lines are content.
fn fenced_lines(lines: &[&str]) -> Vec<bool> {
    let mut inside: Vec<bool> = Vec::with_capacity(lines.len());
    let mut open: Option<(char, usize)> = None;
    for line in lines {
        // Through the quote prefix: a fence opened inside a quote holds its
        // blank lines as content exactly like an unquoted one.
        let trimmed = quoted(line).1.trim_start();
        let first = trimmed.chars().next();
        let run = match first {
            Some(c @ ('`' | '~')) => trimmed.chars().take_while(|x| *x == c).count(),
            _ => 0,
        };
        match open {
            Some((fence_char, open_run)) => {
                inside.push(true);
                if run >= open_run && first == Some(fence_char) {
                    open = None;
                }
            }
            None => {
                if run >= 3 {
                    open = Some((first.expect("a run implies a character"), run));
                    inside.push(true);
                } else {
                    inside.push(false);
                }
            }
        }
    }

    inside
}

/// Whether a line is a list-item marker line: a bullet or an ordered marker
/// followed by content. A marker with nothing after it is not an item.
fn opens_a_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    match chars.next() {
        Some('-' | '*' | '+') => {
            matches!(chars.next(), Some(' ' | '\t')) && !chars.as_str().trim().is_empty()
        }
        Some(c) if c.is_ascii_alphanumeric() => {
            let head: String = trimmed
                .chars()
                .take_while(|x| x.is_ascii_alphanumeric())
                .collect();
            let rest = &trimmed[head.len()..];
            let mut rest = rest.chars();
            matches!(rest.next(), Some('.' | ')'))
                && matches!(rest.next(), Some(' ' | '\t'))
                && !rest.as_str().trim().is_empty()
        }
        _ => false,
    }
}

/// Whether a list is open above the run: the nearest line with content is
/// either a marker line or an item's indented content. Djot has no indented code blocks,
/// so an indented line under a list is that list's content and nothing else.
fn continues_a_list(line: &str) -> bool {
    opens_a_list_item(line) || line.starts_with(' ') || line.starts_with('\t')
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
    let continuation_lines = if masked
        .lines()
        .any(|line| line.trim_start_matches([' ', '\t']).starts_with('+') && line.contains('|'))
    {
        table_continuation_lines(source)
    } else {
        HashSet::new()
    };

    let mut line_start = 0;
    let mut line_number = 1;
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

        if i < line_end && mask[i] == b'+' && !continuation_lines.contains(&line_number) {
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
        line_number += 1;
    }

    String::from_utf8(out).expect("one-byte substitution preserves UTF-8")
}

fn table_continuation_lines(source: &str) -> HashSet<usize> {
    fn visit(blocks: &[BlockNode], lines: &mut HashSet<usize>) {
        for block in blocks {
            match block {
                BlockNode::Table(table) => visit_table(table, lines),
                BlockNode::List(list) => {
                    for item in &list.items {
                        visit(&item.children, lines);
                    }
                }
                BlockNode::BlockQuote(node) => visit(&node.children, lines),
                BlockNode::Admonition(node) => visit(&node.children, lines),
                BlockNode::Div(node) => visit(&node.children, lines),
                BlockNode::LineBlock(node) => visit(&node.children, lines),
                BlockNode::DefinitionList(list) => {
                    for item in &list.items {
                        for definition in &item.definitions {
                            visit(&definition.children, lines);
                        }
                    }
                }
                BlockNode::Figure(figure) => match &*figure.target {
                    FigureTarget::Table(table) => visit_table(table, lines),
                    FigureTarget::BlockQuote(node) => visit(&node.children, lines),
                    _ => {}
                },
                BlockNode::FigureGroup(group) => visit(&group.children, lines),
                BlockNode::Extension(extension) => visit(&extension.children, lines),
                _ => {}
            }
        }
    }

    fn visit_table(table: &crate::ast::Table, lines: &mut HashSet<usize>) {
        for row in &table.rows {
            if let Some(pos) = row.pos {
                lines.extend((pos.start_line + 1)..=pos.end_line);
            }
        }
    }

    let mut lines = HashSet::new();
    let options = crate::Options::default().with_positions(true);
    visit(
        &crate::parse_with_options(source, &options).children,
        &mut lines,
    );
    lines
}

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
const BRACED_DELIMITERS: &[u8] = b",/#=+-*_^~";

/// Every bare run Carve spells (PART 4: `/ * _ ~ =`).
const BARE_DELIMITERS: &[u8] = b"/=~*_";

/// Freeze the Carve constructs in `source` that the caller's language leaves as
/// literal text, given the delimiters that language HANDLES itself.
pub(crate) fn escape_plain_carve_syntax(source: &str, handled: HandledDelimiters<'_>) -> String {
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
        let mut i = 0;
        while i < mask.len() {
            if mask[i] != delim || is_escaped(mask, i) {
                i += 1;
                continue;
            }
            // A leading `{` excludes the run for a delimiter the caller's
            // language spells in BRACED form, and not otherwise; the asymmetry
            // is the point rather than an oversight. Under Djot, `{=x=}` is a
            // highlight in both languages, so the inner `=` is markup that must
            // survive; there is no `{/x/}` that means the same in both, so the
            // `/` inside an escaped brace is literal text and still needs
            // escaping - escaping the brace alone leaves it free to open Carve
            // emphasis and renders `{<em>x</em>}`. Under a profile that owns
            // neither, both get escaped, which is what makes the whole run
            // literal.
            let brace_protects = handled.owns_braced(delim);
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

    let mut i = 0;
    while i < mask.len() {
        if mask[i] != b'#' || is_escaped(mask, i) {
            i += 1;
            continue;
        }
        let before_ok = i == 0 || !(mask[i - 1].is_ascii_alphanumeric() || mask[i - 1] == b'&');
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
fn braced_run_opens(mask: &[u8], from: usize) -> bool {
    mask.get(from).is_some_and(|b| !b.is_ascii_whitespace())
}

/// The offset of the `X}` that closes a `{X` opened before `from`, on the same
/// line, with non-space content between.
fn find_braced_close(mask: &[u8], from: usize, delim: u8) -> Option<usize> {
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

    /// Djot reads any blank run between two markers as one list; Carve reads
    /// three or more as a hard boundary. Passing the run through therefore
    /// split a list the source never split.
    #[test]
    fn a_blank_run_before_a_sibling_marker_does_not_split_the_list() {
        assert_eq!(
            djot_to_carve("- apples\n\n\n\n\n- oranges\n"),
            "- apples\n\n- oranges\n"
        );
    }

    /// The boundary is not a top-level rule, so neither is the collapse.
    #[test]
    fn the_run_collapses_inside_an_item_too() {
        assert_eq!(
            djot_to_carve("- outer\n  - inner\n\n\n\n\n  - inner2\n"),
            "- outer\n  - inner\n\n  - inner2\n"
        );
    }

    /// Two blank lines are the loose separator in both languages, so there is
    /// nothing to correct.
    #[test]
    fn a_shorter_run_is_left_alone() {
        assert_eq!(
            djot_to_carve("- apples\n\n\n- oranges\n"),
            "- apples\n\n\n- oranges\n"
        );
    }

    /// Djot's own way of writing two lists is a different marker, and it means
    /// the same in Carve. Nothing about it is a false boundary.
    #[test]
    fn a_marker_change_still_separates_two_lists() {
        assert_eq!(
            djot_to_carve("- apples\n\n* oranges\n"),
            "- apples\n\n* oranges\n"
        );
    }

    /// The rewrite fires only where the two languages disagree, which needs a
    /// list open above the run. After a paragraph the run says nothing in
    /// either language and the author's layout is left as written.
    #[test]
    fn a_run_that_follows_no_list_keeps_its_lines() {
        assert_eq!(
            djot_to_carve("paragraph\n\n\n\n\n- apples\n"),
            "paragraph\n\n\n\n\n- apples\n"
        );
    }

    /// A run followed by an item's own indented content continues that item at
    /// any length -- N1a closes nothing -- so both languages already agree.
    #[test]
    fn a_run_before_an_items_content_keeps_its_lines() {
        assert_eq!(
            djot_to_carve("- apples\n\n\n\n\n  still apples\n"),
            "- apples\n\n\n\n\n  still apples\n"
        );
    }

    /// N1a applies inside a container too, and there a blank line is written
    /// `>`, so the run and the markers around it are read through the prefix.
    #[test]
    fn a_quoted_run_collapses_and_keeps_its_prefix() {
        assert_eq!(
            djot_to_carve("> 1. one\n>\n>\n>\n> 2. two\n"),
            "> 1. one\n>\n> 2. two\n"
        );
    }

    /// An item's own indentation is what says the line continues the item, and
    /// inside a quote it sits right after the marker -- so the quote prefix is
    /// stripped and the indentation is not.
    #[test]
    fn a_quoted_run_after_an_items_indented_content_still_collapses() {
        assert_eq!(
            djot_to_carve("> - apples\n>   more apples\n>\n>\n>\n> - oranges\n"),
            "> - apples\n>   more apples\n>\n> - oranges\n"
        );
    }

    /// A fence opened inside a quote holds its blank lines as content exactly
    /// like an unquoted one.
    #[test]
    fn a_run_inside_a_quoted_fence_is_content() {
        let source = "> ```\n> - one\n>\n>\n>\n> - two\n> ```\n";
        assert_eq!(djot_to_carve(source), source);
    }

    /// A marker one quote level away separates nothing the run could join.
    #[test]
    fn a_run_and_a_marker_at_different_depths_are_left_alone() {
        let source = "> - apples\n>\n>\n>\n- oranges\n";
        assert_eq!(djot_to_carve(source), source);
    }

    /// Blank lines inside a fenced block are that block's content.
    #[test]
    fn a_run_inside_a_fence_is_content() {
        let source = "```\ncode\n\n\n\n- not a marker\n```\n";
        assert_eq!(djot_to_carve(source), source);
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

    /// A tag is the one construct that is not a pair, so escaping an enclosing
    /// brace cannot neutralize it. Djot has no hashtag - pandoc renders
    /// `a #y b` as `<p>a #y b</p>` - so every `#word` became a Carve tag span
    /// (carve-php#1191).
    #[test]
    fn a_hash_does_not_become_a_tag() {
        assert_eq!(djot_to_carve("a #y b"), "a \\#y b");
        assert_eq!(djot_to_carve("a #1 b"), "a \\#1 b");
        assert_eq!(djot_to_carve("{#y#} x"), "\\{\\#y#} x");
    }

    /// A heading is `#` plus a SPACE and is shared with Djot; `a#y` is not a
    /// tag either; and `&#8212;` is a numeric character reference whose `#`
    /// must stay bare or the entity stops decoding.
    #[test]
    fn the_hash_negatives_stay_bare() {
        assert_eq!(djot_to_carve("# Heading"), "# Heading");
        assert_eq!(djot_to_carve("a#y b"), "a#y b");
        assert_eq!(djot_to_carve("a &#8212; b"), "a &#8212; b");
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

    #[test]
    fn a_table_continuation_row_is_left_alone() {
        let source = "| a | b |\n|---|---|\n| one | x |\n+ continues here | y |\n";
        assert_eq!(table_continuation_lines(source), HashSet::from([4]));
        assert_eq!(djot_to_carve(source), source);
    }

    #[test]
    fn a_plus_bullet_containing_a_pipe_still_becomes_a_dash() {
        assert_eq!(
            djot_to_carve("A paragraph.\n\n+ a bullet with a | pipe\n"),
            "A paragraph.\n\n- a bullet with a | pipe\n"
        );
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

/// The shared escaper corpus (`tests/spec/tests/corpus-escape/`), read directly
/// against the escaper rather than end to end through a converter.
///
/// This lives beside the function rather than in `tests/` because the function
/// is crate-internal: it is one rule with one implementation, not public API.
/// Before markup-carve/carve-rs#995 the only way to reach it from outside was
/// `carve migrate --from djot`, which can only probe inputs that are INERT in
/// Djot - 46 of the corpus's 55, since the other 9 are Djot markup and what
/// came back was the converter doing its job rather than the escaper.
#[cfg(test)]
mod escape_corpus {
    use super::{escape_plain_carve_syntax, HandledDelimiters};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// The profiles THIS crate can produce, by the corpus's names.
    ///
    /// The corpus tells an engine to run "every profile its converters can
    /// produce" and skip the rest, the way the render corpus skips a target an
    /// engine does not implement. All three are listed here deliberately: the
    /// handled set is a parameter now, so a profile with no caller is still a
    /// statement this implementation can be held to, and `markdown` and `plain`
    /// are exactly what a future text-level converter would pass. Which of them
    /// has a caller today is recorded in `a_profile_with_no_caller_is_named`.
    fn profiles() -> BTreeMap<&'static str, HandledDelimiters<'static>> {
        BTreeMap::from([
            ("plain", HandledDelimiters::PLAIN),
            ("markdown", HandledDelimiters::MARKDOWN),
            ("djot", HandledDelimiters::DJOT),
        ])
    }

    fn corpus() -> serde_json::Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/spec/tests/corpus-escape/cases.json");
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "escaper corpus not found at {}: {e}\n\
                 Did you initialize the submodule?\n  git submodule update --init",
                path.display()
            )
        });
        serde_json::from_str(&raw).expect("escaper corpus is JSON")
    }

    #[test]
    fn the_handled_sets_match_the_corpus_profiles() {
        // The sets are spelled in two places - here and in the corpus - and a
        // silent drift between them would make every case below pass while
        // measuring the wrong question.
        let corpus = corpus();
        let declared = corpus["profiles"]
            .as_object()
            .expect("profiles is an object");
        for (name, handled) in profiles() {
            let entry = declared
                .get(name)
                .unwrap_or_else(|| panic!("corpus declares no profile {name}"));
            assert_eq!(
                entry.get("braced").and_then(|v| v.as_str()).unwrap_or(""),
                handled.braced,
                "{name}: braced handled set"
            );
            assert_eq!(
                entry.get("bare").and_then(|v| v.as_str()).unwrap_or(""),
                handled.bare,
                "{name}: bare handled set"
            );
        }
    }

    #[test]
    fn a_case_is_read() {
        // Guards the sweep below against a glob that quietly matches nothing -
        // a checker reporting success having compared nothing is the state this
        // rule was already in.
        let corpus = corpus();
        let cases = corpus["cases"].as_array().expect("cases is an array");
        assert!(cases.len() >= 50, "found {} cases", cases.len());
    }

    #[test]
    fn every_case_matches_under_every_profile() {
        let corpus = corpus();
        let profiles = profiles();
        let mut checked = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for case in corpus["cases"].as_array().expect("cases is an array") {
            let name = case["name"].as_str().expect("a case has a name");
            let input = case["input"].as_str().expect("a case has an input");
            for (profile, expected) in case["expected"]
                .as_object()
                .expect("a case has expectations")
            {
                let Some(handled) = profiles.get(profile.as_str()) else {
                    continue;
                };
                let expected = expected.as_str().expect("an expectation is a string");
                let got = escape_plain_carve_syntax(input, *handled);
                if got != expected {
                    failures.push(format!(
                        "{name} [{profile}]: {input:?} -> {got:?}, expected {expected:?}"
                    ));
                }
                checked += 1;
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {checked} escaper cases diverge:\n{}",
            failures.len(),
            failures.join("\n")
        );
        assert!(checked >= 150, "checked only {checked} case-profile pairs");
    }

    #[test]
    fn escaping_only_ever_inserts_backslashes() {
        // The corpus's own invariant, restated against THIS implementation: an
        // expectation is a fabrication if removing its backslashes does not give
        // the input back. Asserting it on the output rather than on the fixture
        // catches an escaper that rewrites text instead of freezing it.
        let corpus = corpus();
        for case in corpus["cases"].as_array().expect("cases is an array") {
            let input = case["input"].as_str().expect("a case has an input");
            for (profile, handled) in profiles() {
                let got = escape_plain_carve_syntax(input, handled);
                assert_eq!(
                    got.replace('\\', ""),
                    *input,
                    "{} [{profile}] rewrote its input",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn a_delimiter_with_a_space_against_it_is_where_the_opener_test_bites() {
        // `braced_run_opens` is the test that separates "does not open" from
        // "opens and is never closed", and the corpus does NOT reach it: its
        // only inner-space case is `a { ^x^ } b`, whose space sits between the
        // `{` and the delimiter, so the loop never matches the opener at all and
        // every answer this test could give passes. The case that reaches it has
        // the space AFTER the delimiter, and it is pinned here instead.
        assert_eq!(
            escape_plain_carve_syntax("a { ^x^ } b", HandledDelimiters::PLAIN),
            "a { ^x^ } b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a {^ x^} b", HandledDelimiters::PLAIN),
            "a {^ x^} b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a {^ x b", HandledDelimiters::PLAIN),
            "a {^ x b"
        );

        // STATED, NOT ASSERTED AS CORRECT. `a {^ x^} b` renders `a <sup> x</sup>
        // b` in Carve, so the second line above is a LEAK: text that is literal
        // in the calling language reaches Carve as markup. The same holds on the
        // Djot path for the delimiters Djot does not own - `a {, x,} b` is
        // literal Djot and renders `a <sub> x</sub> b` as Carve. Widening the
        // opener to accept a space against the delimiter fixes it, and is left
        // alone here on purpose: the shared corpus has no case for it, carve-php
        // passes the same corpus with the same boundary, and moving one engine
        // off an unpinned shape is how the four spellings drifted apart in the
        // first place. It wants a corpus case and all three engines, not a
        // unilateral change under a ticket about exposing the function.
    }

    #[test]
    fn the_profiles_remain_distinct() {
        // Djot and BBCode are text-level converters. Markdown and HTML build an
        // AST and let the canonical writer emit source, so they escape no text
        // and Markdown passes no handled set.
        // The handled set is what separates the profiles: `*` is Djot markup and
        // is left for the converter, and is literal text under PLAIN. Asserting
        // the two differ is what keeps the parameter load-bearing - a hardwired
        // set would make every profile give the same answer and every case above
        // would still pass.
        assert_eq!(
            escape_plain_carve_syntax("a *x* b", HandledDelimiters::DJOT),
            "a *x* b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a *x* b", HandledDelimiters::PLAIN),
            "a \\*x* b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a ~x~ b", HandledDelimiters::MARKDOWN),
            "a ~x~ b"
        );
        assert_eq!(
            escape_plain_carve_syntax("a ~x~ b", HandledDelimiters::PLAIN),
            "a \\~x~ b"
        );
    }

    /// An at-sign that opens a Carve mention is escaped when it arrives as
    /// text. The sibling of the tag rule, ported from carve-php#1381.
    #[test]
    fn an_at_sign_in_source_text_is_not_a_mention() {
        for (input, want) in [
            ("hi @user ok", "hi \\@user ok"),
            ("@click toggles it", "\\@click toggles it"),
            ("use @keydown.window here", "use \\@keydown.window here"),
            ("see (@can) there", "see (\\@can) there"),
            ("the @-form", "the \\@-form"),
            ("@can and @click", "\\@can and \\@click"),
        ] {
            assert_eq!(
                escape_plain_carve_syntax(input, HandledDelimiters::DJOT),
                want,
                "input {input:?}"
            );
        }
    }

    /// BOUND: the escape mirrors the parser's opener, so an at-sign the parser
    /// never opens on gains no backslash.
    #[test]
    fn an_at_sign_that_opens_nothing_is_left_bare() {
        for input in [
            "mail me at foo@bar.de",
            "a@b",
            "name @ handle",
            "ping @, later",
            "ends with @",
        ] {
            assert_eq!(
                escape_plain_carve_syntax(input, HandledDelimiters::DJOT),
                input,
                "input {input:?}"
            );
        }
    }

    /// BOUND: an at-sign the source already escaped is not escaped twice.
    #[test]
    fn an_already_escaped_at_sign_is_left_alone() {
        assert_eq!(
            escape_plain_carve_syntax("hi \\@user ok", HandledDelimiters::DJOT),
            "hi \\@user ok"
        );
    }

    /// ONLY THE OPENING COLON. Escaping the opener is what makes the whole
    /// shortcode text - the closing colon then has a letter against it and
    /// opens nothing - so a second escape would be bytes PART 11 §4 asks the
    /// writer not to spend. The corpus case `a-symbol-shortcode` pins the
    /// one-escape form; this names WHICH colon under every profile.
    #[test]
    fn a_symbol_shortcode_is_frozen_at_its_opener_only() {
        for (input, expected) in [
            ("a :rocket: b", "a \\:rocket: b"),
            (":rocket:", "\\:rocket:"),
            // The reaction shortcodes: `+` and `-` open a name, `_` does not.
            ("a :+1: b", "a \\:+1: b"),
            ("a :-1: b", "a \\:-1: b"),
            // Two shortcodes against each other are two openers.
            (":a::b:", "\\:a:\\:b:"),
        ] {
            for (profile, handled) in profiles() {
                assert_eq!(
                    escape_plain_carve_syntax(input, handled),
                    expected,
                    "{input:?} under {profile}"
                );
            }
        }
    }

    /// BOUND: THE NEAR NEIGHBOUR THAT MUST NOT MOVE. `a : b : c` is the shape a
    /// rule over every colon would break, and the corpus pins it unchanged
    /// (`a-colon-that-closes-no-shortcode`). The parser opens no symbol at
    /// either colon - a space is not a name character - so neither is offered
    /// an escape here.
    #[test]
    fn a_colon_that_opens_no_shortcode_is_left_bare() {
        for input in [
            // The corpus's own bound.
            "a : b : c",
            // A colon with a letter against it is not an opener.
            "https://example.com",
            "note: see below",
            "12:30:45",
            // A name that never closes.
            "a :rocket b",
            // The fence and the definition marker, which are line-initial
            // constructs the converters already own.
            ":::",
            ": definition",
            // Nothing after the colon at all.
            "ends with :",
        ] {
            for (profile, handled) in profiles() {
                assert_eq!(
                    escape_plain_carve_syntax(input, handled),
                    input,
                    "{input:?} under {profile}"
                );
            }
        }
    }

    /// BOUND: `_` CANNOT OPEN A NAME, because `:_x_:` would steal from
    /// underline - `parse_symbol` excludes it from the first name character's
    /// class and this mirrors that. Asserted on the colon alone: the `_x_` in
    /// it is a bare underline run, which the profiles that do not spell `_`
    /// freeze for their own unrelated reason.
    #[test]
    fn an_underscore_does_not_open_a_symbol_name() {
        for (profile, handled) in profiles() {
            let got = escape_plain_carve_syntax("a :_x_: b", handled);
            assert!(!got.contains("\\:"), "{profile} escaped a colon in {got:?}");
        }
    }

    /// BOUND: a colon the source already escaped is not escaped twice.
    #[test]
    fn an_already_escaped_symbol_opener_is_left_alone() {
        assert_eq!(
            escape_plain_carve_syntax("a \\:rocket: b", HandledDelimiters::DJOT),
            "a \\:rocket: b"
        );
    }
}
