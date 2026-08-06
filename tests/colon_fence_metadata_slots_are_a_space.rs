//! The colon fence's metadata slots are spaces too.
//!
//! `admonition_open = colon_fence, space, admonition_type, [space+,
//! quoted_title], [space+, label]`. PART 7's MARKER SEPARATORS AND PADDING
//! SLOTS decides the terminal by POSITION rather than by the slot's role: a tab
//! is syntax only inside a line's leading indentation run, and every slot on
//! this line sits after the fence.
//!
//! carve#886 briefly said a padding slot took `whitespace` and therefore
//! admitted a tab; carve-rs#712 implemented that faithfully while the clause was
//! live. carve#901 landed as carve#905 and reverted it, leaving the padding
//! slots `space` (carve-rs#722, corpus category 255).
//!
//! ONE CASE PER SLOT, PER DIRECTION. A single input that tabs BOTH slots cannot
//! discriminate: with only the title slot corrected it still fails, for the
//! label's reason, and with only the label slot corrected it still fails for the
//! title's. Every test below isolates one slot, and runs the tab both before and
//! after a space, because the rule is about a RUN and a check on the run's first
//! character is not a check on the rule.

fn html(source: &str) -> String {
    carve::to_html(source)
}

fn assert_opened_nothing(label: &str, out: &str) {
    assert!(
        out.contains(":::"),
        "{label}: opener did not survive as text: {out}"
    );
    assert!(
        !out.contains("<aside"),
        "{label}: opened an admonition: {out}"
    );
    assert!(
        !out.contains("admonition-title"),
        "{label}: took a title: {out}"
    );
    assert!(!out.contains("div-label"), "{label}: took a label: {out}");
}

#[test]
fn a_tab_does_not_pad_the_title_slot() {
    assert_opened_nothing("title, tab first", &html("::: note\t\"Title\"\nx\n:::\n"));
    assert_opened_nothing(
        "title, space then tab",
        &html("::: note \t\"Title\"\nx\n:::\n"),
    );
}

#[test]
fn a_tab_does_not_pad_the_label_slot() {
    // This slot was missed entirely by carve-rs#712, which narrowed only the
    // slot before the title. It stayed `str::trim_start`, i.e.
    // `char::is_whitespace`.
    assert_opened_nothing("label, tab first", &html("::: note \"T\"\t[lbl]\nx\n:::\n"));
    assert_opened_nothing(
        "label, space then tab",
        &html("::: note \"T\" \t[lbl]\nx\n:::\n"),
    );
}

#[test]
fn no_unicode_space_pads_the_title_slot() {
    for (label, ws) in [
        ("form feed", '\u{000c}'),
        ("vertical tab", '\u{000b}'),
        ("en quad", '\u{2000}'),
        ("no-break space", '\u{00a0}'),
    ] {
        assert_opened_nothing(
            &format!("title, {label}"),
            &html(&format!("::: note{ws}\"Title\"\nx\n:::\n")),
        );
        assert_opened_nothing(
            &format!("title, space then {label}"),
            &html(&format!("::: note {ws}\"Title\"\nx\n:::\n")),
        );
    }
}

#[test]
fn no_unicode_space_pads_the_label_slot() {
    for (label, ws) in [
        ("form feed", '\u{000c}'),
        ("vertical tab", '\u{000b}'),
        ("en quad", '\u{2000}'),
        ("no-break space", '\u{00a0}'),
    ] {
        assert_opened_nothing(
            &format!("label, {label}"),
            &html(&format!("::: note \"T\"{ws}[lbl]\nx\n:::\n")),
        );
        assert_opened_nothing(
            &format!("label, space then {label}"),
            &html(&format!("::: note \"T\" {ws}[lbl]\nx\n:::\n")),
        );
    }
}

#[test]
fn a_space_still_pads_both_slots() {
    // The control. Narrowing to `space` must not close the door on the spelling
    // the grammar does admit, in either slot or in a run of them.
    let one = html("::: note \"Title\" [lbl]\nx\n:::\n");
    assert!(one.contains("admonition-title"), "title slot closed: {one}");
    assert!(one.contains("div-label"), "label slot closed: {one}");

    let many = html("::: note  \"Title\"  [lbl]\nx\n:::\n");
    assert_eq!(many, one, "the slots are `space+`, so a run must still pad");
}

#[test]
fn tabbing_both_slots_at_once_proves_nothing_on_its_own() {
    // Kept deliberately, and kept honest: this input fails for EITHER slot's
    // reason, so on its own it cannot tell which one was corrected. The count
    // guard below is what makes the file's coverage a claim rather than an
    // assumption - the two per-slot tests above are what actually discriminate,
    // and this asserts they exist.
    assert_opened_nothing("both slots", &html("::: note\t\"T\"\t[lbl]\nx\n:::\n"));

    // The needles are assembled from halves so they do not appear literally in
    // this file and count themselves - the first draft did, and read 3 for 2.
    let source = include_str!("colon_fence_metadata_slots_are_a_space.rs");
    assert_eq!(
        source
            .matches(concat!("fn a_tab_does_not", "_pad_the_"))
            .count(),
        2,
        "the per-slot tab tests are what discriminate; both must be present"
    );
    assert_eq!(
        source
            .matches(concat!("fn no_unicode_space", "_pads_the_"))
            .count(),
        2,
        "the per-slot Unicode tests are what discriminate; both must be present"
    );
}
