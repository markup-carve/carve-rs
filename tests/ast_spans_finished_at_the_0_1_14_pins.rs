//! AST spans left after carve-rs#4281 (carve-rs#4263, carve-rs#4274). HTML is
//! unchanged by every case here.

use serde_json::Value;

fn ast(source: &str) -> Value {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        source, &options,
    )))
    .expect("the serializer emits JSON")
}

fn at<'a>(node: &'a Value, path: &[&str]) -> &'a Value {
    path.iter()
        .fold(node, |node, step| match step.parse::<usize>() {
            Ok(index) => &node[index],
            Err(_) => &node[*step],
        })
}

type Span = Option<(u64, u64)>;
type Case<'a> = (&'a str, &'a [&'a str], (u64, u64), Vec<Span>);

fn offsets(node: &Value) -> Span {
    let pos = node.get("pos")?;
    Some((pos["startOffset"].as_u64()?, pos["endOffset"].as_u64()?))
}

/// The verse paragraph's span and each lead column's span, in order.
fn verse_lead(doc: &Value, path: &[&str]) -> (Span, Vec<Span>) {
    let paragraph = at(doc, path);
    assert_eq!(paragraph["type"], "paragraph");
    let lead = paragraph["children"]
        .as_array()
        .expect("children")
        .iter()
        .take_while(|node| node["type"] == "non_breaking_space")
        .map(offsets)
        .collect();
    (offsets(paragraph), lead)
}

#[test]
fn a_verse_lead_places_its_authored_spaces_and_omits_columns_cut_from_a_tab() {
    // CARVE-P12-004: a column a tab expanded has no exact source range; an
    // authored space beside it does.
    let item = [
        "children", "0", "items", "0", "children", "1", "children", "0",
    ];
    let nested = [
        "children", "0", "items", "0", "children", "1", "items", "0", "children", "1", "children",
        "0",
    ];
    let quoted_nested = [
        "children", "0", "children", "0", "items", "0", "children", "1", "items", "0", "children",
        "1", "children", "0",
    ];
    let description = [
        "children", "0", "items", "1", "children", "1", "items", "0", "children", "1", "children",
        "0",
    ];
    let top = ["children", "0", "children", "0"];
    let cases: [Case; 6] = [
        (
            "> - a\n>   - a\n>   \t::: |\n>     \t z\n>   \t:::\n",
            &quoted_nested,
            (31, 34),
            vec![None, None, Some((32, 33))],
        ),
        (
            "- a\n  - a\n  \t::: |\n    \t z\n  \t:::\n",
            &nested,
            (23, 26),
            vec![None, None, Some((24, 25))],
        ),
        (
            ":: t\n: a\n  - a\n    \t::: |\n      \t z\n    \t:::\n",
            &description,
            (33, 35),
            vec![Some((33, 34))],
        ),
        (
            "- a\n  ::: |\n  \tx\n  :::\n",
            &item,
            (14, 16),
            vec![None; 4],
        ),
        (
            "::: |\n\t x\n:::\n",
            &top,
            (6, 9),
            vec![None, None, None, None, Some((7, 8))],
        ),
        ("::: |\n \tx\n:::\n", &top, (6, 9), vec![None; 4]),
    ];
    for (source, path, paragraph, lead) in cases {
        assert_eq!(
            verse_lead(&ast(source), path),
            (Some(paragraph), lead),
            "{source:?}"
        );
    }
}

#[test]
fn a_block_comment_in_a_marker_line_note_ends_after_its_closer() {
    // Corpus 951-2: the comment belongs to the note and ends after `%%%`.
    for (source, span) in [
        ("- [^n]: t\n  %%%\n  %%%\n\n    ~~~\n    e\n]\n", (12, 21)),
        ("- [^n]: t\n  %%%\n  z\n  %%%\n    e\n", (12, 25)),
    ] {
        let doc = ast(source);
        let footnote = doc["children"]
            .as_array()
            .expect("children")
            .iter()
            .find(|node| node["type"] == "footnote")
            .expect("the note");
        let comment = &footnote["children"][1];
        assert_eq!(comment["type"], "comment", "{source:?}");
        assert_eq!(offsets(comment), Some(span), "{source:?}");
    }
}

#[test]
fn a_block_after_a_blank_under_a_quoted_note_plus_keeps_its_columns() {
    // Corpus 989-2 and the same shape without the losing duplicate.
    for (source, note) in [
        ("> [^m]: +\n> [^m]: +\n>\n>   - item\n\nsee[^m]\n", (2, 32)),
        ("> [^m]: +\n>\n>   - item\n\nsee[^m]\n", (2, 22)),
    ] {
        let doc = ast(source);
        let footnote = at(&doc, &["children", "2"]);
        assert_eq!(footnote["type"], "footnote", "{source:?}");
        assert_eq!(offsets(footnote), Some(note), "{source:?}");
        let list = &footnote["children"][0];
        let paragraph = &list["items"][0]["children"][0];
        assert_eq!(offsets(list).map(|s| s.1), Some(note.1), "{source:?}");
        assert_eq!(offsets(paragraph).map(|s| s.1), Some(note.1), "{source:?}");
        assert_eq!(
            offsets(&paragraph["children"][0]),
            Some((note.1 - 4, note.1)),
            "{source:?}"
        );
    }
}
