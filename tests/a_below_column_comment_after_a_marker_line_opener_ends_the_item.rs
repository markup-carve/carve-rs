//! A below-column comment after an unclosed marker-line container closes
//! nothing: the document reads as it does with the comment removed
//! (carve#4862, CARVE-P9-013). Oracle: `scripts/spec/layout.mjs` at carve `4577df8f`.

use carve::{to_html, to_html_with_options, Options};

fn tree(src: &str) -> serde_json::Value {
    assert_eq!(
        to_html(src),
        to_html_with_options(src, &Options::default().with_positions(true))
    );
    let json = carve::to_json_with_options(src, &Options::default().with_positions(true));
    serde_json::from_str(&json).unwrap()
}

/// Every node's type and position, in document order.
fn shape(src: &str) -> Vec<String> {
    fn walk(n: &serde_json::Value, depth: usize, out: &mut Vec<String>) {
        if let Some(kind) = n.get("type").and_then(|t| t.as_str()) {
            out.push(format!("{depth}:{kind}"));
        }
        for key in ["children", "items"] {
            if let Some(arr) = n.get(key).and_then(|c| c.as_array()) {
                for c in arr {
                    walk(c, depth + 1, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&tree(src), 0, &mut out);
    out
}

fn quoted(depth: usize, lines: &[&str]) -> String {
    lines
        .iter()
        .map(|line| {
            format!("{}{line}", "> ".repeat(depth))
                .trim_end()
                .to_string()
                + "\n"
        })
        .collect()
}

#[test]
fn the_comment_reads_as_if_absent() {
    let comments: [&[&str]; 3] = [&[" %% c"], &["%% c"], &[" %%%", " hidden", " %%%"]];
    for depth in 0..3 {
        for opener in ["::: inner", ":::", ":::: w"] {
            for comment in comments {
                for tail in [" text", "  text"] {
                    let head = format!("- {opener}");
                    let mut open = vec![head.as_str()];
                    open.extend_from_slice(comment);
                    open.push(tail);
                    let (open, absent) = (quoted(depth, &open), quoted(depth, &[&head, tail]));
                    let mut kept = shape(&open);
                    kept.retain(|node| !node.ends_with(":comment"));
                    assert_eq!(kept, shape(&absent), "{open:?}");
                    assert_eq!(to_html(&open), to_html(&absent), "{open:?}");
                }
            }
        }
    }
}
