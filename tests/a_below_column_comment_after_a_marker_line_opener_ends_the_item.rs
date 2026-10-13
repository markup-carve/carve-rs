//! A below-column comment after an unclosed marker-line container ends the
//! item when that container holds no open paragraph, exactly as the closed twin
//! does (corpus 556-13). Oracle: `scripts/spec/layout.mjs` at carve `0af1cf1a`
//! and its test `a-closed-block-leaves-no-frame-for-the-line-below`.

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
fn the_comment_leaves_like_its_closed_twin() {
    let comments: [&[&str]; 4] = [
        &[" %% c"],
        &["%% c"],
        &[" %%% c"],
        &[" %%%", " hidden", " %%%"],
    ];
    for depth in 0..3 {
        for opener in ["::: inner", ":::", ":::: w"] {
            let closer = format!("  {}", opener.split(' ').next().unwrap());
            for comment in comments {
                for tail in [" text", "  text"] {
                    let head = format!("- {opener}");
                    let mut open = vec![head.as_str()];
                    open.extend_from_slice(comment);
                    open.push(tail);
                    let mut closed = vec![head.as_str(), closer.as_str()];
                    closed.extend_from_slice(comment);
                    closed.push(tail);
                    let (open, closed) = (quoted(depth, &open), quoted(depth, &closed));
                    assert_eq!(shape(&open), shape(&closed), "{open:?}");
                    assert_eq!(to_html(&open), to_html(&closed), "{open:?}");
                }
            }
        }
    }
}

#[test]
fn the_admonition_in_corpus_556_13_ends_on_its_opener_line() {
    let doc = tree("- ::: inner\n %% c\n text\n");
    let admonition = &doc["children"][0]["items"][0]["children"][0];
    assert_eq!(admonition["type"], "admonition");
    assert_eq!(admonition["pos"]["endLine"], 1);
    assert_eq!(admonition["pos"]["endOffset"], 11);
    assert_eq!(doc["children"][1]["type"], "comment");
}

#[test]
fn an_open_paragraph_still_keeps_the_comment() {
    assert_eq!(
        to_html("- ::: inner\n  a\n %% c\n text\n").replace(['\n', ' '], ""),
        "<ul><li><divclass=\"inner\"><p>a</p><p>text</p></div></li></ul>"
    );
}
