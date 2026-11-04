//! markup-carve/carve#5136 (CARVE-P12-022, CARVE-P12-014): a node joined across
//! a `%%` line S1 erased omits `pos`; one the line only trails ends before it.

use serde_json::Value;

fn tree(source: &str) -> Value {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    serde_json::from_str(&carve::to_json_with_options(source, &options)).unwrap()
}

fn span(node: &Value) -> Option<(u64, u64)> {
    let pos = node.get("pos")?;
    Some((pos["startOffset"].as_u64()?, pos["endOffset"].as_u64()?))
}

#[test]
fn the_paragraph_and_its_crossing_break_omit_pos() {
    let doc = tree("a\n%%\tx\nb\n");
    let paragraph = &doc["children"][0];
    assert_eq!(paragraph["type"], "paragraph");
    assert_eq!(span(paragraph), None);
    let children = paragraph["children"].as_array().unwrap();
    assert_eq!(span(&children[0]), Some((0, 1)));
    assert_eq!(children[1]["type"], "soft_break");
    assert_eq!(span(&children[1]), None);
    assert_eq!(span(&children[2]), Some((7, 8)));
    assert_eq!(doc["children"][1]["type"], "comment");
    assert_eq!(span(&doc["children"][1]), Some((2, 6)));
}

#[test]
fn an_item_keeps_its_span_over_its_own_comment() {
    let doc = tree("- a\n  %% c\n  b\n");
    let item = &doc["children"][0]["items"][0];
    assert_eq!(span(item), Some((0, 14)));
    assert_eq!(span(&item["children"][0]), None);
    assert_eq!(item["children"][1]["type"], "comment");
    assert_eq!(span(&item["children"][1]), Some((6, 10)));
}

#[test]
fn a_container_the_line_only_trails_ends_at_its_last_child() {
    let doc = tree("- :: term\n  : payload\n\n    %% c\n- sub\n");
    assert_eq!(span(&doc["children"][0]["items"][0]), Some((0, 21)));
}

#[test]
fn a_footnote_body_paragraph_omits_pos_and_the_note_reaches_its_text() {
    let doc = tree("[^a]: intro\n  %% c\n  more\n\nsee[^a]\n");
    let note = &doc["children"][1];
    assert_eq!(note["type"], "footnote");
    assert_eq!(span(note), Some((0, 25)));
    assert_eq!(span(&note["children"][0]), None);
}

#[test]
fn a_placeholder_comment_is_not_an_erased_line() {
    let doc = tree("> > a\n > +\n > %% c\n > b\n");
    // The erased line takes its `>` with it, so the inner quote reaches `b` (carve#5129).
    assert_eq!(span(&doc["children"][0]["children"][0]), Some((2, 23)));
}

#[test]
fn fmt_still_writes_the_line_back_inside_the_paragraph() {
    for source in [
        "a\n%% x\nb\n",
        "- - x\n  %% c\n    y\n",
        ":: t\n: a\n  %% c\n  tail\n",
    ] {
        let once = carve::to_carve(source);
        let twice = carve::to_carve(&once);
        assert_eq!(once, twice, "{source:?}");
        assert_eq!(carve::to_html(&once), carve::to_html(source), "{source:?}");
    }
    assert_eq!(carve::to_carve("a\n%% x\nb\n"), "a\n%% x\nb\n");
}
