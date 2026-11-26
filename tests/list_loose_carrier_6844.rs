//! carve#6844: under `roundtrip` a loose list with two or more items carries
//! `data-carve-loose` (C2-H-138), and the importer reads it unless C2-H-186
//! ignores it.

use carve::Options;
use carve::api::{self, Carve, ImportFormat, ImportOptions, LossOptions};

fn render(source: &str) -> String {
    let opts = Options {
        roundtrip: true,
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

const LOOSE: &[&str] = &[
    "- ![x](j.png)\n\n- ![x](k.png)\n",
    "- > a\n\n- > b\n",
    "3. > a\n\n4. > b\n",
    "* a\n\n* b\n",
    "- [ ] > a\n\n- [x] > b\n",
    "- ```\n  a\n  ```\n\n- ```\n  b\n  ```\n",
    "- a\n\n  ![](i.png)\n\n- b\n",
    "- x\n  - > a\n\n  - > b\n- y\n",
    "> - > a\n>\n> - > b\n",
];

#[test]
fn a_loose_list_round_trips_under_roundtrip() {
    for source in LOOSE {
        let html = render(source);
        assert!(html.contains(" data-carve-loose"), "{source:?}\n{html}");
        let back = import(&html).source;
        assert_eq!(render(&back), html, "{source:?} imported as {back:?}");
    }
}

#[test]
fn a_one_item_or_tight_list_has_no_loose_carrier() {
    for source in ["- > a\n", "- a\n- b\n", "- > a\n- > b\n"] {
        assert!(!render(source).contains("data-carve-loose"), "{source:?}");
    }
}

#[test]
fn the_carrier_over_a_bare_run_is_ignored() {
    let ignored = [
        "<ul data-carve-loose>\n  <li>a</li>\n  <li><p>b</p></li>\n</ul>",
        "<ul data-carve-loose>\n  <li><input type=\"checkbox\" disabled> a</li>\n  <li><blockquote><p>b</p></blockquote></li>\n</ul>",
        "<ul data-carve-loose>\n  <li><blockquote><p>a</p></blockquote> b <!-- c --></li>\n  <li>d</li>\n</ul>",
    ];
    for html in ignored {
        let out = import(html);
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == "import-carrier-ignored"),
            "{html}\n{:?}",
            out.diagnostics
        );
    }
    let accepted = [
        "<ul data-carve-loose>\n  <li><img src=\"i.png\" alt=\"\"><!-- c --></li>\n  <li><img src=\"j.png\" alt=\"\"></li>\n</ul>",
        "<ul data-carve-loose>\n  <li><!-- c --></li>\n  <li><blockquote><p>b</p></blockquote></li>\n</ul>",
    ];
    for html in accepted {
        let out = import(html);
        assert!(
            !out.diagnostics
                .iter()
                .any(|d| d.code == "import-carrier-ignored"),
            "{html}\n{:?}",
            out.diagnostics
        );
        assert!(out.source.contains("\n\n- "), "{html}\n{}", out.source);
    }
}

#[test]
fn both_carriers_are_both_ignored() {
    let out = import(
        "<ol data-carve-loose data-carve-tight>\n  <li><p>a</p></li>\n  <li><p>b</p></li>\n</ol>",
    );
    let ignored = out
        .diagnostics
        .iter()
        .filter(|d| d.code == "import-carrier-ignored")
        .count();
    assert_eq!(ignored, 2, "{:?}", out.diagnostics);
    // C2-M-033 decides: the items begin with `<p>`.
    assert_eq!(out.source, ". a\n\n. b\n");
}
