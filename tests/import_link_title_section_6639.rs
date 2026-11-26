//! Markdown and Djot import of carve#6639 (a link title with line breaks), carve#6645 (a delimiter in a
//! Djot link destination) and carve#6657 (the attributes of a Djot section), at carve-js `next2`.

use carve::api::{Carve, ImportFormat, ImportOptions, LossOptions, WriteOptions};

fn import(source: &str, format: ImportFormat) -> (String, Vec<String>) {
    let carve = Carve::new();
    let imported = carve
        .import_from(
            source,
            format,
            &ImportOptions::default(),
            &LossOptions::default(),
        )
        .expect("the import succeeds");
    let mut codes: Vec<String> = imported
        .diagnostics
        .iter()
        .filter(|e| e.channel == "render-loss" || e.code == "import-attribute-dropped")
        .map(|e| e.code.clone())
        .collect();
    codes.sort();
    let source = carve
        .write_with_diagnostics(&imported.tree, &WriteOptions::default())
        .expect("the imported tree writes")
        .output;
    (source, codes)
}

#[test]
fn a_title_with_line_breaks_keeps_its_link() {
    let cases = [
        (
            ImportFormat::Markdown,
            "[foo]: /url '\ntitle\nline1\nline2\n'\n\n[foo]\n",
            "[foo](/url \" title line1 line2 \")\n",
        ),
        (
            ImportFormat::Markdown,
            "![a](/i \"p\r\nq\")\n",
            "![a](/i \"p q\")\n",
        ),
        (
            ImportFormat::Gfm,
            "> [a](/u 'x\n> y')\n",
            "> [a](/u \"x y\")\n",
        ),
    ];
    for (format, source, expected) in cases {
        let (written, codes) = import(source, format);
        assert_eq!(written, expected, "{source:?}");
        assert_eq!(codes, ["field-unspellable"], "{source:?}");
    }
}

#[test]
fn a_delimiter_in_a_djot_destination_closes_nothing_before_the_link() {
    let cases = [
        (
            "_[link](http://example.com?foo_bar=1), more text_\n",
            "/[link](http://example.com?foo_bar=1), more text/\n",
        ),
        ("*[closed](hello*)\n", "*[closed](hello*)\n"),
        ("_hello [link](a_b) world_\n", "/hello [link](a_b) world/\n"),
        ("{_a [l](x_}y) b_}\n", "/a [l](x_}y) b/\n"),
        ("_a ![i](x_y) b_\n", "/a ![i](x_y) b/\n"),
        ("[l](x_y_z) w\n", "[l](x_y_z) w\n"),
        ("[before [link](x]y) after\n", "[before [link](x]y) after\n"),
    ];
    for (source, expected) in cases {
        let (written, codes) = import(source, ImportFormat::Djot);
        assert_eq!(written, expected, "{source:?}");
        assert!(codes.is_empty(), "{source:?}: {codes:?}");
    }
}

#[test]
fn a_djot_section_gives_its_heading_only_the_id() {
    let cases: [(&str, &str, usize); 5] = [
        ("{#id .c k=v}\n# H\n", "{#id}\n= H\n", 2),
        (
            "{#id .c}\n# H\n\npara\n\n## Sub\n\nx\n",
            "{#id}\n= H\n\npara\n\n== Sub\n\nx\n",
            1,
        ),
        ("{#id}\n# H\n", "{#id}\n= H\n", 0),
        // Without an id djot.js leaves the attributes on the heading.
        ("{.c k=v}\n# H\n", "{.c k=v}\n= H\n", 0),
        (
            "# Top\n\n{#id k=v}\n## Sub\n\nx\n",
            "= Top\n\n{#id}\n== Sub\n\nx\n",
            1,
        ),
    ];
    for (source, expected, dropped) in cases {
        let (written, codes) = import(source, ImportFormat::Djot);
        assert_eq!(written, expected, "{source:?}");
        assert_eq!(
            codes,
            vec!["import-attribute-dropped"; dropped],
            "{source:?}"
        );
    }
}
