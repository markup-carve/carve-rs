use carve::{to_html_with_options, Options, Spoiler};

#[test]
fn spoiler_classes_keep_the_authored_slot_or_follow_authored_attributes() {
    let extension = Spoiler::new();
    let options = Options::new().with_extension(&extension);
    for (attributes, opening) in [
        (
            "{#s data-x=1}",
            "<details id=\"s\" data-x=\"1\" class=\"spoiler\">",
        ),
        (
            "{#s .x data-x=1}",
            "<details id=\"s\" class=\"spoiler x\" data-x=\"1\">",
        ),
        (
            "{data-x=1 #s .x}",
            "<details data-x=\"1\" id=\"s\" class=\"spoiler x\">",
        ),
    ] {
        let source = format!("{attributes}\n::: spoiler \"Answer\"\nHidden.\n:::\n");
        assert!(to_html_with_options(&source, &options).starts_with(opening));
    }
}
