//! The canonical writer spells the frontmatter format token, the default one
//! included (markup-carve/carve#1040).
//!
//! PART 11 section 6b: the writer "spells the format token on the opening
//! delimiter for EVERY format, the default one included: frontmatter in `yaml`
//! comes back as `---yaml`, never as a bare `---`". The untyped opener is the
//! case the clause was written for, not one it forgot - "`---` and `---yaml`
//! open the same frontmatter (PART 1, "a bare `---` defaults to `yaml`"), so the
//! two are synonyms with nothing in the tree to tell them apart" - and it
//! answers the leniency argument in as many words: "A READER'S LENIENCY IS NOT A
//! WRITER'S LICENSE ... the writer is not parsing, it is choosing, and
//! `frontmatter_format` is one production over every format with no default case
//! in it."
//!
//! This engine reproduced the AUTHORED opener, so a bare `---` came back bare
//! while `---toml` came back typed - the special case for one value that section
//! 6b removes.

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

#[test]
fn a_bare_opener_is_written_back_with_the_default_format_token() {
    let out = fmt("---\ntitle: My Document\nauthor: Jane Doe\ndate: 2026-03-15\n---\n\nContent begins here.\n");

    assert_eq!(
        out,
        "---yaml\ntitle: My Document\nauthor: Jane Doe\ndate: 2026-03-15\n---\n\nContent begins here.\n"
    );
}

#[test]
fn an_empty_frontmatter_block_carries_the_token_too() {
    // The empty content is the blank line between the fences; only the opener
    // changes here.
    assert_eq!(fmt("---\n---\n\nx\n"), "---yaml\n\n---\n\nx\n");
}

#[test]
fn a_named_format_is_reproduced_as_written() {
    assert_eq!(
        fmt("---toml\na = 1\n---\n\nx\n"),
        "---toml\na = 1\n---\n\nx\n"
    );
    assert_eq!(fmt("---json\n{}\n---\n\nx\n"), "---json\n{}\n---\n\nx\n");
    // The token is already `yaml`, so nothing changes and nothing doubles.
    assert_eq!(
        fmt("---yaml\na: 1\n---\n\nx\n"),
        "---yaml\na: 1\n---\n\nx\n"
    );
}

#[test]
fn the_closer_stays_bare() {
    let out = fmt("---\na: 1\n---\n\nx\n");
    assert!(
        out.lines().nth(2) == Some("---"),
        "only the OPENING delimiter carries the token\n{out:?}"
    );
}

#[test]
fn writing_the_token_holds_the_invariants() {
    for src in [
        "---\na: 1\n---\n\nx\n",
        "---\n---\n\nx\n",
        "---toml\na = 1\n---\n\nx\n",
    ] {
        let out = fmt(src);
        assert_eq!(
            carve::to_html(&out),
            carve::to_html(src),
            "PART 11 section 1: fmt preserves what the document says\n{src:?}"
        );
        assert_eq!(
            fmt(&out),
            out,
            "PART 11 section 1: fmt is idempotent\n{out:?}"
        );
    }
}

#[test]
fn a_line_that_only_looks_like_an_opener_is_untouched() {
    // A thematic break is not a frontmatter opener, and gains no token. The
    // writer's byte-0 guard is what keeps it from becoming one.
    let out = fmt("---\n");
    assert!(
        !out.contains("yaml"),
        "a document that has no frontmatter grows none\n{out:?}"
    );
}
