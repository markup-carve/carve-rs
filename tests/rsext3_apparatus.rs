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
