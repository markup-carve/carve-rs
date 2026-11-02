//! A refused `+` is a letter, so the writer escapes it where the block below
//! would attach, and adds no blank line (CARVE-P11-042, carve#4868).

fn fmt(source: &str) -> String {
    carve::to_carve(source)
}

#[test]
fn a_block_that_would_attach_gets_the_escape_and_no_blank() {
    for (source, expected) in [
        ("- - +\n\n  > q\n", "- - \\+\n  > q\n"),
        (
            "- - +\n\n  ```\n  x\n  ```\n",
            "- - \\+\n  ```\n  x\n  ```\n",
        ),
        ("- - +\n\n  # H\n", "- - \\+\n  # H\n"),
        ("- - +\n\n  {.c}\n  > q\n", "- - \\+\n  {.c}\n  > q\n"),
        ("- foo\n  1. +\n   > b\n", "- foo\n  1. \\+\n  > b\n"),
        ("> - - +\n>\n>   > q\n", "> - - \\+\n>   > q\n"),
    ] {
        let written = fmt(source);
        assert_eq!(written, expected, "{source:?}");
        assert_eq!(
            carve::to_html(&written),
            carve::to_html(source),
            "{source:?}"
        );
        assert_eq!(fmt(&written), written, "{source:?}");
    }
}

#[test]
fn a_block_that_cannot_attach_keeps_the_blank() {
    for source in [
        "- - +\n\n  para\n",
        "- - +\n\n  - z\n",
        "- - +\n\n  $$\n  x\n  $$\n",
    ] {
        assert_eq!(fmt(source), source);
    }
}
