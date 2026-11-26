//! carve#6678: the style of an `align` key stands at that key's place when no authored `style` exists (C2-H-009, C2-H-015).

use carve::api::{Carve, Options};

fn html(source: &str) -> String {
    Carve::new()
        .render(source, &Options::default())
        .unwrap()
        .trim_end()
        .to_owned()
}

#[test]
fn align_style_stands_at_the_align_key() {
    let cases = [
        (
            "{align=center k=v}\nText.\n",
            r#"<p style="text-align: center;" k="v">Text.</p>"#,
        ),
        (
            "{k=v align=center}\nText.\n",
            r#"<p k="v" style="text-align: center;">Text.</p>"#,
        ),
        (
            "{.d align=Right data-x=1}\nText.\n",
            r#"<p class="d" style="text-align: right;" data-x="1">Text.</p>"#,
        ),
        (
            "{:de width=3 align=left role=note}\n:::\nInside.\n:::\n",
            "<div lang=\"de\" width=\"3\" style=\"text-align: left;\" role=\"note\">\n  <p>Inside.</p>\n</div>",
        ),
        (
            "{align=center width=3}\n= Heading\n",
            "<section id=\"Heading\">\n  <h1 style=\"text-align: center;\" width=\"3\">Heading</h1>\n</section>",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(html(source), expected, "{source:?}");
    }
}

#[test]
fn an_authored_style_keeps_its_place() {
    assert_eq!(
        html("{style=\"c: d\" k=v align=center}\nText.\n"),
        r#"<p style="c: d; text-align: center;" k="v">Text.</p>"#
    );
    assert_eq!(
        html("{align=center k=v style=\"c: d\"}\nText.\n"),
        r#"<p k="v" style="c: d; text-align: center;">Text.</p>"#
    );
}

#[test]
fn an_ordinary_align_stays_an_attribute() {
    assert_eq!(
        html("{align=justify k=v}\nText.\n"),
        r#"<p align="justify" k="v">Text.</p>"#
    );
}
