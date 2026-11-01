//! CARVE-P9-083, carve#4496: a note's marker-line item or note is measured from
//! its source column, so a line below that column leaves the child. An own-line
//! `+` reads the child's authored marker column (carve#4623).

fn notes(source: &str) -> String {
    let html = carve::to_html(source);
    let body = html
        .split_once("<ol>")
        .map_or(html.as_str(), |(_, rest)| rest);
    body.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn contains(source: &str, expected: &str) {
    let html = notes(source);
    assert!(html.contains(expected), "{source:?}: {html}");
}

#[test]
fn a_block_below_the_item_column_belongs_to_the_note() {
    for (first, line) in [
        ("- a", "    # H"),
        ("- a", "  # H"),
        ("- a", "       # H"),
        ("1. a", "    # H"),
        ("- [ ] a", "    # H"),
    ] {
        let source = format!("[^f]\n\n[^f]: {first}\n{line}\n");
        let close = if first.starts_with('1') {
            "</ol>"
        } else {
            "</ul>"
        };
        contains(&source, &format!("{close} <h1 id=\"H\">H</h1>"));
    }
}

#[test]
fn a_block_at_the_item_column_stays_in_the_item() {
    contains(
        "[^f]\n\n[^f]: - a\n        # H\n",
        "<li>a <h1 id=\"H\">H</h1> </li>",
    );
}

#[test]
fn a_block_below_a_marker_line_note_belongs_to_the_outer_note() {
    let html = notes("[^f]: [^g]: a\n    # H\n\n[^g] [^f]\n");
    assert!(html.contains("<li id=\"fn1\"> <p>a"), "{html}");
    assert!(
        html.contains("<li id=\"fn2\"> <h1 id=\"H\">H</h1>"),
        "{html}"
    );
}

#[test]
fn prose_below_a_marker_line_note_folds_into_its_paragraph() {
    for line in ["z", "  z", "   z", "    z", "      z"] {
        let source = format!("[^f]: [^g]: a\n{line}\n\n[^g] [^f]\n");
        contains(&source, "<li id=\"fn1\"> <p>a z");
    }
    contains(
        "[^a]: [^b]: [^c]: a\nz\n\n[^c] [^b] [^a]\n",
        "<li id=\"fn1\"> <p>a z",
    );
    contains("[^f]: [^g]: - p\n      q\n\n[^g] [^f]\n", "<li>p q</li>");
}

#[test]
fn a_plus_reads_the_childs_authored_marker_column() {
    contains("[^n]: - a\n  +\n  p\n\n[^n]\n", "<li>a + p</li>");
    contains(
        "[^n]: > a\n  +\n  b\n\n[^n]\n",
        "<blockquote><p>a + b</p></blockquote>",
    );
    contains("[^n]: - a\n    +\n    p\n\n[^n]\n", "<li>a + p</li>");
    contains("[^n]: - a\n      +\n      p\n\n[^n]\n", "<li>a p </li>");
}

#[test]
fn a_fence_on_the_item_keeps_the_body_column() {
    contains("[^n]: - ```\n    x\n    ```\n\n[^n]\n", "<pre><code>x");
}
