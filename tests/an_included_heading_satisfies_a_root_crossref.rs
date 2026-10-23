use carve::{
    lint_carve_with_options, IncludeContext, IncludeDenial, IncludeOptions, IncludeResolved,
    Options,
};

fn crossref_findings(entry: &str) -> Vec<String> {
    let resolver = |path: &str, _: &IncludeContext<'_>| -> Result<IncludeResolved, IncludeDenial> {
        match path {
            "part.crv" => Ok(IncludeResolved::from("# Inc\n\nBody.\n".to_string())),
            _ => Err(IncludeDenial::Unresolved),
        }
    };
    let options = Options::new().with_lint_includes(
        IncludeOptions::new()
            .with_resolver(&resolver)
            .with_source_path("page.crv"),
    );
    lint_carve_with_options(entry, &options)
        .into_iter()
        .filter(|warning| warning.rule == "broken-crossref")
        .map(|warning| warning.message)
        .collect()
}

#[test]
fn an_included_heading_satisfies_a_root_crossref() {
    assert!(crossref_findings("# T\n\n{{ part.crv }}\n\nSee </#Inc>.\n").is_empty());
}

#[test]
fn a_crossref_no_include_supplies_is_still_reported() {
    let findings = crossref_findings("# T\n\n{{ part.crv }}\n\nSee </#Nope>.\n");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].contains("</#Nope>"));
}
