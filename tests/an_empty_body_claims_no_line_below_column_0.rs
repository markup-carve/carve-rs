//! §17 L3 (`AND FLUSH-LEFT MEANS COLUMN 0`) gives the continuation marker its
//! own control: a refused `+` behaves "exactly as if the `+` line had been a
//! comment". In the FIRST-BLOCK form - `:  +` for a description, `- +` for a
//! list item - no paragraph is open, so the `+` genuinely IS a marker and the
//! clause reads its payload's column. A payload at any column other than 0 is
//! refused, and the body ends there exactly as it ends at a comment.
//!
//! WHY A RELATION AND NOT A PAIR OF GOLDENS (markup-carve/carve#1821). The
//! clause states that two SPELLINGS give one answer. Two independent goldens
//! cannot express that: a change repairing one spelling and drifting the other
//! passes both. So every row below asserts the marker spelling EQUALS its
//! comment control, and the column-0 rows assert the one pair that must NOT
//! agree - without which a form that refused everything would satisfy the rest.
//!
//! The LIST ITEM rows are the reference the oracle already answered, and this
//! engine did not: `- +` and `- %% c` over a column-1 line each rendered
//! `<li>flush</li>`. Note the two agreed with EACH OTHER while both diverged
//! from the oracle, so the relation alone would have passed - which is why the
//! oracle's answer is pinned beside it.

fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

/// The two spellings of "this body takes nothing here", per container.
fn marker_and_comment(container: &str, payload_col: usize) -> (String, String) {
    let pad = " ".repeat(payload_col);
    match container {
        "description" => (
            format!(":: t\n:  +\n{pad}flush\n"),
            format!(":: t\n:  %% c\n{pad}flush\n"),
        ),
        "item" => (
            format!("- +\n{pad}flush\n"),
            format!("- %% c\n{pad}flush\n"),
        ),
        other => panic!("unknown container {other}"),
    }
}

#[test]
fn a_refused_marker_ends_the_body_exactly_where_its_comment_does() {
    // Columns 1 and 2 are the refused band for the description (content column
    // 3) and column 1 for the item (content column 2). At the container's OWN
    // content column the payload is its first block in both spellings, which is
    // still an agreement - so the relation holds across the whole band.
    for container in ["description", "item"] {
        for payload_col in [1, 2, 3, 4] {
            let (marker, comment) = marker_and_comment(container, payload_col);
            assert_eq!(
                html(&marker),
                html(&comment),
                "{container} at payload column {payload_col}: the marker spelling must end the \
                 body exactly where its comment control does"
            );
        }
    }
}

#[test]
fn the_refused_band_leaves_the_payload_outside_the_container() {
    // The relation above is satisfied by any answer both spellings share, so it
    // cannot say WHICH answer. These rows pin the oracle's.
    for (container, payload_col, expected) in [
        (
            "description",
            1,
            "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>flush</p>",
        ),
        (
            "description",
            2,
            "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>flush</p>",
        ),
        ("item", 1, "<ul>\n  <li></li>\n</ul>\n<p>flush</p>"),
    ] {
        let (marker, comment) = marker_and_comment(container, payload_col);
        assert_eq!(
            html(&marker),
            expected,
            "{container} marker, column {payload_col}"
        );
        assert_eq!(
            html(&comment),
            expected,
            "{container} comment, column {payload_col}"
        );
    }
}

#[test]
fn at_the_content_column_the_payload_is_the_container_s_first_block() {
    // The other end of the band: a line AT the container's content column is
    // its own first block in both spellings. A change that refused everything
    // would pass the rows above and break these.
    let (marker, comment) = marker_and_comment("description", 3);
    let expected = "<dl>\n  <dt>t</dt>\n  <dd>flush</dd>\n</dl>";
    assert_eq!(html(&marker), expected);
    assert_eq!(html(&comment), expected);

    let (marker, comment) = marker_and_comment("item", 2);
    let expected = "<ul>\n  <li>flush</li>\n</ul>";
    assert_eq!(html(&marker), expected);
    assert_eq!(html(&comment), expected);
}

#[test]
fn at_column_0_the_marker_attaches_and_the_comment_does_not() {
    // THE ONE PAIR THAT MUST NOT AGREE. At column 0 the marker is not refused,
    // so the first-block form keeps the one flush-left block it names, while
    // the comment spelling ends the body as any comment does. This is what
    // makes the relation above a statement about the refused band rather than
    // about the construct.
    for (container, attached, refused) in [
        (
            "description",
            "<dl>\n  <dt>t</dt>\n  <dd>flush</dd>\n</dl>",
            "<dl>\n  <dt>t</dt>\n  <dd></dd>\n</dl>\n<p>flush</p>",
        ),
        (
            "item",
            "<ul>\n  <li>flush</li>\n</ul>",
            "<ul>\n  <li></li>\n</ul>\n<p>flush</p>",
        ),
    ] {
        let (marker, comment) = marker_and_comment(container, 0);
        assert_eq!(html(&marker), attached, "{container}: column 0 attaches");
        assert_eq!(
            html(&comment),
            refused,
            "{container}: a comment attaches nothing"
        );
        assert_ne!(
            html(&marker),
            html(&comment),
            "{container}: column 0 is where the two spellings must DIFFER"
        );
    }
}

#[test]
fn a_marker_under_an_open_paragraph_is_lazy_continuation_text() {
    // NOT IN SCOPE, pinned so the port cannot quietly take it. A marker cannot
    // interrupt a paragraph, so under an open body the `+` is ordinary text and
    // stays literal. All four containers agree, and this must not change.
    assert_eq!(
        html(":: t\n:  d\n   +\nflush\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d\n+\nflush</dd>\n</dl>"
    );
}

#[test]
fn the_item_holds_only_invisible_blocks_when_its_marker_line_is_a_comment() {
    // MEASURED ON THE TREE, NOT THE HTML. A comment renders to nothing either
    // way, so `<li></li>` cannot show whether the parser built an empty item or
    // an item holding a comment - and the first attempt at this fix asked
    // `children.is_empty()`, which the comment node answered "not empty" while
    // the rendered output looked identical. Assert the shape the guard reads.
    let json = carve::to_json_with_options("- %% c\n flush\n", &carve::Options::default());
    assert!(
        json.contains("\"type\":\"comment\""),
        "the item keeps its comment node: {json}"
    );
    assert!(
        !json.contains("flush\"") || json.contains("\"type\":\"paragraph\""),
        "the payload is a paragraph outside the item: {json}"
    );
}
