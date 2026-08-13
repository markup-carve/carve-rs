use carve::{parse, render_ansi, render_carve, render_plain_text, to_html};

#[test]
fn renders_the_fixed_registry_and_value_mappings() {
    let source = "[CSS]{dfn abbr=\"Cascading Style Sheets\"}\n[Noon]{time=\"12:00\"} [x]{code mark samp var kbd cite}";
    assert_eq!(
        to_html(source),
        "<p><dfn><abbr title=\"Cascading Style Sheets\">CSS</abbr></dfn>\n<time datetime=\"12:00\">Noon</time> <cite><kbd><var><samp><mark><code>x</code></mark></samp></var></kbd></cite></p>"
    );
}

#[test]
fn keeps_remaining_hardened_attributes_on_one_outer_span() {
    assert_eq!(
        to_html("[*Ctrl*+C]{#copy .shortcut kbd data-key=\"copy\" onclick=\"alert(1)\"}"),
        "<p><span id=\"copy\" class=\"shortcut\" data-key=\"copy\"><kbd><strong>Ctrl</strong>+C</kbd></span></p>"
    );
}

#[test]
fn preserves_non_html_targets() {
    let source = "[Ctrl]{kbd}";
    let doc = parse(source);
    assert_eq!(render_plain_text(&doc).unwrap(), "Ctrl\n");
    assert_eq!(render_ansi(&doc).unwrap(), "Ctrl\n");
    // PART 11 §6c: a value-less attribute comes back as the bare name, which is
    // also the form PART 9 §10 documents for this construct.
    assert_eq!(render_carve(&doc).unwrap(), "[Ctrl]{kbd}\n");
}

#[test]
fn leaves_unknown_and_case_variant_attributes_ordinary() {
    assert_eq!(
        to_html("[x]{widget KBD}"),
        "<p><span widget=\"\" KBD=\"\">x</span></p>"
    );
}

#[test]
fn explicit_abbr_takes_precedence_over_automatic_expansion() {
    assert_eq!(
        to_html("*[HTML]: Hyper Text Markup Language\n\n[HTML]{abbr=\"Custom\"}"),
        "<p><abbr title=\"Custom\">HTML</abbr></p>"
    );
}
