//! Profile-based feature restriction: resolution, disallowed actions, link
//! policy, `max_nesting` / `max_length`, the four presets, and golden parity
//! with carve-php. Mirrors carve-js' `test/profiles.test.ts`.

use carve::profile::{canonical_block_type, canonical_inline_type};
use carve::{apply_profile, DisallowedAction, LinkPolicy, Options, Profile, ProfileViolationError};

fn html(src: &str, profile: Profile) -> String {
    carve::to_html_with_options(src, &Options::new().with_profile(profile))
}

fn try_html(src: &str, profile: Profile) -> Result<String, ProfileViolationError> {
    carve::try_to_html_with_options(src, &Options::new().with_profile(profile))
}

// ---- type resolution ----

#[test]
fn denies_a_type_not_in_the_allowlist() {
    let p = Profile::comment();
    assert!(!p.is_type_allowed("heading"));
    assert!(p.is_type_allowed("paragraph"));
}

#[test]
fn deny_list_beats_allow_list() {
    let p = Profile::default()
        .allow_block(Some(&["paragraph", "heading"]))
        .deny_block(&["heading"]);
    assert!(!p.is_type_allowed("heading"));
    assert!(p.is_type_allowed("paragraph"));
}

#[test]
fn null_allow_list_means_all_allowed_except_denied() {
    let p = Profile::default().deny_inline(&["raw_inline"]);
    assert!(p.is_type_allowed("emphasis"));
    assert!(!p.is_type_allowed("raw_inline"));
}

#[test]
fn document_always_allowed_unknown_denied() {
    let p = Profile::minimal();
    assert!(p.is_type_allowed("document"));
    assert!(!p.is_type_allowed("not_a_real_type"));
}

// ---- canonical type mapping ----

#[test]
fn maps_block_variants_to_canonical_names() {
    use carve::*;
    assert_eq!(
        canonical_block_type(&BlockNode::CodeBlock(CodeBlock {
            attrs: None,
            lang: None,
            title: None,
            label: None,
            content: String::new(),
            pos: None,
        })),
        Some("code_block")
    );
    assert_eq!(
        canonical_block_type(&BlockNode::ThematicBreak(ThematicBreak::default())),
        Some("thematic_break")
    );
    // An admonition keeps its own canonical name; profiles.md requires it to
    // be nameable on its own, and the deny check covers it under `div` too
    // (carve issue 362).
    assert_eq!(
        canonical_block_type(&BlockNode::Admonition(Admonition {
            attrs: None,
            kind: "note".into(),
            title: None,
            label: None,
            children: vec![],
            pos: None,
        })),
        Some("admonition")
    );
    // Outside profiles.md's vocabulary, so a profile cannot name it - but it
    // reports its own name rather than None, and resolves on its axis like any
    // unmapped type instead of being denied outright.
    assert_eq!(
        canonical_block_type(&BlockNode::AbbreviationDef(AbbreviationDef {
            abbr: "HTML".into(),
            expansion: "x".into(),
            pos: None,
        })),
        Some("abbreviation_def")
    );
}

#[test]
fn maps_inline_variants_to_canonical_names() {
    use carve::*;
    let emph = |kind| {
        InlineNode::Emphasis(Emphasis {
            attrs: None,
            kind,
            children: vec![],
            pos: None,
        })
    };
    assert_eq!(
        canonical_inline_type(&emph(EmphasisKind::Italic)),
        Some("emphasis")
    );
    assert_eq!(
        canonical_inline_type(&emph(EmphasisKind::Strong)),
        Some("strong")
    );
    assert_eq!(
        canonical_inline_type(&emph(EmphasisKind::BoldItalic)),
        Some("strong")
    );
    assert_eq!(
        canonical_inline_type(&emph(EmphasisKind::Super)),
        Some("superscript")
    );
    assert_eq!(
        canonical_inline_type(&emph(EmphasisKind::Sub)),
        Some("subscript")
    );
    assert_eq!(
        canonical_inline_type(&emph(EmphasisKind::Highlight)),
        Some("highlight")
    );
    // A tag folds into `mention` - it is not in the canonical vocabulary. An
    // autolink IS, so it keeps its own name and is covered by `link` in the
    // deny check rather than by being renamed (carve issue 362).
    assert_eq!(
        canonical_inline_type(&InlineNode::Tag(Tag {
            attrs: None,
            name: "x".into(),
            pos: None,
        })),
        Some("mention")
    );
    assert_eq!(
        canonical_inline_type(&InlineNode::AutoLink(AutoLink {
            attrs: None,
            href: "https://x".into(),
            text: "https://x".into(),
            pos: None,
        })),
        Some("autolink")
    );
    // critic insert/delete fold to insert/delete.
    assert_eq!(
        canonical_inline_type(&InlineNode::CriticInsert(CriticInsert {
            children: vec![],
            attrs: None,
            pos: None,
        })),
        Some("insert")
    );
    assert_eq!(
        canonical_inline_type(&InlineNode::CriticDelete(CriticDelete {
            children: vec![],
            attrs: None,
            pos: None,
        })),
        Some("delete")
    );
    // These are in profiles.md's inline vocabulary. They used to map to None,
    // which the resolver read as "deny", so `full()` deleted them (carve#419).
    assert_eq!(
        canonical_inline_type(&InlineNode::Symbol(Symbol {
            name: "x".into(),
            attrs: None,
            pos: None,
        })),
        Some("symbol")
    );
    assert_eq!(
        canonical_inline_type(&InlineNode::CrossRef(CrossRef {
            target: "x".into(),
            href: None,
            pos: None,
        })),
        Some("heading_ref")
    );
}

// ---- disallowed actions ----

#[test]
fn to_text_replaces_denied_inline_with_label() {
    assert_eq!(
        html("![alt](x.png)", Profile::minimal()),
        "<p>[img: alt]</p>"
    );
    assert_eq!(
        html("[text](https://x.com)", Profile::minimal()),
        "<p>text</p>"
    );
}

#[test]
fn to_text_wraps_denied_block_in_paragraph() {
    assert_eq!(html("# Title", Profile::comment()), "<p># Title</p>");
}

#[test]
fn strip_removes_denied_node_and_subtree() {
    let p = Profile::comment().on_disallowed(DisallowedAction::Strip);
    assert_eq!(
        html("text ![alt](x.png) more", p.clone()),
        "<p>text  more</p>"
    );
    assert_eq!(html("# H\n\nbody", p), "<p>body</p>");
}

#[test]
fn error_collects_violations_and_returns_err() {
    let p = Profile::comment().on_disallowed(DisallowedAction::Error);
    let err = try_html("# H", p).unwrap_err();
    assert_eq!(err.violations.len(), 1);
    assert_eq!(err.violations[0].node_type, "heading");
    assert_eq!(err.violations[0].reason, "element_not_allowed");
    assert!(err.violations[0]
        .message()
        .contains("'heading' is not allowed"));
    assert!(err.to_string().contains("'heading' is not allowed"));
}

#[test]
fn records_violations_without_error_for_to_text() {
    let result = apply_profile(
        carve::parse("# H\n\n![a](x.png)"),
        &Profile::comment(),
        None,
    )
    .unwrap();
    let mut types: Vec<String> = result
        .violations
        .iter()
        .map(|v| v.node_type.clone())
        .collect();
    types.sort();
    assert_eq!(types, vec!["heading".to_string(), "image".to_string()]);
}

#[test]
fn to_text_degrades_a_node_whose_extractor_comes_back_empty_instead_of_deleting_it() {
    // `CaptionNumber` extracts to "" (its `number` field is not text, so
    // `extract_inline_text` has nothing to say). That is a missing extractor
    // arm, not "no content" - `to_text` must not delete the node, it must
    // record a `to_text_yielded_nothing` violation and substitute the literal
    // marker `[caption_number]`.
    let doc = carve::Document {
        frontmatter: Default::default(),
        frontmatter_raw: None,
        footnote_defs: Default::default(),
        footnote_def_pos: Default::default(),
        children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
            attrs: None,
            children: vec![
                carve::InlineNode::text("see ".to_string()),
                carve::InlineNode::CaptionNumber(carve::CaptionNumber {
                    number: None,
                    pos: None,
                }),
            ],
            ..Default::default()
        })],
        source_len: 0,
        ingest_payload_len: 0,
    };
    let p = Profile::default().deny_inline(&["caption_number"]);
    let result = apply_profile(doc, &p, None).unwrap();

    assert!(
        result
            .violations
            .iter()
            .any(|v| v.node_type == "caption_number" && v.reason == "to_text_yielded_nothing"),
        "{:?}",
        result.violations
    );
    assert_eq!(result.doc.children.len(), 1, "the paragraph must survive");
    match &result.doc.children[0] {
        carve::BlockNode::Paragraph(para) => {
            let text: String = para
                .children
                .iter()
                .map(|n| match n {
                    carve::InlineNode::Text(t) => t.value.clone(),
                    _ => String::new(),
                })
                .collect();
            assert_eq!(text, "see [caption_number]");
        }
        other => panic!("expected a surviving paragraph, got {other:?}"),
    }
}

#[test]
fn to_text_still_drops_a_comment_it_cannot_degrade() {
    // A comment is the one node that genuinely renders nothing: dropping it
    // loses no visible content, so it stays a deletion, not a marker.
    let p = Profile::default().deny_block(&["comment"]);
    let out = html("text\n\n%% a note\n\nmore", p.clone());
    assert!(!out.contains("[comment]"), "{out}");
    assert!(!out.contains("note"), "{out}");

    let result = apply_profile(carve::parse("%% a note"), &p, None).unwrap();
    assert!(
        !result
            .violations
            .iter()
            .any(|v| v.reason == "to_text_yielded_nothing"),
        "{:?}",
        result.violations
    );
}

#[test]
fn filters_a_denied_figure_target_node() {
    let p = Profile::default()
        .allow_block(Some(&["paragraph", "figure"]))
        .allow_inline(Some(&["text"]))
        .deny_inline(&["image"]);
    let out = html("![alt](x.png)\n^ cap", p);
    assert!(out.contains("[img: alt]"), "{out}");
    assert!(!out.contains("<img"), "{out}");
}

#[test]
fn filters_denied_nodes_inside_a_referenced_footnote_definition() {
    let p = Profile::default().deny_inline(&["image"]);
    let out = html("text[^1]\n\n[^1]: note ![a](x.png)", p);
    assert!(out.contains("note [img: a]"), "{out}");
    assert!(!out.contains("<img"), "{out}");
}

#[test]
fn denying_table_cell_flattens_cells_to_text() {
    // table_cell / table_row are nodes; an allowlist that omits table_cell
    // denies it. carve-rs keeps the row/cell structure valid (renderer
    // requirement) but the cells carry only flattened text.
    let p = Profile::default()
        .allow_block(Some(&["table", "table_row", "paragraph"]))
        .allow_inline(None);
    let out = html("| a | b |\n|---|---|\n| 1 | 2 |", p);
    assert!(out.contains("<table>"), "{out}");
    // The strong/inline structure inside cells is gone (here cells are plain).
    assert!(
        out.contains("<td>1</td>") || out.contains("<th>a</th>"),
        "{out}"
    );
}

#[test]
fn denying_table_row_with_strip_removes_table() {
    // All rows stripped -> the table has no rows -> cleanup removes the empty
    // table entirely (parity with carve-php, which emits nothing here).
    let p = Profile::default()
        .allow_block(Some(&["table", "table_cell", "paragraph"]))
        .allow_inline(None)
        .on_disallowed(DisallowedAction::Strip);
    let out = html("| a | b |\n|---|---|\n| 1 | 2 |", p);
    assert!(out.is_empty(), "table fully removed: {out:?}");
}

#[test]
fn denying_table_row_with_error_collects_violation() {
    let p = Profile::default()
        .allow_block(Some(&["table", "paragraph"]))
        .allow_inline(None)
        .on_disallowed(DisallowedAction::Error);
    let err =
        carve::try_to_html_with_options("| a |\n|---|\n| 1 |", &Options::new().with_profile(p))
            .unwrap_err();
    assert!(
        err.violations.iter().any(|v| v.node_type == "table_row"),
        "{:?}",
        err.violations
    );
}

#[test]
fn internal_only_allows_non_http_absolute_urls_matching_php() {
    // Deliberate parity with carve-php / carve-js: internal_only only gates
    // http/https hosts; a non-http absolute scheme (e.g. ftp) that is not in
    // the dangerous-scheme deny list passes. Dangerous schemes
    // (javascript/data/file/vbscript) are still blocked by the default deny
    // list. Documented so a future "fix" does not silently diverge.
    let lp = LinkPolicy::internal_only();
    assert!(lp.is_url_allowed("ftp://evil.example/file", None));
    assert!(!lp.is_url_allowed("https://ext.com", None));
    assert!(!lp.is_url_allowed("javascript:alert(1)", None));
}

// ---- max_nesting ----

#[test]
fn flattens_list_nesting_deeper_than_limit() {
    // minimal = 2; the deepest item is converted to text in place.
    let out = html("- a\n  - b\n    - c", Profile::minimal());
    assert!(out.contains("b - c"), "{out}");
    assert!(!out.contains("<li>c</li>"), "{out}");
}

#[test]
fn max_nesting_zero_is_unlimited() {
    let p = Profile::default().set_max_nesting(0);
    let out = html("- a\n  - b\n    - c\n      - d", p);
    assert!(out.contains('d'), "{out}");
}

// ---- max_length ----

#[test]
fn errors_when_source_byte_length_exceeds_limit() {
    let p = Profile::default().set_max_length(5);
    let err = try_html("hello world", p).unwrap_err();
    assert!(err.to_string().contains("maximum length"), "{}", err);
}

#[test]
fn allows_input_within_the_length_limit() {
    let p = Profile::default().set_max_length(100);
    assert_eq!(html("hi", p), "<p>hi</p>");
}

#[test]
fn untrusted_presets_carry_a_default_length_cap() {
    assert_eq!(Profile::comment().max_length(), Profile::COMMENT_MAX_LENGTH);
    assert_eq!(Profile::minimal().max_length(), Profile::MINIMAL_MAX_LENGTH);
    // Trusted presets stay unlimited.
    assert_eq!(Profile::full().max_length(), 0);
    assert_eq!(Profile::article().max_length(), 0);
}

#[test]
fn comment_preset_rejects_an_over_cap_body() {
    let over = "a".repeat(Profile::COMMENT_MAX_LENGTH + 1);
    let err = try_html(&over, Profile::comment()).unwrap_err();
    assert!(err.to_string().contains("maximum length"), "{}", err);
    // A short comment still renders.
    assert_eq!(html("hi there", Profile::comment()), "<p>hi there</p>");
}

#[test]
fn a_preset_cap_is_overridable() {
    let long = "word ".repeat(Profile::MINIMAL_MAX_LENGTH);
    let p = Profile::minimal().set_max_length(0);
    assert!(carve::try_to_html_with_options(&long, &Options::new().with_profile(p)).is_ok());
}

#[test]
fn max_length_blocks_infallible_render_before_hooks() {
    use std::cell::Cell;

    struct CountingHook<'a>(&'a Cell<usize>);

    impl carve::CarveExtension for CountingHook<'_> {
        fn name(&self) -> &'static str {
            "counting-hook"
        }

        fn before_render(
            &self,
            doc: carve::Document,
            _ctx: &carve::BeforeRenderContext<'_>,
        ) -> carve::Document {
            self.0.set(self.0.get() + 1);
            doc
        }
    }

    let calls = Cell::new(0);
    let hook = CountingHook(&calls);
    let options = Options::new()
        .with_extension(&hook)
        .with_profile(Profile::default().set_max_length(1));

    assert_eq!(carve::to_html_with_options("too long", &options), "");
    assert_eq!(carve::to_markdown_with_options("too long", &options), "");
    assert_eq!(calls.get(), 0);
}

// ---- link policy ----

#[test]
fn blocks_dangerous_schemes_by_default() {
    let lp = LinkPolicy::unrestricted();
    assert!(!lp.is_url_allowed("javascript:alert(1)", None));
    assert!(!lp.is_url_allowed("data:text/html,x", None));
    assert!(lp.is_url_allowed("https://ok.com", None));
}

#[test]
fn internal_only_blocks_external_keeps_relative_and_fragment() {
    let lp = LinkPolicy::internal_only();
    assert!(!lp.is_url_allowed("https://ext.com", None));
    assert!(lp.is_url_allowed("/local", None));
    assert!(lp.is_url_allowed("#sec", None));
}

#[test]
fn allowlist_permits_listed_domains_and_subdomains_only() {
    let lp = LinkPolicy::allowlist(vec!["good.com".into()]);
    assert!(lp.is_url_allowed("https://good.com/p", None));
    assert!(lp.is_url_allowed("https://a.good.com/p", None));
    assert!(!lp.is_url_allowed("https://bad.com/p", None));
}

#[test]
fn comment_profile_adds_nofollow_ugc_rel_to_surviving_links() {
    assert_eq!(
        html("[text](https://x.com)", Profile::comment()),
        "<p><a href=\"https://x.com\" rel=\"nofollow ugc\">text</a></p>"
    );
}

#[test]
fn merges_rel_onto_an_existing_rel_attribute() {
    assert_eq!(
        html(
            "[text](https://x.com){.cls #id rel=\"me\"}",
            Profile::comment()
        ),
        "<p><a href=\"https://x.com\" class=\"cls\" id=\"id\" rel=\"me nofollow ugc\">text</a></p>"
    );
}

#[test]
fn a_denied_link_url_follows_the_disallowed_action() {
    let p = Profile::full().set_link_policy(Some(LinkPolicy::internal_only()));
    assert_eq!(html("[x](https://ext.com)", p), "<p>x</p>");
}

// ---- presets ----

#[test]
fn full_allows_everything() {
    let p = Profile::full();
    assert!(p.is_type_allowed("raw_block"));
    assert!(p.is_type_allowed("heading"));
    assert!(p.is_type_allowed("math"));
}

#[test]
fn article_denies_only_raw_block_inline() {
    let p = Profile::article();
    assert!(!p.is_type_allowed("raw_block"));
    assert!(!p.is_type_allowed("raw_inline"));
    assert!(p.is_type_allowed("heading"));
    assert!(p.is_type_allowed("table"));
}

#[test]
fn comment_allowlist_denies_headings_images_tables_footnotes() {
    let p = Profile::comment();
    for t in [
        "heading",
        "image",
        "table",
        "footnote_ref",
        "div",
        "thematic_break",
    ] {
        assert!(!p.is_type_allowed(t), "{t} should be denied");
    }
    for t in [
        "paragraph",
        "list",
        "block_quote",
        "code_block",
        "link",
        "highlight",
    ] {
        assert!(p.is_type_allowed(t), "{t} should be allowed");
    }
}

#[test]
fn minimal_denies_link_image_highlight_keeps_paragraphs_lists() {
    let p = Profile::minimal();
    assert!(!p.is_type_allowed("link"));
    assert!(!p.is_type_allowed("image"));
    assert!(!p.is_type_allowed("highlight"));
    assert!(p.is_type_allowed("paragraph"));
    assert!(p.is_type_allowed("list"));
    assert!(!p.is_type_allowed("block_quote"));
}

#[test]
fn applies_to_non_html_renderers_too() {
    let md = carve::to_markdown_with_options(
        "# Title",
        &Options::new().with_profile(Profile::comment()),
    );
    assert!(md.contains("# Title"), "{md}");
    let plain = carve::to_plain_text_with_options(
        "![a](x.png)",
        &Options::new().with_profile(Profile::minimal()),
    );
    assert!(plain.contains("[img: a]"), "{plain}");
}

// ---- golden parity with carve-php ----
//
// Expected strings were produced by carve-php (and verified byte-identical to
// carve-rs' own HTML for these inputs):
//
//   printf '%s' INPUT | php -r 'require ".../vendor/autoload.php";
//     echo (new Carve\CarveConverter(profile: Carve\Profile::PRESET()))
//       ->convert(file_get_contents("php://stdin"));'

struct Golden {
    preset: fn() -> Profile,
    src: &'static str,
    out: &'static str,
}

#[test]
fn golden_parity_with_carve_php() {
    let cases = [
        // article: raw block disabled, everything else passes.
        Golden {
            preset: Profile::article,
            src: "``` =html\n<b>x</b>\n```",
            out: "<p>&lt;b&gt;x&lt;/b&gt;</p>",
        },
        // comment: headings/images/tables -> to_text, links get nofollow ugc.
        Golden {
            preset: Profile::comment,
            src: "# Hello world",
            out: "<p># Hello world</p>",
        },
        Golden {
            preset: Profile::comment,
            src: "![alt text](img.png)",
            out: "<p>[img: alt text]</p>",
        },
        Golden {
            preset: Profile::comment,
            src: "![](img.png)",
            out: "<p>[img]</p>",
        },
        Golden {
            preset: Profile::comment,
            src: "| a | b |\n|---|---|\n| 1 | 2 |",
            out: "<p>a | b<br>\n1 | 2</p>",
        },
        Golden {
            preset: Profile::comment,
            src: "[text](https://example.com)",
            out: "<p><a href=\"https://example.com\" rel=\"nofollow ugc\">text</a></p>",
        },
        Golden {
            preset: Profile::comment,
            src: "[home](/home)",
            out: "<p><a href=\"/home\" rel=\"nofollow ugc\">home</a></p>",
        },
        Golden {
            preset: Profile::comment,
            src: "``` =html\n<b>x</b>\n```",
            out: "<p>&lt;b&gt;x&lt;/b&gt;</p>",
        },
        Golden {
            preset: Profile::comment,
            src: "- one\n- two",
            out: "<ul>\n  <li>one</li>\n  <li>two</li>\n</ul>",
        },
        Golden {
            preset: Profile::comment,
            src: "> quoted text",
            out: "<blockquote><p>quoted text</p></blockquote>",
        },
        Golden {
            preset: Profile::comment,
            src: "`inline code`",
            out: "<p><code>inline code</code></p>",
        },
        // minimal: links/images -> to_text, blockquote -> to_text.
        Golden {
            preset: Profile::minimal,
            src: "[text](https://example.com)",
            out: "<p>text</p>",
        },
        Golden {
            preset: Profile::minimal,
            src: "![alt text](img.png)",
            out: "<p>[img: alt text]</p>",
        },
        Golden {
            preset: Profile::minimal,
            src: "> quoted text",
            out: "<p>&gt; quoted text</p>",
        },
        Golden {
            preset: Profile::minimal,
            src: "- one\n- two",
            out: "<ul>\n  <li>one</li>\n  <li>two</li>\n</ul>",
        },
        // full: passthrough.
        Golden {
            preset: Profile::full,
            src: "Just a paragraph with *bold* and /italic/.",
            out: "<p>Just a paragraph with <strong>bold</strong> and <em>italic</em>.</p>",
        },
    ];
    for case in cases {
        assert_eq!(
            html(case.src, (case.preset)()),
            case.out,
            "input: {:?}",
            case.src
        );
    }
}

// ---- frontmatter deniability (carve#422) ----
//
// `frontmatter` is in the normative Block vocabulary (docs/profiles.md), so a
// profile can name it, but carve-rs keeps it on `Document.frontmatter_raw` /
// `Document.frontmatter` rather than as a walkable child. Before the fix,
// `deny_block(["frontmatter"])` was a silent no-op: no error, no violation,
// metadata stayed.
//
// The rendered HTML is byte-identical whether frontmatter is allowed or
// denied - frontmatter renders nothing either way, exactly like `comment`
// (docs/profiles.md, "Some types are deniable in the tree but invisible in
// rendered output"). So these tests assert on the serialized AST and the
// violation report, never on the HTML "changing", and pin the HTML
// byte-identity itself as a regression guard.

const FRONTMATTER_SOURCE: &str = "---\ntitle: Secret\n---\n\nBody.\n";

#[test]
fn full_profile_keeps_frontmatter_in_the_serialized_ast_with_no_violation() {
    let doc = carve::parse(FRONTMATTER_SOURCE);
    let result = apply_profile(doc, &Profile::full(), None).unwrap();

    assert!(result.doc.frontmatter_raw.is_some());
    let json = carve::to_json(&result.doc);
    assert!(json.contains("\"type\":\"frontmatter\""), "{json}");
    assert!(
        !result
            .violations
            .iter()
            .any(|v| v.node_type == "frontmatter"),
        "{:?}",
        result.violations
    );
}

#[test]
fn denying_frontmatter_strips_it_from_the_serialized_ast_and_records_one_violation() {
    let doc = carve::parse(FRONTMATTER_SOURCE);
    let profile = Profile::default().deny_block(&["frontmatter"]);
    let result = apply_profile(doc, &profile, None).unwrap();

    assert!(result.doc.frontmatter_raw.is_none());
    assert!(result.doc.frontmatter.is_empty());
    let json = carve::to_json(&result.doc);
    assert!(!json.contains("frontmatter"), "{json}");

    let violations: Vec<_> = result
        .violations
        .iter()
        .filter(|v| v.node_type == "frontmatter")
        .collect();
    assert_eq!(violations.len(), 1, "{:?}", result.violations);
    assert_eq!(violations[0].reason, "element_not_allowed");
}

#[test]
fn denying_frontmatter_does_not_change_the_rendered_html() {
    let allowed = html(FRONTMATTER_SOURCE, Profile::full());
    let denied = html(
        FRONTMATTER_SOURCE,
        Profile::default().deny_block(&["frontmatter"]),
    );

    assert_eq!(allowed, "<p>Body.</p>");
    assert_eq!(denied, "<p>Body.</p>");
    assert_eq!(allowed, denied);
}

#[test]
fn denying_frontmatter_strips_a_programmatically_built_map_with_no_raw_form() {
    // `Document`'s fields are public, so a caller can build one with only the
    // `frontmatter` map populated (no `frontmatter_raw`) - e.g. an extension
    // that injects metadata directly. `render_carve` serializes that map
    // independently of the raw form, so the denial must clear it too, not
    // only the `frontmatter_raw` field the parser normally sets alongside it.
    let doc = carve::Document {
        frontmatter: std::collections::BTreeMap::from([(
            "title".to_string(),
            "Secret".to_string(),
        )]),
        frontmatter_raw: None,
        footnote_defs: Default::default(),
        footnote_def_pos: Default::default(),
        children: vec![carve::BlockNode::Paragraph(carve::Paragraph {
            attrs: None,
            children: vec![carve::InlineNode::text("Body.".to_string())],
            ..Default::default()
        })],
        source_len: 0,
        ingest_payload_len: 0,
    };
    let profile = Profile::default().deny_block(&["frontmatter"]);
    let result = apply_profile(doc, &profile, None).unwrap();

    assert!(result.doc.frontmatter.is_empty());
    assert_eq!(
        result
            .violations
            .iter()
            .filter(|v| v.node_type == "frontmatter")
            .count(),
        1,
        "{:?}",
        result.violations
    );
}

#[test]
fn frontmatter_metadata_never_reaches_rendered_html_either_way() {
    let allowed = html(FRONTMATTER_SOURCE, Profile::full());
    let denied = html(
        FRONTMATTER_SOURCE,
        Profile::default().deny_block(&["frontmatter"]),
    );

    assert!(!allowed.contains("Secret"), "{allowed}");
    assert!(!allowed.contains("title"), "{allowed}");
    assert!(!denied.contains("Secret"), "{denied}");
    assert!(!denied.contains("title"), "{denied}");
}
