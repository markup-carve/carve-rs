//! An opening `(`, `"` or `'` after whitespace or the start of content counts as
//! whitespace for the flag test (markup-carve/carve#4813).

use carve::to_html;

#[test]
fn a_flag_after_spaced_opening_punctuation_stays_literal() {
    for (source, expected) in [
        (
            "use (--force) or \"--force\" or [--force] or --force\n",
            "<p>use (--force) or “--force” or [--force] or --force</p>",
        ),
        ("(--x)\n", "<p>(--x)</p>"),
        ("a '--x'\n", "<p>a ‘--x’</p>"),
        ("a {%c%}(--x)\n", "<p>a (--x)</p>"),
        ("[(--x)] y\n", "<p>[(--x)] y</p>"),
        ("{_(--x)_}\n", "<p><u>(--x)</u></p>"),
        (
            "range (1--10) and a(--x)\n",
            "<p>range (1–10) and a(–x)</p>",
        ),
        ("a ((--x))\n", "<p>a ((–x))</p>"),
        ("a (-- x)\n", "<p>a (– x)</p>"),
    ] {
        assert_eq!(to_html(source).trim(), expected, "{source:?}");
    }
}
