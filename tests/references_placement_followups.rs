use carve::{
    lint_carve, lint_carve_with_options, to_html_with_options, Citations, ListTable, Options,
};

const CONTAINED: &str = "See [@x].\n\n> ::: references\n> :::\n\n[@x]: Source\n";

#[test]
fn contained_references_warn_only_with_citations() {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    let warnings = lint_carve_with_options(CONTAINED, &options);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].rule, "references-placement-in-container");
    assert_eq!((warnings[0].line, warnings[0].column), (3, 3));
    assert!(lint_carve(CONTAINED).is_empty());
}

#[test]
fn a_top_level_marker_remains_valid() {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    let source = "See [@x].\n\n::: references\n:::\n\n[@x]: Source\n";
    assert!(lint_carve_with_options(source, &options).is_empty());
}

#[test]
fn list_and_footnote_bodies_are_containers() {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    let list = "- ::: references\n  :::\n";
    let footnote = "Note[^a].\n\n[^a]: body\n\n    ::: references\n    :::\n";
    for source in [list, footnote] {
        let warnings = lint_carve_with_options(source, &options);
        assert_eq!(warnings.len(), 1, "{source:?}: {warnings:?}");
        assert_eq!(warnings[0].rule, "references-placement-in-container");
    }
}

#[test]
fn a_list_table_cell_is_a_container() {
    let citations = Citations::new();
    let list_table = ListTable::new();
    let options = Options::new()
        .with_extension(&citations)
        .with_extension(&list_table);
    let source = "::: list-table\n- - ::: references\n    :::\n  - B\n:::\n";
    let warnings = lint_carve_with_options(source, &options);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].rule, "references-placement-in-container");
}

#[test]
fn a_nested_marker_does_not_consume_a_later_top_level_marker() {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    let source =
        "See [@x].\n\n> ::: references\n> :::\n\n::: references\n:::\n\n## After\n\n[@x]: Source\n";
    let warnings = lint_carve_with_options(source, &options);
    assert_eq!(warnings.len(), 1, "{warnings:?}");

    let html = to_html_with_options(source, &options);
    assert!(html.contains("<div class=\"references\">\n  <ol class=\"references\">\n    <li id=\"ref-x\">Source</li>\n  </ol>\n</div>"), "{html}");
    assert!(
        html.find("<ol class=\"references\">").unwrap()
            < html.find("<section id=\"After\">").unwrap()
    );
}
