use carve::api::{self, Carve, Options, extensions};

fn html(source: &str, bundle: extensions::Bundle) -> String {
    let mut host = Carve::new();
    host.register(bundle).unwrap();
    host.render(source, &Options::default()).unwrap()
}

#[test]
fn external_links_cover_links_and_autolinks() {
    let out = html(
        "[a](https://x.org) <http://y.org> [b](/local)\n",
        extensions::external_links(extensions::ExternalLinksOptions::default()),
    );
    assert_eq!(
        out,
        "<p><a href=\"https://x.org\" rel=\"noopener noreferrer\" target=\"_blank\">a</a> <a href=\"http://y.org\" rel=\"noopener noreferrer\" target=\"_blank\">http://y.org</a> <a href=\"/local\">b</a></p>"
    );
}

#[test]
fn navigation_factories_preserve_heading_and_link_behavior() {
    // C2-H-208: the shift is the core option `headingShift`, not an extension.
    let opts = Options {
        heading_shift: 1,
        ..Options::default()
    };
    let shifted = Carve::new().render("= A\n", &opts).unwrap();
    assert!(shifted.contains("<h2>A</h2>"), "{shifted}");
    let permalink = html(
        "= A\n",
        extensions::heading_permalinks(extensions::HeadingPermalinksOptions::default()),
    );
    assert!(
        permalink.contains(
            "<h1>A <a href=\"#A\" class=\"permalink\" aria-label=\"Permalink\">¶</a></h1>"
        ),
        "{permalink}"
    );
    let numbered = html(
        "= A\n\n== B\n",
        extensions::heading_numbers(extensions::HeadingNumbersOptions::default()),
    );
    assert!(
        numbered.contains("<span class=\"section-number\">1.1</span> B"),
        "{numbered}"
    );
    let reference = html(
        "= A\n\n[A][]\n",
        extensions::heading_reference(extensions::HeadingReferenceOptions::default()),
    );
    assert!(reference.contains("class=\"heading-ref\""), "{reference}");
    let autolink = html(
        "Visit https://x.org/a. Mail a@b.co.\n",
        extensions::autolink(extensions::AutolinkOptions::default()),
    );
    assert!(
        autolink.contains("<a href=\"https://x.org/a\">https://x.org/a</a>"),
        "{autolink}"
    );
    assert!(
        autolink.contains("<a href=\"mailto:a@b.co\">a@b.co</a>"),
        "{autolink}"
    );
}

#[test]
fn link_policy_filters_domains() {
    let options = extensions::LinkPolicyOptions {
        denied_domains: vec!["bad.org".into()],
        rel: vec!["nofollow".into()],
        ..Default::default()
    };
    let out = html(
        "[a](https://bad.org) [b](https://ok.org)\n",
        extensions::link_policy(options),
    );
    assert_eq!(
        out,
        "<p>a <a href=\"https://ok.org\" rel=\"nofollow\">b</a></p>"
    );
}

#[test]
fn external_links_are_available_at_the_json_boundary() {
    let request = serde_json::json!({"call":"render", "source":"[docs](https://example.com)", "options":{"extensions":["carve.externalLinks"]}});
    let response: serde_json::Value =
        serde_json::from_str(&api::json::call(&request.to_string())).unwrap();
    assert_eq!(
        response["ok"]["html"],
        "<p><a href=\"https://example.com\" rel=\"noopener noreferrer\" target=\"_blank\">docs</a></p>"
    );
}

#[test]
fn external_links_keep_working_with_authored_and_generated_autolinks() {
    let request = serde_json::json!({"call":"render", "source":"[x](https://ext.org) and https://a.org and <http://b.org>\n", "options":{"extensions":["carve.autolink", "carve.externalLinks"]}});
    let response: serde_json::Value =
        serde_json::from_str(&api::json::call(&request.to_string())).unwrap();
    assert!(response.get("error").is_none(), "{response}");
    let html = response["ok"]["html"].as_str().unwrap();
    for href in ["https://ext.org", "https://a.org", "http://b.org"] {
        assert!(
            html.contains(&format!(
                "href=\"{href}\" rel=\"noopener noreferrer\" target=\"_blank\""
            )),
            "{html}"
        );
    }
}

#[test]
fn table_of_contents_renders_and_falls_back() {
    let source = "= Intro\n\n== Usage\n\n=== Deep\n\n== Next\n";
    let out = html(
        source,
        extensions::table_of_contents(extensions::TableOfContentsOptions::default()),
    );
    assert!(
        out.starts_with("<nav class=\"toc\" aria-label=\"Table of contents\">"),
        "{out}"
    );
    assert!(out.contains("<li><a href=\"#Deep\">Deep</a></li>"), "{out}");
    let mut host = Carve::new();
    host.register(
        extensions::table_of_contents(extensions::TableOfContentsOptions::default())
            .without_renderers(),
    )
    .unwrap();
    let fallback = host.render(source, &Options::default()).unwrap();
    assert!(fallback.starts_with("<div class=\"toc\">"), "{fallback}");
    let mut host = Carve::new();
    host.register(extensions::heading_numbers(
        extensions::HeadingNumbersOptions::default(),
    ))
    .unwrap();
    host.register(extensions::table_of_contents(
        extensions::TableOfContentsOptions::default(),
    ))
    .unwrap();
    let with_numbers = host.render("= A\n", &Options::default()).unwrap();
    assert!(
        with_numbers.contains("<li><a href=\"#A\">A</a></li>"),
        "{with_numbers}"
    );
    assert!(
        !with_numbers.contains("<li><a href=\"#A\"><span class=\"section-number\""),
        "{with_numbers}"
    );
}

#[test]
fn toc_placement_claims_the_directive() {
    let out = html("= A\n\n::: toc\n:::\n", extensions::toc_placement());
    assert!(out.contains("<nav class=\"toc\""), "{out}");
    assert!(out.contains("<li><a href=\"#A\">A</a></li>"), "{out}");
    let source = "= A\n\n== B\n\n{depth=1 .side}\n::: toc \"On this page\"\nIntro text.\n:::\n";
    let out = html(source, extensions::toc_placement());
    assert!(out.contains("class=\"toc side\""), "{out}");
    assert!(out.contains("aria-labelledby=\"toc-title-1\""), "{out}");
    assert!(
        out.contains("<p class=\"admonition-title\" id=\"toc-title-1\">On this page</p>"),
        "{out}"
    );
    assert!(out.contains("<li><a href=\"#A\">A</a></li>"), "{out}");
    assert!(!out.contains("href=\"#B\""), "{out}");
}

#[test]
fn autolink_matches_urls_and_email_without_nested_links() {
    let out = html(
        "Visit https://x.org/a. Mail a@b.co, or mailto:c@d.org!\n",
        extensions::autolink(extensions::AutolinkOptions::default()),
    );
    assert_eq!(
        out,
        "<p>Visit <a href=\"https://x.org/a\">https://x.org/a</a>. Mail <a href=\"mailto:a@b.co\">a@b.co</a>, or <a href=\"mailto:c@d.org\">mailto:c@d.org</a>!</p>"
    );
    let source = "`https://x.org` [https://y.org](/p) xhttps://z.org\n";
    assert_eq!(
        html(
            source,
            extensions::autolink(extensions::AutolinkOptions::default())
        ),
        api::render(source, &Options::default()).unwrap()
    );
}

#[test]
fn heading_reference_respects_definitions() {
    let out = html(
        "= Setup\n\n[Setup][] [here][Setup] [Doc][]\n\n[Doc]: https://x.org\n",
        extensions::heading_reference(extensions::HeadingReferenceOptions::default()),
    );
    assert!(
        out.contains("<a href=\"#Setup\" class=\"heading-ref\">Setup</a>"),
        "{out}"
    );
    assert!(out.contains("<a href=\"https://x.org\">Doc</a>"), "{out}");
    let mut host = Carve::new();
    host.register(extensions::heading_numbers(
        extensions::HeadingNumbersOptions::default(),
    ))
    .unwrap();
    host.register(extensions::heading_reference(
        extensions::HeadingReferenceOptions::default(),
    ))
    .unwrap();
    let out = host
        .render("= Setup\n\n[Setup][]\n", &Options::default())
        .unwrap();
    assert!(
        out.contains("<a href=\"#Setup\" class=\"heading-ref\">Setup</a>"),
        "{out}"
    );
}

#[test]
fn link_policy_filters_internal_and_numeric_hosts() {
    let options = extensions::LinkPolicyOptions {
        allow_internal: false,
        ..Default::default()
    };
    assert_eq!(
        html(
            "[x](doc/page) [y](a/b:c)\n",
            extensions::link_policy(options)
        ),
        "<p>x y</p>"
    );
    let options = extensions::LinkPolicyOptions {
        denied_domains: vec!["127.0.0.1".into()],
        ..Default::default()
    };
    assert_eq!(
        html(
            "[a](http://2130706433/) [b](http://0x7f.0.0.1/x) [c](https://ok.org)\n",
            extensions::link_policy(options)
        ),
        "<p>a b <a href=\"https://ok.org\">c</a></p>"
    );
}

#[test]
fn permalink_retains_explicit_mixed_case_id() {
    let options = extensions::HeadingPermalinksOptions {
        lowercase_ids: true,
        ..Default::default()
    };
    let out = html(
        "{#MiXeD}\n= Heading\n",
        extensions::heading_permalinks(options),
    );
    assert!(out.contains("id=\"MiXeD\""), "{out}");
    assert!(out.contains("href=\"#MiXeD\""), "{out}");
}

#[test]
fn link_policy_reads_bracketed_ipv6_host() {
    let options = extensions::LinkPolicyOptions {
        denied_domains: vec!["[::1]".into()],
        ..Default::default()
    };
    assert_eq!(
        html(
            "[local](http://[::1]/) [public](https://[2001:db8::1]/)\n",
            extensions::link_policy(options)
        ),
        "<p>local <a href=\"https://[2001:db8::1]/\">public</a></p>"
    );
}

#[test]
fn heading_number_xref_keeps_the_title_markup() {
    let out = html(
        "{#sub}\n== Sub *bold*\n\nSee </#sub>.\n",
        extensions::heading_numbers(extensions::HeadingNumbersOptions::default()),
    );
    assert!(
        out.contains(r##"<a href="#sub">Section 1 - Sub <strong>bold</strong></a>"##),
        "{out}"
    );
}

#[test]
fn heading_number_xref_to_a_linked_title_does_not_nest_links() {
    let out = html(
        "{#g}\n= See [guide](/guide) *now*\n\nGo to </#g>.\n",
        extensions::heading_numbers(extensions::HeadingNumbersOptions::default()),
    );
    assert!(
        out.contains(r##"<a href="#g">Section 1 - See guide <strong>now</strong></a>"##),
        "{out}"
    );
}

#[test]
fn toc_placement_lists_only_top_level_headings() {
    // carve#6531: the placed table lists the headings a section wraps, as tableOfContents does.
    let out = html(
        "= Top\n\n::: note\n= Inside\n:::\n\n::: toc\n:::\n",
        extensions::toc_placement(),
    );
    assert!(out.contains("<li><a href=\"#Top\">Top</a></li>"), "{out}");
    assert!(!out.contains("<a href=\"#Inside\">"), "{out}");
}

#[test]
fn heading_numbers_count_depth_not_level() {
    // carve#6531: a skipped level gives no duplicate number.
    let numbers = || extensions::heading_numbers(extensions::HeadingNumbersOptions::default());
    let skipped = html("= A\n\n=== C\n\n== B\n\n==== D\n", numbers());
    assert!(
        skipped.contains("<h3><span class=\"section-number\">1.1</span> C</h3>"),
        "{skipped}"
    );
    assert!(
        skipped.contains("<h2><span class=\"section-number\">1.2</span> B</h2>"),
        "{skipped}"
    );
    assert!(
        skipped.contains("<h4><span class=\"section-number\">1.2.1</span> D</h4>"),
        "{skipped}"
    );
    let leading = html("== B\n\n= A\n\n== C\n", numbers());
    assert!(
        leading.contains("<h2><span class=\"section-number\">1</span> B</h2>"),
        "{leading}"
    );
    assert!(
        leading.contains("<h1><span class=\"section-number\">2</span> A</h1>"),
        "{leading}"
    );
    assert!(
        leading.contains("<h2><span class=\"section-number\">2.1</span> C</h2>"),
        "{leading}"
    );
}

#[test]
fn heading_numbers_skip_headings_in_containers() {
    // carve#6531: headings in a note or list item get no number.
    let out = html(
        "= A\n\n::: note\n== N\n:::\n\n- item\n\n  == L\n\n== B\n",
        extensions::heading_numbers(extensions::HeadingNumbersOptions::default()),
    );
    assert!(out.contains("<h2 id=\"N\">N</h2>"), "{out}");
    assert!(out.contains("<h2 id=\"L\">L</h2>"), "{out}");
    assert!(
        out.contains("<h2><span class=\"section-number\">1.1</span> B</h2>"),
        "{out}"
    );
}
