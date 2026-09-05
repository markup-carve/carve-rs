//! A comment's span begins at its `%` markup, not in the leading indentation
//! (markup-carve/carve#1928; the oracle enforces it via `INDENT_LATITUDE` in
//! `scripts/spec/ast-positions.mjs`, markup-carve/carve#1940).
//!
//! A LEAF span begins at its markup; only a CONTAINER keeps the latitude to
//! begin part way into the leading indentation. A `comment` is a leaf, so it
//! begins at the `%`. This engine used to start it at the enclosing body's
//! content column - one to three codepoints early - which is the latitude the
//! ruling withdrew from leaves.
//!
//! ORACLE: the executable spec at spec main `a9a2fa79`; `OPENING_MARKUP.comment`
//! is `/^%/` and `comment` is absent from `INDENT_LATITUDE`.

use carve::{to_html, to_html_with_options, Options};

/// The comment node's (startColumn, startOffset).
fn comment_start(src: &str) -> (usize, usize) {
    // #908: the facade and the position path agree (comments render nothing, so
    // the HTML is trivially equal, but the guard is kept uniform).
    assert_eq!(
        to_html(src),
        to_html_with_options(src, &Options::default().with_positions(true))
    );
    let json = carve::to_json_with_options(src, &Options::default().with_positions(true));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    fn find(n: &serde_json::Value) -> Option<(usize, usize)> {
        if n.get("type").and_then(|t| t.as_str()) == Some("comment") {
            let p = n.get("pos")?;
            return Some((
                p.get("startColumn")?.as_u64()? as usize,
                p.get("startOffset")?.as_u64()? as usize,
            ));
        }
        for key in ["children", "definitions", "items", "terms"] {
            if let Some(arr) = n.get(key).and_then(|c| c.as_array()) {
                for c in arr {
                    if let Some(hit) = find(c) {
                        return Some(hit);
                    }
                }
            }
        }
        None
    }
    find(&v).expect("a comment node with a position")
}

#[test]
fn under_a_description_body_it_starts_at_the_percent() {
    // `%` is column 5, offset 26 - not column 4 (the body's content column).
    assert_eq!(
        comment_start(":: term\n:  definition\n    %% c\ntail\n"),
        (5, 26)
    );
}

#[test]
fn under_a_nested_item_it_starts_at_the_percent() {
    assert_eq!(comment_start("- - x\n    %% c\n"), (5, 10));
}

#[test]
fn inside_a_quote_it_starts_at_the_percent() {
    assert_eq!(comment_start("> x\n> %% c\n"), (3, 6));
}

#[test]
fn a_top_level_indented_comment_starts_at_the_percent() {
    assert_eq!(comment_start("  %% c\n"), (3, 2));
}

#[test]
fn a_comment_fence_starts_at_its_percent_run() {
    assert_eq!(
        comment_start(":: t\n:  d\n    %%% c\n    x\n    %%%\n"),
        (5, 14)
    );
}
