//! PART 9 §16a AN IMPORTER DOES NOT BAKE A DERIVED NAME INTO SOURCE
//! (markup-carve/carve#1500, reconciled with Extensions §1.5 in
//! markup-carve/carve#1511; carve-rs#1209, ported from markup-carve/carve-js#1296).
//!
//! An importer DROPS an attribute whose value EQUALS what the renderer derives
//! for that element and KEEPS every other one - the rule a `<th>`'s generated
//! `scope` already follows. It reaches the accessible names the engine writes
//! on its own, together with the `role` beside each.
//!
//! WHY A ROUND TRIP IS NOT THE TEST. Every shape below rebuilds byte-identical
//! at the default labels WHILE carrying the defect, so a round-trip assertion
//! passes and nothing is learned. The assertion has to be that the derived name
//! is ABSENT from the imported source - and, at the end of this file, that a
//! non-default `labels` map still reaches a document that has been imported.
//!
//! WHY A NON-DEFAULT MAP IS THE DISCRIMINATOR. Rendering at the English
//! defaults cannot tell a name the engine DERIVED from one an author WROTE:
//! both read `aria-label="Tabs"`. Rendering the same source with a sentinel map
//! separates them - a value that tracks the map was the engine's, a value that
//! does not was the document's. markup-carve/carve#1511 found no fixture in the
//! spec repo had ever rendered with a non-default map, so every one of the keys
//! had only ever been checked at its English default.
//!
//! THE CONTROLS ARE THE POINT. Reading the clause as "drop the name on a named
//! construct" rather than as "drop a value equal to the derived one" takes an
//! author's real label with it, which is the accessibility regression
//! carve-rs#1060 records. Every family below carries a row whose name DIFFERS
//! and has to survive.

use carve::extensions::{
    CodeGroup, ContentMode, FencedRender, FencedRenderOptions, Index, Tabs, TabsMode, TabsOptions,
};
use carve::html_import::{html_to_carve, HtmlImportOptions};
use carve::{CarveExtension, Options};

fn html(src: &str, ext: &dyn CarveExtension) -> String {
    let mut o = Options::new();
    o.extensions.push(ext);
    carve::to_html_with_options(src, &o)
}

fn html_with_label(src: &str, ext: &dyn CarveExtension, key: &str, value: &str) -> String {
    let mut o = Options::new();
    o.extensions.push(ext);
    o.labels.insert(key.to_string(), value.to_string());
    carve::to_html_with_options(src, &o)
}

fn import(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

fn diagnostics(html: &str) -> Vec<String> {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics
        .iter()
        .map(|d| d.code.as_str().to_string())
        .collect()
}

// FAMILY 1 - DERIVED FROM THE ELEMENT'S OWN CLASS WORD.
//
// A diagram fence's name defaults to the extension's own class word, which is
// why Extensions §1.5 keeps it OUT of the `labels` map: there is no fixed
// English string to translate. The derived value is readable off the element
// itself, so the drop needs no knowledge of the render's options.

#[test]
fn a_diagram_fence_drops_the_name_that_is_its_own_class_word() {
    let out = html(
        "``` mermaid\ngraph TD; A-->B;\n```\n",
        &FencedRender::new("mermaid"),
    );
    assert_eq!(
        out,
        "<pre class=\"mermaid\" role=\"img\" aria-label=\"mermaid\">graph TD; A-->B;</pre>"
    );

    let source = import(&out);
    assert_eq!(
        source, "{.mermaid}\n```\ngraph TD; A-->B;\n```\n",
        "{source}"
    );
}

#[test]
fn a_diagram_fence_keeps_a_name_that_differs_and_still_drops_the_role() {
    let source = import(
        "<pre class=\"mermaid\" role=\"img\" aria-label=\"Architecture overview\">graph TD;</pre>",
    );
    assert_eq!(
        source, "{.mermaid aria-label=\"Architecture overview\"}\n```\ngraph TD;\n```\n",
        "{source}"
    );
}

#[test]
fn a_diagram_fence_keeps_a_role_the_renderer_does_not_derive() {
    let source = import("<pre class=\"mermaid\" role=\"note\">x</pre>");
    assert!(source.contains("role=note"), "{source}");
}

/// `<pre>` ONLY, though a json-mode fence wraps in a `<div>`. That mode puts the
/// payload in a `<script>` the importer drops, so such a div never comes back as
/// a fence for a renderer to name again - the drop would be a pure loss there,
/// and a classed `<div role="img">` is far likelier to be some other producer's
/// than a `<pre>` is.
#[test]
fn a_json_mode_fence_div_keeps_its_name_because_nothing_writes_it_back() {
    let out = html("``` chart\n{\"a\":1}\n```\n", &FencedRender::chart());
    assert!(
        out.starts_with("<div class=\"chart\" role=\"img\" aria-label=\"chart\">"),
        "{out}"
    );

    let source = import(&out);
    assert!(source.contains("aria-label=chart"), "{source}");
    assert!(source.contains("role=img"), "{source}");
}

// FAMILY 2 - AN AUTHORED DEFAULT FROM THE `labels` MAP.
//
// A tab set and a code group take their name from a key the host can set, so
// unlike family 1 an author MAY have written the same words. The rule stays
// value-matched: the ENGLISH DEFAULT is dropped, because at that value the
// renderer writes it back and the output is identical either way. Anything else
// - a German render, an author's own name - is kept.

#[test]
fn a_tab_set_drops_the_group_name_at_its_documented_default() {
    let out = html(
        ":::: tabs\n\n::: tab [First]\na\n:::\n\n::::\n",
        &Tabs::new(),
    );
    assert!(
        out.contains("<div class=\"tabs\" role=\"group\" aria-label=\"Tabs\">"),
        "{out}"
    );

    let source = import(&out);
    assert!(source.contains("{.tabs}"), "{source}");
    assert!(!source.contains("aria-label=Tabs"), "{source}");
    assert!(!source.contains("role=group"), "{source}");
}

/// `tablist` is the other value the same element derives - the `aria` mode's -
/// so it goes for the same reason `group` does.
#[test]
fn a_tab_set_drops_the_aria_mode_role_too() {
    let out = html(
        ":::: tabs\n\n::: tab [First]\na\n:::\n\n::::\n",
        &Tabs::with_options(TabsOptions {
            mode: TabsMode::Aria,
            ..Default::default()
        }),
    );
    assert!(
        out.contains("<div class=\"tabs\" role=\"tablist\" aria-label=\"Tabs\">"),
        "{out}"
    );

    let source = import(&out);
    assert!(!source.contains("aria-label=Tabs"), "{source}");
    assert!(!source.contains("role=tablist"), "{source}");
}

#[test]
fn a_tab_set_keeps_a_group_name_rendered_from_a_non_default_labels_map() {
    let out = html_with_label(
        ":::: tabs\n\n::: tab [First]\na\n:::\n\n::::\n",
        &Tabs::new(),
        "tabsGroup",
        "Registerkarten",
    );
    let source = import(&out);
    assert!(source.contains("aria-label=Registerkarten"), "{source}");
}

/// The control that stops the fix over-reaching: an author's genuinely-written
/// label still imports. This is what a blanket `aria-label` drop cost
/// (carve-rs#1060).
#[test]
fn a_tab_set_keeps_a_group_name_the_author_wrote() {
    let source = import("<div class=\"tabs\" role=\"group\" aria-label=\"Build steps\">x</div>");
    assert!(source.contains("aria-label=\"Build steps\""), "{source}");
}

#[test]
fn a_code_group_drops_the_group_name_at_its_documented_default() {
    let out = html(
        "::: code-group\n\n``` php [PHP]\n1\n```\n\n:::\n",
        &CodeGroup::new(),
    );
    assert!(
        out.contains("<div class=\"code-group\" role=\"group\" aria-label=\"Code examples\">"),
        "{out}"
    );

    let source = import(&out);
    assert!(source.contains("{.code-group}"), "{source}");
    assert!(!source.contains("aria-label=\"Code examples\""), "{source}");
}

#[test]
fn a_code_group_keeps_a_group_name_rendered_from_a_non_default_labels_map() {
    let out = html_with_label(
        "::: code-group\n\n``` php [PHP]\n1\n```\n\n:::\n",
        &CodeGroup::new(),
        "codeGroup",
        "Codebeispiele",
    );
    assert!(import(&out).contains("aria-label=Codebeispiele"), "{out}");
}

/// The rule is keyed on the element the renderer derives FOR. A grouping div
/// that is not a tab set derives nothing, so both attributes stay.
#[test]
fn an_element_the_renderer_never_names_keeps_the_same_pair() {
    let source = import("<div role=\"group\" aria-label=\"Tabs\">x</div>");
    assert!(source.contains("role=group"), "{source}");
    assert!(source.contains("aria-label=Tabs"), "{source}");
}

// FAMILY 3 - DERIVED FROM A SIBLING THE DOCUMENT ALREADY CARRIES.
//
// A `css`-mode tab panel is named by its own tab's `[label]`, which §16a lists
// among the strings that get no key precisely because the author already wrote
// it once, in the document. The importer reads the same string off the `<label>`
// control that names the panel.

#[test]
fn a_css_mode_panel_drops_the_name_it_takes_from_its_own_tab_label() {
    let out = html(
        ":::: tabs\n\n::: tab [First]\na\n:::\n\n::::\n",
        &Tabs::new(),
    );
    assert!(
        out.contains("<div class=\"tabs-panel\" role=\"group\" aria-label=\"First\">"),
        "{out}"
    );

    let source = import(&out);
    assert!(source.contains("{.tabs-panel}"), "{source}");
    assert!(!source.contains("aria-label=First"), "{source}");
}

#[test]
fn a_code_group_panel_drops_its_name_the_same_way() {
    let out = html(
        "::: code-group\n\n``` php [PHP]\n1\n```\n\n:::\n",
        &CodeGroup::new(),
    );
    assert!(
        out.contains("<div class=\"code-group-panel\" role=\"group\" aria-label=\"PHP\">"),
        "{out}"
    );
    assert!(!import(&out).contains("aria-label=PHP"), "{out}");
}

#[test]
fn a_panel_keeps_a_name_that_differs_from_its_tab_label() {
    let source = import(
        "<div class=\"tabs\"><label for=\"t1\" class=\"tabs-label\">First</label>\
         <div class=\"tabs-panel\" role=\"group\" aria-label=\"Erste\">x</div></div>",
    );
    assert!(source.contains("aria-label=Erste"), "{source}");
}

/// A panel with no control before it - a fragment cut mid-set - derives no name,
/// and guessing one there would drop a label nothing writes back.
#[test]
fn a_panel_with_no_control_before_it_keeps_its_name() {
    let source = import("<div class=\"tabs-panel\" role=\"group\" aria-label=\"First\">x</div>");
    assert!(source.contains("aria-label=First"), "{source}");
}

// FAMILY 4 - A COMPOSITE OF A MAP LABEL AND THE DOCUMENT'S OWN WORDS.
//
// An index back-link is named `{indexBackref} {term}`, or `{indexBackref}
// {term} {k}` for the kth of several. Both halves are on the page - the term is
// the entry's own text and k is the link's position among its siblings - so the
// whole derived value is reconstructable and the match stays exact.

#[test]
fn an_index_back_link_drops_the_composite_name_for_a_single_occurrence() {
    let out = html("A :index[gadget] word.\n\n::: index\n:::\n", &Index::new());
    assert!(out.contains("aria-label=\"Back to gadget\""), "{out}");
    assert!(!import(&out).contains("aria-label"), "{out}");
}

#[test]
fn an_index_back_link_drops_the_numbered_name_of_the_kth_occurrence() {
    let out = html(
        "A :index[gadget] and :index[gadget].\n\n::: index\n:::\n",
        &Index::new(),
    );
    assert!(out.contains("aria-label=\"Back to gadget 1\""), "{out}");
    assert!(out.contains("aria-label=\"Back to gadget 2\""), "{out}");
    assert!(!import(&out).contains("aria-label"), "{out}");
}

#[test]
fn an_index_back_link_keeps_a_name_rendered_from_a_non_default_labels_map() {
    let out = html_with_label(
        "A :index[gadget] word.\n\n::: index\n:::\n",
        &Index::new(),
        "indexBackref",
        "Zurück zu",
    );
    assert!(
        import(&out).contains("aria-label=\"Zurück zu gadget\""),
        "{out}"
    );
}

#[test]
fn an_index_back_link_keeps_a_name_the_author_wrote() {
    let source = import(
        "<ul class=\"index\"><li>gadget <a href=\"#idx-gadget-1\" class=\"index-backref\" \
         aria-label=\"Zum Gerät\">x</a></li></ul>",
    );
    assert!(source.contains("aria-label=\"Zum Gerät\""), "{source}");
}

// A TITLE PARAGRAPH'S COUNTER ID IS NOT A FAMILY HERE, AND THE MEASUREMENT SAYS
// WHY. carve-js drops `id="adm-N"` off a `<p class="admonition-title">` whose
// parent `<aside>` names it back. This engine never gets that far: `<aside>` is
// not a block tag here, so a canonical admonition is UNWRAPPED and its title
// paragraph flattened into the surrounding inline run - the id is gone before
// any drop could reach it. That unwrap is markup-carve/carve-php#1543, and when
// it lands the family lands with it. Pinned so the day the aside survives, this
// test says what has to follow it.
#[test]
fn an_admonition_title_id_never_reaches_source_because_the_aside_is_unwrapped() {
    let out = carve::to_html("::: note \"A\"\nx\n:::\n");
    assert!(
        out.contains("<p class=\"admonition-title\" id=\"adm-1\">A</p>"),
        "{out}"
    );

    let source = import(&out);
    assert!(!source.contains("adm-1"), "{source}");
    assert!(!source.contains("admonition-title"), "{source}");
}

/// A counter-shaped id on a title the counter never counted stays, and stays for
/// the same reason the rule is value-matched: matching the SHAPE `adm-N` is a
/// guess this rule does not make.
#[test]
fn a_counter_shaped_id_on_a_title_no_counter_counted_is_kept() {
    let source = import("<p class=\"admonition-title\" id=\"adm-1\">A</p>");
    assert!(source.contains("#adm-1"), "{source}");
}

// THE POINT OF THE WHOLE PASS, measured on the ENGINE rather than the importer.
// A name in source WINS over the one an extension derives, so a derived name
// that came back from an import pins the document to the English default
// forever while every byte of today's output is unchanged - which is also why a
// round trip cannot detect this and the rows above assert ABSENCE instead.
//
// carve-js states the same point end to end: import a tab set, re-render it
// with a non-default `labels` map, and watch the map reach it. That half is not
// portable here yet - no construct in this file comes back from an import AS a
// construct (a tab set returns as bare divs, a fence loses its language), which
// is markup-carve/carve-php#1543 and not this. So the precedence is measured on
// a shape the engine claims directly.

#[test]
fn a_name_in_source_wins_over_the_one_an_extension_derives() {
    let renamed = FencedRender::with_options(FencedRenderOptions {
        label: "Diagramm".into(),
        ..FencedRenderOptions::new(vec!["mermaid".into()], None, None, ContentMode::Text)
    });

    let clean = html("``` mermaid\ngraph TD;\n```\n", &renamed);
    assert!(clean.contains("aria-label=\"Diagramm\""), "{clean}");

    let baked = html(
        "{aria-label=mermaid role=img}\n``` mermaid\ngraph TD;\n```\n",
        &renamed,
    );
    assert!(baked.contains("aria-label=\"mermaid\""), "{baked}");
    assert!(!baked.contains("Diagramm"), "{baked}");
}

/// THE DROP DOES NOT LOSE A NAME, IT REMOVES A FALSE ONE. Review of this change
/// asked whether dropping is lossy while the CONSTRUCT does not survive an
/// import - a fence comes back with no language, a tab set as bare divs
/// (markup-carve/carve-php#1543) - so no renderer writes the value back today.
/// Measured: kept, the pair rides a plain `<pre><code>` and announces literal
/// source as an image named `mermaid`. The element is not a diagram any more, so
/// the name does not describe it. When the construct survives, the renderer
/// names it again and the drop becomes the no-op §16a describes.
#[test]
fn an_imported_fence_is_not_announced_as_a_diagram_it_is_no_longer() {
    let mermaid = FencedRender::new("mermaid");
    let imported = import(&html("``` mermaid\ngraph TD;\n```\n", &mermaid));

    let again = html(&imported, &mermaid);
    assert!(again.contains("<pre class=\"mermaid\"><code>"), "{again}");
    assert!(!again.contains("role=\"img\""), "{again}");
    assert!(!again.contains("aria-label"), "{again}");
}

/// NOTHING IS LOST, SO NOTHING IS DIAGNOSED. The renderer writes the value back,
/// so a value-matched drop is not a lossy decision - the same reason the
/// `<figure>` and `<blockquote cite>` imports report nothing.
#[test]
fn a_value_matched_drop_reports_no_attribute_dropped() {
    let out = html(
        "``` mermaid\ngraph TD; A-->B;\n```\n",
        &FencedRender::new("mermaid"),
    );
    assert_eq!(diagnostics(&out), Vec::<String>::new());
}

// THE BUDGET IS UNTOUCHED. The lookups read a sibling's text without charging
// it, and they do it on an explicit stack rather than by recursion - the
// importer's depth limit is a COUNTER, and a caller may raise it past what the
// native stack holds. Asserted as an EQUALITY against a structurally identical
// element the rule does not match, never against a number: a threshold written
// down is a threshold that rots.

fn smallest_budget(html: &str, set: impl Fn(&mut HtmlImportOptions, usize)) -> usize {
    for n in 1..4096 {
        let mut o = HtmlImportOptions::default();
        set(&mut o, n);
        if html_to_carve(html, &o).is_ok() {
            return n;
        }
    }
    panic!("no budget admitted this document");
}

#[test]
fn a_matched_element_costs_the_same_budget_as_one_the_rule_does_not_match() {
    let matched = "<div class=\"tabs\"><label for=\"t\" class=\"tabs-label\">First</label>\
                   <div class=\"tabs-panel\" role=\"group\" aria-label=\"First\">x</div></div>";
    let unmatched = "<div class=\"tabz\"><label for=\"t\" class=\"tabz-label\">First</label>\
                     <div class=\"tabz-panel\" role=\"group\" aria-label=\"First\">x</div></div>";

    assert_eq!(
        smallest_budget(matched, |o, n| o.max_nodes = n),
        smallest_budget(unmatched, |o, n| o.max_nodes = n),
    );
    assert_eq!(
        smallest_budget(matched, |o, n| o.max_depth = n),
        smallest_budget(unmatched, |o, n| o.max_depth = n),
    );
}
