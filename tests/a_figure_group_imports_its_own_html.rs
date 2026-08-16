//! The HTML importer reads this engine's own composite-figure shape back.
//!
//! `<figure class="carve-figure-group">` returns as a `figure_group`: the
//! structural classes come off, each `carve-figure-panel` figure rebuilds the
//! figure it wrapped, and the group's own `<figcaption>` - its LAST direct
//! one, since a panel's sits a level down - is the group caption. That closes
//! the render/import round trip for PART 9 §4c output. A `<figure>` WITHOUT
//! the own-output class is still an unsupported element and keeps the pre-§4c
//! treatment.

fn round_trip(source: &str) -> String {
    let html = carve::to_html(source);
    let result = carve::html_to_carve(&html, &carve::HtmlImportOptions::default())
        .expect("own output imports");
    carve::to_html(&result.value)
}

#[test]
fn the_basic_group_round_trips_to_the_same_html() {
    // The rendered caption carries the RESOLVED number, so the re-imported
    // document captions the group with that literal text - and renders the
    // same bytes.
    let source = "\
{#fig-x .columns-2}
::: figure
{#fig-x-a}
![one](a.png)
^ (a) One

{#fig-x-b}
![two](b.png)
^ (b) Two
:::
^ Figure #: Group caption
";
    assert_eq!(round_trip(source), carve::to_html(source));
}

#[test]
fn stray_content_and_an_uncaptioned_group_survive() {
    let source = "\
::: figure
Shot on the same day.

![one](a.png)
^ (a) One
:::
";
    assert_eq!(round_trip(source), carve::to_html(source));
}

#[test]
fn a_wrapped_table_panel_unwraps_to_the_table() {
    let source = "\
::: figure
| Kind | N |
|------|---|
| a    | 1 |
:::
^ Figure #: Mixed
";
    assert_eq!(round_trip(source), carve::to_html(source));
}

#[test]
fn a_foreign_figure_is_still_unsupported() {
    // No own-output class: the importer must not guess a group out of it.
    let result = carve::html_to_carve(
        "<figure><img src=\"a.png\" alt=\"one\"><figcaption>One</figcaption></figure>",
        &carve::HtmlImportOptions::default(),
    )
    .expect("imports");
    assert!(
        !result.value.contains("::: figure"),
        "a foreign figure became a group: {}",
        result.value
    );
}
