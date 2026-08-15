//! The canonical writer emits a composite figure in the authored form.
//!
//! PART 11 §10g: the attribute line where attributes exist, the bare
//! `::: figure` opener, the children with one blank line between them, the
//! closer at the opener's width, and the group caption as a `^ ` line after
//! the closer with its `#` written back. The caret after a group closer is a
//! CAPTION POSITION, not text, so the writer must not spell it `\^ ` - while
//! a paragraph a blank line pair DETACHED from the closer keeps its escape,
//! because bare it would re-parse as the group caption.

const AUTHORED: &str = "\
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

#[test]
fn fmt_reproduces_the_authored_form() {
    assert_eq!(carve::to_carve(AUTHORED), AUTHORED);
}

#[test]
fn fmt_is_idempotent_and_html_stable() {
    let once = carve::to_carve(AUTHORED);
    assert_eq!(carve::to_carve(&once), once);
    assert_eq!(carve::to_html(&once), carve::to_html(AUTHORED));
}

#[test]
fn a_detached_caption_keeps_its_escape() {
    // Corpus 318-composite-figures-6: two blank lines detach, and the `^ `
    // line is an ordinary paragraph. Written bare after the closer it would
    // re-parse as the group caption, so the writer owes it the escape - and
    // the round trip must hold.
    let source = "\
::: figure
![one](a.png)
^ (a) One
:::


^ Figure #: Detached
";
    let formatted = carve::to_carve(source);
    assert!(formatted.contains("\\^ Figure #: Detached"), "{formatted}");
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}

#[test]
fn a_captioned_group_needs_no_escape_on_a_following_caret() {
    // The slot is filled: a `^ ` paragraph after a captioned group re-parses
    // as a paragraph bare, and PART 11 §4 asks for the minimal form.
    let source = "\
::: figure
![one](a.png)
^ (a) One
:::
^ Figure #: Group

^ Not a caption
";
    let formatted = carve::to_carve(source);
    assert!(
        !formatted.contains("\\^ Not a caption"),
        "over-escaped: {formatted}"
    );
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
}

#[test]
fn the_metadata_spellings_round_trip_as_containers() {
    // Corpus 318-composite-figures-8: a titled or labeled `::: figure` is a
    // generic container and must write back as one.
    let source = "\
::: figure \"A titled figure div\"
![one](a.png)
^ (a) One
:::

::: figure [g]
Body.
:::
";
    let formatted = carve::to_carve(source);
    assert!(
        formatted.contains("::: figure \"A titled figure div\""),
        "{formatted}"
    );
    assert!(formatted.contains("::: figure [g]"), "{formatted}");
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
}

#[test]
fn a_nested_demoted_figure_round_trips() {
    // Corpus 318-composite-figures-9: the inner bare opener is a generic
    // container; the writer's inward-widening fences must reproduce a parse
    // with the same shape.
    let source = "\
::: figure
:::: figure
![one](a.png)
^ (a) One
::::
:::
^ Figure #: Outer only
";
    let formatted = carve::to_carve(source);
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}

#[test]
fn an_uncaptioned_group_round_trips() {
    let source = "\
::: figure
![one](a.png)
^ (a) One

![two](b.png)
^ (b) Two
:::
";
    assert_eq!(carve::to_carve(source), source);
}
