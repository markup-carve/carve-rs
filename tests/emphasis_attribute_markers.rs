use carve::{html_to_carve, to_carve, to_html, HtmlImportOptions};

#[test]
fn shared_html_import_target() {
    let html = r#"<p>a <strong><span id="id" key="*">b</span></strong></p>"#;
    let source = html_to_carve(html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert_eq!(source, "a {*[b]{#id key=\"*\"}*}\n");
    assert_eq!(to_html(&source).trim(), html);
}

#[test]
fn child_attribute_markers_survive_formatting() {
    for marker in ['*', '/', '_', '~', '=', '^', ',', '+', '-'] {
        let source = format!("{{{marker}[b]{{key=\"{marker}\"}}{marker}}}");
        let written = to_carve(&source);
        assert_eq!(to_html(&written), to_html(&source), "{source}");
        assert_eq!(to_carve(&written), written, "{source}");
    }
    for source in [
        r#"/*[b]{key="/"}*/"#,
        r#"/*[b]{key="*"}*/"#,
        r#"{_[b]{id="a_"}_}"#,
        r#"{_[b]{class="a_"}_}"#,
        r#"{*[b]{key="*}"}*}"#,
        r#"{*[/b/]{key="*"}*}"#,
    ] {
        let written = to_carve(source);
        assert_eq!(to_html(&written), to_html(source), "{source}");
        assert_eq!(to_carve(&written), written, "{source}");
    }
    assert_eq!(to_carve("*[b]{#id key=x}*"), "*[b]{#id key=x}*\n");
}
