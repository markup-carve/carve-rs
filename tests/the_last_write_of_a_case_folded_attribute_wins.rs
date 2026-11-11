//! markup-carve/carve#5982 (PART 10 section 1): spellings of one attribute name
//! fold by case into one attribute, at the first spelling's place, with the
//! value of the LAST write, `#id` included.

#[test]
fn the_last_write_wins_across_spellings() {
    for (source, html) in [
        (
            "[x](/u){#a ID=b #c}\n",
            "<p><a href=\"/u\" id=\"c\">x</a></p>",
        ),
        ("[x](/u){#a ID=b}\n", "<p><a href=\"/u\" id=\"b\">x</a></p>"),
        ("[x](/u){ID=b #a}\n", "<p><a href=\"/u\" ID=\"a\">x</a></p>"),
        (
            "[x](/u){ID=b #a ID=c}\n",
            "<p><a href=\"/u\" ID=\"c\">x</a></p>",
        ),
        (
            "[x](/u){#a Id=b #c}\n",
            "<p><a href=\"/u\" id=\"c\">x</a></p>",
        ),
        (
            "[x](/u){#a #c ID=b}\n",
            "<p><a href=\"/u\" id=\"b\">x</a></p>",
        ),
        (
            "[x](/u){ID=b id=c}\n",
            "<p><a href=\"/u\" ID=\"c\">x</a></p>",
        ),
        (
            "[x](/u){#a id=b #c}\n",
            "<p><a href=\"/u\" id=\"c\">x</a></p>",
        ),
        (
            "[x](/u){#a ID=b}{#c}\n",
            "<p><a href=\"/u\" id=\"c\">x</a></p>",
        ),
        ("{#a ID=b}\n{#c}\npara\n", "<p id=\"c\">para</p>"),
        (
            "[x](/u){title=a TITLE=b title=c}\n",
            "<p><a href=\"/u\" title=\"c\">x</a></p>",
        ),
        (
            "[t]{:de LANG=fr :it}\n",
            "<p><span lang=\"it\">t</span></p>",
        ),
    ] {
        assert_eq!(carve::to_html(source).trim_end(), html, "{source:?}");
    }
}

#[test]
fn the_ast_records_each_slot_once() {
    let json = carve::to_json(&carve::parse("[x](/u){#a ID=b #c}\n"));
    assert!(json.contains("\"order\":[\"#id\",\"ID\"]"), "{json}");
}
