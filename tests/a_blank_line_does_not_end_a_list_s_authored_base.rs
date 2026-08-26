//! A list that took an authored base keeps it across a blank line.
//!
//! PART 9 §24 C3's authored-base clause (carve#1729) puts a recognized block
//! opener at or past a body's minimum content column in that body and makes
//! its authored visual column the local `block_base`, so that "the block's
//! payload, continuations and closer are measured relative to that base". A
//! list item's continuations include the blank-separated indented block of
//! PART 9 §17 FORM A.
//!
//! The engine ended the rebased run at the blank line instead. The marker
//! above the blank had moved to the base and the block below it had not, so
//! the item's content column had moved out from under the block and every
//! payload column read alike - the block landed beside the list whether it was
//! written at the item's content column, past it, or short of it
//! (carve-rs#1423). The definition-list branch beside this one was #1419,
//! fixed in #1422; the footnote branch was #1415, fixed in #1420.
//!
//! WHAT THE RUN MUST NOT SWALLOW. Below the item's content column the
//! blank-separated block is the ENCLOSING BODY's, not the list's. Rebasing it
//! would move it into the list's coordinate system one or two columns short of
//! the content column, where the ordinary list rule reads it as lazy text -
//! which is precisely what the body's own block is not. Ending the run at the
//! blank hid that: the block never entered the run at all.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

/// The footnote body's minimum content column is 2. The list is authored at
/// `opener`, so the item's content column is `opener + 2`. Only the column the
/// quote is written at varies.
fn footnote(opener: usize, payload: usize) -> String {
    format!(
        "[^n]: intro\n\n{}- item\n\n{}> quote\n\nsee[^n]\n",
        " ".repeat(opener),
        " ".repeat(payload)
    )
}

/// The quote is the item's own block.
const INSIDE: &str = "      <ul>\n        <li>item\n          \
                      <blockquote><p>quote</p></blockquote>\n        \
                      </li>\n      </ul>";

/// The quote is the footnote body's own block, a sibling of the list.
const BESIDE: &str = "      <ul>\n        <li>item</li>\n      </ul>\n      \
                      <blockquote><p>quote</p></blockquote>";

#[test]
fn at_or_past_the_items_content_column_the_block_is_the_items() {
    // The upper band. Every opener column, and every payload column from the
    // item's own content column up.
    for opener in 2..=6 {
        for payload in opener + 2..=10 {
            let output = html(&footnote(opener, payload));
            assert!(
                output.contains(INSIDE),
                "opener {opener}, payload {payload}:\n{output}"
            );
        }
    }
}

#[test]
fn below_the_items_content_column_the_block_is_the_bodys_own() {
    // The middle band, from the body's own minimum column up to but not
    // including the item's content column. The quote OPENS - it is measured in
    // the footnote body's coordinates, where it is flush or indented under
    // four - and it is a sibling of the list rather than lazy text inside it.
    for opener in 2..=6 {
        for payload in 2..opener + 2 {
            let output = html(&footnote(opener, payload));
            assert!(
                output.contains(BESIDE),
                "opener {opener}, payload {payload}:\n{output}"
            );
        }
    }
}

#[test]
fn below_the_bodys_minimum_column_the_block_leaves_the_footnote() {
    // The lower band. Nothing in it belongs to the footnote at all, so the
    // list is the whole of the body either way.
    for opener in 2..=6 {
        let flush = html(&footnote(opener, 0));
        assert!(
            flush.starts_with("<blockquote><p>quote</p></blockquote>\n<p>see"),
            "opener {opener}:\n{flush}"
        );
        assert!(
            flush.contains("      <ul>\n        <li>item</li>\n      </ul>"),
            "opener {opener}:\n{flush}"
        );

        // One column in, the line is lazy against the paragraph the footnote
        // definition left open above it, so the `>` never opens.
        let lazy = html(&footnote(opener, 1));
        assert!(
            lazy.starts_with("<p>&gt; quote</p>\n<p>see"),
            "opener {opener}:\n{lazy}"
        );
        assert!(!lazy.contains("<blockquote>"), "opener {opener}:\n{lazy}");
    }
}

#[test]
fn a_list_at_the_bodys_own_minimum_column_is_unchanged() {
    // A list authored AT the body's minimum column takes no authored base, so
    // there is no rebased run for a blank to end. Its row of the ladder read
    // correctly before this change and still does; the two tests above cover
    // it as `opener == 2`, and this one says so as its own claim.
    for payload in 2..=10 {
        let output = html(&footnote(2, payload));
        let expected = if payload >= 4 { INSIDE } else { BESIDE };
        assert!(output.contains(expected), "payload {payload}:\n{output}");
    }
}

#[test]
fn the_rule_is_not_about_quotes() {
    // Every recognized opener takes the same base. A heading closes itself, so
    // the reading cannot be explained by a quote swallowing the line.
    let output = html("[^n]: intro\n\n   - item\n\n     # h\n\nsee[^n]\n");
    assert!(
        output.contains(
            "      <ul>\n        <li>item\n          \
             <h1 id=\"h\">h</h1>\n        </li>\n      </ul>"
        ),
        "{output}"
    );
}

#[test]
fn a_definition_body_reads_the_same_ladder() {
    // The other container PART 9 §24 C3 names. Its minimum content column is
    // 3 rather than 2, so the bands sit one column further along, and the rule
    // is otherwise identical.
    for opener in 3..=6 {
        for payload in opener + 2..=10 {
            let output = html(&format!(
                ":: term\n:  intro\n\n{}- item\n\n{}> quote\n",
                " ".repeat(opener),
                " ".repeat(payload)
            ));
            assert!(
                output.contains(
                    "<li>item\n        <blockquote><p>quote</p></blockquote>\n      </li>"
                ),
                "opener {opener}, payload {payload}:\n{output}"
            );
        }
    }
}

#[test]
fn a_list_item_host_keeps_its_own_answer_at_every_column() {
    // THE CLAUSE NAMES TWO CONTAINERS AND THIS IS NOT ONE OF THEM. A list item
    // hosting the same list at the same column is outside PART 9 §24 C3, and
    // the branch this ticket changes cannot reach it: it runs only with
    // `include_sublists`, which is true at the definition-body and
    // footnote-body call sites and false at all three list-item ones, and a
    // marker line registers its content column and returns before a list-item
    // call gets that far.
    //
    // Applying the clause to every container instead trades this ticket's
    // defect for a different one. carve-js shipped that trade and needed two
    // further tickets to unpick it (markup-carve/carve-js#1508, undone in
    // markup-carve/carve-js#1520), so the answer is pinned here BY VALUE at
    // every column: the whole
    // document is asserted, and any change that moves list-item placement
    // fails this test rather than passing it quietly.
    let outer_sibling = "<ul>\n  <li>item\n    <ul>\n      <li>inner</li>\n    </ul>\n    \
                         <blockquote><p>quote</p></blockquote>\n  </li>\n</ul>";
    let inner_own = "<ul>\n  <li>item\n    <ul>\n      <li>inner\n        \
                     <blockquote><p>quote</p></blockquote>\n      </li>\n    </ul>\n  \
                     </li>\n</ul>";
    for payload in 0..=8 {
        let output = html(&format!(
            "- item\n\n   - inner\n\n{}> quote\n",
            " ".repeat(payload)
        ));
        let expected = match payload {
            0 => "<ul>\n  <li>item\n    <ul>\n      <li>inner</li>\n    </ul>\n  </li>\n</ul>\n\
                  <blockquote><p>quote</p></blockquote>"
                .to_string(),
            1 => "<ul>\n  <li>item\n    <ul>\n      <li>inner</li>\n    </ul>\n  </li>\n</ul>\n\
                  <p>&gt; quote</p>"
                .to_string(),
            2..=4 => outer_sibling.to_string(),
            _ => inner_own.to_string(),
        };
        assert_eq!(output, expected, "payload {payload}");
    }
}

#[test]
fn the_blank_line_makes_no_difference_at_or_past_the_content_column() {
    // The defect stated as its own invariant, over the band it governs. A
    // blank line loosens the item; it does not move ownership, so at and past
    // the item's content column the two spellings agree.
    //
    // The band BELOW the content column is deliberately not asserted here: the
    // two spellings genuinely differ there one column past the marker, and
    // they differ in the same way at an opener column that takes no authored
    // base at all, so that divergence has a different mechanism and is
    // measured on its own ticket rather than folded into this one.
    for opener in 2..=6 {
        for payload in opener + 2..=10 {
            let indent = " ".repeat(payload);
            let marker = " ".repeat(opener);
            let blank = html(&format!(
                "[^n]: intro\n\n{marker}- item\n\n{indent}> quote\n\nsee[^n]\n"
            ));
            let tight = html(&format!(
                "[^n]: intro\n\n{marker}- item\n{indent}> quote\n\nsee[^n]\n"
            ));
            assert_eq!(
                blank.contains(INSIDE),
                tight.contains(INSIDE),
                "opener {opener}, payload {payload}:\nblank:\n{blank}\ntight:\n{tight}"
            );
        }
    }
}

#[test]
fn a_rebased_list_renders_as_the_same_list_written_at_column_zero() {
    // THE CLAUSE'S OWN WORDS, AS AN EXECUTABLE INVARIANT. PART 9 §24 C3 makes
    // the opener's authored visual column the local `block_base` and says "the
    // block's payload, continuations and closer are measured relative to that
    // base". Measured relative to its base, a list authored inside a footnote
    // body IS the same list written at column 0 - and what that list renders as
    // is not in dispute: every engine and the executable oracle agree on it.
    //
    // This is the test that decides the shapes the ladder above cannot. Where a
    // lazy continuation line sits between the marker and the item's content
    // column, carve-js at 10a1698e and the executable oracle at carve 7604aac6
    // each read the footnote body differently from how they read the identical
    // document at column 0, and they differ from each other as well. The
    // grammar has a standing answer for that case: "two different incoherences
    // are not evidence for one of them", so the coherent rule wins over either
    // reading. The engine went from 25 of these 125 to all 125.
    for opener in 2..=6 {
        for continuation in [None, Some(0), Some(1), Some(2), Some(3)] {
            for payload in 2..=6 {
                let mut flat = "- item\n".to_string();
                let mut nested = format!("{}- item\n", " ".repeat(opener));
                if let Some(column) = continuation {
                    flat.push_str(&format!("{}lazy\n", " ".repeat(column)));
                    nested.push_str(&format!("{}lazy\n", " ".repeat(opener + column)));
                }
                flat.push_str(&format!("\n{}> quote\n", " ".repeat(payload)));
                nested.push_str(&format!("\n{}> quote\n", " ".repeat(opener + payload)));

                let want = list_region(&html(&flat));
                let got = list_region(&html(&format!("[^n]: intro\n\n{nested}\nsee[^n]\n")));
                assert_eq!(
                    want, got,
                    "opener {opener}, continuation {continuation:?}, payload {payload}"
                );
            }
        }
    }
}

/// The `<ul>` element and everything in it, dedented to column 0 so the same
/// list reads the same whatever depth it was rendered at.
fn list_region(html: &str) -> String {
    let lines: Vec<&str> = html.lines().collect();
    let open = lines
        .iter()
        .position(|line| line.trim() == "<ul>")
        .unwrap_or_else(|| panic!("no list in:\n{html}"));
    let mut depth = 0usize;
    let mut close = None;
    for (index, line) in lines.iter().enumerate().skip(open) {
        match line.trim() {
            "<ul>" => depth += 1,
            "</ul>" => {
                depth -= 1;
                if depth == 0 {
                    close = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close.unwrap_or_else(|| panic!("unclosed list in:\n{html}"));
    let pad = lines[open].len() - lines[open].trim_start().len();
    lines[open..=close]
        .iter()
        .map(|line| line.strip_prefix(&" ".repeat(pad)).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}
