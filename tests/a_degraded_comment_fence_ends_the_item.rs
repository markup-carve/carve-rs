//! A COMMENT FENCE WITH NO CLOSER ENDS THE ITEM IT WAS WRITTEN IN
//! (markup-carve/carve-rs#1512).
//!
//! PART 9 §28 degrades an unterminated `%%%` to an ordinary line comment, and a
//! line comment closes the paragraph above it. With no paragraph left, a line
//! written BELOW the item's content column continues nothing, so the executable
//! spec (`tests/spec` at carve `86569bd`) ends the list and reparses the line at
//! document level. This engine kept it inside the item.
//!
//! IT IS THE FENCE, NOT THE COMMENT. A plain `%%` at the same column closes the
//! paragraph too and the item survives it; a `%%%` WITH a closer is a real span
//! and ends nothing; a `%%%` written BELOW the content column reached no
//! container and ends nothing either. All three are pinned here as controls,
//! each against the executable spec's own output.
//!
//! MEASURED, NOT ASSUMED. 396 documents: the l(ist)/q(uote) container prefixes
//! to depth three, a comment line at the innermost content column in three
//! widths (`%%`, `%%%`, `%%%%`), and one following line at every column strictly
//! below it, in four kinds. Before: 82 disagreed with the executable spec.
//! After: 30, with 52 fixed and 0 newly broken. The 30 that remain are two
//! separate defects, both filed: the plain-`%%` rows (18) are
//! markup-carve/carve-rs#1517, and the 12 deeper-nesting rows where the fence
//! sits at a DESCENDANT's content column are markup-carve/carve-rs#1518.

use carve::{to_html, to_html_with_options, Options};

fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

#[test]
fn the_reported_document_ends_the_list() {
    assert_eq!(
        both_paths("- a\n  %%% x\n # h\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<p># h</p>",
    );
}

#[test]
fn every_following_line_kind_leaves_the_item() {
    // Each expectation is the executable spec's output for that document. The
    // line reparses at DOCUMENT level, which is why `# h` and `---` are text
    // there: at column 1 neither is an opener.
    for (src, expected) in [
        ("- x\n  %%% x\n b\n", "<ul>\n  <li>x</li>\n</ul>\n<p>b</p>"),
        (
            "- x\n  %%% x\n # h\n",
            "<ul>\n  <li>x</li>\n</ul>\n<p># h</p>",
        ),
        (
            "- x\n  %%% x\n ---\n",
            "<ul>\n  <li>x</li>\n</ul>\n<p>\u{2014}</p>",
        ),
        (
            "- x\n  %%% x\n - y\n",
            "<ul>\n  <li>x</li>\n</ul>\n<ul>\n  <li>y</li>\n</ul>",
        ),
    ] {
        assert_eq!(both_paths(src), expected, "{src:?}");
    }
}

#[test]
fn a_wider_fence_answers_the_same_way() {
    // §28 degrades on the absence of a CLOSER, not on the width.
    assert_eq!(
        both_paths("- x\n  %%%% x\n b\n"),
        "<ul>\n  <li>x</li>\n</ul>\n<p>b</p>",
    );
}

#[test]
fn a_blank_line_does_not_change_the_answer() {
    assert_eq!(
        both_paths("- x\n  %%% x\n\n b\n"),
        "<ul>\n  <li>x</li>\n</ul>\n<p>b</p>",
    );
}

#[test]
fn the_item_ends_inside_a_quote_too() {
    assert_eq!(
        both_paths("> - x\n>   %%% x\n>  # h\n"),
        "<blockquote>\n  <ul>\n    <li>x</li>\n  </ul>\n  <p># h</p>\n</blockquote>",
    );
}

#[test]
fn an_at_column_line_after_the_fence_is_still_the_item_s() {
    // The rule is about the BAND below the content column. A line AT the column
    // is item content as it always was, and it does not reopen the band for the
    // line under it.
    assert_eq!(
        both_paths("- x\n  %%% x\n  # h\n"),
        "<ul>\n  <li>x\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>",
    );
    assert_eq!(
        both_paths("- x\n  %%% x\n  b\n c\n"),
        "<ul>\n  <li>x\n    b\n  </li>\n</ul>\n<p>c</p>",
    );
}

#[test]
fn a_line_comment_leaves_the_item_open() {
    // THE FIRST CONTROL. `%%` closes the paragraph and the item survives it, so
    // a rule widened from the fence to every comment would take this with it.
    assert_eq!(
        both_paths("- x\n  %% x\n b\n"),
        "<ul>\n  <li>x\n    b\n  </li>\n</ul>",
    );
}

#[test]
fn a_terminated_fence_leaves_the_item_open() {
    // THE SECOND CONTROL. With a closer it is a real span, not a degraded one.
    assert_eq!(
        both_paths("- x\n  %%% c\n  %%%\n b\n"),
        "<ul>\n  <li>x\n    b\n  </li>\n</ul>",
    );
}

#[test]
fn a_fence_below_the_content_column_ends_nothing() {
    // THE THIRD CONTROL. Written below the column the fence reached no
    // container, so it is lazy paragraph text and ends nothing.
    assert_eq!(
        both_paths("- x\n %%% x\n b\n"),
        "<ul>\n  <li>x\n    b\n  </li>\n</ul>",
    );
}
