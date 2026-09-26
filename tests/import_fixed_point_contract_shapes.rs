//! Three import shapes whose written source was not a `fmt` fixed point
//! (markup-carve/carve#2383, #2384, #2385, #2396).

use carve::{html_to_carve, to_carve, to_html, HtmlImportOptions};

fn migrated(html: &str) -> (String, Vec<(String, String)>) {
    let result = html_to_carve(html, &HtmlImportOptions::default()).expect("import");
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.as_str().to_string(),
                d.path.clone().unwrap_or_default(),
            )
        })
        .collect();
    (result.value, rows)
}

fn fixed(source: &str) {
    assert_eq!(to_carve(source), source, "fmt fixed point");
}

#[test]
fn an_attribute_value_holding_a_pipe_keeps_its_cell() {
    let (out, rows) = migrated(
        "<table><tr><td id=\"c\" data-x=\"a|b\">t</td><td><span data-y=\"p|q\">u</span></td></tr></table>",
    );
    assert_eq!(out, "|{#c data-x=\"a\\|b\"} t | [u]{data-y=\"p\\|q\"} |\n");
    assert!(rows.is_empty());
    fixed(&out);
    assert!(to_html(&out).contains("<td id=\"c\" data-x=\"a|b\">t</td>"));
}

#[test]
fn a_description_before_the_first_term_is_written_as_blocks() {
    let (out, rows) =
        migrated("<dl class=\"k\"><dd><div id=\"p\" class=\"noprint\"><i>x</i></div></dd></dl>");
    assert_eq!(out, "{#p}\n::: noprint\n/x/\n:::\n");
    assert_eq!(
        rows,
        [
            ("attribute-dropped".to_string(), "/dl[1]".to_string()),
            ("element-unwrapped".to_string(), "/dl[1]/dd[1]".to_string()),
        ]
    );
    fixed(&out);
}

#[test]
fn an_attribute_value_with_a_line_break_is_dropped() {
    let (out, rows) =
        migrated("<div class=\"h\" data-copy=\"a\nb\"><pre><code>x</code></pre></div>");
    assert_eq!(out, "::: h\n```\nx\n```\n:::\n");
    assert_eq!(
        rows,
        [("attribute-dropped".to_string(), "/div[1]".to_string())]
    );
    fixed(&out);
}

#[test]
fn a_comment_with_a_line_break_in_a_heading_is_dropped() {
    let (out, rows) = migrated("<h2>a <!-- x\ny --> b</h2>");
    assert_eq!(out, "## a  b\n");
    assert_eq!(
        rows,
        [(
            "element-dropped".to_string(),
            "/h2[1]/comment()[2]".to_string()
        )]
    );
    fixed(&out);
}
