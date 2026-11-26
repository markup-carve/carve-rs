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
fn a_claimed_directive_keeps_its_authored_id() {
    let out = html(
        ":index[x]\n\n{#ix .wide}\n::: index\nSee also.\n:::\n",
        extensions::index(extensions::IndexOptions::default()),
    );
    assert!(
        out.contains("<span id=\"idx-x-1\" class=\"index-term\"></span>"),
        "{out}"
    );
    assert!(
        out.contains("<div id=\"ix\" class=\"index wide\">"),
        "{out}"
    );
    assert!(out.contains("<ul class=\"index\">"), "{out}");
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
fn standard_cite_and_references_share_the_registered_claim() {
    let source = "See :cite[knuth1984]{locator=\"p. 33\"} and :cite[knuth1984; lamport1994].\n\n::: references\n:: knuth1984\n: Knuth.\n\n:: lamport1994\n: Lamport.\n:::\n";
    for cite_first in [true, false] {
        let mut host = Carve::new();
        if cite_first {
            host.enable_standard("carve.cite", false).unwrap();
        }
        host.register(extensions::citations(
            extensions::CitationsOptions::default(),
        ))
        .unwrap();
        if !cite_first {
            host.enable_standard("carve.cite", false).unwrap();
        }
        let result = host
            .render_with_diagnostics(source, &Options::default())
            .unwrap();
        assert!(
            result
                .diagnostics
                .iter()
                .all(|entry| !entry.code.starts_with("extension-")),
            "{:?}",
            result.diagnostics
        );
        assert!(
            result
                .html
                .contains("[<a href=\"#ref-knuth1984\" data-cite-key=\"knuth1984\">1</a>, p. 33]"),
            "{}",
            result.html
        );
        assert!(
            result
                .html
                .contains("<li id=\"ref-lamport1994\">Lamport.</li>"),
            "{}",
            result.html
        );
    }
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

#[test]
fn apparatus_options_and_backlinks_match_reference() {
    let index = html(
        ":index[x]\n\n::: index\n:::\n",
        extensions::index(extensions::IndexOptions {
            backref_label: Some("Zurück zu".into()),
        }),
    );
    assert!(index.contains("aria-label=\"Zurück zu x\""), "{index}");

    let author_date = html(
        ":cite[doe]{mode=\"suppress-author\"}\n\n::: references\n:: [doe]{author=\"Doe\" year=\"2020\"}\n: Book.\n:::\n",
        extensions::citations(extensions::CitationsOptions {
            mode: extensions::CitationMode::AuthorDate,
            bibliography: None,
        }),
    );
    assert!(
        author_date.contains("(<a href=\"#ref-doe\" data-cite-key=\"doe\">2020</a>)"),
        "{author_date}"
    );

    let csl = html(
        "A :cite[x].",
        extensions::citations(extensions::CitationsOptions {
            mode: extensions::CitationMode::Numbered,
            bibliography: Some(vec![serde_json::json!({"id":"x", "title":"<b>"})]),
        }),
    );
    assert!(csl.contains("id=\"cite-x-1\""), "{csl}");
    assert!(
        csl.contains(
            "<li id=\"ref-x\">&lt;b&gt;. <a href=\"#cite-x-1\" class=\"ref-backref\">↩</a></li>"
        ),
        "{csl}"
    );
}

const DOE: &str = "See :cite[doe].\n\n::: bibliography\n:: [doe]{author=\"Doe\" year=\"2020\"}\n: Doe, J. A book.\n:::\n";

#[test]
fn standard_cite_renderer_takes_the_host_mode() {
    let render = |mode: &str| {
        let mut host = Carve::new();
        host.enable_standard_cite_renderer(mode).unwrap();
        host.render(DOE, &Options::default()).unwrap()
    };
    let numbered = [
        "<p>See [<a href=\"#ref-doe\" data-cite-key=\"doe\">1</a>].</p>",
        "<div class=\"bibliography\">",
        "  <ol class=\"references\">",
        "    <li id=\"ref-doe\">Doe, J. A book.</li>",
        "  </ol>",
        "</div>",
    ]
    .join("\n");
    assert_eq!(render("numbered").trim_end(), numbered);
    let mut host = Carve::new();
    host.enable_standard("carve.cite", true).unwrap();
    assert_eq!(
        host.render(DOE, &Options::default()).unwrap().trim_end(),
        numbered
    );
    assert_eq!(
        render("author-date").trim_end(),
        [
            "<p>See (<a href=\"#ref-doe\" data-cite-key=\"doe\">Doe 2020</a>).</p>",
            "<div class=\"bibliography\">",
            "  <ul class=\"references\">",
            "    <li id=\"ref-doe\">Doe, J. A book.</li>",
            "  </ul>",
            "</div>",
        ]
        .join("\n")
    );
    assert!(Carve::new().enable_standard_cite_renderer("other").is_err());
}

#[test]
fn pool_only_entry_closes_the_last_directive() {
    let out = html(
        "See :cite[c].\n\n::: references\n:: a\n: A.\n:::\n\n::: references\n:: b\n: B.\n:::\n",
        extensions::citations(extensions::CitationsOptions {
            mode: extensions::CitationMode::Numbered,
            bibliography: Some(vec![serde_json::json!({"id":"c", "title":"Pool."})]),
        }),
    );
    assert_eq!(
        out.trim_end(),
        [
            "<p>See [<a id=\"cite-c-1\" href=\"#ref-c\" data-cite-key=\"c\">3</a>].</p>",
            "<div class=\"references\">",
            "  <ol class=\"references\">",
            "    <li id=\"ref-a\">A.</li>",
            "  </ol>",
            "</div>",
            "<div class=\"references\">",
            "  <ol start=\"2\" class=\"references\">",
            "    <li id=\"ref-b\">B.</li>",
            "    <li id=\"ref-c\">Pool.. <a href=\"#cite-c-1\" class=\"ref-backref\">↩</a></li>",
            "  </ol>",
            "</div>",
        ]
        .join("\n")
    );
}

/// C2-X-072: a bundle member listed beside its bundle registers once, in either order.
#[test]
fn bundle_member_beside_its_bundle_registers_once() {
    let cases = [
        (
            ["carve.glossary", "carve.term"],
            "See :term[API].\n\n::: glossary\n:: API\n: An interface.\n:::\n",
            "<a href=\"#gloss-API\" class=\"term\">API</a>",
        ),
        (
            ["carve.index", "carve.indexTerm"],
            "A :index[cat] here.\n\n::: index\n:::\n",
            "<a href=\"#idx-cat-1\" class=\"index-backref\"",
        ),
    ];
    for (names, source, piece) in cases {
        for order in [names, [names[1], names[0]]] {
            let mut host = Carve::new();
            for name in order {
                host.enable_standard(name, true).unwrap();
            }
            let out = host.render(source, &Options::default()).unwrap();
            assert!(out.contains(piece), "{order:?}: {out}");
        }
    }
}

#[test]
fn a_member_keeps_the_renderer_its_own_name_asks_for() {
    let source = "See :term[API].\n\n::: glossary\n:: API\n: An interface.\n:::\n";
    let mut host = Carve::new();
    host.enable_standard("carve.glossary", false).unwrap();
    let plain = host.render(source, &Options::default()).unwrap();
    host.enable_standard("carve.term", true).unwrap();
    let out = host.render(source, &Options::default()).unwrap();
    assert_ne!(plain, out);
    assert!(
        out.contains("<a href=\"#gloss-API\" class=\"term\">API</a>"),
        "{out}"
    );
}

#[test]
fn only_a_member_another_standard_name_brought_is_shared() {
    let mut twice = Carve::new();
    twice.enable_standard("carve.term", true).unwrap();
    assert!(twice.enable_standard("carve.term", true).is_err());
    let mut custom = Carve::new();
    custom.register(extensions::glossary()).unwrap();
    assert!(custom.enable_standard("carve.term", true).is_err());
}
