//! carve#4806: the bare delimiter guards read a whole Unicode letter or number.

use carve::{parse, render_carve, to_html};

#[test]
fn a_unicode_word_holds_its_delimiters_as_text() {
    let source = "Москва/Питер/Киев\n日本/中国/韩国\nснейк_кейс_тут\nä/ö/ü\n𝐀*x*𝐀\n";
    assert_eq!(to_html(source).trim(), format!("<p>{}</p>", source.trim()));
}

#[test]
fn a_flanking_unicode_word_blocks_each_way_in() {
    for source in ["é/x/{.k}a", "é*x*", "*x*é", "٣_x_"] {
        assert_eq!(to_html(source).trim(), format!("<p>{source}</p>"));
    }
}

#[test]
fn punctuation_and_braces_still_open() {
    assert_eq!(to_html("€/x/€").trim(), "<p>€<em>x</em>€</p>");
    assert_eq!(to_html("中{/国/}中").trim(), "<p>中<em>国</em>中</p>");
}

#[test]
fn the_writer_braces_emphasis_next_to_a_unicode_word() {
    let written = render_carve(&parse("中{/国/}中")).unwrap();
    assert_eq!(to_html(&written), to_html("中{/国/}中"));
}
