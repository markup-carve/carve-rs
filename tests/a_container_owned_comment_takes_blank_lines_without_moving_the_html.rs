//! carve#5192: `fmt` writes a container-owned `%%` line with a blank line
//! before and after it, as CARVE-P11-079 (b) does for a document-owned one.
//! The blanks never change the HTML: an item's trailing run of comments takes
//! them around the run, and a blank that would loosen a list is not written.

fn assert_fmt(source: &str, expected: &str) {
    let written = carve::to_carve(source);
    assert_eq!(written, expected, "fmt of {source:?}");
    assert_eq!(carve::to_carve(&written), written, "second fmt of {source:?}");
    assert_eq!(
        carve::to_html(&written),
        carve::to_html(source),
        "HTML of {source:?}"
    );
}

#[test]
fn the_ruled_shapes() {
    assert_fmt("- a\n\n  %% just a note\n- b\n", "- a\n\n  %% just a note\n\n- b\n");
    assert_fmt("> a\n> %% b\n", "> a\n>\n> %% b\n");
}

#[test]
fn a_comment_run_takes_its_blank_lines_around_the_run() {
    // A blank between the two would make three blank lines with the erased
    // lines gone, the hard boundary that ends the list (CARVE-P9-014).
    for source in [
        "- a\n\n  %% n\n  %% t\n- b\n",
        "- a\n  %% n\n\n  %% t\n- b\n",
    ] {
        assert_fmt(source, "- a\n\n  %% n\n  %% t\n\n- b\n");
    }
    assert_fmt(
        "- a\n\n  ```\n  x\n  ```\n  %% n\n  %% t\n- b\n",
        "- a\n\n  ```\n  x\n  ```\n\n  %% n\n  %% t\n\n- b\n",
    );
}

#[test]
fn a_comment_run_with_no_marker_below_keeps_its_blank_lines() {
    assert_fmt("%% a\n%% b\n", "%% a\n\n%% b\n");
    assert_fmt("> a\n> %% b\n>\n> %% c\n", "> a\n>\n> %% b\n>\n> %% c\n");
    let source = "- a\n\n  b\n\n  %% n\n\n  %% t\n\n  c\n";
    assert_fmt(source, source);
}

#[test]
fn no_blank_loosens_a_tight_list() {
    // After a sub-list, before a fence: a blank there loosens the item.
    let source = "- - s\n  %% note\n  ```\n  y\n  ```\n- next\n";
    assert_fmt(source, source);
    // A trailing comment of a loose item above a tight list's marker.
    let source = "- - a\n\n    b\n    %% note\n- next\n";
    assert_fmt(source, source);
}
