//! A composite figure survives every target, degraded the SAME way everywhere.
//!
//! PART 11 §10g: Markdown emits the panels in order, each panel caption as an
//! emphasized `*...*` paragraph after its host, and the group caption LAST as
//! a bold `**...**` paragraph with its number resolved. Plain text and the
//! terminal put the group caption FIRST - on a caption-less target it is the
//! only line that says what the following blocks are one of - then each
//! panel's caption line over its host's degradation. Stray content is
//! preserved in place on every target; nothing is silently dropped.

const GROUP: &str = "\
{#fig-x}
::: figure
{#fig-x-a}
![one](a.png)
^ (a) One

Shot on the same day.

{#fig-x-b}
![two](b.png)
^ (b) Two
:::
^ Figure #: Group caption
";

#[test]
fn markdown_puts_the_group_caption_last_in_bold() {
    assert_eq!(
        carve::to_markdown(GROUP),
        "\
![one](a.png)

*(a) One*

Shot on the same day.

![two](b.png)

*(b) Two*

**Figure 1: Group caption**
"
    );
}

#[test]
fn plain_text_puts_the_group_caption_first() {
    assert_eq!(
        carve::to_plain_text(GROUP),
        "\
Figure 1: Group caption

(a) One
one

Shot on the same day.

(b) Two
two
"
    );
}

#[test]
fn the_terminal_orders_like_plain_text() {
    let out = carve::to_ansi(GROUP);
    let group = out
        .find("Figure 1: Group caption")
        .expect("the group caption");
    let first_panel = out.find("(a) One").expect("the first panel caption");
    let stray = out
        .find("Shot on the same day.")
        .expect("the stray paragraph");
    let second_panel = out.find("(b) Two").expect("the second panel caption");
    assert!(group < first_panel, "{out:?}");
    assert!(first_panel < stray, "{out:?}");
    assert!(stray < second_panel, "{out:?}");
}

#[test]
fn an_uncaptioned_table_panel_degrades_as_a_table() {
    let source = "\
::: figure
| Kind | N |
|------|---|
| a    | 1 |
:::
^ Figure #: Mixed
";
    let plain = carve::to_plain_text(source);
    assert!(plain.starts_with("Figure 1: Mixed\n\n"), "{plain}");
    assert!(plain.contains("Kind"), "the table was dropped: {plain}");
}
