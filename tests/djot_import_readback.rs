/// A Djot autolink body that already carries a scheme is a URL, so the importer must not
/// copy djot.js's second `mailto:` (jgm/djot.js#162). Such an address only keeps its
/// autolink where Carve reads one back: a dash run or an ellipsis becomes punctuation.
#[test]
fn djot_address_autolinks_keep_a_single_scheme() {
    for (source, expected) in [
        ("<mailto:x@y>\n", "<mailto:x@y>\n"),
        ("<http://u@x/y>\n", "<http://u@x/y>\n"),
        ("<mailto:a'tis@b.c>\n", "<mailto:a'tis@b.c>\n"),
        (
            "<mailto:a--@b.c>\n",
            "[mailto\\:a\\-\\-\\@b\\.c](mailto:a--@b.c)\n",
        ),
        (
            "<mailto:a...@b.c>\n",
            "[mailto\\:a\\.\\.\\.\\@b\\.c](mailto:a...@b.c)\n",
        ),
        (
            "<x:y[^a|b]@z.io>\n",
            "[x\\:y\\[\\^a\\|b\\]\\@z\\.io](x:y%5B^a%7Cb%5D@z.io)\n",
        ),
        ("<a@b.c>\n", "<a@b.c>\n"),
    ] {
        let value = carve::djot_to_carve(source);
        assert_eq!(value, expected, "{source:?}");
        assert!(
            !carve::to_html(&value).contains("mailto:mailto:"),
            "{source:?}: {value}"
        );
    }
}

/// Djot attaches an attribute line with no block after it to nothing, so the importer
/// consumes it before writing any closer, synthetic or real.
#[test]
fn djot_trailing_orphan_attribute_lines_reach_no_closer() {
    for source in [
        "::: foo\n# b\n{.c}\n:::\n",
        "::: foo\n# b\n{.c}\n",
        "::::: foo\n::: bar\n# b\n{.c}\n",
    ] {
        let value = carve::djot_to_carve(source);
        assert!(!value.contains("{.c}"), "{source:?}: {value}");
        assert!(
            !carve::to_html(&value).contains("b-c"),
            "{source:?}: {value}"
        );
    }
}

/// A brace that fails to parse as attributes is literal Djot text, and so is any brace
/// after it in the same run, or a later pass reads `{"` as a forced quote and eats it.
#[test]
fn djot_rejected_attribute_runs_keep_every_brace() {
    for (source, braces) in [("[t]{k=\"{\"\"}\n", 2), ("para [t]{k=\"{\"\"} tail\n", 2)] {
        let value = carve::djot_to_carve(source);
        assert_eq!(value.matches("\\{").count(), braces, "{source:?}: {value}");
    }
}

/// An empty Djot attribute block is preserved as a comment, which already separates the
/// spans around it, so the emphasis beside it needs no braced form.
#[test]
fn djot_empty_attributes_leave_adjacent_emphasis_bare() {
    for (source, expected) in [
        ("*a*{}*b*\n", "*a*{%%}*b*\n"),
        ("_a_{}_b_\n", "/a/{%%}/b/\n"),
        ("[x]{}*s*\n", "[x]{}*s*\n"),
        ("{}\nhi\n", "%%\nhi\n"),
    ] {
        assert_eq!(carve::djot_to_carve(source), expected, "{source:?}");
    }
}

/// A line the code mask hides is fenced payload, so its pipes are not table edges and
/// must not pick up an escape.
#[test]
fn djot_table_edge_escapes_leave_fenced_payloads_alone() {
    assert_eq!(
        carve::djot_to_carve("```\n|`x`|\n```\n"),
        "```\n|`x`|\n```\n"
    );
    assert_eq!(carve::djot_to_carve("para\n|`x`|\n"), "para\n\\|`x`|\n");
}
