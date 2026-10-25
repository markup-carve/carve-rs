use carve::{djot_to_carve, migrate_djot, migrate_html, HtmlImportOptions};

#[test]
fn recovers_djot_attribute_parts() {
    assert_eq!(
        migrate_djot("{#ch1.1}\n# Head\n").value,
        "{#ch1 .1}\n# Head\n"
    );
    assert_eq!(
        djot_to_carve("{#1start .2part}\n# Head\n"),
        "{#1start .2part}\n# Head\n"
    );
    assert_eq!(
        djot_to_carve("Word{#1start .2part}\n"),
        "[Word]{#1start .2part}\n"
    );
    let result = migrate_djot("{.good #bad!id .bad?class}\n# Head\n");
    assert_eq!(result.value, "{.good}\n# Head\n");
    let dropped: Vec<_> = result
        .report
        .diagnostics
        .iter()
        .filter(|d| d.code == "djot-attribute-dropped")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        dropped,
        [
            "Djot attribute id could not be represented in Carve and was dropped.",
            "Djot attribute class could not be represented in Carve and was dropped.",
        ]
    );
    let result = migrate_djot("Word{#part.2 .bad?class}\n");
    assert_eq!(result.value, "[Word]{#part .2}\n");
    assert!(result
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "djot-attribute-dropped" && d.message.contains("class")));
}

#[test]
fn imports_github_footnotes() {
    let html = "<p>x<sup><a href=\"#fn-1\" id=\"fnref-1\" data-footnote-ref>1</a></sup></p><section data-footnotes class=\"footnotes\"><h2 class=\"sr-only\">Footnotes</h2><ol><li id=\"fn-1\"><p>note <a href=\"#fnref-1\" data-footnote-backref class=\"data-footnote-backref\">↩</a></p></li></ol></section>";
    let value = migrate_html(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert!(value.contains("x[^1]"), "{value}");
    assert!(value.contains("[^1]: note"), "{value}");
    assert!(
        !value.contains("Footnotes") && !value.contains('↩'),
        "{value}"
    );

    let html = "<p>A<sup><a href=\"#note-x\" id=\"fnref-1\" data-footnote-ref>X</a></sup> B<sup><a href=\"#note-x\" id=\"fnref-1-2\" data-footnote-ref>X</a></sup></p><section data-footnotes><h3 class=\"sr-only\">Notes</h3><ol><li id=\"note-x\"><p>body <a href=\"#fnref-1\" data-footnote-backref>back</a></p></li></ol></section>";
    let value = migrate_html(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert!(value.contains("A[^1] B[^1]"), "{value}");
    assert!(value.contains("[^1]: body"), "{value}");
    assert!(
        !value.contains("Notes") && !value.contains("back"),
        "{value}"
    );
}
