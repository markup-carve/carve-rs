use carve::api::{Carve, Options, extensions};

fn html(input: &str, bundle: extensions::Bundle) -> String {
    let mut host = Carve::new();
    host.register(bundle).unwrap();
    let result = host
        .render_with_diagnostics(input, &Options::default())
        .unwrap();
    assert!(
        result
            .diagnostics
            .iter()
            .all(|entry| !entry.code.starts_with("extension-")),
        "{:?}",
        result.diagnostics
    );
    result.html
}

#[test]
fn glossary_links_terms_and_places_entries() {
    let source = "Use :term[HTTP] and :term[FTP].\n\n::: glossary\n:: HTTP\n: HyperText Transfer Protocol.\n:::\n";
    let out = html(source, extensions::glossary());
    assert!(
        out.contains("<a href=\"#gloss-HTTP\" class=\"term\">HTTP</a>"),
        "{out}"
    );
    assert!(out.contains("<span class=\"term\">FTP</span>"), "{out}");
    assert!(out.contains("<dt id=\"gloss-HTTP\">HTTP</dt>"), "{out}");
}

#[test]
fn index_sorts_entries_and_links_each_use() {
    let source = "See :index[Zed] and :index[HTTP].\n\n::: index\n:::\n";
    let out = html(
        source,
        extensions::index(extensions::IndexOptions::default()),
    );
    assert!(
        out.contains("<span id=\"idx-zed-1\" class=\"index-term\"></span>"),
        "{out}"
    );
    assert!(
        out.contains("<span id=\"idx-http-1\" class=\"index-term\"></span>"),
        "{out}"
    );
    assert!(out.contains("<ul class=\"index\">"), "{out}");
    assert!(
        out.find("href=\"#idx-http-1\"").unwrap() < out.rfind("href=\"#idx-zed-1\"").unwrap(),
        "{out}"
    );
}

#[test]
fn citations_resolve_keys_and_place_references() {
    let source = "See :cite[doe]{locator=\"p. 4\"}.\n\n::: references\n:: doe\n: Doe, J. (2020). /A book/.\n:::\n";
    let out = html(
        source,
        extensions::citations(extensions::CitationsOptions::default()),
    );
    assert!(
        out.contains("[<a href=\"#ref-doe\" data-cite-key=\"doe\">1</a>, p. 4]"),
        "{out}"
    );
    assert!(
        out.contains("<li id=\"ref-doe\">Doe, J. (2020). <em>A book</em>.</li>"),
        "{out}"
    );
}

#[test]
fn all_three_bundles_share_a_document() {
    let source = "See :cite[doe]{locator=\"p. 4\"} and :term[HTTP]:index[HTTP].\n\n::: glossary\n:: HTTP\n: HyperText Transfer Protocol.\n:::\n\n::: references\n:: doe\n: Doe, J. (2020). /A book/.\n:::\n\n::: index\n:::\n";
    let mut host = Carve::new();
    host.register(extensions::glossary()).unwrap();
    host.register(extensions::index(extensions::IndexOptions::default()))
        .unwrap();
    host.register(extensions::citations(
        extensions::CitationsOptions::default(),
    ))
    .unwrap();
    let out = host.render(source, &Options::default()).unwrap();
    assert!(
        out.contains("[<a href=\"#ref-doe\" data-cite-key=\"doe\">1</a>, p. 4]"),
        "{out}"
    );
    assert!(
        out.contains("<a href=\"#gloss-HTTP\" class=\"term\">HTTP</a>"),
        "{out}"
    );
    assert!(
        out.contains("<span id=\"idx-http-1\" class=\"index-term\"></span>"),
        "{out}"
    );
    assert!(
        out.contains("<li id=\"ref-doe\">Doe, J. (2020). <em>A book</em>.</li>"),
        "{out}"
    );
    assert!(out.contains("<li>HTTP <a href=\"#idx-http-1\" class=\"index-backref\" aria-label=\"Back to HTTP\">↩</a></li>"), "{out}");
}
