//! Writer regressions of the `carve.tabs` source form (lane rsext).

use carve::api::{Carve, WriteOptions};

fn format_tabs(src: &str) -> (String, Vec<String>) {
    let opts = WriteOptions {
        extensions: vec!["carve.tabs".into()],
        ..WriteOptions::default()
    };
    let written = Carve::new().format_with_diagnostics(src, &opts).unwrap();
    let codes = written.diagnostics.iter().map(|d| d.code.clone()).collect();
    (written.output, codes)
}

/// carve-rs#4565, C2-X-045: a tabs group inside a tab is claimed too and is
/// written back as it stands.
#[test]
fn a_tabs_group_nested_in_a_tab_is_written_back() {
    let src = "\
:::::: tabs
::::: tab \"A\"
:::: tabs
::: tab \"B\"
x
:::
::::
:::::
::::::
";
    assert_eq!(format_tabs(src), (src.to_string(), Vec::new()));
}

#[test]
fn a_nested_group_with_siblings_and_a_reference_is_written_back() {
    let src = "\
:::::: tabs
::::: tab \"A\"
lead [x][r]

:::: tabs
::: tab \"B\"
x
:::

::: tab \"C\"
y
:::
::::
:::::

::::: tab \"D\"
z
:::::
::::::

[r]: http://x/
";
    // The writer shortens the run of the last tab, as carve-js does.
    let expected = src.replace("::::: tab \"D\"\nz\n:::::", "::: tab \"D\"\nz\n:::");
    assert_eq!(format_tabs(src), (expected, Vec::new()));
}

/// C2-X-026: a tab body is written in the document's context, so a shortcut
/// reference to a definition outside the group reads back as a link.
#[test]
fn a_shortcut_reference_in_a_tab_is_written_back() {
    let src = "\
[r0]: http://x/0

:::: tabs
::: tab \"T0\"
see [r0].
:::
::::
";
    assert_eq!(format_tabs(src), (src.to_string(), Vec::new()));
}

/// markup-carve/carve#6345 shape 2, C2-W-089: an escaped bracket that matches a
/// defined label keeps its escape, and the group is written back.
#[test]
fn an_escaped_label_in_a_tab_is_written_back() {
    let src = "\
[r0]: http://x/0

:::: tabs
::: tab \"T0\"
\\[r0] lit
:::
::::
";
    assert_eq!(format_tabs(src), (src.to_string(), Vec::new()));
}
