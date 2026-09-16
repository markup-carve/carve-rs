//! A task box is CONTENT, so it is not part of the container prefix the
//! writer's position tests measure from (markup-carve/carve-rs#1674).

use carve::{parse, render_markdown};

fn md(src: &str) -> String {
    render_markdown(&parse(src)).expect("the document writes")
}

#[test]
fn a_hash_after_a_task_box_is_written_bare() {
    // No reader takes it for a heading: after `[ ] ` the line is paragraph
    // text. Measured through markdown-it-py and pulldown-cmark, with task
    // lists on and off, both spellings render the same thing.
    for (src, want) in [
        ("- [ ] \\# not a heading\n", "- [ ] # not a heading\n"),
        ("> - [ ] \\# x\n", "> - [ ] # x\n"),
        ("- [ ] \\# a_\n- [ ] _b\n", "- [ ] # a_\n- [ ] _b\n"),
        ("- [ ] \\#\tx\n", "- [ ] #\tx\n"),
        ("- [ ] \\## x\n", "- [ ] ## x\n"),
    ] {
        assert_eq!(md(src), want, "{src:?}");
    }
}

#[test]
fn a_heading_in_a_task_item_needs_no_closing_escape_either() {
    // The same position decides M1f's trailing run, and the line is not
    // emitted as a heading a reader can see.
    assert_eq!(md("- [ ] # a ##\n"), "- [ ] # a ##\n");
}

#[test]
fn a_heading_in_a_plain_item_keeps_both_escapes() {
    // CONTROL. Without a box the item's content IS the heading, so M1f's
    // trailing-run escape still applies.
    assert_eq!(md("- # a ##\n"), "- # a \\##\n");
}

#[test]
fn a_task_item_still_ends_the_block_the_next_one_opens() {
    // CONTROL for the arm that stays: two task items are two blocks, so the
    // underscores do not pair across them.
    assert_eq!(md("- [ ] _a\n- [ ] b_\n"), "- [ ] _a\n- [ ] b_\n");
}
