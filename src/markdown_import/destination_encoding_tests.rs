#[test]
fn destinations_keep_uri_encoding_and_email_schemes() {
    for (source, expected) in [
            ("[x](< >)", "<p><a href=\"%20\">x</a></p>"),
            ("![y](< >)", "<img src=\"%20\" alt=\"y\">"),
            ("[x](a%zz)", "<p><a href=\"a%zz\">x</a></p>"),
            ("[x](mailto:a@b.example)", "<p><a href=\"mailto:a@b.example\">x</a></p>"),
            ("<foo@bar.example.com>", "<p><a href=\"mailto:foo@bar.example.com\">foo@bar.example.com</a></p>"),
            ("<foo+special@Bar.baz-bar0.com>", "<p><a href=\"mailto:foo+special@Bar.baz-bar0.com\">foo+special@Bar.baz-bar0.com</a></p>"),
            ("<https://example.com?find=\\*>", "<p><a href=\"https://example.com?find=\\*\">https://example.com?find=\\*</a></p>"),
            ("<https://foo.bar.`baz>`", "<p><a href=\"https://foo.bar.`baz\">https://foo.bar.`baz</a>`</p>"),
            ("[x](<a b>)", "<p><a href=\"a%20b\">x</a></p>"),
            ("[x](<é/中?q=a&b=c>)", "<p><a href=\"é/中?q=a&amp;b=c\">x</a></p>"),
            ("a ![x](<a b>)", "<p>a <img src=\"a%20b\" alt=\"x\"></p>"),
            ("[x](a%20b)", "<p><a href=\"a%20b\">x</a></p>"),
            ("[x](<a'b>)", "<p><a href=\"a&apos;b\">x</a></p>"),
        ] {
            assert_eq!(crate::to_html(&super::markdown_to_carve(source)), expected, "{source}");
        }
}
