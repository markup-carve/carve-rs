#[test]
fn formatting_preserves_rendered_content_and_is_idempotent() {
    for source in [
        "`\n\t> x\n",
        "~``` x\n[d]: u ```\n",
        "`\n``\n",
        "`\n``x\n",
        "1. [d]: u\n",
        "- A\n{x}\n*[A]: }\n",
    ] {
        let formatted = carve::to_carve(source);
        assert_eq!(
            carve::to_html(&formatted),
            carve::to_html(source),
            "{source:?}"
        );
        assert_eq!(carve::to_carve(&formatted), formatted, "{source:?}");
    }
}

#[test]
fn a_sigil_after_emphasis_does_not_force_braces() {
    for source in ["~s~@t\n", "~s~#t\n", "~s~:name:\n"] {
        assert_eq!(carve::to_carve(source), source);
    }
}
