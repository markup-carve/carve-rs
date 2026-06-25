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
