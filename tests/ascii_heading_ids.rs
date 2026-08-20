//! The opt-in ASCII-folding heading-id transform, and its cross-engine contract.
//!
//! WHAT MAKES THIS A CONTRACT AND NOT A PREFERENCE. carve-js `asciiHeadingIds`
//! and carve-php `AsciiHeadingIdsExtension` carry the SAME 903-entry
//! transliteration table, verified key-for-key and value-for-value when this
//! landed, and carve-php's default path is the baked table rather than ICU
//! (ICU is opt-in there precisely so an id does not depend on whether ext-intl
//! is installed). So the three engines can agree byte for byte, and the table
//! below is the measurement: every row was produced by running carve-js in both
//! modes over the same input, and carve-php was checked against the strict
//! column on all 26 - zero mismatches.
//!
//! THE TWO MODES ARE NOT A CARVE-RS INVENTION. carve-js has offered `fold` and
//! `strict` from the start; carve-php implements exactly `strict`. The issue
//! that asked for this (markup-carve/carve-rs#1159) read the two engines as
//! DISAGREEING about unmappable scripts - they do not, they expose the same
//! disagreement as two named modes, and this offers both.

use carve::{AsciiHeadingIds, Options};

fn id(text: &str, ascii: AsciiHeadingIds, lowercase: bool) -> String {
    let options = Options::new()
        .with_ascii_heading_ids(ascii)
        .with_lowercase_heading_ids(lowercase);
    let html = carve::to_html_with_options(&format!("# {text}\n"), &options);
    let start = html.find("id=\"").expect("a heading carries an id") + 4;
    let rest = &html[start..];
    rest[..rest.find('"').expect("the id is quoted")].to_string()
}

/// (source text, default id, folded id, strict id)
const PARITY: &[(&str, &str, &str, &str)] = &[
    (
        "Ärger mit Umlauten",
        "Ärger-mit-Umlauten",
        "Arger-mit-Umlauten",
        "Arger-mit-Umlauten",
    ),
    ("Grüße", "Grüße", "Grusse", "Grusse"),
    ("Straße", "Straße", "Strasse", "Strasse"),
    ("Œuvre æsop", "Œuvre-æsop", "OEuvre-aesop", "OEuvre-aesop"),
    ("Łódź", "Łódź", "Lodz", "Lodz"),
    ("Ñandú", "Ñandú", "Nandu", "Nandu"),
    (
        "Crème brûlée",
        "Crème-brûlée",
        "Creme-brulee",
        "Creme-brulee",
    ),
    ("Café 42", "Café-42", "Cafe-42", "Cafe-42"),
    ("ĐàNẵng", "ĐàNẵng", "DaNang", "DaNang"),
    ("Ãpple", "Ãpple", "Apple", "Apple"),
    ("ÿ ø å", "ÿ-ø-å", "y-o-a", "y-o-a"),
    ("Ωmega", "Ωmega", "Ωmega", "mega"),
    ("北京 city", "北京-city", "北京-city", "city"),
    ("日本語", "日本語", "日本語", "s"),
    ("Привет мир", "Привет-мир", "Privet-mir", "Privet-mir"),
    ("Ça va", "Ça-va", "Ca-va", "Ca-va"),
    (
        "Þórr í Ásgarði",
        "Þórr-í-Ásgarði",
        "THorr-i-Asgardi",
        "THorr-i-Asgardi",
    ),
    (
        "1 leading digit",
        "s-1-leading-digit",
        "s-1-leading-digit",
        "s-1-leading-digit",
    ),
    ("---", "s", "s", "s"),
    (
        "Æther & Œther",
        "Æther-Œther",
        "AEther-OEther",
        "AEther-OEther",
    ),
    ("Hello World", "Hello-World", "Hello-World", "Hello-World"),
    ("İstanbul", "İstanbul", "Istanbul", "Istanbul"),
    ("Ǆ digraph", "Ǆ-digraph", "DZ-digraph", "DZ-digraph"),
    ("ﬁ ligature", "ﬁ-ligature", "ﬁ-ligature", "ligature"),
    ("½ portion", "s-½-portion", "s-1-2-portion", "s-1-2-portion"),
    (
        "Ötzi's Straße",
        "Ötzi-s-Straße",
        "Otzi-s-Strasse",
        "Otzi-s-Strasse",
    ),
];

#[test]
fn every_mode_matches_carve_js_and_carve_php() {
    for (text, off, fold, strict) in PARITY {
        assert_eq!(&id(text, AsciiHeadingIds::Off, false), off, "off: {text}");
        assert_eq!(
            &id(text, AsciiHeadingIds::Fold, false),
            fold,
            "fold: {text}"
        );
        assert_eq!(
            &id(text, AsciiHeadingIds::Strict, false),
            strict,
            "strict: {text}"
        );
    }
}

#[test]
fn the_default_is_off_so_nothing_moves_unasked() {
    let plain = carve::to_html("# Grüße\n");
    assert!(plain.contains(r#"id="Grüße""#), "got {plain}");
    assert_eq!(id("Grüße", AsciiHeadingIds::Off, false), "Grüße");
}

#[test]
fn strict_guarantees_ascii_where_fold_only_tries() {
    // The whole difference between the modes, on one input: a script the table
    // does not cover survives a fold and does not survive strict.
    assert_eq!(id("Ωmega", AsciiHeadingIds::Fold, false), "Ωmega");
    assert_eq!(id("Ωmega", AsciiHeadingIds::Strict, false), "mega");
    for (text, _, _, strict) in PARITY {
        assert!(
            strict
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "strict left a non-ASCII byte in {strict:?} (from {text:?})"
        );
    }
}

#[test]
fn the_transforms_are_orthogonal() {
    // Combined, in the one order the slug applies them: transliterate, then
    // lowercase. Reversing it would hand the table a folded code point.
    assert_eq!(id("Ärger", AsciiHeadingIds::Fold, true), "arger");
    assert_eq!(id("Straße", AsciiHeadingIds::Strict, true), "strasse");
    assert_eq!(id("Ärger", AsciiHeadingIds::Off, true), "ärger");
}

#[test]
fn a_crossref_resolves_against_the_ids_the_option_produced() {
    // The index and the reference have to spell a target the same way. They are
    // built from one `HeadingIdOptions` for that reason: threading the flags
    // separately is how a reference resolves against ids nothing produced.
    let options = Options::new().with_ascii_heading_ids(AsciiHeadingIds::Fold);
    let html = carve::to_html_with_options("# Grüße\n\nSee </#Grusse>.\n", &options);
    assert!(html.contains(r##"href="#Grusse""##), "got {html}");
}
