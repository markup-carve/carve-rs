use carve::{CodeCallouts, Options};

fn h(source: &str) -> String {
    let cc = CodeCallouts::new();
    let options = Options::new().with_extension(&cc);
    carve::to_html_with_options(source, &options)
        .trim()
        .to_string()
}

fn off(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

const SRC: &str = "```js\nconst x = compute();   <1>\nreturn x * 2;          <2>\n```\n\n<1> Runs the expensive step once.\n<2> Doubles the result.";

#[test]
fn full_golden_matches_carve_js() {
    let expected = "<pre><code class=\"language-js\">const x = compute();   <b class=\"callout\" data-callout=\"1\">1</b>\nreturn x * 2;          <b class=\"callout\" data-callout=\"2\">2</b>\n</code></pre>\n<ol class=\"callouts\">\n  <li value=\"1\">Runs the expensive step once.</li>\n  <li value=\"2\">Doubles the result.</li>\n</ol>";
    assert_eq!(h(SRC), expected);
}

#[test]
fn renders_in_code_marker_bubbles() {
    let out = h(SRC);
    assert!(out.contains("const x = compute();   <b class=\"callout\" data-callout=\"1\">1</b>"));
    assert!(out.contains("return x * 2;          <b class=\"callout\" data-callout=\"2\">2</b>"));
}

#[test]
fn binds_following_list_with_explicit_values() {
    let out = h(SRC);
    assert!(out.contains("<ol class=\"callouts\">"));
    assert!(out.contains("<li value=\"1\">Runs the expensive step once.</li>"));
    assert!(out.contains("<li value=\"2\">Doubles the result.</li>"));
}

#[test]
fn preserves_non_sequential_marker() {
    let out = h("```\nfoo()  <3>\n```\n\n<3> only three.");
    assert!(out.contains("data-callout=\"3\">3</b>"));
    assert!(out.contains("<li value=\"3\">only three.</li>"));
}

#[test]
fn escapes_code_around_marker() {
    let out = h("```\na < b && c;  <1>\n```\n\n<1> note.");
    assert!(out.contains("a &lt; b &amp;&amp; c;  <b class=\"callout\" data-callout=\"1\">1</b>"));
}

#[test]
fn no_marker_does_not_bind() {
    let out = h("```\nplain();\n```\n\n<1> orphan.");
    assert!(!out.contains("class=\"callouts\""));
    assert!(out.contains("&lt;1&gt; orphan."));
}

#[test]
fn non_item_line_does_not_bind_but_marker_renders() {
    let out = h("```\nfoo()  <1>\n```\n\n<1> first.\nnot a callout line.");
    assert!(!out.contains("class=\"callouts\""));
    assert!(out.contains("data-callout=\"1\">1</b>"));
}

#[test]
fn marker_renders_without_a_following_list() {
    let out = h("```\nfoo()  <1>\n```\n\nordinary paragraph.");
    assert!(out.contains("data-callout=\"1\">1</b>"));
    assert!(!out.contains("class=\"callouts\""));
}

#[test]
fn carries_authored_attrs_onto_ol() {
    let out = h("```\nfoo()  <1>\n```\n\n{#notes .wide}\n<1> note.");
    assert!(out.contains("<ol id=\"notes\" class=\"callouts wide\">"));
}

#[test]
fn does_not_crash_on_definition_list() {
    let out = h(":: term\n:  a definition\n\n```\nx  <1>\n```\n\n<1> note.");
    assert!(out.contains("<dl>"));
    assert!(out.contains("data-callout=\"1\">1</b>"));
}

#[test]
fn only_trailing_marker_per_line() {
    let out = h("```\nVec<2> v;  <1>\n```\n\n<1> note.");
    assert!(out.contains("Vec&lt;2&gt; v;  <b class=\"callout\" data-callout=\"1\">1</b>"));
}

#[test]
fn off_leaves_markers_literal() {
    let out = off(SRC);
    assert!(out.contains("&lt;1&gt;"));
    assert!(!out.contains("class=\"callout"));
    assert!(out.contains("<p>&lt;1&gt; Runs the expensive step once."));
}
