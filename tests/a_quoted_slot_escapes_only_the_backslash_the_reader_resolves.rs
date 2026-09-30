//! A quoted attribute value and a quoted title read `\X` as an escape only when
//! X is ASCII punctuation, so the writer doubles a backslash only where the
//! reader would otherwise resolve it.
//!
//! The trailing case is the one nothing else covers: the character after a
//! value's last backslash is the closing delimiter, which is punctuation, so a
//! bare backslash there swallows the delimiter and the value never terminates.

fn round_trip(source: &str) -> String {
    carve::to_carve(source)
}

#[test]
fn a_backslash_before_a_non_punctuation_character_writes_bare() {
    for source in [
        "[a](/u \"t\\zu\")\n",
        "[a]{k=\"t\\zu\"}\n",
        "[a](/u 't\\zu')\n",
    ] {
        let written = round_trip(source);
        assert!(
            !written.contains("t\\\\zu"),
            "the writer invented an escape the reader does not resolve: {written:?}"
        );
        assert_eq!(
            carve::to_html(&written),
            carve::to_html(source),
            "the bare spelling does not read back: {written:?}"
        );
    }
}

#[test]
fn a_trailing_backslash_keeps_its_partner() {
    for source in ["[a](/u \"t\\\\\")\n", "[a]{k=\"t\\\\\"}\n"] {
        let written = round_trip(source);
        assert!(
            written.contains("t\\\\"),
            "a trailing backslash lost its partner and swallows the delimiter: {written:?}"
        );
        assert_eq!(
            carve::to_html(&written),
            carve::to_html(source),
            "the written form does not read back: {written:?}"
        );
        assert_eq!(round_trip(&written), written, "not idempotent: {written:?}");
    }
}
