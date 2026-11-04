//! A `%%` line below the column of an open `%%%` span is that span's payload,
//! not a comment line that layout erases (PART 0 S1, carve#4994, carve-rs#3826).
//! Expected HTML is the spec model's at carve main.

fn html(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn the_comment_line_keeps_the_list_and_the_quote_as_the_model_reads_them() {
    for (source, expected) in [
        (
            "x[^n]\n\n- a\n  [^n]: %%%\n\n%% c\n  more\n- c\n",
            "<ul><li>amore</li><li>c</li></ul>",
        ),
        // The note-column line after the comment is visible to §17 L1c.
        (
            "x[^n]\n\n- a\n  [^n]: %%%\n\n%% c\n    more\n- c\n",
            "<ul><li>a</li><li>c</li></ul>",
        ),
        (
            "- a\n  %%%\n\n%% c\n  more\n- c\n",
            "<ul><li>a</li><li>c</li></ul>",
        ),
        (
            "> - a\n>   %%%\n> %% c\n>   more\n- c\n",
            "<blockquote><ul><li>a</li></ul></blockquote><ul><li>c</li></ul>",
        ),
        // The line that ends a descendant's span leaves no paragraph to continue.
        (
            "- a\n  - b\n    %%%\n%% c\nmore\n- c\n",
            "<ul><li>a<ul><li>b</li></ul></li></ul><p>more- c</p>",
        ),
        (
            "- a\n  - b\n    %%%\n%%%% c\nmore\n- c\n",
            "<ul><li>a<ul><li>b</li></ul></li></ul><p>more- c</p>",
        ),
    ] {
        let out = html(source);
        assert!(out.contains(expected), "{source:?} -> {out}");
    }
}

#[test]
fn the_comment_line_is_traced_as_payload() {
    let trace = carve::decision_trace("- a\n  %%%\n\n%% c\n  more\n- c\n");
    let line = trace
        .lines()
        .find(|line| line.contains("\"line\":4,"))
        .unwrap();
    assert!(line.contains("\"form\":\"payload\""), "{line}");
}
