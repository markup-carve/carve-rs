//! An attached fence past a blank line reaches its closer at the host's body
//! column (carve#4392). An attached div ends at a sibling of its description
//! (carve#4798). A fence on a note's marker-line item reads its payload at the
//! item's content column (carve#4755), and a definition-shaped payload keeps a
//! line block's stanza open under nested items (carve#4757).

use carve::to_html;

fn squeeze(html: &str) -> String {
    html.split('\n').map(str::trim).collect::<Vec<_>>().join("")
}

#[test]
fn an_attached_fence_past_a_blank_closes_at_the_note_body_column() {
    for host in ["[^f]: +\n", "[^f]: t\n+\n"] {
        let html = to_html(&format!("{host}```\na\n\n  ```\n  c\nsee\n\nx[^f]\n"));
        assert!(html.contains("<p>c\nsee"), "{host:?}: {html}");
        assert!(html.contains("<code>a\n\n</code>"), "{host:?}: {html}");
    }
}

#[test]
fn an_attached_fence_past_a_blank_closes_at_the_description_body_column() {
    let html = to_html(":: t\n: +\n```\na\n\n  ```\n  c\nsee\n");
    assert!(
        html.contains("<p>c\nsee</p>") || html.contains("c\nsee"),
        "{html}"
    );
}

#[test]
fn an_indented_closer_before_a_blank_stays_payload() {
    let html = to_html("[^f]: +\n```\na\n  ```\n  c\nsee\n\nx[^f]\n");
    assert!(html.contains("a\n  ```\n"), "{html}");
}

#[test]
fn an_attached_div_ends_at_a_description_sibling() {
    for (source, expected) in [
        (
            ":: t\n: +\n:::\n: b\n:::\n",
            "<dl><dt>t</dt><dd><div></div></dd><dd>b</dd></dl><div></div>",
        ),
        (
            ":: t\n: a\n+\n:::\n: b\n:::\n",
            "<dl><dt>t</dt><dd><p>a</p><div></div></dd><dd>b</dd></dl><div></div>",
        ),
        (
            ":: t\n: +\n:::\n:: u\n: v\n",
            "<dl><dt>t</dt><dd><div></div></dd><dt>u</dt><dd>v</dd></dl>",
        ),
        (
            ":: t\n: +\n:::\n- x\n:::\n",
            "<dl><dt>t</dt><dd><div><ul><li>x</li></ul></div></dd></dl>",
        ),
    ] {
        assert_eq!(squeeze(&to_html(source)), expected, "{source:?}");
    }
}

#[test]
fn a_fence_on_a_note_marker_line_item_reads_at_the_item_column() {
    let html = to_html("x[^n]\n\n[^n]: - ```js\n        a\n        ```\n");
    assert!(
        html.contains("<code class=\"language-js\">a\n</code>"),
        "{html}"
    );
    let missed = squeeze(&to_html("x[^n]\n\n[^n]: - ```js\n    a\n    ```\n"));
    assert!(
        missed.contains("<code class=\"language-js\"></code></pre></li></ul><p>a</p>"),
        "{missed}"
    );
}

#[test]
fn a_definition_shaped_payload_keeps_a_nested_stanza_open() {
    for payload in ["[x]: /u", "[^1]: n", "{x}"] {
        let html = to_html(&format!("- - :: u\n    : ::: |\n      {payload}\nb\n"));
        assert!(html.contains("<br>\nb</p>"), "{payload}: {html}");
    }
}
