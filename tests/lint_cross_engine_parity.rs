use carve::lint_carve;

fn findings(source: &str, rule: &str) -> Vec<(usize, usize)> {
    lint_carve(source)
        .into_iter()
        .filter(|w| w.rule == rule)
        .map(|w| (w.line, w.column))
        .collect()
}

#[test]
fn an_unused_definition_is_reported_at_its_own_bracket() {
    let rule = "unused-link-reference-definition";
    assert_eq!(findings("> [r]: /u\n", rule), [(1, 3)]);
    assert_eq!(findings("- a\n  [r]: /u\n more\ntail\n", rule), [(2, 3)]);
    assert_eq!(findings("* * [d]: u\n :\n", rule), [(1, 5)]);
    assert_eq!(
        findings("- > - para\n  >   [r]: /u\ntail\n", rule),
        [(2, 7)]
    );
    assert_eq!(findings("[r]: /u\n", rule), [(1, 1)]);
}

#[test]
fn a_definition_used_by_an_empty_destination_or_a_captioned_image_is_used() {
    let rule = "unused-link-reference-definition";
    assert_eq!(findings("[r]:\n\n[x][r]\n", rule), []);
    assert_eq!(findings("[r]:   \n\n[x][r]\n", rule), []);
    assert_eq!(findings("![a][ok]\n^ cap\n\n[ok]: /p.png\n", rule), []);
    assert_eq!(findings("![t[z]][r]\n^ cap\n\n[r]: /i.png\n", rule), []);
}

#[test]
fn an_emphasis_opener_inside_a_destination_or_title_is_not_reported() {
    let rule = "unclosed-inline-emphasis";
    for source in [
        "![a](/p.png \"T\")\n",
        "a ![t[z]](/i.png) b\n",
        "a ![t`]`z](/i.png) b\n",
        "a ![t{# ] #}z](/i.png) b\n",
        "![a\nb](/i)\n",
        "![a](/i \"t\nu\")\n",
        "[![a](/i)]{.c}\n",
        "![a](/i 't\\'u')\n",
        "![a](/i){???}\n",
        "[t](/u 'T'   ) and ![t](/img \"T\"  )\n",
    ] {
        assert_eq!(findings(source, rule), [], "{source:?}");
    }
}

#[test]
fn a_delimiter_inside_code_or_a_cross_reference_does_not_close_an_opener() {
    let rule = "unclosed-inline-emphasis";
    assert_eq!(findings("*a ` b*\n", rule), [(1, 1)]);
    assert_eq!(findings("x /a </#nothing> y\n", rule), [(1, 3)]);
    assert_eq!(
        findings("{#intro}\n# Intro\n\nx /a </#intro> y\n", rule),
        [(4, 3)]
    );
    assert_eq!(findings("*a b*\n", rule), []);
}

#[test]
fn a_parsed_autolink_is_not_a_bare_html_tag() {
    assert_eq!(findings("<vbscript:msgbox>\n", "bare-html-tag"), []);
    assert_eq!(findings("a <span> b\n", "bare-html-tag"), [(1, 3)]);
}

#[test]
fn a_heading_in_an_unused_footnote_is_no_cross_reference_target() {
    assert_eq!(
        findings("[^n]: # H\n\n</#H>\n", "broken-crossref"),
        [(3, 1)]
    );
    assert_eq!(
        findings("[^n]: # H\n\n</#H> see[^n]\n", "broken-crossref"),
        []
    );
    assert_eq!(
        carve::to_html("[^n]: # H\n\n</#H>\n").trim(),
        "<p>&lt;/#H&gt;</p>"
    );
}
