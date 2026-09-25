//! `style` reaches the report through the refusal policy every other attribute
//! goes through (markup-carve/carve#2267, clause `docs/html-import-contract.md`
//! under *A refused declaration in `style` is a refused attribute*, ticket
//! markup-carve/carve-rs#1892).
//!
//! Inside an element `roundtrip` keeps whole, no CSS mapping runs, so
//! `style-unmapped` there named a mapping that did not happen - and this engine
//! wrote that row from the walk, unowned, so `keep_raw` could not take it back:
//! the kept element's own `style` was reported under the wrong code, ahead of
//! the element's own rows, and a descendant's `style` was reported not at all.
//!
//! The rows are asserted WHOLE and in ORDER - code, severity, fidelity,
//! confidence, path, message - because the code alone passed while the row sat
//! in the wrong position and the descendant was silent, and the spec repo's
//! cross-engine gate compares all six fields in document order.

use carve::html_import::{html_to_carve, HtmlImportMode, HtmlImportOptions};

type Row = (String, String, String, String, String, String);

fn import(html: &str, mode: HtmlImportMode) -> (String, Vec<Row>) {
    let options = HtmlImportOptions {
        mode,
        ..Default::default()
    };
    let result = html_to_carve(html, &options).unwrap();
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.as_str().to_owned(),
                d.severity.as_str().to_owned(),
                d.fidelity.as_str().to_owned(),
                d.confidence.as_str().to_owned(),
                d.path.clone().unwrap_or_default(),
                d.message.clone(),
            )
        })
        .collect();

    (result.value, rows)
}

fn rows(html: &str) -> Vec<Row> {
    import(html, HtmlImportMode::Roundtrip).1
}

fn row(severity: &str, fidelity: &str, path: &str, message: &str) -> Row {
    (
        "attribute-preserved".to_owned(),
        severity.to_owned(),
        fidelity.to_owned(),
        "exact".to_owned(),
        path.to_owned(),
        message.to_owned(),
    )
}

fn preserved(severity: &str, path: &str, message: &str) -> Row {
    row(severity, "preserved", path, message)
}

fn raw_preserved(path: &str, tag: &str) -> Row {
    (
        "raw-preserved".to_owned(),
        "warning".to_owned(),
        "degraded".to_owned(),
        "exact".to_owned(),
        path.to_owned(),
        format!("Preserved unsupported <{tag}> element as raw HTML"),
    )
}

/// The ticket's payload: a refused declaration on the kept element AND on a
/// descendant, each with its own reason.
const REPRO: &str = concat!(
    r#"<form style="background:url(javascript:x)" onclick="y()">"#,
    r#"<p style="width:expression(alert(1))">a</p></form>"#
);

#[test]
fn both_styles_in_the_repro_are_reported_where_the_contract_puts_them() {
    assert_eq!(
        rows(REPRO),
        vec![
            preserved(
                "error",
                "/form[1]",
                "Preserved style with a denied URL scheme in a declaration value on <form> \
                 in the raw HTML this element is kept as"
            ),
            preserved(
                "error",
                "/form[1]",
                "Preserved event-handler attribute onclick on <form> \
                 in the raw HTML this element is kept as"
            ),
            raw_preserved("/form[1]", "form"),
            preserved(
                "error",
                "/form[1]/p[1]",
                "Preserved style with a construct the CSS sanitizer refuses on <p> \
                 inside the raw HTML <form> is kept as"
            ),
        ]
    );
}

/// Nothing is removed: the ruling is about the report, not about the bytes
/// (markup-carve/carve#2261).
#[test]
fn the_kept_bytes_still_carry_both_declarations() {
    let (value, report) = import(REPRO, HtmlImportMode::Roundtrip);
    assert!(
        value.contains(r#"style="background:url(javascript:x)""#),
        "{value}"
    );
    assert!(
        value.contains(r#"style="width:expression(alert(1))""#),
        "{value}"
    );
    assert!(
        !report.iter().any(|(code, ..)| code == "style-unmapped"),
        "{report:?}"
    );
}

/// CONTROL FOR THE CLASS BOUNDARY. Without it, a change that marks every kept
/// `style` an error passes the assertion above.
#[test]
fn benign_css_in_kept_bytes_is_info() {
    assert_eq!(
        rows(r#"<form style="color:red"><a href="/ok" style="color:blue">t</a></form>"#),
        vec![
            preserved(
                "info",
                "/form[1]",
                "Preserved style on <form> in the raw HTML this element is kept as"
            ),
            raw_preserved("/form[1]", "form"),
            preserved(
                "info",
                "/form[1]/a[1]",
                "Preserved style on <a> inside the raw HTML <form> is kept as"
            ),
        ]
    );
}

/// CONTROL FOR THE ROUTING. Outside kept bytes the CSS mapping does run, so an
/// unmapped declaration is still `style-unmapped` in both modes that map CSS.
#[test]
fn a_style_outside_kept_bytes_stays_style_unmapped() {
    for mode in [HtmlImportMode::Roundtrip, HtmlImportMode::Semantic] {
        let (value, report) = import(r#"<p style="color:red">x</p>"#, mode);
        assert_eq!(
            report,
            vec![(
                "style-unmapped".to_owned(),
                "info".to_owned(),
                "degraded".to_owned(),
                "exact".to_owned(),
                "/p[1]".to_owned(),
                "CSS declarations were not mapped".to_owned(),
            )],
            "{mode:?}"
        );
        assert_eq!(value, "x\n", "{mode:?}");
    }
}

/// `safe` and `semantic` UNWRAP the form, so no bytes are kept and the mapping
/// is the only reading - the state the routing must not reach outside the
/// exemption. Both styles are named, both elements are reported, and the
/// dangerous CSS does not survive.
#[test]
fn the_modes_that_keep_no_bytes_still_read_the_mapping() {
    for mode in [HtmlImportMode::Safe, HtmlImportMode::Semantic] {
        let (value, report) = import(REPRO, mode);
        assert!(
            !report
                .iter()
                .any(|(code, ..)| code == "attribute-preserved"),
            "{mode:?}: {report:?}"
        );
        assert_eq!(
            report
                .iter()
                .filter(|(code, ..)| code == "style-unmapped")
                .map(|(.., path, _)| path.as_str())
                .collect::<Vec<_>>(),
            vec!["/form[1]", "/form[1]/p[1]"],
            "{mode:?}"
        );
        assert!(!value.contains("expression("), "{mode:?}: {value}");
    }
}

/// The reason comes from a closed set of two, so every value lands in one of
/// three buckets. A `url(...)` with no denied scheme is what shows the two
/// `error` reasons are not the same test: the sanitizer blanks the value for the
/// construct, and no scheme in it is denied.
#[test]
fn every_value_reads_as_one_of_three_classes() {
    let cases = [
        (
            "background:url(javascript:x)",
            "error",
            "style with a denied URL scheme in a declaration value",
        ),
        (
            "background:url(vbscript:x)",
            "error",
            "style with a denied URL scheme in a declaration value",
        ),
        // A `)` inside a quoted argument does not end the argument, so the
        // scheme is still the one the reader sees.
        (
            "background:url('javascript:x)y')",
            "error",
            "style with a denied URL scheme in a declaration value",
        ),
        (
            "width:expression(alert(1))",
            "error",
            "style with a construct the CSS sanitizer refuses",
        ),
        (
            "background:url(pic.png)",
            "error",
            "style with a construct the CSS sanitizer refuses",
        ),
        (
            "behavior:url(x.htc)",
            "error",
            "style with a construct the CSS sanitizer refuses",
        ),
        // BOTH READINGS READ THE TEXT THE RENDERER ACTS ON, which is comment
        // stripped and escape decoded (carve-rs#1921, after carve-php#2391 named
        // the preprocessing as one seam). Read as written instead, a
        // commented-out denied URL took the scheme reason about a declaration
        // the renderer writes back untouched, and an escaped scheme was
        // invisible to the scan and took the weaker of the two reasons.
        ("/* url(javascript:x) */ color:red", "info", "style"),
        (
            r"background:url(javas\63ript:x)",
            "error",
            "style with a denied URL scheme in a declaration value",
        ),
        // The comment strip is not a licence to under-report: the same denied
        // URL outside the comment still carries the scheme reason.
        (
            "/* c */ background:url(javascript:x)",
            "error",
            "style with a denied URL scheme in a declaration value",
        ),
        // And an escaped construct with no URL in it stays on the second reason,
        // because the set is closed at two (markup-carve/carve#2267).
        (
            r"width:expr\65 ssion(alert(1))",
            "error",
            "style with a construct the CSS sanitizer refuses",
        ),
        ("color:red", "info", "style"),
        ("text-align:left", "info", "style"),
        // The sanitizer answers `""` for an empty value as well as for a blanked
        // one, so an empty `style` is the case that reads refused from the
        // answer alone. It is one row like any other `style` in the bytes.
        ("", "info", "style"),
        ("   ", "info", "style"),
    ];
    for (declaration, severity, subject) in cases {
        assert_eq!(
            rows(&format!(r#"<form style="{declaration}">t</form>"#))[0],
            preserved(
                severity,
                "/form[1]",
                &format!("Preserved {subject} on <form> in the raw HTML this element is kept as")
            ),
            "style=\"{declaration}\""
        );
    }
}

/// The inline arm keeps a raw span through its own walk, which used to keep the
/// rows that walk had already written.
#[test]
fn the_inline_arm_reports_a_style_in_its_kept_bytes() {
    assert_eq!(
        rows(
            r#"<p>a <output style="color:red"><b style="width:expression(1)">t</b></output> b</p>"#
        ),
        vec![
            preserved(
                "info",
                "/p[1]/output[2]",
                "Preserved style on <output> in the raw HTML this element is kept as"
            ),
            raw_preserved("/p[1]/output[2]", "output"),
            preserved(
                "error",
                "/p[1]/output[2]/b[1]",
                "Preserved style with a construct the CSS sanitizer refuses on <b> \
                 inside the raw HTML <output> is kept as"
            ),
        ]
    );
}

/// POSITION IS HALF THE TICKET. A `style` row is ordered like any other
/// refusal - by the column the element spells the attribute in, not by the class
/// the importer put it in - and it used to sort ahead of every one of them.
#[test]
fn the_style_row_sits_where_the_element_spells_the_attribute() {
    let subjects = |html: &str| {
        rows(html)
            .into_iter()
            .take(2)
            .map(|(.., message)| message)
            .collect::<Vec<_>>()
    };
    let style = "Preserved style on <form> in the raw HTML this element is kept as";
    let handler =
        "Preserved event-handler attribute onclick on <form> in the raw HTML this element is kept as";
    assert_eq!(
        subjects(r#"<form style="color:red" onclick="a()">t</form>"#),
        vec![style, handler]
    );
    assert_eq!(
        subjects(r#"<form onclick="a()" style="color:red">t</form>"#),
        vec![handler, style]
    );
}

/// carve-rs#1921: the ticket's own two values, WHOLE, because only the class and
/// the reason move and the code was already right.
///
/// The commented one is the case that made the report describe something the
/// renderer did not do: the declaration reaches the kept bytes untouched, so an
/// `error` about a denied scheme in it named a danger that is not there.
#[test]
fn a_commented_denied_url_is_not_a_danger_and_an_escaped_one_is_the_scheme() {
    let commented = r#"<form style="color:red;/*url(javascript:x)*/" onclick="y()">a</form>"#;
    assert_eq!(
        rows(commented),
        vec![
            preserved(
                "info",
                "/form[1]",
                "Preserved style on <form> in the raw HTML this element is kept as"
            ),
            preserved(
                "error",
                "/form[1]",
                "Preserved event-handler attribute onclick on <form> \
                 in the raw HTML this element is kept as"
            ),
            raw_preserved("/form[1]", "form"),
        ]
    );
    // What the row is now honest about: the bytes carry the declaration as
    // written, and the renderer leaves it alone.
    assert!(import(commented, HtmlImportMode::Roundtrip)
        .0
        .contains(r#"style="color:red;/*url(javascript:x)*/""#));
    assert!(
        carve::to_html(r#"[a]{style="color:red;/*url(javascript:x)*/"}"#)
            .contains(r#"style="color:red;/*url(javascript:x)*/""#)
    );

    assert_eq!(
        rows(r#"<form style="background:url(java\73 cript:x)">a</form>"#)[0],
        preserved(
            "error",
            "/form[1]",
            "Preserved style with a denied URL scheme in a declaration value on <form> \
             in the raw HTML this element is kept as"
        )
    );
}

/// THE CLASS IS A BICONDITIONAL AGAINST THE SANITIZER, not an imitation of it:
/// the row is `error` exactly where the renderer blanks the value. Pinned over
/// the same table as the reasons above so a later value cannot drift from it.
#[test]
fn the_row_is_an_error_exactly_where_the_renderer_blanks_the_value() {
    for value in [
        "color:red",
        "color:red;/*url(javascript:x)*/",
        "color:red;/*expression(alert(1))*/",
        "/*c*/color:red",
        "background:url(javascript:x)",
        r"background:url(java\73 cript:x)",
        r"background:\75 rl(javascript:x)",
        "background:url(pic.png)",
        "width:expression(alert(1))",
        r"width:expr\65 ssion(alert(1))",
        "behavior:url(x.htc)",
        "-moz-binding:url(#x)",
        "@import url(x.css)",
        // A denied scheme at the HEAD of the value, with no `url(...)` in it, is
        // blanked and takes the second reason. The set is closed at two, so it
        // carries every refusal the first does not name.
        "javascript:alert(1)",
        "",
    ] {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        let blanked = !value.is_empty()
            && carve::to_html(&format!("[a]{{style=\"{escaped}\"}}")).contains(r#"style="""#);
        let (_, severity, ..) = rows(&format!(r#"<form style="{value}">t</form>"#))[0].clone();
        assert_eq!(
            severity == "error",
            blanked,
            "style=\"{value}\": row said {severity}, the renderer blanked={blanked}"
        );
    }
}
