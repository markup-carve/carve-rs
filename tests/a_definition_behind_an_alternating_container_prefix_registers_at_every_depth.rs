//! A definition registers at the innermost content column behind EVERY
//! quote/list prefix, not only the ones whose quotes happen to be flush left
//! (markup-carve/carve-rs#1096, spec PART 1 S4, corpus category `360`).
//!
//! THE COLUMN IS REACHED BY COMPOSING THE STRIPS, NOT BY WALKING THE PREFIX.
//! The definition pre-pass used to key its column frames on the number of
//! FLUSH-LEFT `>` runs a line opens with, so a quote written at a list item's
//! content column was not a container to it at all. `- > - - x` recorded the
//! outer item's column and nothing else, the two list levels opened inside the
//! quote never existed, and a definition at their content column reached no
//! column and folded as lazy paragraph text.
//!
//! The tell was that the SAME body one container shallower registered: peel the
//! outer item off `- > - - x` and `> - - x` is a document this engine has always
//! read correctly. An outer container cannot change what an inner body's column
//! 0 is (PART 9 §24 C5), so an engine that answers the two differently is
//! answering the same question twice.
//!
//! Measured over the whole prefix space rather than the five reported shapes:
//! all 62 sequences of `l`(ist) and `q`(uote) up to depth five, on both
//! definition kinds. The executable spec and carve-js answer 62 of 62 on both;
//! before this fix carve-rs answered 57.

use carve::to_html;

/// Every quote/list prefix up to depth five, outermost container first.
fn prefixes() -> Vec<String> {
    let mut out = Vec::new();
    for depth in 1..=5 {
        for n in 0..(1u32 << depth) {
            out.push(
                (0..depth)
                    .map(|bit| if n >> bit & 1 == 0 { 'l' } else { 'q' })
                    .collect(),
            );
        }
    }
    out
}

/// The MARKER line: each container written as its own two-column marker.
fn opener(prefix: &str) -> String {
    let mut line: String = prefix
        .chars()
        .map(|c| if c == 'q' { "> " } else { "- " })
        .collect();
    line.push('x');
    line
}

/// The CONTINUATION line's prefix: a quote repeats its marker, an item is
/// carried by the indent its marker established. Both are two columns wide, so
/// the content column is the same on both lines.
fn carried(prefix: &str) -> String {
    prefix
        .chars()
        .map(|c| if c == 'q' { "> " } else { "  " })
        .collect()
}

fn document(prefix: &str, body: &str, tail: &str) -> String {
    format!("{}\n{}{}\n{}", opener(prefix), carried(prefix), body, tail)
}

fn link_doc(prefix: &str) -> String {
    document(prefix, "[r]: /url", "\nSee [r][].\n")
}

fn footnote_doc(prefix: &str) -> String {
    document(prefix, "[^f]: note", "\nSee [^f].\n")
}

fn heading_doc(prefix: &str) -> String {
    document(prefix, "# h", "")
}

fn declining(kind: &str, needle: &str, build: fn(&str) -> String) -> Vec<String> {
    prefixes()
        .into_iter()
        .filter(|prefix| {
            let html = to_html(&build(prefix));
            if html.contains(needle) {
                return false;
            }
            eprintln!("{kind} {prefix}:\n{}\n{html}", build(prefix));
            true
        })
        .collect()
}

#[test]
fn the_link_kind_registers_at_all_sixty_two_prefixes() {
    let declined = declining("link", "href=\"/url\"", link_doc);
    assert!(
        declined.is_empty(),
        "declined at {} of 62 prefixes: {declined:?}",
        declined.len()
    );
}

#[test]
fn the_footnote_kind_registers_at_all_sixty_two_prefixes() {
    // Both pre-passes share the column tracker, so both kinds move together. A
    // fix that moved only one would sort definitions by kind, which is the tell
    // this family keeps producing.
    let declined = declining("footnote", "doc-endnotes", footnote_doc);
    assert!(
        declined.is_empty(),
        "declined at {} of 62 prefixes: {declined:?}",
        declined.len()
    );
}

#[test]
fn the_heading_control_agrees_with_the_definition_at_every_prefix() {
    // THE CONTROL THIS FIX IS MEASURED AGAINST. A `# h` written at the same
    // column is a heading inside the innermost container at all 62 prefixes,
    // and was before the fix too - so the block layer always reached the
    // column and only the pre-pass did not. The two agreeing everywhere is the
    // proof; the two disagreeing anywhere is the defect, whichever way round.
    let heading = declining("heading", "<h1", heading_doc);
    assert!(
        heading.is_empty(),
        "the heading control itself declined at {heading:?}"
    );
    let link = declining("link", "href=\"/url\"", link_doc);
    assert_eq!(
        link, heading,
        "the definition and the heading disagree about which prefixes reach the column"
    );
}

#[test]
fn peeling_a_container_off_changes_nothing() {
    // PART 9 §24 C5: what a container hands down is a body, and an outer
    // container cannot change what that body's column 0 is. So every prefix
    // must answer exactly as its own tail does, all the way down to the bare
    // one-container case. This is the internal contradiction that decided the
    // ruling, written as an assertion.
    for prefix in prefixes() {
        let peeled = &prefix[1..];
        if peeled.is_empty() {
            continue;
        }
        let whole = to_html(&link_doc(&prefix)).contains("href=\"/url\"");
        let inner = to_html(&link_doc(peeled)).contains("href=\"/url\"");
        assert_eq!(
            whole,
            inner,
            "`{prefix}` and its peel `{peeled}` disagree:\n{}\n{}",
            link_doc(&prefix),
            link_doc(peeled)
        );
    }
}

#[test]
fn the_five_reported_shapes_register_and_leave_no_text_behind() {
    // The ticket's own table. The failure was NOT lossy - the line came back as
    // paragraph text - so registering it must also remove it, otherwise the fix
    // has only moved the defect (carve-php drops the line entirely on the same
    // shape, markup-carve/carve-php#1431).
    for prefix in ["lqll", "lqlll", "lqllq", "lqqll", "qlqll"] {
        let html = to_html(&link_doc(prefix));
        assert!(html.contains("href=\"/url\""), "{prefix}: {html}");
        assert!(
            !html.contains("[r]: /url"),
            "{prefix}: the definition stayed visible: {html}"
        );
        let note = to_html(&footnote_doc(prefix));
        assert!(note.contains("doc-endnotes"), "{prefix}: {note}");
        assert!(
            !note.contains("[^f]: note"),
            "{prefix}: the definition stayed visible: {note}"
        );
    }
}

#[test]
fn an_indented_quote_that_reaches_no_item_is_still_text() {
    // INTENDED SURVIVOR, and the one a looser fix breaks. `> a` opens no item,
    // so the indented `>` on the next line reaches no content column, is
    // ordinary text, and nothing may register from it (carve-rs#1082). A fix
    // that stripped any indented `>` would pass all 62 above and break this.
    let src = "> a\n>   > [r]: /url\n\nSee [r][].\n";
    let html = to_html(src);
    assert!(!html.contains("href=\"/url\""), "{html}");
    assert!(html.contains("&gt; [r]: /url"), "{html}");
}

#[test]
fn a_definition_between_two_content_columns_is_still_text() {
    // INTENDED SURVIVOR. Under `- > - - x` the live columns are 2, 6 and 8 and
    // nothing between them; a definition written at 7 reaches none of them
    // exactly and folds as the item's paragraph text (PART 9 §24 C3). Composing
    // the strips must FIND the columns, never relax which ones exist. The
    // executable spec answers this document the same way.
    let src = "- > - - x\n  >    [r]: /url\n\nSee [r][].\n";
    let html = to_html(src);
    assert!(
        html.contains("[r]: /url"),
        "the line stopped being text: {html}"
    );
    assert!(
        !html.contains("href=\"/url\""),
        "registered from a column nothing reaches: {html}"
    );
}

#[test]
fn a_comment_fence_at_these_prefixes_still_hides_the_definition() {
    // INTENDED SURVIVOR. Reaching the definition is not registering it
    // (markup-carve/carve#1341): the widened strip must stop at a comment span
    // exactly as the narrow one did.
    for prefix in ["lqll", "qlqll"] {
        let carried = carried(prefix);
        let src = format!(
            "{}\n{carried}%%%\n{carried}[r]: /url\n{carried}%%%\n\nSee [r][].\n",
            opener(prefix)
        );
        assert!(
            !to_html(&src).contains("href=\"/url\""),
            "a commented-out definition registered at {prefix}: {}",
            to_html(&src)
        );
    }
}
