//! Past the nesting cap an opener DEGRADES to literal paragraph text (§25): it
//! does not vanish, and the run it forms ends at the first blank line like any
//! other paragraph.
//!
//! Both halves were wrong here in ways nothing could catch: no corpus document
//! reaches the cap at all, so 200 titles and no trace of the rest looked the
//! same as a correct render (carve-rs#530).

use carve::to_html;

const CAP: usize = 200;

/// Run a cap-deep case on a thread with room.
///
/// A document nested to the cap costs one debug-build frame per level, and a
/// test thread gets 2 MiB. The library handles it - the release binary renders
/// these on the main thread - so this is the harness making room, not a
/// behaviour change (carve-rs#530).
fn with_room(case: fn()) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(case)
        .expect("thread spawns")
        .join()
        .expect("the case finishes");
}

fn openers(n: usize) -> String {
    ":::: note\n".repeat(n)
}

#[test]
fn an_over_cap_opener_survives_a_closer_after_it() {
    with_room(an_over_cap_opener_survives_a_closer_after_it_case);
}

fn an_over_cap_opener_survives_a_closer_after_it_case() {
    // The same input WITHOUT the closers already kept them, which is what made
    // the loss look like a depth problem rather than a closer one.
    let html = to_html(&format!("{}::::\n::::\n::::\nz\n", openers(CAP + 3)));

    assert_eq!(html.matches("note").count(), CAP + 3);
}

#[test]
fn one_closer_is_enough_to_have_lost_them() {
    with_room(one_closer_is_enough_to_have_lost_them_case);
}

fn one_closer_is_enough_to_have_lost_them_case() {
    let html = to_html(&format!("{}::::\nz\n", openers(CAP + 3)));

    assert_eq!(html.matches("note").count(), CAP + 3);
}

#[test]
fn a_blank_line_ends_the_flattened_run() {
    with_room(a_blank_line_ends_the_flattened_run_case);
}

fn a_blank_line_ends_the_flattened_run_case() {
    // A single paragraph held a literal blank line, which no other paragraph in
    // the language can contain, and swallowed the block after it.
    let html = to_html(&format!("{}\ny\n", openers(CAP + 3)));

    assert_eq!(html.matches("<p>").count(), 2);
    assert!(
        html.contains("<p>y</p>"),
        "the text after the blank is its own block"
    );
}

#[test]
fn text_directly_after_the_openers_still_joins_them() {
    with_room(text_directly_after_the_openers_still_joins_them_case);
}

fn text_directly_after_the_openers_still_joins_them_case() {
    // No blank: one paragraph, the shape carve#494 surveyed.
    let html = to_html(&format!("{}x\n", openers(CAP + 3)));

    assert_eq!(html.matches("<p>").count(), 1);
    assert_eq!(html.matches("note").count(), CAP + 3);
}

#[test]
fn under_the_cap_nothing_flattens() {
    with_room(under_the_cap_nothing_flattens_case);
}

fn under_the_cap_nothing_flattens_case() {
    let html = to_html(&format!("{}x\n", openers(3)));

    assert_eq!(html.matches("<p>").count(), 1);
    assert_eq!(html.matches("aside").count(), 6);
}

// --- An invisible construct in the gap (carve-rs#557) ---

#[test]
fn a_blank_before_a_sibling_loosens_across_an_invisible_construct() {
    with_room(a_blank_before_a_sibling_loosens_across_an_invisible_construct_case);
}

fn a_blank_before_a_sibling_loosens_across_an_invisible_construct_case() {
    // The comment is not the item's second block - it renders nothing - but the
    // blank is still between this item and the next, and a blank before a
    // sibling loosens the list. Corpus 87-compact-list-blocks-6.
    assert_eq!(
        to_html("- a\n\n  %% just a note\n- b"),
        "<ul>\n  <li><p>a</p></li>\n  <li><p>b</p></li>\n</ul>"
    );
}

#[test]
fn an_invisible_continuation_alone_still_leaves_the_item_tight() {
    with_room(an_invisible_continuation_alone_still_leaves_the_item_tight_case);
}

fn an_invisible_continuation_alone_still_leaves_the_item_tight_case() {
    // Corpus 87-compact-list-blocks-5: with no sibling after it, there is
    // nothing for the blank to loosen against.
    assert_eq!(to_html("- a\n\n  [r]: /u"), "<ul>\n  <li>a</li>\n</ul>");
}

#[test]
fn the_flattened_run_still_resolves_escapes() {
    with_room(the_flattened_run_still_resolves_escapes_case);
}

fn the_flattened_run_still_resolves_escapes_case() {
    // The block and inline passes share one depth counter, so at the cap the
    // inline pass refused too and the run was published verbatim - which is
    // how `fmt` stopped round-tripping: the canonical form escapes a flattened
    // opener, and re-parsing kept the backslashes.
    let openers: String = (0..CAP)
        .map(|i| format!("{} note\n", ":".repeat(3 + i)))
        .collect();
    let closers: String = (0..CAP)
        .rev()
        .map(|i| format!("{}\n", ":".repeat(3 + i)))
        .collect();
    let html = to_html(&format!("{openers}\\:\\: x\n{closers}"));

    assert!(html.contains("<p>:: x</p>"), "escapes resolve at the cap");
    assert!(
        !html.contains("\\:"),
        "no backslash survives into the output"
    );
}
