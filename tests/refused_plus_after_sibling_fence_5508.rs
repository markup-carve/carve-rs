//! An attached fence keeps payload at its content column, and a later list
//! marker receives its own refused continuation marker (carve#5507, #5508).

fn compact(source: &str) -> String {
    carve::to_html(source)
        .split('\n')
        .map(str::trim)
        .collect::<String>()
}

#[test]
fn a_refused_plus_after_a_new_list_kind_does_not_become_text() {
    for (first, second, list) in [("1. +", "- z", "ul"), ("- +", "1. z", "ol")] {
        let source = format!("{first}\n```\n{second}\n+\n\ny\n");
        assert_eq!(
            compact(&source),
            format!(
                "<{}><li><pre><code></code></pre></li></{}><{list}><li>z</li></{list}><p>y</p>",
                if first == "1. +" { "ol" } else { "ul" },
                if first == "1. +" { "ol" } else { "ul" }
            ),
            "{source}"
        );
    }
}

#[test]
fn a_bare_quote_marker_does_not_end_the_fence_like_a_blank() {
    let source = "1. +\n```\n- z\n+\n>\n";
    assert_eq!(
        compact(source),
        "<ol><li><pre><code>- z+&gt;</code></pre></li></ol>"
    );
}
