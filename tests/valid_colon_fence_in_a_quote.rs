//! A VALID colon fence that opens and closes inside a block quote leaves NO open
//! paragraph, so PART 1 S4 ends the quote and the flush-left line below it is a
//! top-level block.
//!
//! This is the non-absorbed sibling of `absorbed_colon_fence_in_a_quote.rs`.
//! There, §12's absorption makes the fence-shaped line ordinary prose, so the
//! quote's paragraph stays OPEN and the lazy line folds in. Here the fence is a
//! real container that opens and closes normally: when the closer is consumed
//! the container's paragraph closes with it, and nothing anywhere in the open
//! stack holds an open paragraph. S4's "otherwise" branch then applies.
//!
//! PART 1 S4, as written in `resources/grammar.ebnf`:
//!
//! ```text
//! LAZY CONTINUATION: if ANY container in the open stack holds an OPEN
//! PARAGRAPH [...] L folds into the INNERMOST such paragraph and NOTHING
//! closes [...] Otherwise close the unmatched containers and re-classify the
//! residue in the surviving context (S3).
//!
//! NO OPEN PARAGRAPH, NO LAZY LINE -- NORMATIVE. The "otherwise" is not a
//! leftover case [...]
//! ```
//!
//! markup-carve/carve#920 ruled S4 is read as written: a container holding no
//! paragraph holds no OPEN paragraph, whether it is still empty (that ticket's
//! shapes A and C) or has already closed the one it had (this file). carve-rs
//! answers all of them that way and does not move; carve-js and carve-php fold
//! the line into the quote instead, and the ruling records that as their
//! divergence.
//!
//! EVERY TEST HERE IS A CONTROL. Nothing in this file changes behavior -- it
//! pins a reading that was already correct, because a sweep of the corpus for a
//! valid `:::` container inside a block quote followed by a column-0 line
//! returns no document (markup-carve/carve-rs#741, the class catalogued in
//! markup-carve/carve#755). Without these assertions the ruling is unenforced
//! here and the next change to the quote's `ParaOpen` gate silently reverses it.
//!
//! Measured at carve-rs 3db9c4e against spec cf5c03a.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn a_closed_admonition_leaves_the_quote_with_no_open_paragraph() {
    // The ticket's shape. The admonition opened, took `body`, and closed on its
    // own `:::`. That closer ended the admonition's paragraph, and the quote
    // never had one of its own, so `tail` has nothing to fold into.
    assert_eq!(
        html("> ::: note\n> body\n> :::\ntail\n"),
        "<blockquote>\n  <aside class=\"admonition note\" aria-label=\"Note\">\n    <p>body</p>\n  </aside>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn quoted_prose_after_the_closer_reopens_a_paragraph_and_the_line_folds() {
    // THE DISCRIMINATOR, and the reason the test above is about an open
    // paragraph rather than about closers. One quoted line after the fence gives
    // the quote a paragraph of its own, and S4's first branch applies: `tail`
    // folds into it and NOTHING closes.
    //
    // A gate that ended the quote on the closer itself -- rather than on the
    // absence of an open paragraph -- passes the test above and fails this one.
    assert_eq!(
        html("> ::: note\n> body\n> :::\n> more\ntail\n"),
        "<blockquote>\n  <aside class=\"admonition note\" aria-label=\"Note\">\n    <p>body</p>\n  </aside>\n  <p>more\ntail</p>\n</blockquote>"
    );
}

#[test]
fn an_empty_closed_fence_answers_the_same_way() {
    // The container never held a paragraph at all, rather than having closed
    // one. S4 does not distinguish the two: neither holds an OPEN paragraph.
    // This is markup-carve/carve#920 shape A with the opener CLOSED, and it must
    // not diverge from the open form pinned in
    // `absorbed_colon_fence_in_a_quote.rs`.
    assert_eq!(
        html("> ::: note\n> :::\ntail\n"),
        "<blockquote>\n  <aside class=\"admonition note\" aria-label=\"Note\">\n\n  </aside>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn it_holds_at_quote_depth_two() {
    // Depth is not a parameter of this rule (carve#506, carve-rs#452). `tail`
    // fails to match BOTH quote prefixes, and neither quote holds an open
    // paragraph, so both close.
    assert_eq!(
        html("> > ::: note\n> > body\n> > :::\ntail\n"),
        "<blockquote>\n  <blockquote>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>body</p>\n    </aside>\n  </blockquote>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn the_rule_is_not_width_tagged() {
    // §12's absorption is not width-tagged and neither is this: a four-colon
    // fence closed by its own four-colon closer leaves exactly as little open as
    // a three-colon one. A gate keyed on the literal `:::` would pass every
    // other test in this file.
    assert_eq!(
        html("> :::: note\n> body\n> ::::\ntail\n"),
        "<blockquote>\n  <aside class=\"admonition note\" aria-label=\"Note\">\n    <p>body</p>\n  </aside>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn a_generic_div_answers_the_same_way() {
    // No type word, so a plain `<div>` rather than an admonition. The rule is
    // about whether a paragraph is open, never about which container closed.
    assert_eq!(
        html("> :::\n> body\n> :::\ntail\n"),
        "<blockquote>\n  <div>\n    <p>body</p>\n  </div>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn the_top_level_form_is_unchanged() {
    // CONTROL of the controls: the same four lines without the quote. There is
    // no partial match and so no S4 question at all, which is what isolates the
    // shapes above to the quote's lazy-continuation gate.
    assert_eq!(
        html("::: note\nbody\n:::\ntail\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>body</p>\n</aside>\n<p>tail</p>"
    );
}
