//! carve#6832: a paragraph a tightness rule writes bare keeps its `<p>` where
//! the bare HTML would import as other blocks (C2-H-018, C2-M-012, C2-M-013),
//! and `data-carve-tight` stands over such a `<p>` (C2-H-186).

use carve::Options;
use carve::api::{self, Carve, ImportFormat, ImportOptions, LossOptions};

fn render(source: &str, roundtrip: bool) -> String {
    let opts = Options {
        roundtrip,
        ..Options::default()
    };
    api::render(source, &opts).expect("the source renders")
}

fn import(html: &str) -> api::Imported {
    Carve::new()
        .import_from(
            html,
            ImportFormat::Html,
            &ImportOptions::default(),
            &LossOptions::default(),
        )
        .expect("the import succeeds")
}

const RENDERS: &[(&str, &str)] = &[
    (
        "- a\n\n  ![](i.png)\n",
        "<ul>\n  <li><p>a</p>\n    <img src=\"i.png\" alt=\"\">\n  </li>\n</ul>",
    ),
    (
        "- a\n\n  %% x\n\n  ![](i.png)\n",
        "<ul>\n  <li><p>a</p>\n    <img src=\"i.png\" alt=\"\">\n  </li>\n</ul>",
    ),
    (
        "- ![](i.png)![](j.png)\n",
        "<ul>\n  <li><p><img src=\"i.png\" alt=\"\"><img src=\"j.png\" alt=\"\"></p></li>\n</ul>",
    ),
    (
        "- ![](i.png)\n  ![](j.png)\n",
        "<ul>\n  <li><p><img src=\"i.png\" alt=\"\">\n<img src=\"j.png\" alt=\"\"></p></li>\n</ul>",
    ),
    (
        ":: t\n: ![](a.png)![](b.png)\n",
        "<dl>\n  <dt>t</dt>\n  <dd><p><img src=\"a.png\" alt=\"\"><img src=\"b.png\" alt=\"\"></p></dd>\n</dl>",
    ),
    (
        "- a ![](i.png)\n",
        "<ul>\n  <li>a <img src=\"i.png\" alt=\"\"></li>\n</ul>",
    ),
    (
        "- ![](i.png)\n",
        "<ul>\n  <li>\n    <img src=\"i.png\" alt=\"\">\n  </li>\n</ul>",
    ),
    (":: t\n: a\n", "<dl>\n  <dt>t</dt>\n  <dd>a</dd>\n</dl>"),
];

#[test]
fn a_bare_form_that_would_not_import_back_keeps_its_p() {
    for (source, html) in RENDERS {
        assert_eq!(render(source, false), *html, "{source:?}");
    }
}

#[test]
fn render_import_render_is_a_fixed_point() {
    for (source, html) in RENDERS {
        let back = import(html).source;
        assert_eq!(
            render(&back, false),
            *html,
            "{source:?} imported as {back:?}"
        );
    }
}

/// The list's spacing, read from the written source: a loose list puts a
/// blank line before its last item `c`.
fn spacing(html: &str) -> &'static str {
    let source = import(html).source;
    if source.contains("\n\n- c") {
        "loose"
    } else {
        "tight"
    }
}

#[test]
fn data_carve_tight_stands_over_a_kept_p() {
    let cases = [
        (
            "<ul data-carve-tight>\n  <li><p><img src=\"i.png\" alt=\"\"><img src=\"j.png\" alt=\"\"></p></li>\n  <li>c</li>\n</ul>",
            "tight",
        ),
        (
            "<ul data-carve-tight>\n  <li><p>a</p>\n    <!-- x -->\n    <img src=\"i.png\" alt=\"\">\n  </li>\n  <li>c</li>\n</ul>",
            "tight",
        ),
        (
            "<ul data-carve-tight>\n  <li><p>a</p></li>\n  <li>c</li>\n</ul>",
            "loose",
        ),
        (
            "<ul data-carve-tight>\n  <li><p>a <img src=\"i.png\" alt=\"\"></p></li>\n  <li>c</li>\n</ul>",
            "loose",
        ),
    ];
    for (html, with_carrier) in cases {
        assert_eq!(spacing(html), with_carrier, "{html:?}");
        // C2-M-033 without the carrier is unchanged.
        assert_eq!(
            spacing(&html.replace(" data-carve-tight", "")),
            "loose",
            "{html:?}"
        );
    }
}

#[test]
fn a_tight_list_round_trips_under_roundtrip() {
    let html = render("- ![](i.png)![](j.png)\n- c\n", true);
    assert!(html.contains("<ul data-carve-tight>"), "{html}");
    assert_eq!(render(&import(&html).source, true), html);
}
