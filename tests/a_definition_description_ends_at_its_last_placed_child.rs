//! A definition_description ends at its last placed child, not over a footnote
//! definition hoisted out of it (markup-carve/carve#1943; the hoisted-sibling
//! rule is markup-carve/carve#1522).
//!
//! PART 12 SS4: a container with no closer ends at its last placed child. A
//! footnote definition authored at the description body's content column is
//! hoisted out by PART 9 SS7, and a hoisted sibling is not a child - so the
//! description must not reach over it. The enclosing definition_list derives its
//! own end from the description, so both narrow together.
//!
//! ORACLE: the executable spec at spec main `a9a2fa79`; carve-js and carve-php
//! both stop the description at the last placed child.

use carve::{to_html, to_html_with_options, Options};

fn spans(src: &str, ty: &str) -> Vec<(u64, u64)> {
    assert_eq!(
        to_html(src),
        to_html_with_options(src, &Options::default().with_positions(true))
    );
    let json = carve::to_json_with_options(src, &Options::default().with_positions(true));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut out = Vec::new();
    fn walk(n: &serde_json::Value, ty: &str, out: &mut Vec<(u64, u64)>) {
        if n.get("type").and_then(|t| t.as_str()) == Some(ty) {
            if let Some(p) = n.get("pos") {
                if let (Some(s), Some(e)) = (
                    p.get("startOffset").and_then(|x| x.as_u64()),
                    p.get("endOffset").and_then(|x| x.as_u64()),
                ) {
                    out.push((s, e));
                }
            }
        }
        for key in ["children", "definitions", "items", "terms"] {
            if let Some(arr) = n.get(key).and_then(|c| c.as_array()) {
                for c in arr {
                    walk(c, ty, out);
                }
            }
        }
    }
    walk(&v, ty, &mut out);
    out
}

/// The reported document: line 3 authors a footnote definition at the body's
/// content column; it is hoisted, so the bullet list is the last placed child.
#[test]
fn it_stops_at_the_list_over_a_hoisted_footnote_definition() {
    let src = ":: t\n:  - a\n    [^n]: note text\n";
    assert_eq!(spans(src, "definition_description"), vec![(5, 11)]);
    assert_eq!(spans(src, "definition_list"), vec![(0, 11)]);
}

/// A description with no hoisting is unchanged: it ends at its content.
#[test]
fn a_plain_description_is_unchanged() {
    let src = ":: t\n:  plain body\n";
    assert_eq!(spans(src, "definition_description"), vec![(5, 18)]);
}
