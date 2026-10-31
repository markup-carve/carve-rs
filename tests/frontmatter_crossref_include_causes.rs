//! Cause tickets markup-carve/carve#4453 (indented frontmatter delimiters),
//! #4461 (empty caption label), #4450 (duplicate explicit heading id) and
//! #4463 (malformed or nested unresolved code include).

use carve::to_html;

#[track_caller]
fn html(source: &str) -> String {
    to_html(source).trim().to_string()
}

#[test]
fn an_indented_delimiter_neither_opens_nor_closes_frontmatter() {
    let expected = "<hr>\n<p>a</p>\n<hr>\n<p>x</p>";
    for source in [" ---\na\n---\nx\n", "---\na\n ---\nx\n", "   ---\na\n   ---\nx\n"] {
        assert_eq!(html(source), expected, "{source:?}");
    }
    assert_eq!(html("---\na: 1\n---\nx\n"), "<p>x</p>");
    // An indented line inside the block is content, not its closer.
    assert_eq!(html("---\na\n ---\nb: 1\n---\nx\n"), "<p>x</p>");
}

#[test]
fn a_reference_to_an_empty_caption_label_is_the_number_alone() {
    assert_eq!(
        html("{#t}\n| a |\n^ # x\n\n</#t>\n"),
        "<table id=\"t\">\n  <caption>1 x</caption>\n  <tbody>\n    <tr><td>a</td></tr>\n  </tbody>\n</table>\n<p><a href=\"#t\">1</a></p>"
    );
    assert!(html("{#t}\n| a |\n^ Table # x\n\n</#t>\n").ends_with("<a href=\"#t\">Table 1</a></p>"));
}

#[test]
fn a_duplicate_explicit_heading_id_resolves_to_the_first() {
    assert_eq!(
        html("{#x}\n# A\n\n{#x}\n# B\n\n</#x>\n"),
        "<section id=\"x\">\n  <h1>A</h1>\n</section>\n<section id=\"x\">\n  <h1>B</h1>\n  <p><a href=\"#x\">A</a></p>\n</section>"
    );
}

#[test]
fn a_malformed_code_language_is_literal_inside_a_paragraph() {
    assert_eq!(html("x {{ a @code:a,b }} y\n"), "<p>x {{ a @code:a,b }} y</p>");
    assert_eq!(
        html("> x {{ _a_ @code:a,b }} y\n"),
        "<blockquote><p>x {{ _a_ @code:a,b }} y</p></blockquote>"
    );
    assert_eq!(
        html("x {{ _a_ @code:c++ }} y\n"),
        "<p>x {{ <u>a</u> <span class=\"mention\"><strong>@code</strong></span>:c++ }} y</p>"
    );
}

#[test]
fn an_unresolved_standalone_code_include_is_literal_in_every_host() {
    assert_eq!(
        html("> {{ _a_ @code:js }}\n"),
        "<blockquote><p>{{ _a_ @code:js }}</p></blockquote>"
    );
    assert_eq!(
        html("- {{ _a_ @code:js }}\n"),
        "<ul>\n  <li>{{ _a_ @code:js }}</li>\n</ul>"
    );
    assert_eq!(
        html(":: t\n: {{ _a_ @code:js }}\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>{{ _a_ @code:js }}</dd>\n</dl>"
    );
}
