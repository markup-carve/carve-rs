//! A composite figure draws ONE number, and its panels draw letters off it.
//!
//! PART 9 §4c (markup-carve/carve#1122): the group caption's `#` takes one
//! number from its label's sequence, like any figure; the panels take NOTHING
//! from the document sequence. A panel with an id resolves `</#id>` as the
//! group's number plus a letter by panel order among the panels - and only
//! when the group itself drew a number; an unnumbered group's panels stay
//! plain anchors, exactly as an id on an uncaptioned figure does.

const TWO_GROUPS: &str = "\
{#fig-first}
![lead](lead.png)
^ Figure #: First

{#fig-x}
::: figure
{#fig-x-a}
![one](a.png)
^ (a) One

{#fig-x-b}
![two](b.png)
^ (b) Two
:::
^ Figure #: Second

See </#fig-x> and </#fig-x-a> and </#fig-x-b>.
";

#[test]
fn the_group_takes_one_number_from_the_shared_sequence() {
    let html = carve::to_html(TWO_GROUPS);
    assert!(html.contains("Figure 1: First"), "{html}");
    assert!(html.contains("Figure 2: Second"), "{html}");
    assert!(
        !html.contains("Figure 3"),
        "panels drew from the sequence: {html}"
    );
}

#[test]
fn a_panel_id_resolves_with_the_group_number_and_a_letter() {
    let html = carve::to_html(TWO_GROUPS);
    assert!(html.contains("<a href=\"#fig-x\">Figure 2</a>"), "{html}");
    assert!(
        html.contains("<a href=\"#fig-x-a\">Figure 2a</a>"),
        "{html}"
    );
    assert!(
        html.contains("<a href=\"#fig-x-b\">Figure 2b</a>"),
        "{html}"
    );
}

#[test]
fn the_letter_counts_panels_not_children() {
    // A stray paragraph between the panels is preserved content, not a panel,
    // so it must not advance the letter.
    let source = "\
{#g}
::: figure
A note between panels.

{#p-one}
![one](a.png)
^ (a) One

{#p-two}
![two](b.png)
^ (b) Two
:::
^ Figure #: Group

See </#p-two>.
";
    let html = carve::to_html(source);
    assert!(html.contains("<a href=\"#p-two\">Figure 1b</a>"), "{html}");
}

#[test]
fn an_unnumbered_group_registers_no_panel_letters() {
    // No `#` in the group caption: the group never enters a counter, so its
    // panel ids are anchors but not caption crossref targets.
    let source = "\
::: figure
{#p}
![one](a.png)
^ (a) One
:::
^ Just a caption

See </#p>.
";
    let html = carve::to_html(source);
    assert!(
        !html.contains(">Figure"),
        "a panel letter appeared without a group number: {html}"
    );
}

#[test]
fn the_numbers_survive_the_json_ingest_path() {
    // PART 12 §5/§6: numbering re-derives on ingest through the same pass the
    // parse runs, so the two paths cannot disagree about the letters.
    let doc = carve::parse(TWO_GROUPS);
    let json = carve::to_json(&doc);
    let back = carve::from_json(&json).expect("own output decodes");
    assert_eq!(carve::to_json(&back), json);
    let html = carve::render_html(&back).expect("renders");
    assert!(
        html.contains("<a href=\"#fig-x-a\">Figure 2a</a>"),
        "{html}"
    );
}
