//! carve#5987: a tab on a line a `+` looks past is measured from each frame
//! column (PART 9 §24 C5), so a tab-led note attribute line stays the note's.
//! The writer hoists a note that would take the paragraph below it as a lazy line.

const ROW_1075: &str = "1. x\n   1. +\n   [^m]: n\n   \t{#i}\n   q\n\np[^m]\n";

const ROW_1075_HTML: &str = r##"<ol>
  <li>x
    <ol>
      <li>q</li>
    </ol>
  </li>
</ol>
<p>p<a id="fnref1" href="#fn1" role="doc-noteref"><sup>1</sup></a></p>
<section role="doc-endnotes" aria-label="Footnotes">
  <hr>
  <ol>
    <li id="fn1">
      <p>n<a href="#fnref1" role="doc-backlink" aria-label="Back to reference">↩</a></p>
    </li>
  </ol>
</section>"##;

#[test]
fn corpus_1075_renders_and_formats() {
    assert_eq!(carve::to_html(ROW_1075).trim(), ROW_1075_HTML);
    let out = carve::to_carve(ROW_1075);
    assert_eq!(out, "1. x\n   1. q\n\np[^m]\n\n[^m]: n\n");
    assert_eq!(carve::to_carve(&out), out);
    assert_eq!(carve::to_html(&out).trim(), ROW_1075_HTML);
}

#[test]
fn a_tab_past_the_frame_column_matches_its_space_column() {
    // After the item frame at column 3, `   \t` reaches column 7, as seven spaces do.
    for (tabbed, spaced) in [
        ("   \t", "       "),
        ("   \t ", "        "),
        ("    \t", "       "),
    ] {
        for attrs in ["{#i}", "{.c}", "more"] {
            let source =
                |lead: &str| format!("1. x\n   1. +\n   [^m]: n\n{lead}{attrs}\n   q\n\np[^m]\n");
            assert_eq!(
                carve::to_html(&source(tabbed)),
                carve::to_html(&source(spaced)),
                "{:?}",
                source(tabbed)
            );
            let out = carve::to_carve(&source(tabbed));
            assert_eq!(carve::to_carve(&out), out, "{:?}", source(tabbed));
            assert_eq!(carve::to_html(&out), carve::to_html(&source(tabbed)));
        }
    }
}

#[test]
fn a_tab_two_frames_deep_restarts_at_the_inner_frame() {
    // Frames at 2 and 5: `  ` then `   \t` reaches 9, past the note body at 7.
    let tabbed = "- x\n  - y\n     - +\n     [^m]: n\n     \t{#i}\n     q\n\np[^m]\n";
    let spaced = "- x\n  - y\n     - +\n     [^m]: n\n         {#i}\n     q\n\np[^m]\n";
    assert_eq!(carve::to_html(tabbed), carve::to_html(spaced));
}

#[test]
fn a_note_between_item_blocks_is_hoisted_off_the_paragraph_below() {
    // Written between the empty sub-item and `q`, the note took `q` as its lazy line.
    let source = "- x\n   - y\n     - +\n     [^m]: n\n       {#i}\n      q\n\np[^m]\n";
    let out = carve::to_carve(source);
    assert_eq!(
        out,
        "- x\n  - y\n    - {empty}\n    q\n\np[^m]\n\n[^m]: n\n"
    );
    assert_eq!(carve::to_carve(&out), out);
    assert_eq!(carve::to_html(&out), carve::to_html(source));
}
