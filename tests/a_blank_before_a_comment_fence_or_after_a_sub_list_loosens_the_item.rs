//! A blank line above an invisible line still parts the item's paragraphs
//! (PART 9 §17 L1b): an unclosed `%%%` is one comment line (§28), a closed
//! comment block renders nothing, and a sub-list's extent holds the blank
//! written after it.

fn loose(source: &str) -> bool {
    carve::to_html(source).contains("<li><p>")
}

#[test]
fn a_blank_before_a_comment_fence_loosens_the_next_paragraph() {
    for source in [
        "- o\n  :: t\n  : x\n\n\n\n  %%%\n  : d\n",
        "- o\n  :: t\n  : x\n\n\n\n  %%%\n  c\n  %%%\n  : d\n",
        "- o\n  :: t\n  : x\n\n\n\n  %%%%\n  %%%\n  y\n",
        "- o\n  > q\n\n\n\n  %%%\n  y\n",
        "1. o\n   ```\n   c\n   ```\n\n   %%% c\n   y\n",
    ] {
        assert!(loose(source), "{source:?} -> {}", carve::to_html(source));
    }
}

#[test]
fn a_blank_after_a_sub_list_loosens_a_paragraph_behind_a_comment() {
    for source in [
        "- o\n  - s\n\n  %% c\n  y\n",
        "- o\n  - s\n\n  %%%\n  y\n",
        "> - o\n>   - s\n>\n>   %%%\n>   c\n>   %%%\n>   y\n",
    ] {
        assert!(loose(source), "{source:?} -> {}", carve::to_html(source));
    }
}

#[test]
fn a_comment_inside_the_sub_list_does_not_carry_its_blank_out() {
    let source = "- host\n  - - a\n\n    %% note\n  x\n";
    assert!(!loose(source), "{}", carve::to_html(source));
}

#[test]
fn the_writer_adds_no_blank_between_a_sub_list_and_a_comment_above_a_paragraph() {
    let source = "- - x\n  %% c\n  y\n";
    assert_eq!(carve::to_carve(source), source);
    assert_eq!(
        carve::to_html(&carve::to_carve(source)),
        carve::to_html(source)
    );
}
