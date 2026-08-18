//! PART 9 §23: every SOFT line break inside a stanza becomes a HARD break, at
//! EVERY DEPTH (markup-carve/carve#1351, corpus `348`).
//!
//! The conversion ran on the stanza's top-level nodes only, so a closed inline
//! construct spanning a boundary kept the bare newline: `*a` over `b*` rendered
//! a bare newline inside the `strong` while the same two lines without the
//! emphasis got a `<br>`. Ruled two ways for a while - §23's sentence one way,
//! four rows on carve-js#1127 the other - and pinned neither way until now.
//!
//! THE ENGINE BROKE THE CLAUSE'S INVARIANT AGAINST ITSELF, which is what made it
//! a defect rather than a reading. §23's neighbour A BACKSLASH BREAK IS NOT
//! ADDITIVE states that ONE line boundary produces ONE `<br>`, however the
//! boundary is spelled - and this engine emitted the break inside the `strong`
//! for the backslash spelling and nothing for the bare one. Same boundary, same
//! container, two answers by spelling.
//!
//! THE TEST IS NODE KIND, NOT DEPTH. Both worked exemptions in that clause turn
//! on there being no node: a backslash consumes its own newline, and a verbatim
//! run carries the newline as its content, so "there is no boundary left in the
//! tree". An emphasis run consumes nothing - the boundary is a node beside its
//! text - so the exemption never reached it. Both exemptions therefore need no
//! code and get none; they are pinned below as the controls they are.

use carve::to_html;

#[test]
fn a_closed_emphasis_spanning_a_boundary_hardens_it() {
    // Corpus `348-3`. Both boundaries harden: the one inside the `strong` and
    // the one after it.
    assert_eq!(
        to_html("::: |\n*Roses are red,\nViolets are blue.*\nAnd so are you.\n:::\n"),
        "<div class=\"line-block\">\n  <p><strong>Roses are red,<br>\nViolets are blue.</strong><br>\nAnd so are you.</p>\n</div>"
    );
}

#[test]
fn the_two_spellings_of_one_boundary_now_agree() {
    // The invariant the engine used to break against itself. ONE boundary, ONE
    // break, however it is spelled - so these two documents must render the
    // same, and the backslash must not add a second one.
    let bare = to_html("::: |\n*a\nb*\n:::\n");
    let slash = to_html("::: |\n*a\\\nb*\n:::\n");
    assert_eq!(bare, slash, "the spelling changed the answer");
    assert_eq!(bare.matches("<br>").count(), 1, "{bare}");
    assert!(bare.contains("<strong>a<br>\nb</strong>"), "{bare}");
}

#[test]
fn a_verbatim_run_spanning_a_boundary_is_still_exempt() {
    // Corpus `348-2`, and the control that says the rule is about node KIND. The
    // run ATE the newline - it is content inside the `code` value, not a node -
    // so there is no boundary left to convert and the bare newline stays.
    assert_eq!(
        to_html("::: |\na `b\nc` d\n:::\n"),
        "<div class=\"line-block\">\n  <p>a <code>b\nc</code> d</p>\n</div>"
    );
    // A math span carries the break the same way, for the same reason.
    let math = to_html("::: |\na $`x\ny`$ b\n:::\n");
    assert!(!math.contains("<br>"), "{math}");
}

#[test]
fn every_container_that_can_span_a_boundary_hardens_it() {
    // The rule is not about emphasis; it is about a boundary that survives as a
    // node. Every inline container that can hold one is checked, because the
    // walk has to reach the slot each of them keeps its inlines in.
    for src in [
        "::: |\n*a\nb*\n:::\n",       // emphasis
        "::: |\n/a\nb/\n:::\n",       // the other emphasis
        "::: |\n[a\nb](/u)\n:::\n",   // link
        "::: |\n{.x}a\nb{.x}\n:::\n", // span
        "::: |\n^[a\nb]\n:::\n",      // footnote body
        "::: |\n{+a\nb+}\n:::\n",     // inline extension
    ] {
        assert!(
            to_html(src).contains("<br>"),
            "the boundary stayed soft in {src:?}: {}",
            to_html(src)
        );
    }
}

#[test]
fn a_nested_container_hardens_at_its_own_depth() {
    // Two levels down, so the walk has to recurse rather than look one deep.
    let out = to_html("::: |\n*/a\nb/*\n:::\n");
    assert!(out.contains("<br>"), "{out}");
}

#[test]
fn the_indent_after_a_hardened_boundary_is_still_content() {
    // Corpus `348-5`. §23 MEDIAL GAPS makes a leading run NBSP content, and
    // hardening the boundary above it must not disturb that.
    assert_eq!(
        to_html("::: |\n*Roses are red,\n  Violets are blue.*\n:::\n"),
        "<div class=\"line-block\">\n  <p><strong>Roses are red,<br>\n&nbsp;&nbsp;Violets are blue.</strong></p>\n</div>"
    );
}

#[test]
fn nothing_outside_a_line_block_hardens() {
    // THE CONTROL ON THE WHOLE CHANGE. §23 is a rule about stanzas. An ordinary
    // paragraph keeps its soft breaks at every depth, emphasis or not.
    for src in ["*a\nb*\n", "a\nb\n", "> *a\nb*\n"] {
        assert!(
            !to_html(src).contains("<br>"),
            "hardened outside a line block: {src:?} -> {}",
            to_html(src)
        );
    }
}

#[test]
fn the_writer_stops_spelling_the_break_with_a_backslash_at_depth() {
    // PART 11 §7c, amended by the same ruling: the writer spells a hard break
    // with a backslash only where the bare newline would NOT give it back. At a
    // nested boundary the bare newline now does, so the backslash comes off -
    // which is what the spec's own `.fmt` sidecar for corpus `348-4` says.
    let src = "::: |\n*Roses are red,\\\nViolets are blue.*\n:::\n";
    let written = carve::render_carve(&carve::parse(src)).expect("a line block writes back");
    assert_eq!(written, "::: |\n*Roses are red,\nViolets are blue.*\n:::\n");
    // And it is render-preserving, which is the property that licenses dropping
    // it at all.
    assert_eq!(to_html(&written), to_html(src));
}
