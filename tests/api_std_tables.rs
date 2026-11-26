//! PROVISIONAL `listTable` and `colorSwatch` of `carve::api::extensions`,
//! ported from carve-js (carve-js#5070, test/extension-migration/).

use carve::api::extensions::{self, Bundle, ColorSwatchOptions, SwatchPosition, SwatchShape};
use carve::api::{self, Carve, Mode, Options};

fn html_with(source: &str, bundle: Bundle, opts: &Options) -> String {
    let mut carve = Carve::new();
    carve.register(bundle).unwrap();
    carve.render(source, opts).unwrap()
}

fn html(source: &str, bundle: Bundle) -> String {
    html_with(source, bundle, &Options::default())
}

fn swatch(source: &str, options: ColorSwatchOptions) -> String {
    html(source, extensions::color_swatch(options))
}

#[test]
fn list_table_head_caption_and_body() {
    let src = "{header-rows=1}\n::: list-table \"Quarterly results\"\n- - Region\n  - Notes\n- - EMEA\n  - Strong quarter.\n:::\n";
    assert_eq!(
        html(src, extensions::list_table()),
        "<table>\n  <caption>Quarterly results</caption>\n  <thead>\n    <tr><th scope=\"col\">Region</th><th scope=\"col\">Notes</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>EMEA</td><td>Strong quarter.</td></tr>\n  </tbody>\n</table>"
    );
    let static_mode = Options {
        mode: Mode::Static,
        ..Options::default()
    };
    assert_eq!(
        html_with(src, extensions::list_table(), &static_mode),
        html(src, extensions::list_table())
    );
}

#[test]
fn list_table_spans_padding_and_footer() {
    assert_eq!(
        html(
            "{footer-rows=1}\n::: list-table\n- - A\n  - <\n- - ^\n  - B\n- - Z\n:::\n",
            extensions::list_table()
        ),
        "<table>\n  <tbody>\n    <tr><td colspan=\"2\">A</td></tr>\n    <tr><td>^</td><td>B</td></tr>\n  </tbody>\n  <tfoot>\n    <tr><td>Z</td><td></td></tr>\n  </tfoot>\n</table>"
    );
    assert!(
        html(
            "::: list-table\n- - A\n  - B\n- - ^\n  - C\n:::\n",
            extensions::list_table()
        )
        .contains("<td rowspan=\"2\">A</td>")
    );
}

#[test]
fn list_table_alignment_widths_and_consumed_keys() {
    let out = html(
        "{aligns=\"left,right\" valigns=\"top,bottom\" widths=\"30,70\" .t}\n::: list-table\n- -{align=center valign=middle} A\n  - B\n:::\n",
        extensions::list_table(),
    );
    assert!(out.starts_with("<table class=\"t\">\n  <colgroup>\n    <col style=\"width: 30%;\">"));
    assert!(out.contains("<td style=\"text-align: center; vertical-align: middle;\">A</td>"));
    assert!(out.contains("<td style=\"text-align: right; vertical-align: bottom;\">B</td>"));
    assert!(!out.contains("aligns=") && !out.contains(" align="));
}

#[test]
fn list_table_that_is_not_a_table_stays_core() {
    let src = "::: list-table\nplain\n:::\n";
    assert_eq!(
        html(src, extensions::list_table()),
        api::render(src, &Options::default()).unwrap()
    );
}

#[test]
fn color_swatch_values() {
    let default = ColorSwatchOptions::default;
    assert_eq!(
        swatch(":color[#ff8800]", default()),
        "<p><span class=\"swatch\"><span class=\"swatch-chip\" style=\"background-color:#ff8800\"></span> #ff8800</span></p>"
    );
    assert_eq!(
        swatch(":color[!`#ff8800`]", default()),
        swatch(":color[#ff8800]", default())
    );
    assert!(swatch(":color[rgb(1 2 3 / 50%)]", default()).contains("swatch-chip"));
    assert!(!swatch(":color[notacolor]", default()).contains("swatch"));
}

#[test]
fn color_swatch_contrast_label() {
    let default = ColorSwatchOptions::default;
    assert_eq!(
        swatch(":color[#0d1117]{contrast}", default()),
        "<p><span class=\"swatch-label\" style=\"background:#0d1117;color:#fff\">#0d1117</span></p>"
    );
    assert_eq!(
        swatch(":color[rgb(240,246,252)]{contrast}", default()),
        "<p><span class=\"swatch-label\" style=\"background:rgb(240,246,252);color:#000\">rgb(240,246,252)</span></p>"
    );
    // A transparent color has no readable contrast, so it keeps the swatch.
    assert!(swatch(":color[#00000000]{contrast}", default()).contains("swatch-chip"));
}

#[test]
fn color_swatch_options() {
    let opts = |position, shape, tint, reveal| ColorSwatchOptions {
        position,
        shape,
        tint,
        reveal,
    };
    assert_eq!(
        swatch(
            ":color[red]",
            opts(SwatchPosition::None, SwatchShape::Square, false, false)
        ),
        "<p><span class=\"swatch swatch-chip-only\" title=\"red\"><span class=\"swatch-chip\" style=\"background-color:red\"></span></span></p>"
    );
    assert_eq!(
        swatch(
            ":color[red]",
            opts(SwatchPosition::After, SwatchShape::Ring, false, false)
        ),
        "<p><span class=\"swatch\">red <span class=\"swatch-chip swatch-chip-ring\" style=\"border-color:red\"></span></span></p>"
    );
    assert_eq!(
        swatch(
            ":color[red]",
            opts(SwatchPosition::Before, SwatchShape::Round, true, true)
        ),
        "<p><span class=\"swatch swatch-tint swatch-reveal\" style=\"background-color:color-mix(in srgb, red 12%, transparent)\" tabindex=\"0\"><span class=\"swatch-chip swatch-chip-round\" style=\"background-color:red\"></span> <span class=\"swatch-val\">red</span></span></p>"
    );
}
