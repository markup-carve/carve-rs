#[test]
fn repeated_colon_runs_past_a_line_blocks_column_remain_payload() {
    let source = "::: |\na\n :::\n :::\n";
    let expected = "<div class=\"line-block\">\n  <p>a<br>\n&nbsp;:::<br>\n&nbsp;:::</p>\n</div>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn a_colon_run_at_the_openers_column_still_closes_the_block() {
    let source = "::: |\na\n :::\n:::\ntail\n";
    let expected = "<div class=\"line-block\">\n  <p>a<br>\n&nbsp;:::</p>\n</div>\n<p>tail</p>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn repeated_nested_colon_runs_past_the_openers_column_remain_payload() {
    let source = "- ::: |\n  a\n   :::\n   :::\n";
    let expected = "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>a<br>\n&nbsp;:::<br>\n&nbsp;:::</p>\n    </div>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn quote_shaped_payload_after_a_lazy_stanza_line_stays_in_the_line_block() {
    let source = "- intro\n > ```\n- ::: |\n  > code\nafter\n  > ```\n";
    let expected = "<ul>\n  <li>intro</li>\n</ul>\n<blockquote>\n  <pre><code></code></pre>\n</blockquote>\n<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>&gt; code<br>\nafter<br>\n&gt; <code></code></p>\n    </div>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn an_indented_blank_after_a_lazy_line_does_not_claim_the_next_item() {
    let source = "- ::: |\n  a\nlazy\n  \n- b\n";
    let expected = "<ul>\n  <li>\n    <div class=\"line-block\">\n      <p>a<br>\nlazy</p>\n    </div>\n  </li>\n  <li><p>b</p></li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
