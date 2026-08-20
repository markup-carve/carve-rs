use carve::Options;

fn main() {
    let cases = [
        ("image + plain caption", "![alt](x.png)\n^ a caption\n"),
        (
            "image + # placeholder",
            "![alt](x.png)\n^ Figure #: a caption\n",
        ),
        ("image + bare # caption", "![alt](x.png)\n^ #: a caption\n"),
        ("permalink-less heading", "# Hello World\n"),
        ("footnote", "Text[^a]\n\n[^a]: Note body.\n"),
    ];
    let opts = Options::new();
    for (name, src) in cases {
        println!(
            "--- {name} ---\n{}\n",
            carve::to_html_with_options(src, &opts)
        );
    }
}
