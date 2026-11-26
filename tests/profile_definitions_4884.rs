//! Profile reports of definitions (C2-H-163, C2-H-178, carve#6876): an
//! allow-list profile reports every definition it leaves out except an
//! `abbreviation_def`, from source and from a supplied tree alike.

use carve::api::{self, Options, Profile};

fn reports(rendered: &api::Rendered) -> Vec<String> {
    rendered
        .diagnostics
        .iter()
        .filter(|d| d.code == "profile-construct-degraded")
        .map(|d| d.message.clone())
        .collect()
}

fn opts(profile: Profile) -> Options {
    Options {
        profile,
        ..Options::default()
    }
}

#[test]
fn snippet_definition_is_reported_and_writes_nothing() {
    for (profile, name) in [(Profile::Comment, "comment"), (Profile::Minimal, "minimal")] {
        let rendered =
            api::render_with_diagnostics("{$x}: Hi\n\nUse it.\n", &opts(profile)).expect("renders");
        assert_eq!(rendered.html, "<p>Use it.</p>");
        assert_eq!(
            reports(&rendered),
            [format!(
                "Degraded a construct the profile \"{name}\" does not allow: snippet_definition."
            )]
        );
        let range = rendered.diagnostics[0].range.as_ref().expect("a range");
        assert_eq!((range.start_offset, range.end_offset), (0, 8));
    }
}

#[test]
fn snippet_definition_inside_the_profile_is_not_reported() {
    let rendered =
        api::render_with_diagnostics("{$x}: Hi\n\nA {{ $x }} b.\n", &opts(Profile::Full))
            .expect("renders");
    assert_eq!(rendered.html, "<p>A Hi b.</p>");
    assert!(reports(&rendered).is_empty());
}

#[test]
fn abbreviation_definition_stays_unreported() {
    let rendered =
        api::render_with_diagnostics("*[HTML]: Hyper\n\nSome HTML.\n", &opts(Profile::Comment))
            .expect("renders");
    assert_eq!(
        reports(&rendered),
        ["Degraded a construct the profile \"comment\" does not allow: abbreviation."]
    );
}

#[test]
fn tree_render_names_the_profile_and_the_construct() {
    let json = r#"{"astVersion":"2.0","type":"document","children":[{"type":"citation_definition","key":"k","namespace":"citation","children":[{"type":"text","value":"Smith 2020"}]},{"type":"paragraph","children":[{"type":"text","value":"A b."}]}]}"#;
    let tree = api::decode_tree(json).expect("the tree decodes");
    let rendered =
        api::render_tree_with_diagnostics(&tree, &opts(Profile::Minimal)).expect("renders");
    // C2-A-119: a citation_definition writes nothing, degraded or not.
    assert_eq!(rendered.html, "<p>A b.</p>");
    assert_eq!(
        reports(&rendered),
        ["Degraded a construct the profile \"minimal\" does not allow: citation_definition."]
    );
}

#[test]
fn tree_render_keeps_the_nesting_message_and_the_range() {
    let source = "- a\n  - b\n    - c\n";
    let tree = api::parse(source, &Options::default()).expect("parses");
    let rendered =
        api::render_tree_with_diagnostics(&tree, &opts(Profile::Minimal)).expect("renders");
    let loss = rendered
        .diagnostics
        .iter()
        .find(|d| d.code == "profile-construct-degraded")
        .expect("the third list is past the bound");
    assert!(
        loss.message.starts_with(
            "Degraded list because it exceeds the nesting limit of profile \"minimal\""
        )
    );
    let range = loss.range.as_ref().expect("a range");
    assert_eq!(range.start_offset, 14);
}
