//! Rulings of freeze candidate 2, revision 1 that no corpus entry holds.
use carve::options::Options;
use serde_json::Value;

fn tree(input: &str) -> Value {
    serde_json::from_str(&carve::tree_json(input, &Options::default())).unwrap()
}

#[test]
fn c2_b_190_description_past_the_cap_is_text() {
    // C2-B-234: as text the line closes the list.
    let input = format!("{}:: [\n{}: {{", "> ".repeat(199), "> ".repeat(199));
    let html = carve::render(&input, &Options::default());
    assert!(html.contains("<dt>[</dt>"), "{html}");
    assert!(html.contains("<p>: {</p>"), "{html}");
    assert!(!html.contains("<dd>"), "{html}");
    let input = format!("{}:: [\n{} : {{", "> ".repeat(199), "> ".repeat(199));
    let html = carve::render(&input, &Options::default());
    assert!(html.contains("<dt>[\n: {</dt>"), "{html}");
}

#[test]
fn c2_b_130_space_behind_a_comment_does_not_lead_the_line() {
    let html = carve::render("::: |\n{%%} -\n:::", &Options::default());
    assert_eq!(html, "<div class=\"line-block\">\n  <p> -</p>\n</div>");
}

#[test]
fn c2_a_158_line_of_spaces_has_no_children_and_no_pos() {
    let t = tree("::: |\nx\n \ny\n:::");
    let line = &t["children"][0]["children"][1];
    assert_eq!(line["children"], serde_json::json!([]));
    assert!(line.get("pos").is_none(), "{line}");
}

#[test]
fn c2_n_072_include_attributes_are_not_written_and_reserve_no_id() {
    let input = "{#a .c}\n{{ x.crv }}\n\n{#a}\n= A";
    let html = carve::render(input, &Options::default());
    assert!(html.starts_with("<p>{{ x.crv }}</p>"), "{html}");
    assert!(html.contains("<section id=\"a\">"), "{html}");
    let out = carve::render_with_diagnostics(input, &Options::default());
    assert!(!out.codes().iter().any(|c| c == "duplicate-id"));
}

#[test]
fn c2_n_074_unexpanded_include_numbers_and_writes_its_caption() {
    let input = "{table}\n{{ t.csv @code:csv }}\n^ Table #: T\n\n| a |\n^ Table #: U";
    let html = carve::render(input, &Options::default());
    assert!(
        html.starts_with("<p>{{ t.csv @code:csv }}</p>\n<p>Table 1: T</p>"),
        "{html}"
    );
    assert!(html.contains("<caption>Table 2: U</caption>"), "{html}");
}

#[test]
fn c2_b_147_caption_below_include_without_code_is_text() {
    let html = carve::render("{{ a.crv }}\n^ cap", &Options::default());
    assert_eq!(html, "<p>{{ a.crv }}</p>\n<p>^ cap</p>");
}

#[test]
fn c2_b_122_further_words_after_an_opener_are_reported() {
    for input in ["```js\t\"T\"\nx\n```", "::: *\ny\n:::", "``` js T\nx\n```"] {
        let out = carve::render_with_diagnostics(input, &Options::default());
        assert!(
            out.codes().iter().any(|c| c == "fence-title-syntax"),
            "{input}"
        );
    }
    let out = carve::render_with_diagnostics("``` js \"T\" [L]\nx\n```", &Options::default());
    assert_eq!(out.codes(), Vec::<String>::new());
}

#[test]
fn c2_b_113_unclosed_fences_are_reported_once() {
    let out = carve::render_with_diagnostics("```\na\n\n%%%\nb", &Options::default());
    assert_eq!(out.codes(), ["unclosed-code-fence"]);
    let out = carve::render_with_diagnostics("%%%\nb", &Options::default());
    assert_eq!(out.codes(), ["unclosed-comment-fence"]);
    let out = carve::render_with_diagnostics("```\na\n```", &Options::default());
    assert_eq!(out.codes(), Vec::<String>::new());
}
