//! PART 11 section 9: a hard break is emitted as a BACKSLASH before the newline,
//! never as two trailing spaces.
//!
//! Both mean `<br />` to a CommonMark reader -- verified against commonmark.js. The
//! difference is what survives handling: trailing whitespace is removed by editors
//! that strip on save, by `git apply --whitespace=fix` and by CI whitespace checks,
//! and losing ONE of the two spaces is enough for the break to VANISH rather than
//! degrade.
//!
//! A line block converts to hard breaks, so this was our own output carrying the
//! fragile spelling (carve#352, corpus 41-line-blocks).

#[test]
fn an_explicit_hard_break_is_a_backslash() {
    assert_eq!(carve::to_markdown("a\\\nb\n"), "a\\\nb\n");
}

#[test]
fn no_line_ends_in_whitespace() {
    // The property that matters: the break cannot be destroyed by whitespace
    // handling. Stated this way it keeps holding if the spelling is revisited.
    let out = carve::to_markdown("a\\\nb\n");
    for line in out.lines() {
        assert_eq!(line, line.trim_end_matches([' ', '\t']), "in {out:?}");
    }
}

#[test]
fn a_line_block_uses_it() {
    let src = "::: |\nStanza one,\nstill one.\n\nStanza two.\n:::\n";
    assert_eq!(
        carve::to_markdown(src),
        "Stanza one,\\\nstill one.\n\nStanza two.\n"
    );
}

#[test]
fn a_soft_break_stays_a_plain_newline() {
    assert_eq!(carve::to_markdown("a\nb\n"), "a\nb\n");
}
