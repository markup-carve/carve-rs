//! A blank line inside verbatim content carries no structural indent
//! (markup-carve/carve#1040).
//!
//! PART 11 section 7 is NO WHITESPACE-ONLY LINE: "`fmt` never emits a line whose
//! only content is ASCII spaces or tabs. Such a line is emitted EMPTY." Its
//! verbatim exception applies "to the STRUCTURAL INDENT only: a verbatim line
//! sitting inside a list item carries the item's content-column indent, and when
//! the verbatim content on that line is EMPTY the indent alone is what remains -
//! that is layout, and it is omitted."
//!
//! THE LIST ITEM WAS THE ONLY CONTAINER THAT HELD. Its writer drops the indent
//! itself (carve-rs#440); every other container relied on `restore_verbatim`,
//! whose comment claimed "a later trim removes a whitespace-only line". Nothing
//! does: `normalize` runs its whitespace-only pass BEFORE `restore_verbatim`,
//! while the line still carries the blank sentinel and so is not whitespace-only
//! yet. A check that cannot fail, and a code fence under a footnote definition
//! or a definition-list description came out with an indented blank line.
//!
//! THE BLOCK QUOTE IS THE ONE PREFIX THAT STAYS, and it is asserted here so a
//! wider strip cannot pass: `>` is not layout, and an EMPTY line would close the
//! quote and take the open fence with it.

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

/// Every line of `out` is either empty or carries something other than ASCII
/// space and tab - the section 7 property, asserted over the emitted bytes.
fn whitespace_only_lines(out: &str) -> Vec<(usize, String)> {
    out.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && line.trim_matches([' ', '\t']).is_empty())
        .map(|(i, line)| (i + 1, line.to_string()))
        .collect()
}

#[test]
fn a_fenced_block_under_a_footnote_definition_keeps_its_blank_line_empty() {
    let out = fmt("[^f]: n\n+\n```\na\n\nb\n```\n\nsee[^f]\n");

    assert_eq!(
        whitespace_only_lines(&out),
        Vec::<(usize, String)>::new(),
        "section 7: the indent of an EMPTY verbatim line is layout and is omitted\n{out:?}"
    );
    assert!(
        out.contains("  ```\n  a\n\n  b\n  ```"),
        "the fence body keeps its own indent, the blank line takes none\n{out:?}"
    );
}

#[test]
fn a_fenced_block_under_a_definition_description_keeps_its_blank_line_empty() {
    let out = fmt(":: t\n:  d\n+\n```\na\n\nb\n```\n");

    assert_eq!(
        whitespace_only_lines(&out),
        Vec::<(usize, String)>::new(),
        "section 7: the indent of an EMPTY verbatim line is layout and is omitted\n{out:?}"
    );
    assert!(
        out.contains("   ```\n   a\n\n   b\n   ```"),
        "the fence body keeps its own indent, the blank line takes none\n{out:?}"
    );
}

#[test]
fn a_fenced_block_in_a_list_item_keeps_its_blank_line_empty() {
    let out = fmt("- ```\n  a\n\n  b\n  ```\n");

    assert_eq!(
        whitespace_only_lines(&out),
        Vec::<(usize, String)>::new(),
        "the container that already held, kept honest\n{out:?}"
    );
}

#[test]
fn a_block_quote_keeps_its_marker_on_a_blank_verbatim_line() {
    let out = fmt("> ```\n> a\n>\n> b\n> ```\n");

    assert_eq!(
        whitespace_only_lines(&out),
        Vec::<(usize, String)>::new(),
        "`>` is not whitespace, so the line was never whitespace-only\n{out:?}"
    );
    assert!(
        out.lines().all(|line| line.starts_with('>')),
        "an EMPTY line would close the quote and take the open fence with it\n{out:?}"
    );
    // The fence survives as ONE code block: the blank line inside it did not end
    // the quote, so `a` and `b` are still one block rather than two.
    assert_eq!(
        carve::to_html(&out).matches("<pre>").count(),
        1,
        "the round trip keeps one fenced block\n{out:?}"
    );
}

#[test]
fn the_blank_line_stays_inside_the_fence_on_the_round_trip() {
    for src in [
        "[^f]: n\n+\n```\na\n\nb\n```\n\nsee[^f]\n",
        ":: t\n:  d\n+\n```\na\n\nb\n```\n",
        "> ```\n> a\n>\n> b\n> ```\n",
    ] {
        let out = fmt(src);
        assert_eq!(
            carve::to_html(&out),
            carve::to_html(src),
            "PART 11 section 1: fmt preserves what the document says\n{src:?}\n{out:?}"
        );
        assert_eq!(
            fmt(&out),
            out,
            "PART 11 section 1: fmt is idempotent\n{out:?}"
        );
    }
}
