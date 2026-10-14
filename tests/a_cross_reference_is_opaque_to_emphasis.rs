//! PART 8 ranks cross-references with autolinks, above emphasis: the `/` of
//! `</#id>` is never a closer for a bare marker opened before it.

fn html(src: &str) -> String {
    carve::to_html(src).trim_end().to_string()
}

#[test]
fn an_unclosed_slash_stays_literal_before_a_resolved_cross_reference() {
    assert!(html("{#intro}\n# Intro\n\nx /a </#intro> y\n")
        .contains("<p>x /a <a href=\"#intro\">Intro</a> y</p>"));
}

#[test]
fn an_unclosed_slash_stays_literal_before_an_unresolved_cross_reference() {
    assert_eq!(
        html("x /a </#nothing> y\n"),
        "<p>x /a &lt;/#nothing&gt; y</p>"
    );
}

#[test]
fn an_emphasis_pairs_around_a_cross_reference() {
    assert_eq!(
        html("/a </#nothing> b/\n"),
        "<p><em>a &lt;/#nothing&gt; b</em></p>"
    );
}

#[test]
fn a_marker_inside_the_target_does_not_close() {
    assert_eq!(
        html("*a </#x*> b*\n"),
        "<p><strong>a &lt;/#x*&gt; b</strong></p>"
    );
}

#[test]
fn the_autolink_control_is_unchanged() {
    assert_eq!(
        html("x /a <https://x.y/b/> y\n"),
        "<p>x /a <a href=\"https://x.y/b/\">https://x.y/b/</a> y</p>"
    );
}
