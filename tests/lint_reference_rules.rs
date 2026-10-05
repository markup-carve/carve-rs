use carve::lint_carve;

fn has(source: &str, rule: &str) -> bool {
    lint_carve(source)
        .iter()
        .any(|warning| warning.rule == rule)
}

#[test]
fn unresolved_references_and_heading_collisions_are_reported() {
    for (source, rule) in [
        ("# A\n\n# A\n", "duplicate-heading-id"),
        ("see </#nope>\n", "broken-crossref"),
        ("see [text][nope]\n", "unresolved-reference-link"),
        ("see[^nope]\n", "unresolved-footnote"),
        ("text\n\n[^a]: never used\n", "unused-footnote-definition"),
        (
            "x[^a]\n\n[^a]: one\n\n[^a]: two\n",
            "duplicate-footnote-definition",
        ),
        (
            "see [^a b]\n\n[^a b]: one\n\n[^a  b]: two\n",
            "footnote-labels-differ-only-in-whitespace",
        ),
    ] {
        assert!(has(source, rule), "{rule}: {source:?}");
    }
}

#[test]
fn resolved_references_are_not_reported() {
    let source =
        "# Target\n\nsee </#Target> [Target][] [text][r] and [^a b]\n\n[r]: u\n\n[^a  b]: note\n";
    let warnings = lint_carve(source);
    for rule in [
        "broken-crossref",
        "unresolved-reference-link",
        "unresolved-footnote",
        "unused-footnote-definition",
    ] {
        assert!(
            !warnings.iter().any(|w| w.rule == rule),
            "{rule}: {warnings:?}"
        );
    }
}

#[test]
fn definition_examples_in_code_comments_and_literal_prose_are_not_duplicates() {
    for body in [
        "```\n[^a]: sample\n```",
        "%%%\n[^a]: sample\n%%%",
        "\\[^a]: sample",
        "`[^a]: sample`",
    ] {
        let source = format!("x[^a]\n\n[^a]: note\n\n{body}\n");
        assert!(!has(&source, "duplicate-footnote-definition"), "{source}");
    }
}

#[test]
fn nested_references_and_original_unicode_byte_offsets_are_checked() {
    let source = "😀\r\n\r\n> see </#missing>\r\n";
    let warning = lint_carve(source)
        .into_iter()
        .find(|w| w.rule == "broken-crossref")
        .unwrap();
    assert_eq!(warning.line, 3);
    assert_eq!(&source[warning.start..warning.end], "</#missing>");
    assert!(has("[^a]: see[^missing]\n", "unresolved-footnote"));
}

#[test]
fn repeated_definitions_inside_quotes_keep_the_authored_location() {
    let source = "x[^a]\r\n\r\n> [^a]: one\r\n\r\n> [^a]: two\r\n";
    let warning = lint_carve(source)
        .into_iter()
        .find(|w| w.rule == "duplicate-footnote-definition")
        .unwrap();
    assert_eq!(warning.line, 5);
    assert_eq!(warning.column, 3);
    assert_eq!(&source[warning.start..warning.end], "[^a]:");
}

#[test]
fn reference_checks_use_the_renderers_heading_id_options() {
    let options = carve::Options::default().with_lowercase_heading_ids(true);
    let warnings =
        carve::lint_carve_with_options("# TARGET\n\nsee </#target> [TARGET][]\n", &options);
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(w.rule, "broken-crossref" | "unresolved-reference-link")),
        "{warnings:?}"
    );
}

#[test]
fn heading_collisions_follow_assigned_ids_and_document_order() {
    for (source, line) in [
        ("# A\n\n{#A}\n# X\n", 1),
        ("x[^n]\n\n[^n]: text\n\n  # A\n\n# A\n", 5),
        ("{#same}\n# First\n\n{#same}\n# Second\n", 5),
    ] {
        let warnings: Vec<_> = lint_carve(source)
            .into_iter()
            .filter(|w| w.rule == "duplicate-heading-id")
            .collect();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].line, line, "{warnings:?}");
    }
}

#[test]
fn nested_list_definitions_are_checked() {
    let source = "x[^a]\n\n- [^a]: one\n\n- - [^a]: two\n";
    let warning = lint_carve(source)
        .into_iter()
        .find(|w| w.rule == "duplicate-footnote-definition")
        .unwrap();
    assert_eq!(warning.line, 5);
    assert_eq!(warning.column, 5);
    assert_eq!(&source[warning.start..warning.end], "[^a]:");
}

fn rules(source: &str) -> Vec<&'static str> {
    lint_carve(source).into_iter().map(|w| w.rule).collect()
}

#[test]
fn a_crossref_to_an_id_on_another_element_names_it() {
    for (source, kind, id) in [
        ("{#para}\nA para.\n\nSee </#para>.\n", "paragraph", "para"),
        (
            "{#tbl}\n| A |\n|---|\n| 1 |\n\nSee </#tbl>.\n",
            "table",
            "tbl",
        ),
        ("[x]{#Spot}\n\nSee </#Spot>.\n", "span", "Spot"),
    ] {
        let warnings = lint_carve(source);
        assert_eq!(
            warnings.iter().map(|w| w.rule).collect::<Vec<_>>(),
            ["broken-crossref"],
            "{source:?}"
        );
        assert!(
            warnings[0]
                .message
                .contains(&format!("which is on a {kind}")),
            "{warnings:?}"
        );
        assert!(
            warnings[0].message.contains(&format!("[text](#{id})")),
            "{warnings:?}"
        );
    }
    assert!(lint_carve("See </#nope>.\n")[0]
        .message
        .contains("has no matching heading id"));
}

#[test]
fn a_fragment_link_that_matches_no_id_is_reported_at_the_link() {
    let source = "# Intro\n\nSee [bad](#nope).\n";
    let warnings = lint_carve(source);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].rule, "broken-fragment-link");
    assert!(warnings[0].message.contains("\"#nope\""));
    assert_eq!((warnings[0].line, warnings[0].column), (3, 5));
    assert_eq!(&source[warnings[0].start..warnings[0].end], "[bad](#nope)");
}

#[test]
fn a_broken_fragment_link_is_found_in_every_container() {
    for source in [
        "> [x](#nope)\n",
        "- [x](#nope)\n",
        "Text[^n].\n\n[^n]: [x](#nope)\n",
        "[x][r]\n\n[r]: #nope\n",
    ] {
        assert_eq!(rules(source), ["broken-fragment-link"], "{source:?}");
    }
}

#[test]
fn a_link_in_an_unreferenced_footnote_definition_is_not_checked() {
    assert_eq!(
        rules("[^u]: see [x](#nope)\n\nBody.\n"),
        ["unused-footnote-definition"]
    );
}

#[test]
fn a_case_only_near_miss_is_named() {
    let warnings = lint_carve("# Getting Started\n\n[x](#getting-started)\n");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0]
        .message
        .contains("\"Getting-Started\" differs only in case"));
}

#[test]
fn fragment_links_to_rendered_ids_are_not_reported() {
    for source in [
        "# Getting Started\n\n[x](#Getting-Started)\n",
        "# A\n\n# A\n\n[x](#A-2)\n",
        "# Über uns\n\n[x](#%C3%9Cber-uns) [y](#Über-uns)\n",
        "{#tbl}\n| A |\n|---|\n| 1 |\n\n[x](#tbl)\n",
        "[x]{#sp}\n\n[y](#sp)\n",
        "Text[^n].\n\n[^n]: Back to [ref](#fnref1).\n\n[x](#fn1)\n",
        "``` =html\n<div id=\"raw\"></div>\n```\n\n[x](#raw)\n",
        "``` =html\n<a name=\"old\"></a>\n```\n\n[x](#old)\n",
        "[x](#top) [y](#)\n",
        "[x](#:~:text=word)\n",
        "# Intro\n\n[x](#Intro:~:text=word)\n",
        "[x](other.crv#nope) [y](https://example.com/#nope)\n",
        "# Intro\n\nsee </#Intro> and [Intro][]\n",
    ] {
        assert!(
            !rules(source).contains(&"broken-fragment-link"),
            "{source:?}: {:?}",
            lint_carve(source)
        );
    }
    let options = carve::Options::default().with_lowercase_heading_ids(true);
    assert!(carve::lint_carve_with_options(
        "# Getting Started\n\n[x](#getting-started)\n",
        &options
    )
    .is_empty());
}

#[test]
fn ids_only_spelled_inside_raw_html_text_do_not_count() {
    for html in [
        "<div title=\" id=phantom\"></div>",
        "<!-- <div id=\"phantom\"></div> -->",
    ] {
        let source = format!("``` =html\n{html}\n```\n\n[x](#phantom)\n");
        assert_eq!(rules(&source), ["broken-fragment-link"], "{source:?}");
    }
    assert_eq!(
        rules("``` html\n<div id=\"raw\"></div>\n```\n\n[x](#raw)\n"),
        ["broken-fragment-link"]
    );
}

#[test]
fn raw_html_ids_are_read_the_way_a_browser_decodes_them() {
    let source = "``` =html\n<div title=\">\" id=\"r&amp;d\"></div><p id=\"&#1114112;\"></p>\n```\n\n[x](#r&d) [y](#nope)\n";
    assert_eq!(rules(source), ["broken-fragment-link"]);
    let deep = format!(
        "``` =html\n{}<p id=\"deep\"></p>\n```\n\n[x](#deep) [y](#nope)\n",
        "<div>".repeat(10000)
    );
    assert_eq!(rules(&deep), ["broken-fragment-link"]);
}

#[test]
fn fragment_links_follow_the_registered_extensions() {
    let citations = carve::Citations::new();
    let options = carve::Options::new().with_extension(&citations);
    let found: Vec<_> = carve::lint_carve_with_options("[x](#ref-smith) [y](#nope)\n", &options)
        .into_iter()
        .map(|w| w.rule)
        .collect();
    assert_eq!(found, ["broken-fragment-link"]);
    let unused = "[@a]: [Entry]{#entry}\n\n[x](#entry)\n";
    let found: Vec<_> = carve::lint_carve_with_options(unused, &options)
        .into_iter()
        .map(|w| w.rule)
        .collect();
    assert!(found.contains(&"broken-fragment-link"), "{found:?}");
    let tabs = carve::Tabs::new();
    let options = carve::Options::new().with_extension(&tabs);
    assert!(carve::lint_carve_with_options("[x](#tab-1)\n", &options).is_empty());
}
