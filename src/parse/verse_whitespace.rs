/// Expand the whitespace a line block preserves to non-breaking spaces, so a
/// verse line's layout survives; tabs advance to the next 4-column stop.
///
/// Leading whitespace is preserved down to a single column. An INNER or
/// TRAILING run of TWO OR MORE columns is a medial gap - the alignment a
/// caesura or a column of aligned text is made of - and is preserved too
/// (grammar §23). A lone inner space stays an ordinary, collapsible space so a
/// long line can still wrap between words.
///
/// Uses the generated-NBSP placeholder (HTML folds it to `&nbsp;`; plain/ANSI
/// turn it back into an ASCII space), so it stays distinct from a literal
/// U+00A0 typed in the source.
pub(super) fn expand_line_block_ws(
    line: &str,
    mut source_columns: Option<&mut Vec<Option<usize>>>,
) -> String {
    let mut out = String::with_capacity(line.len());
    let mut columns = 0usize;
    let mut seen_content = false;
    let mut source_column = 0;
    let mut chars = line.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        if ch != ' ' && ch != '\t' {
            if let Some(map) = source_columns.as_deref_mut() {
                map.push(Some(source_column));
            }
            let mut width = 1;
            while let Some((_, next)) = chars.peek() {
                if *next == ' ' || *next == '\t' {
                    break;
                }
                if let Some(map) = source_columns.as_deref_mut() {
                    map.push(Some(source_column + width));
                }
                width += 1;
                chars.next();
            }
            let end = chars.peek().map_or(line.len(), |(index, _)| *index);
            out.push_str(&line[start..end]);
            source_column += width;
            seen_content = true;
            columns += width;
            continue;
        }

        let source_start = source_column;
        source_column += 1;
        let mut has_tab = ch == '\t';
        let mut width = if ch == '\t' { 4 - (columns % 4) } else { 1 };
        while let Some((_, next)) = chars.peek() {
            match next {
                ' ' => width += 1,
                '\t' => width += 4 - ((columns + width) % 4),
                _ => break,
            }
            has_tab |= *next == '\t';
            source_column += 1;
            chars.next();
        }
        columns += width;

        if !seen_content || width >= 2 {
            for index in 0..width {
                if let Some(map) = source_columns.as_deref_mut() {
                    map.push((!has_tab).then_some(source_start + index));
                }
                out.push(crate::NBSP_PLACEHOLDER);
            }
        } else if chars.peek().is_some() {
            if let Some(map) = source_columns.as_deref_mut() {
                map.push((!has_tab).then_some(source_start));
            }
            out.push(' ');
        }
        // ...and a ONE-COLUMN run at the END of the line is dropped, like
        // trailing whitespace anywhere else (PART 2 NO TRAILING WHITESPACE,
        // carve#926). The ORDER is what decides this line: §23 converts an
        // inner or trailing run of TWO OR MORE columns into NBSP CONTENT first,
        // and content is not whitespace - so the rule never reaches those, and
        // `abc<SP><SP>` still ends in two non-breaking spaces. What it does
        // reach is the one-column case, which §23 leaves as an ordinary space.
        //
        // A trailing TAB is not the one-column case: it expands to the next tab
        // stop, which is at least two columns from anywhere it can start, so it
        // becomes NBSP content and survives.
    }

    if let Some(map) = source_columns {
        map.push(Some(source_column));
    }
    out
}
