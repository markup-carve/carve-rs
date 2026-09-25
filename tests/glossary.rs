use carve::{Glossary, Options};

fn h(source: &str) -> String {
    let glossary = Glossary::new();
    let options = Options::new().with_extension(&glossary);
    carve::to_html_with_options(source, &options)
        .trim()
        .to_string()
}

fn off(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

const GLOSS: &str =
    "::: glossary\n:: HTTP\n:  HyperText Transfer Protocol.\n\n:: HTML\n:  HyperText Markup Language.\n:::";
const GLOSS_HTTP: &str = "::: glossary\n:: HTTP\n:  HyperText Transfer Protocol.\n:::";

#[test]
fn renders_dl_with_gloss_ids() {
    let out = h(GLOSS);
    assert!(out.contains("<dl class=\"glossary\">"));
    assert!(out.contains("<dt id=\"gloss-http\">HTTP</dt>"));
    assert!(out.contains("<dd>HyperText Transfer Protocol.</dd>"));
    assert!(out.contains("<dt id=\"gloss-html\">HTML</dt>"));
}

#[test]
fn full_golden_matches_carve_js() {
    let out = h(&format!("Use :term[HTTP] and :term[FTP].\n\n{GLOSS}"));
    assert_eq!(
        out,
        "<p>Use <a href=\"#gloss-http\" class=\"term\">HTTP</a> and <span class=\"term\">FTP</span>.</p>\n\
<dl class=\"glossary\">\n  <dt id=\"gloss-http\">HTTP</dt>\n  <dd>HyperText Transfer Protocol.</dd>\n  \
<dt id=\"gloss-html\">HTML</dt>\n  <dd>HyperText Markup Language.</dd>\n</dl>"
    );
}

#[test]
fn term_links_to_defined_entry() {
    let out = h(&format!("Use :term[HTTP].\n\n{GLOSS}"));
    assert!(out.contains("<a href=\"#gloss-http\" class=\"term\">HTTP</a>"));
}

#[test]
fn undefined_term_degrades_to_span() {
    let out = h(&format!("Use :term[FTP].\n\n{GLOSS}"));
    assert!(out.contains("<span class=\"term\">FTP</span>"));
    assert!(!out.contains("href=\"#gloss-ftp\""));
}

#[test]
fn entries_in_source_order() {
    let out = h(GLOSS);
    assert!(out.find("gloss-http").unwrap() < out.find("gloss-html").unwrap());
}

#[test]
fn duplicate_slug_first_wins_id() {
    let out = h("::: glossary\n:: HTTP\n:  One.\n\n:: HTTP\n:  Two.\n:::");
    assert_eq!(out.matches("id=\"gloss-http\"").count(), 1);
    assert!(out.contains("<dt>HTTP</dt>"));
}

#[test]
fn off_uses_generic_fallback() {
    let out = off("Use :term[HTTP].");
    assert!(out.contains("<span class=\"ext-term\">HTTP</span>"));
}

#[test]
fn nested_in_blockquote() {
    let out = h(
        "Use :term[HTTP].\n\n> ::: glossary\n> :: HTTP\n> :  HyperText Transfer Protocol.\n> :::",
    );
    assert!(out.contains("<dt id=\"gloss-http\">HTTP</dt>"));
    assert!(out.contains("<a href=\"#gloss-http\" class=\"term\">HTTP</a>"));
}

#[test]
fn preserves_intro_prose_and_second_list() {
    let out = h("::: glossary\nProtocols below.\n\n:: HTTP\n:  One.\n\n:: FTP\n:  Two.\n:::");
    assert!(out.contains("Protocols below."));
    assert!(out.contains("<dt id=\"gloss-http\">HTTP</dt>"));
    assert!(out.contains("<dt id=\"gloss-ftp\">FTP</dt>"));
}

#[test]
fn trailing_note_keeps_source_order() {
    let out = h("::: glossary\n:: HTTP\n:  One.\n\nSee the RFCs.\n:::");
    assert!(out.find("gloss-http").unwrap() < out.find("See the RFCs.").unwrap());
}

#[test]
fn carries_inline_attrs_on_term() {
    let out = h(&format!("Use :term[HTTP]{{.abbr #use}}.\n\n{GLOSS_HTTP}"));
    assert!(out.contains("href=\"#gloss-http\""));
    assert!(out.contains("id=\"use\""));
    assert!(out.contains("class=\"term abbr\""));
}

#[test]
fn carries_block_attrs_on_dl() {
    let out = h("{#terms .wide}\n::: glossary\n:: HTTP\n:  One.\n:::");
    assert!(out.contains("<dl id=\"terms\" class=\"glossary wide\">"));
}

#[test]
fn drops_author_href_case_insensitively() {
    let out = h(&format!(
        "Use :term[HTTP]{{HREF=\"#other\"}}.\n\n{GLOSS_HTTP}"
    ));
    assert!(!out.contains("#other"));
    assert_eq!(out.to_lowercase().matches("href=").count(), 1);
}

/// The column each of an element's own tags is written at.
fn tag_columns(html: &str, tag: &str) -> Vec<usize> {
    html.lines()
        .filter(|line| line.trim_start().starts_with(tag))
        .map(|line| line.len() - line.trim_start().len())
        .collect()
}

#[test]
fn a_placed_dl_writes_both_its_tags_at_one_column() {
    // CARVE-P10-010: the two tags may carry the nesting indentation or sit at
    // column 0, and ONE COLUMN SERVES BOTH. The opener sat a level below its own
    // closer, which conforms under neither choice.
    //
    // OFF COLUMN 0, which is the position the rest of this file never spells -
    // and at column 0 all three engines agree, so nothing there can see this.
    let nested = h("# H\n\n::: glossary\n:: term\n:  def\n:::");
    assert_eq!(
        nested,
        "<section id=\"H\">\n  <h1>H</h1>\n  <dl class=\"glossary\">\n    \
         <dt id=\"gloss-term\">term</dt>\n    <dd>def</dd>\n  </dl>\n</section>"
    );
    assert_eq!(tag_columns(&nested, "<dl"), vec![2]);
    assert_eq!(tag_columns(&nested, "</dl>"), vec![2]);
    // The rows sit one level inside their own element and move with the column
    // it took, which is what the clause pins for a glossary rather than the
    // column-0 anchoring it gives a `toc`.
    assert_eq!(tag_columns(&nested, "<dt"), vec![4]);
    assert_eq!(tag_columns(&nested, "<dd>"), vec![4]);

    // The same document FLATTENED to top level, which is how every case above
    // spells it. Its columns are 0 and 2, so an assertion written against it
    // cannot fail on the divergence this test is for - the pair is what makes
    // the case a guard rather than a restatement of the old golden.
    let flat = h("::: glossary\n:: term\n:  def\n:::");
    assert_eq!(tag_columns(&flat, "<dl"), vec![0]);
    assert_eq!(tag_columns(&flat, "</dl>"), vec![0]);
    assert_eq!(tag_columns(&flat, "<dt"), vec![2]);
    assert_ne!(
        tag_columns(&nested, "<dl"),
        tag_columns(&flat, "<dl"),
        "the nested spelling must be the one under test"
    );
}

#[test]
fn a_second_placed_dl_writes_both_its_tags_at_one_column() {
    // The first list is the framework-indented first line; a later one is not,
    // so it supplies its own column. Both still answer the clause.
    let out = h("# H\n\n::: glossary\n:: a\n:  1\n\nnote\n\n:: b\n:  2\n:::");
    assert_eq!(tag_columns(&out, "<dl"), vec![2, 2]);
    assert_eq!(tag_columns(&out, "</dl>"), vec![2, 2]);
    assert_eq!(tag_columns(&out, "<dt"), vec![4, 4]);
}

#[test]
fn a_placed_dl_inside_a_container_follows_the_container() {
    let out = h("# H\n\n::: note\n::: glossary\n:: term\n:  def\n:::\n:::");
    assert_eq!(tag_columns(&out, "<dl"), vec![4]);
    assert_eq!(tag_columns(&out, "</dl>"), vec![4]);
    assert_eq!(tag_columns(&out, "<dt"), vec![6]);
}
