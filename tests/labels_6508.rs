//! C2-B-234 (carve#6508): a caption and a term continue only on lines indented
//! past their marker, like a heading.
use carve::lint::{LintOptions, lint};
use carve::options::Options;

fn html(input: &str) -> String {
    carve::render(input, &Options::default())
}

fn codes(input: &str) -> Vec<String> {
    lint(input, &LintOptions::default())
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn an_unindented_line_below_a_caption_starts_the_next_block() {
    for (next, block) in [
        ("Next prose.", "<p>Next prose.</p>"),
        ("# H", "<h1>H</h1>\n</section>"),
        ("> q", "<blockquote><p>q</p></blockquote>"),
        ("- i", "<ul>\n  <li>i</li>\n</ul>"),
    ] {
        let out = html(&format!("![a](a.png)\n^ The caption\n{next}\n"));
        assert!(
            out.contains("<figcaption>The caption</figcaption>"),
            "{next}: {out}"
        );
        assert!(out.ends_with(block), "{next}: {out}");
    }
}

#[test]
fn an_indented_line_continues_a_caption() {
    let out = html("| a |\n^ one\n  two\n   three\n");
    assert!(out.contains("<caption>one\ntwo\nthree</caption>"), "{out}");
    let out = html("> | a |\n> ^ c\n>   d\n");
    assert!(out.contains("<caption>c\nd</caption>"), "{out}");
}

#[test]
fn a_comment_line_does_not_end_a_caption() {
    let out = html("| a |\n^ one\n%% note\n  two\n");
    assert!(out.contains("<caption>one\n"), "{out}");
    assert!(out.contains("two</caption>"), "{out}");
}

#[test]
fn a_caret_line_directly_below_a_caption_is_text() {
    let input = "| a |\n^ one\n^ two\n";
    let out = html(input);
    assert!(out.contains("<caption>one</caption>"), "{out}");
    assert!(out.ends_with("<p>^ two</p>"), "{out}");
    assert!(codes(input).iter().any(|c| c == "unattached-caption"));
}

#[test]
fn an_unindented_line_below_a_term_closes_the_list() {
    for (next, block) in [
        ("More prose.", "<p>More prose.</p>"),
        ("# H", "<h1>H</h1>\n</section>"),
        ("> q", "<blockquote><p>q</p></blockquote>"),
        ("- i", "<ul>\n  <li>i</li>\n</ul>"),
    ] {
        let out = html(&format!(":: Term\n{next}\n"));
        assert!(
            out.starts_with("<dl>\n  <dt>Term</dt>\n</dl>\n"),
            "{next}: {out}"
        );
        assert!(out.ends_with(block), "{next}: {out}");
    }
}

#[test]
fn an_indented_line_continues_a_term() {
    assert_eq!(
        html(":: Term\n   more\n: desc\n"),
        "<dl>\n  <dt>Term\nmore</dt>\n  <dd>desc</dd>\n</dl>"
    );
    assert_eq!(
        html("- :: t\n     u\n"),
        "<ul>\n  <li>\n    <dl>\n      <dt>t\nu</dt>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn a_description_or_term_line_at_the_list_column_keeps_the_list() {
    assert_eq!(
        html(":: a\n: b\n:: c\n"),
        "<dl>\n  <dt>a</dt>\n  <dd>b</dd>\n  <dt>c</dt>\n</dl>"
    );
}

#[test]
fn term_multi_line_reports_only_a_description_mark() {
    let found = |input: &str| {
        lint(input, &LintOptions::default())
            .into_iter()
            .filter(|d| d.code == "description-term-multi-line")
            .filter_map(|d| d.pos.map(|p| (p.start_line, p.start_column)))
            .collect::<Vec<_>>()
    };
    assert_eq!(found(":: a\n   : b\n"), [(2, 4)]);
    assert_eq!(found(":: a\n\t: b\n"), [(2, 2)]);
    assert!(found(":: a\n  b\n").is_empty());
    assert!(found(":: a\nb\n").is_empty());
}
