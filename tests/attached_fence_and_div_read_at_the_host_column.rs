//! A fence attached at the marker column does not close at the host's body
//! column (carve#5410). An attached div ends at a sibling of its description
//! (carve#4798). A fence on a note's marker-line item reads its payload at the
//! item's content column (carve#4755), and a definition-shaped payload keeps a
//! line block's stanza open under nested items (carve#4757).

use carve::to_html;

fn squeeze(html: &str) -> String {
    html.split('\n').map(str::trim).collect::<Vec<_>>().join("")
}

#[test]
fn an_indented_fence_after_a_blank_opens_in_the_note_body() {
    let html = to_html("[^f]: +\n```\na\n\n  ```\n  c\nsee\n\nx[^f]\n");
    assert!(html.contains("<code>a\n</code>"), "{html}");
    assert!(html.contains("<code>c\n</code>"), "{html}");
}

#[test]
fn a_later_note_plus_keeps_its_body_column_closer() {
    let html = to_html("[^f]: t\n+\n```\na\n\n  ```\n  c\nsee\n\nx[^f]\n");
    assert!(html.contains("<code>a\n\n</code>"), "{html}");
    assert!(html.contains("<p>c\nsee"), "{html}");
}

#[test]
fn an_indented_fence_after_a_blank_opens_in_the_description_body() {
    let html = to_html(":: t\n: +\n```\na\n\n  ```\n  c\nsee\n");
    assert!(html.contains("<code>a\n</code>"), "{html}");
    assert!(html.contains("<code>c\n</code>"), "{html}");
}

#[test]
fn an_attached_fence_closes_only_at_its_opener_column() {
    for (source, first, second) in [
        ("- +\n```\na\n\n  ```\n  c\n", "<li>", "<code>c\n</code>"),
        (
            ":: t\n: +\n```\na\n\n  ```\n  c\n",
            "<dd>",
            "<code>c\n</code>",
        ),
        (
            "[^1]: +\n```\na\n\n  ```\n  c\n\n[^1]\n",
            "<li id=\"fn1\">",
            "<code>c\n\n</code>",
        ),
    ] {
        let html = to_html(source);
        assert!(html.contains(first), "{source:?}: {html}");
        assert!(html.contains("<code>a\n</code>"), "{source:?}: {html}");
        assert!(html.contains(second), "{source:?}: {html}");
    }
    for source in [
        "- +\n```\na\n\n```\n",
        ":: t\n: +\n```\na\n\n```\n",
        "[^1]: +\n```\na\n\n```\n\n[^1]\n",
    ] {
        assert!(to_html(source).contains("<code>a\n\n</code>"), "{source:?}");
    }
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
