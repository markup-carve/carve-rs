//! carve#1468 / carve#1469: a Tier-3 extension that writes an element writes its
//! accessible NAME too. Each shape below had a role, or a visible label on its
//! parts, and nothing a reader could use to tell the whole from the next one.

use carve::extensions::{CodeGroup, FencedRender, FencedRenderOptions, Index, Tabs, TabsMode};
use carve::Options;

fn html(src: &str, ext: &dyn carve::CarveExtension) -> String {
    let mut o = Options::new();
    o.extensions.push(ext);
    carve::to_html_with_options(src, &o)
}

fn html_with_labels(src: &str, ext: &dyn carve::CarveExtension, key: &str, value: &str) -> String {
    let mut o = Options::new();
    o.extensions.push(ext);
    o.labels.insert(key.to_string(), value.to_string());
    carve::to_html_with_options(src, &o)
}

#[test]
fn a_lone_index_back_link_is_named_by_label_and_term() {
    let out = html("A :index[widget] here.\n\n::: index\n:::\n", &Index::new());
    assert!(
        out.contains(
            "<a href=\"#idx-widget-1\" class=\"index-backref\" aria-label=\"Back to widget\">\u{21a9}</a>"
        ),
        "{out}"
    );
}

/// The whole point: an index entry carries ONE back-link per occurrence, so
/// without the ordinal a reader meets a row of identical unnamed arrows. PART 9
/// §16's rule is mirrored - the name is the label plus what the link VISIBLY
/// says - so the ordinal appears in both (WCAG 2.5.3).
#[test]
fn the_kth_back_link_is_numbered_visibly_and_in_its_name() {
    let out = html(
        "A :index[widget] and :index[widget] again.\n\n::: index\n:::\n",
        &Index::new(),
    );
    assert!(
        out.contains("aria-label=\"Back to widget 1\">\u{21a9}<sup>1</sup></a>"),
        "{out}"
    );
    assert!(
        out.contains("aria-label=\"Back to widget 2\">\u{21a9}<sup>2</sup></a>"),
        "{out}"
    );
}

#[test]
fn the_tab_set_is_named_without_inventing_roles_the_css_mode_cannot_honor() {
    let src = ":::: tabs\n\n::: tab [One]\na\n:::\n\n::::\n";
    let out = html(src, &Tabs::new());
    assert!(
        out.contains("<div class=\"tabs\" role=\"group\" aria-label=\"Tabs\">"),
        "{out}"
    );
}

#[test]
fn aria_mode_keeps_tablist_and_gains_the_missing_name() {
    let src = ":::: tabs\n\n::: tab [One]\na\n:::\n\n::::\n";
    let opts = carve::extensions::TabsOptions {
        mode: TabsMode::Aria,
        ..Default::default()
    };
    let out = html(src, &Tabs::with_options(opts));
    assert!(
        out.contains("<div class=\"tabs\" role=\"tablist\" aria-label=\"Tabs\">"),
        "{out}"
    );
}

#[test]
fn the_code_group_names_itself_rather_than_redirecting_to_tabs() {
    let src = "::: code-group\n\n``` php [PHP]\n1\n```\n\n:::\n";
    let out = html(src, &CodeGroup::new());
    assert!(
        out.contains("<div class=\"code-group\" role=\"group\" aria-label=\"Code examples\">"),
        "{out}"
    );
}

#[test]
fn a_diagram_fence_is_an_image_with_a_name() {
    let out = html(
        "``` mermaid\ngraph TD;\n```\n",
        &FencedRender::new("mermaid"),
    );
    assert!(
        out.contains("<pre class=\"mermaid\" role=\"img\" aria-label=\"mermaid\">"),
        "{out}"
    );
}

/// An `img` with no accessible name is SKIPPED, which is worse than the source
/// being read out - so an empty label removes the role as well.
#[test]
fn role_and_name_are_written_together_or_not_at_all() {
    let opts = FencedRenderOptions {
        label: String::new(),
        ..FencedRenderOptions::new(
            vec!["mermaid".to_string()],
            None,
            None,
            carve::extensions::ContentMode::Text,
        )
    };
    let out = html(
        "``` mermaid\ngraph TD;\n```\n",
        &FencedRender::with_options(opts),
    );
    assert!(out.contains("<pre class=\"mermaid\">"), "{out}");
    assert!(!out.contains("role=\"img\""), "{out}");
}

/// The author who cared enough to NAME the fence is exactly the one who must not
/// lose the role: without it the source is still announced as prose.
#[test]
fn an_authored_name_still_gets_the_role() {
    let out = html(
        "{aria-label=\"Deploy flow\"}\n``` mermaid\ngraph TD;\n```\n",
        &FencedRender::new("mermaid"),
    );
    assert!(out.contains("aria-label=\"Deploy flow\""), "{out}");
    assert!(out.contains("role=\"img\""), "{out}");
    assert!(!out.contains("aria-label=\"mermaid\""), "{out}");
}

#[test]
fn the_authors_own_name_wins_under_any_ascii_casing() {
    let src = "{ARIA-LABEL=\"Mine\"}\n:::: tabs\n\n::: tab [One]\na\n:::\n\n::::\n";
    let out = html(src, &Tabs::new());
    assert!(out.contains("ARIA-LABEL=\"Mine\""), "{out}");
    assert!(!out.contains("aria-label=\"Tabs\""), "{out}");
}

/// ONE labels map localizes every engine-written string. With a per-extension
/// option as the only spelling, switching a document to German meant finding
/// several call sites and silently missing one; PART 9 §16a forbids making a
/// host configure the same text twice.
#[test]
fn one_labels_map_reaches_every_extension() {
    let out = html_with_labels(
        "A :index[Gerät] hier.\n\n::: index\n:::\n",
        &Index::new(),
        "indexBackref",
        "Zurück zu",
    );
    assert!(out.contains("aria-label=\"Zurück zu Gerät\""), "{out}");

    let out = html_with_labels(
        ":::: tabs\n\n::: tab [Eins]\na\n:::\n\n::::\n",
        &Tabs::new(),
        "tabsGroup",
        "Registerkarten",
    );
    assert!(out.contains("aria-label=\"Registerkarten\""), "{out}");

    let out = html_with_labels(
        "::: code-group\n\n``` php [PHP]\n1\n```\n\n:::\n",
        &CodeGroup::new(),
        "codeGroup",
        "Codebeispiele",
    );
    assert!(out.contains("aria-label=\"Codebeispiele\""), "{out}");
}

#[test]
fn an_extension_option_overrides_the_map_for_that_instance() {
    let out = html_with_labels(
        "A :index[widget] here.\n\n::: index\n:::\n",
        &Index::new().with_backref_label("Explicit"),
        "indexBackref",
        "Zurück zu",
    );
    assert!(out.contains("aria-label=\"Explicit widget\""), "{out}");
    assert!(!out.contains("Zurück zu"), "{out}");
}
