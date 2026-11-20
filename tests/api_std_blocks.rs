//! PROVISIONAL block and inline standard extensions of
//! `carve::api::extensions`: details, spoiler, wikilinks and the document
//! transforms, ported from carve-js (carve-js#5070,
//! test/extensions/blocks-inline.test.ts).

use carve::api::extensions::{self, Bundle, WikilinksOptions};
use carve::api::{self, Carve, Mode, Options, WriteOptions};
use serde_json::Value;
use std::sync::Arc;

fn host(bundle: Bundle) -> Carve {
    let mut carve = Carve::new();
    carve.register(bundle).unwrap();
    carve
}

fn html_with(source: &str, bundle: Bundle, opts: &Options) -> String {
    host(bundle).render(source, opts).unwrap()
}

fn html(source: &str, bundle: Bundle) -> String {
    html_with(source, bundle, &Options::default())
}

fn static_mode() -> Options {
    Options {
        mode: Mode::Static,
        ..Options::default()
    }
}

fn core(source: &str) -> String {
    api::render(source, &Options::default()).unwrap()
}

fn code_text(output: &str) -> &str {
    output
        .split_once("<code class=\"language-php\">")
        .and_then(|(_, rest)| rest.split_once("</code>"))
        .map_or_else(|| panic!("no code element in {output}"), |(text, _)| text)
}

#[test]
fn details_renders_a_disclosure_with_the_title_as_summary() {
    assert_eq!(
        html(
            "::: details \"More info\"\nHidden until expanded.\n:::\n",
            extensions::details()
        ),
        "<details>\n  <summary>More info</summary>\n  <p>Hidden until expanded.</p>\n</details>"
    );
}

#[test]
fn details_keeps_code_payload_bytes() {
    assert_eq!(
        html(
            "::: details \"Req\"\n```php\nx\n```\n:::\n",
            extensions::details()
        ),
        "<details>\n  <summary>Req</summary>\n  <pre><code class=\"language-php\">x\n</code></pre>\n</details>"
    );
    let nested = html(
        "- item\n\n  ::: details \"Req\"\n  ```php\n  first\n    second\n  ```\n  :::\n",
        extensions::details(),
    );
    assert!(
        nested.contains("first\n  second\n</code></pre>"),
        "{nested}"
    );
    let raw = html_with(
        "::: details \"Req\"\n```=html\n<div>\n x\n</div>\n```\n:::\n",
        extensions::details(),
        &Options {
            raw_html: true,
            ..Options::default()
        },
    );
    assert!(raw.contains("<div>\n x\n</div>"), "{raw}");
    let mixed = html_with(
        "::: details \"Req\"\nbefore\n\n```=html\n<div>\n x\n</div>\n```\n\nafter\n:::\n",
        extensions::details(),
        &Options {
            raw_html: true,
            ..Options::default()
        },
    );
    assert!(
        mixed.contains("  <p>before</p>\n<div>\n x\n</div>\n  <p>after</p>"),
        "{mixed}"
    );
    for source in [
        "::: details \"Req\"\n```=html\n<p>hello</p>\n```\n\nhello\n:::\n",
        "::: details \"Req\"\nhello\n\n```=html\n<p>hello</p>\n```\n:::\n",
    ] {
        let output = html_with(
            source,
            extensions::details(),
            &Options {
                raw_html: true,
                ..Options::default()
            },
        );
        let matches: Vec<_> = output
            .lines()
            .filter(|line| line.trim() == "<p>hello</p>")
            .collect();
        assert_eq!(matches.len(), 2, "{output}");
        assert_eq!(
            matches.iter().filter(|line| line.starts_with("  ")).count(),
            1,
            "{output}"
        );
    }
    let textarea = html_with(
        "::: details \"Req\"\n```=html\n<textarea>\n x\n</textarea>\n```\n:::\n",
        extensions::details(),
        &Options {
            raw_html: true,
            ..Options::default()
        },
    );
    assert!(
        textarea.contains("<textarea>\n x\n</textarea>"),
        "{textarea}"
    );
}

#[test]
fn details_keeps_ordinary_inline_code_indented() {
    assert_eq!(
        html(
            "::: details \"Req\"\nA `code` example.\n:::\n",
            extensions::details()
        ),
        "<details>\n  <summary>Req</summary>\n  <p>A <code>code</code> example.</p>\n</details>"
    );
}

#[test]
fn details_keeps_one_thousand_code_variants() {
    for i in 0..1_000 {
        let prefix = if i % 2 == 0 { "" } else { "  " };
        let lead = " ".repeat(i % 7);
        let second = " ".repeat((i / 7) % 7);
        let mut lines = Vec::new();
        if !prefix.is_empty() {
            lines.extend(["- item".to_string(), String::new()]);
        }
        lines.push(format!("{prefix}::: details \"Req\""));
        if i % 3 == 0 {
            lines.extend([format!("{prefix}before"), prefix.to_string()]);
        }
        lines.extend([
            format!("{prefix}```php"),
            format!("{prefix}{lead}x"),
            format!("{prefix}{second}y"),
            format!("{prefix}```"),
        ]);
        if i % 5 == 0 {
            lines.extend([prefix.to_string(), format!("{prefix}after")]);
        }
        lines.push(format!("{prefix}:::"));
        let source = format!("{}\n", lines.join("\n"));
        let rendered = html(&source, extensions::details());
        let (_, content) = rendered
            .split_once("class=\"language-php\">")
            .unwrap_or_else(|| panic!("variant {i}: {rendered}"));
        let (content, _) = content.split_once("</code>").unwrap();
        assert_eq!(
            content,
            format!("{lead}x\n{second}y\n"),
            "variant {i}: {source}"
        );
    }
}

#[test]
fn details_keeps_one_thousand_raw_variants() {
    for i in 0..1_000 {
        let prefix = if i % 2 == 0 { "" } else { "  " };
        let lead = " ".repeat(i % 7);
        let second = " ".repeat((i / 7) % 7);
        let mut lines = Vec::new();
        if !prefix.is_empty() {
            lines.extend(["- item".to_string(), String::new()]);
        }
        lines.push(format!("{prefix}::: details \"Req\""));
        if i % 3 == 0 {
            lines.extend([format!("{prefix}before"), prefix.to_string()]);
        }
        lines.extend([
            format!("{prefix}```=html"),
            format!("{prefix}<div>"),
            format!("{prefix}{lead}x"),
            format!("{prefix}{second}y"),
            format!("{prefix}</div>"),
            format!("{prefix}```"),
        ]);
        if i % 5 == 0 {
            lines.extend([prefix.to_string(), format!("{prefix}after")]);
        }
        lines.push(format!("{prefix}:::"));
        let source = format!("{}\n", lines.join("\n"));
        let rendered = html_with(
            &source,
            extensions::details(),
            &Options {
                raw_html: true,
                ..Options::default()
            },
        );
        assert!(
            rendered.contains(&format!("<div>\n{lead}x\n{second}y\n</div>")),
            "variant {i}: {source}\n{rendered}"
        );
    }
}

#[test]
fn details_default_summary_inline_markup_and_escapes() {
    assert!(
        html("::: details\nx\n:::\n", extensions::details()).contains("<summary>Details</summary>")
    );
    assert!(
        html("::: details \"a *b* <c>\"\nx\n:::\n", extensions::details())
            .contains("<summary>a <strong>b</strong> &lt;c&gt;</summary>")
    );
}

#[test]
fn details_attributes_and_nesting_in_a_list() {
    assert!(
        html(
            "{#faq .wide}\n::: details \"Q\"\nA\n:::\n",
            extensions::details()
        )
        .starts_with(r#"<details id="faq" class="wide">"#)
    );
    assert_eq!(
        html(
            "- x\n\n  ::: details\n  a\n\n  b\n  :::\n",
            extensions::details()
        ),
        "<ul>\n  <li>x\n    <details>\n      <summary>Details</summary>\n      <p>a</p>\n      <p>b</p>\n    </details>\n  </li>\n</ul>"
    );
}

#[test]
fn details_preserves_fenced_code_text_at_every_depth() {
    let cases = [
        (
            "one line",
            "::: details \"Req\"\n```php\nx\n```\n:::\n",
            "x\n",
        ),
        (
            "multiple lines",
            "::: details \"Req\"\n```php\nx\n  y\nz\n```\n:::\n",
            "x\n  y\nz\n",
        ),
        (
            "list item",
            "- item\n\n  ::: details \"Req\"\n  ```php\n  x\n    y\n  ```\n  :::\n",
            "x\n  y\n",
        ),
        (
            "nested details",
            ":::: details \"Outer\"\n::: details \"Req\"\n```php\nx\n  y\n```\n:::\n::::\n",
            "x\n  y\n",
        ),
    ];
    for (name, source, expected) in cases {
        let output = html(source, extensions::details());
        assert_eq!(code_text(&output), expected, "{name}: {output}");
    }

    assert_eq!(
        html(
            "::: details \"Req\"\n```php\nx\n```\n:::\n",
            extensions::details()
        ),
        "<details>\n  <summary>Req</summary>\n  <pre><code class=\"language-php\">x\n</code></pre>\n</details>"
    );
}

#[test]
fn details_preserves_generated_fence_payloads() {
    for n in 0..250 {
        let payload = if n % 2 == 0 {
            format!("line-{n}\n")
        } else {
            format!("line-{n}\n  value-{n}\n")
        };
        let mut indented = String::new();
        for line in payload.lines() {
            indented.push_str("  ");
            indented.push_str(line);
            indented.push('\n');
        }
        let sources = [
            format!("::: details \"Req\"\n```php\n{payload}```\n:::\n"),
            format!("- item\n\n  ::: details \"Req\"\n  ```php\n{indented}  ```\n  :::\n"),
            format!(
                ":::: details \"Outer\"\n::: details \"Req\"\n```php\n{payload}```\n:::\n::::\n"
            ),
            format!("::: details \"Req\"\ntext\n\n```php\n{payload}```\n\nafter\n:::\n"),
        ];
        for source in sources {
            let output = html(&source, extensions::details());
            assert_eq!(code_text(&output), payload, "variant {n}: {source}");
        }
    }
}

#[test]
fn details_static_mode_is_expanded_once() {
    assert!(
        html_with(
            "::: details\nx\n:::\n",
            extensions::details(),
            &static_mode()
        )
        .starts_with("<details open>")
    );
    assert!(
        html_with(
            "{open}\n::: details\nx\n:::\n",
            extensions::details(),
            &static_mode()
        )
        .starts_with(r#"<details open="">"#)
    );
}

#[test]
fn details_leaves_admonitions_and_other_divs() {
    assert!(
        html("::: note\nx\n:::\n", extensions::details()).contains(r#"class="admonition note""#)
    );
    assert!(html("::: other\nx\n:::\n", extensions::details()).contains(r#"<div class="other">"#));
    assert_eq!(
        core("::: details \"T\"\nx\n:::\n"),
        "<div class=\"details\">\n  <p class=\"admonition-title\">T</p>\n  <p>x</p>\n</div>"
    );
}

#[test]
fn details_claim_writes_back_and_falls_back_without_a_renderer() {
    let src = "::: details \"T\"\nx\n:::\n";
    let carve = host(extensions::details());
    let tree = carve.parse(src, &Options::default()).unwrap();
    let doc: Value = serde_json::from_str(&api::encode_tree(&tree)).unwrap();
    assert_eq!(doc["children"][0]["name"], "carve.details");
    assert_eq!(
        doc["children"][0]["source"],
        serde_json::json!({"form": "div", "name": "details", "title": "T", "body": "x\n"})
    );
    assert_eq!(carve.write(&tree, &WriteOptions::default()).unwrap(), src);
    let only = host(extensions::details().without_renderers());
    let r = only
        .render_with_diagnostics(src, &Options::default())
        .unwrap();
    assert_eq!(r.html, "<p><strong>T</strong></p>\n<p>x</p>");
    assert!(
        r.diagnostics
            .iter()
            .any(|e| e.code == "extension-fallback-rendered")
    );
}

#[test]
fn spoiler_inline_is_a_span_with_author_classes() {
    assert_eq!(
        html(
            "Plot: :spoiler[the butler]{.x onclick=\"a()\"}.",
            extensions::spoiler()
        ),
        r#"<p>Plot: <span class="spoiler x">the butler</span>.</p>"#
    );
}

#[test]
fn spoiler_block_is_a_disclosure() {
    assert_eq!(
        html(
            "::: spoiler \"Ending\"\nEveryone lives.\n:::\n",
            extensions::spoiler()
        ),
        "<details class=\"spoiler\">\n  <summary>Ending</summary>\n  <p>Everyone lives.</p>\n</details>"
    );
    assert!(
        html("::: spoiler\nx\n:::\n", extensions::spoiler()).contains("<summary>Spoiler</summary>")
    );
}

#[test]
fn spoiler_static_mode_reveals_both_forms() {
    assert_eq!(
        html_with(":spoiler[x]", extensions::spoiler(), &static_mode()),
        r#"<p><span class="spoiler spoiler-revealed">x</span></p>"#
    );
    assert_eq!(
        html_with(
            "::: spoiler \"E\"\nx\n:::\n",
            extensions::spoiler(),
            &static_mode()
        ),
        "<section class=\"spoiler spoiler-revealed\">\n  <h3 class=\"spoiler-title\">E</h3>\n  <p>x</p>\n</section>"
    );
}

#[test]
fn spoiler_core_forms_without_the_extension() {
    assert_eq!(
        core(":spoiler[x]"),
        r#"<p><span class="ext-spoiler">x</span></p>"#
    );
    assert_eq!(
        core("::: spoiler\nx\n:::\n"),
        "<div class=\"spoiler\">\n  <p>x</p>\n</div>"
    );
}

fn wiki() -> Bundle {
    extensions::wikilinks(WikilinksOptions::default())
}

#[test]
fn wikilinks_slug_and_anchor() {
    assert_eq!(
        html(":wiki[Main Page#top]", wiki()),
        r#"<p><a href="main-page#top" class="wikilink" data-wikilink="Main Page">Main Page</a></p>"#
    );
    assert_eq!(
        html(":wiki[docs/Install Guide|the guide]", wiki()),
        r#"<p><a href="docs/install-guide" class="wikilink" data-wikilink="docs/Install Guide">the guide</a></p>"#
    );
}

#[test]
fn wikilinks_options_and_url_policy() {
    let opts = || {
        extensions::wikilinks(WikilinksOptions {
            url_generator: Some(Arc::new(|p: &str| format!("/w/{p}"))),
            css_class: Some("a b".into()),
            new_window: true,
        })
    };
    assert_eq!(
        html(":wiki[P]", opts()),
        r#"<p><a href="/w/P" class="a b" data-wikilink="P" target="_blank" rel="noopener">P</a></p>"#
    );
    assert_eq!(
        html(
            ":wiki[P]{rel=opener TARGET=x data-wikilink=y HREF=/evil lang=de}",
            opts()
        ),
        r#"<p><a href="/w/P" class="a b" data-wikilink="P" lang="de" target="_blank" rel="noopener">P</a></p>"#
    );
    let evil = extensions::wikilinks(WikilinksOptions {
        url_generator: Some(Arc::new(|_: &str| "javascript:alert(1)".to_string())),
        ..WikilinksOptions::default()
    });
    assert_eq!(html(":wiki[P]", evil), "<p>P</p>");
}

#[test]
fn wikilinks_without_a_renderer_is_the_core_text_fallback() {
    assert_eq!(
        core(":wiki[P]"),
        r#"<p><span class="ext-wiki">P</span></p>"#
    );
    let tree = host(wiki().without_renderers())
        .parse(":wiki[P]", &Options::default())
        .unwrap();
    let doc: Value = serde_json::from_str(&api::encode_tree(&tree)).unwrap();
    assert_eq!(doc["children"][0]["children"][0]["name"], "carve.wiki");
}

#[test]
fn default_attributes_merge_under_the_authored_ones() {
    let bundle = extensions::default_attributes(vec![
        (
            "heading".into(),
            vec![
                ("class".into(), "h".into()),
                ("id".into(), "x".into()),
                ("data-a".into(), "1".into()),
            ],
        ),
        ("code_block".into(), vec![("class".into(), "c".into())]),
    ]);
    let opts = Options {
        sections: false,
        ..Options::default()
    };
    assert_eq!(
        html_with("{#own data-a=2}\n# H\n\n```\nx\n```\n", bundle, &opts),
        "<h1 id=\"own\" class=\"h\" data-a=\"2\">H</h1>\n<pre class=\"c\"><code>x\n</code></pre>"
    );
}

#[test]
fn tab_normalize_changes_code_only() {
    assert_eq!(
        html(
            "a\tb `c\td`\n\n```\n\tx\n```\n",
            extensions::tab_normalize(4).unwrap()
        ),
        "<p>a\tb <code>c    d</code></p>\n<pre><code>    x\n</code></pre>"
    );
    assert_eq!(
        html("```\n\tx\n```\n", extensions::tab_normalize(2).unwrap()),
        "<pre><code>  x\n</code></pre>"
    );
    assert!(extensions::tab_normalize(17).is_err());
}

#[test]
fn inline_footnotes_become_parentheses() {
    assert_eq!(
        html(
            "a^[two *b*] c\n",
            extensions::inline_footnotes_to_parentheses()
        ),
        "<p>a (two <strong>b</strong>) c</p>"
    );
}

#[test]
fn a_host_renderer_replaces_a_bundled_one() {
    struct X;
    impl api::Renderer for X {
        fn render(&self, _: &Value, _: &api::RenderContext<'_>) -> Result<String, api::Error> {
            Ok("<x>".into())
        }
    }
    let mut bundle = extensions::details();
    bundle.extensions[0].renderers = vec![(api::Channel::Interactive, Box::new(X))];
    assert_eq!(html("::: details\nx\n:::\n", bundle), "<x>");
}

#[test]
fn the_json_boundary_selects_them_by_name() {
    let out = api::json::call(
        r#"{"call":"render","source":"::: details \"Q\"\nA\n:::\n\nSee :wiki[Main Page] and :spoiler[x].","options":{"extensions":["carve.details","carve.wiki","carve.spoiler"],"renderers":{"carve.details":"standard","carve.wiki":"standard","carve.spoiler":"standard"}}}"#,
    );
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v["ok"]["html"],
        "<details>\n  <summary>Q</summary>\n  <p>A</p>\n</details>\n<p>See <a href=\"main-page\" class=\"wikilink\" data-wikilink=\"Main Page\">Main Page</a> and <span class=\"spoiler\">x</span>.</p>"
    );
}

#[test]
fn the_issue_example() {
    let all = Bundle::merge([extensions::details(), extensions::spoiler(), wiki()]);
    assert_eq!(
        html(
            "{#faq}\n::: details \"More *info*\"\nHidden.\n:::\n\nPlot :spoiler[the butler], :wiki[Main Page#top].\n",
            all
        ),
        "<details id=\"faq\">\n  <summary>More <strong>info</strong></summary>\n  <p>Hidden.</p>\n</details>\n<p>Plot <span class=\"spoiler\">the butler</span>, <a href=\"main-page#top\" class=\"wikilink\" data-wikilink=\"Main Page\">Main Page</a>.</p>"
    );
}
