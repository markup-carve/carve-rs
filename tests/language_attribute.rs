use carve::{parse, render_carve, render_html};

fn html(source: &str) -> String {
    render_html(&parse(&format!("{source}\n"))).unwrap()
}

#[test]
fn shorthand_desugars_to_lang() {
    assert_eq!(
        html("[x]{:fr}").trim(),
        r#"<p><span lang="fr">x</span></p>"#
    );
    assert_eq!(html("[x]{:}").trim(), r#"<p><span lang="">x</span></p>"#);
    assert_eq!(
        html("[x]{lang=de :fr}").trim(),
        r#"<p><span lang="fr">x</span></p>"#
    );
    assert_eq!(
        html("[x]{#id :fr .cls}").trim(),
        r#"<p><span id="id" lang="fr" class="cls">x</span></p>"#
    );
}

#[test]
fn malformed_tags_remain_literal() {
    for attribute in [
        ":tada:",
        ":en_US",
        ":en--GB",
        ":-en",
        ":en-",
        ":français",
        ":abcdefghi",
    ] {
        let output = html(&format!("[x]{{{attribute}}}"));
        assert!(!output.contains("<span"), "{attribute}: {output}");
        assert!(!output.contains("lang="), "{attribute}: {output}");
    }
}

#[test]
fn sigil_takes_no_padding() {
    assert_eq!(
        html("[x]{: fr}").trim(),
        r#"<p><span lang="" fr="">x</span></p>"#
    );
}

#[test]
fn canonical_writer_uses_shorthand() {
    assert_eq!(
        render_carve(&parse("[x]{lang=fr}\n")).unwrap(),
        "[x]{:fr}\n"
    );
    assert_eq!(
        render_carve(&parse("[x]{lang=\"\"}\n")).unwrap(),
        "[x]{:}\n"
    );
    assert_eq!(
        render_carve(&parse("[x]{:fr lang=de}\n")).unwrap(),
        "[x]{:de}\n"
    );
    assert_eq!(
        render_carve(&parse("[x]{lang=en_US}\n")).unwrap(),
        "[x]{lang=en_US}\n"
    );
}
