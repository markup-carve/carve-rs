//! Markdown and Djot import of the cases where carve-rs and carve-js disagreed in
//! carve-conformance#47, at the readings djot.js, its test suite and C2-W-069 give.

use carve::api::{Carve, ImportFormat, ImportOptions, LossOptions, WriteOptions};

fn import(source: &str, format: ImportFormat) -> String {
    let carve = Carve::new();
    let imported = carve
        .import_from(
            source,
            format,
            &ImportOptions::default(),
            &LossOptions::default(),
        )
        .expect("the import succeeds");
    carve
        .write_with_diagnostics(&imported.tree, &WriteOptions::default())
        .expect("the imported tree writes")
        .output
}

#[test]
fn imports_follow_the_source_format() {
    let cases = [
        (
            ImportFormat::Djot,
            "{#id .class\n  style=\"color:red\"}\nA paragraph\n",
            "{#id .class style=color:red}\nA paragraph\n",
        ),
        (
            ImportFormat::Djot,
            "> {key=\"bar\n>    a\\$bim\"}\n> ou\n",
            "> {key=\"bar a$bim\"}\n> ou\n",
        ),
        (
            ImportFormat::Djot,
            "{a=\" inline text\n",
            "{a=“ inline text\n",
        ),
        (ImportFormat::Djot, "> # Heading\n", "> = Heading\n"),
        (
            ImportFormat::Djot,
            ": apple\n\n  Paragraph\n\n: orange\n",
            ":: apple\n: Paragraph\n:: orange\n: %%\n",
        ),
        (ImportFormat::Djot, "ab \t \\  \t\nc\n", "ab\\\nc\n"),
        (
            ImportFormat::Djot,
            "*[closed](hello\\*)\n",
            "*[closed](hello*)\n",
        ),
        (
            ImportFormat::Djot,
            "[w](https://en.wikipedia.org/wiki/Foo_(bar)) now\n",
            "[w](https://en.wikipedia.org/wiki/Foo_(bar)) now\n",
        ),
        (ImportFormat::Djot, "\\\\$`e=mc^2`\n", "\\\\$`e=mc^2`\n"),
        (ImportFormat::Djot, "\\$`e=mc^2`\n", "\\$`e=mc^2`\n"),
        (ImportFormat::Djot, "\n|`|\n", "|`|`\n"),
        (
            ImportFormat::Djot,
            "|a|b|\n|c|`d\n",
            "| a | b |\n\n|c|`d`\n",
        ),
        (
            ImportFormat::Markdown,
            "*foo**bar**baz*\n",
            "/foo{*bar*}baz/\n",
        ),
        (
            ImportFormat::Markdown,
            "**foo*bar*baz**\n",
            "*foo{/bar/}baz*\n",
        ),
        (ImportFormat::Djot, "`c`\\\nc\n", "`c`\\\nc\n"),
        (ImportFormat::Djot, "{'b\n", "‘b\n"),
    ];
    for (format, source, expected) in cases {
        assert_eq!(import(source, format), expected, "{source:?}");
    }
}
