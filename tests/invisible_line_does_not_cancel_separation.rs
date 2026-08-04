//! PART 9 section 17 L1b (markup-carve/carve#630): an invisible line does not
//! cancel a blank-line separation.
//!
//! carve#621 settled that an invisible construct is not the second PARAGRAPH
//! that loosens an item, because it renders nothing. This carries the same fact
//! one step further: such a line cannot stand BETWEEN the blank and the
//! paragraph after it either, because it is not a separator. Reading only the
//! first collected block found the comment and called the item tight.
//!
//! Every expectation below was measured against carve-js and agrees.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src)
}

const LOOSE: &str = "<ul>\n  <li><p>a</p>\n    <p>text</p>\n  </li>\n</ul>";

#[test]
fn a_comment_between_the_blank_and_the_paragraph_still_loosens() {
    assert_eq!(html("- a\n\n  %% n\n  text\n"), LOOSE);
}

#[test]
fn a_definition_there_loosens_the_same_way() {
    // The other invisible kind. It renders nothing either, so it is not a
    // separator either.
    assert_eq!(html("- a\n\n  [r]: /u\n  text\n"), LOOSE);
}

#[test]
fn a_run_of_invisible_lines_loosens_too() {
    // The scan has to look PAST all of them, not just one.
    assert_eq!(html("- a\n\n  %% n\n  %% m\n  text\n"), LOOSE);
}

#[test]
fn an_invisible_line_with_no_paragraph_behind_it_stays_tight() {
    // The control for over-reach. Without it these tests would pass just as
    // well if an item had simply stopped going tight - carve#621's half of the
    // rule still holds, and this is the case that pins it.
    assert_eq!(html("- a\n\n  %% n\n"), "<ul>\n  <li>a</li>\n</ul>");
}

#[test]
fn a_sub_block_behind_an_invisible_line_stays_tight() {
    // The other control. L1 loosens on a second PARAGRAPH; a sub-list after the
    // blank keeps the item tight (section 17 L2), and looking past the comment
    // must not turn that into a paragraph.
    assert_eq!(
        html("- a\n\n  %% n\n  - b\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}
