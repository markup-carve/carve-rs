//! A blank inside an item's `%%%` span with nothing visible after it, up to
//! the sibling marker, still separates the items (PART 9 §17 L1c,
//! carve-rs#3798).

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_span_on_the_marker_line_loosens_the_list() {
    for source in [
        "- %%%\n    n\n\n%%%\n- c\n",
        "- %%%\n\n%%%\n- c\n",
        "- %%%\n  n\n\n  %%%\n- c\n",
        "1. %%%\n\n   %%%\n2. c\n",
    ] {
        assert!(
            html(source).contains("<li><p>c</p></li>"),
            "{source:?} -> {}",
            html(source)
        );
    }
}

#[test]
fn a_span_on_a_note_lead_loosens_the_list() {
    for source in [
        "x[^n]\n\n- a\n  [^n]: %%%\n\n\n%% c\n- c\n",
        "x[^n]\n\n- a\n  [^n]: %%%\n\n%%%\n- c\n",
        // The note's content is hoisted, so nothing of the item follows the blank.
        "x[^n]\n\n- a\n  [^n]: %%%\n\n%% c\n  %%%\n        visible\n- c\n",
    ] {
        let out = html(source);
        assert!(
            out.contains("<li><p>a</p></li>") && out.contains("<li><p>c</p></li>"),
            "{source:?} -> {out}"
        );
    }
}

#[test]
fn a_note_that_hands_back_to_its_item_keeps_the_list_tight() {
    // `more` is the item's: the blank stands inside the note, before host content.
    let source = "x[^n]\n\n- a\n  [^n]: %%%\n\n%% c\n  more\n- c\n";
    assert!(
        !html(source).contains("<li><p>"),
        "{source:?} -> {}",
        html(source)
    );
}
