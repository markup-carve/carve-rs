//! `url_char`'s CHARACTER CLASSES, pinned by codepoint rather than by bytes.
//!
//! PART 3's clause AN AUTOLINK BODY ADMITS NON-ASCII AND EXCLUDES FORMAT
//! CHARACTERS spells the last alternative `unicode_url_char - format_char -
//! control_char` (carve#844, carve#860). The corpus pins the readable half - an
//! internationalized domain, a non-ASCII path, a byte order mark, a zero-width
//! space, a no-break space. It cannot reasonably pin the rest: a control
//! character in a fixture is invisible in review and one editor save from
//! vanishing, and the FORMAT category has 170 codepoints in 21 ranges.
//!
//! So the class is asserted here, over a table built from `char::from_u32` -
//! no invisible byte in any source file. BOTH sides are listed, because a rule
//! that only ever sees rejections is satisfied by an autolink production that
//! matches nothing.
//!
//! The count of codepoints EXAMINED is asserted too, not just the count of
//! failures: a table that silently lost its rows would otherwise report a clean
//! run (carve#755, and the same guard as the spec repo's
//! `tests/url-char-classes.test.mjs`).

fn cp(n: u32) -> char {
    char::from_u32(n).expect("codepoint")
}

/// The character sits between the host and its TLD, where it is invisible.
fn in_body(c: char) -> String {
    format!("<https://e{c}.com/>\n")
}

fn is_autolink(src: &str) -> bool {
    carve::to_html(src).contains("<a href=")
}

/// ADMITTED: outside ASCII, not whitespace, not a format character, not a
/// control character. One per general category a URL might plausibly carry, so
/// the rule cannot be re-implemented as "Unicode letters" and stay green.
const ADMITTED: &[(u32, &str)] = &[
    (0x00e9, "LATIN SMALL LETTER E WITH ACUTE (Ll)"),
    (0x4f8b, "CJK IDEOGRAPH (Lo)"),
    (0x0301, "COMBINING ACUTE ACCENT (Mn)"),
    (0x0663, "ARABIC-INDIC DIGIT THREE (Nd)"),
    (0x2160, "ROMAN NUMERAL ONE (Nl)"),
    (0x3001, "IDEOGRAPHIC COMMA (Po)"),
    (0x2013, "EN DASH (Pd)"),
    (0x20ac, "EURO SIGN (Sc)"),
    (0x221a, "SQUARE ROOT (Sm)"),
    (0x1f600, "GRINNING FACE (So, astral)"),
    (0xe000, "PRIVATE USE (Co)"),
    (0x0378, "UNASSIGNED (Cn)"),
];

/// REJECTED, half one: General_Category=Cf. One from each BMP range of the
/// property plus five astral ranges.
const FORMAT: &[(u32, &str)] = &[
    (0x00ad, "SOFT HYPHEN"),
    (0x0601, "ARABIC SIGN SANAH"),
    (0x061c, "ARABIC LETTER MARK"),
    (0x06dd, "ARABIC END OF AYAH"),
    (0x070f, "SYRIAC ABBREVIATION MARK"),
    (0x0890, "ARABIC POUND MARK ABOVE"),
    (0x08e2, "ARABIC DISPUTED END OF AYAH"),
    (0x180e, "MONGOLIAN VOWEL SEPARATOR"),
    (0x200b, "ZERO WIDTH SPACE"),
    (0x200e, "LEFT-TO-RIGHT MARK"),
    (0x202e, "RIGHT-TO-LEFT OVERRIDE"),
    (0x2060, "WORD JOINER"),
    (0x2066, "LEFT-TO-RIGHT ISOLATE"),
    (0xfeff, "ZERO WIDTH NO-BREAK SPACE (BOM)"),
    (0xfff9, "INTERLINEAR ANNOTATION ANCHOR"),
    (0x110bd, "KAITHI NUMBER SIGN (astral)"),
    (0x13430, "EGYPTIAN HIEROGLYPH VERTICAL JOINER (astral)"),
    (0x1bca0, "SHORTHAND FORMAT LETTER OVERLAP (astral)"),
    (0x1d173, "MUSICAL SYMBOL BEGIN BEAM (astral)"),
    (0xe0001, "LANGUAGE TAG (astral)"),
    (0xe0020, "TAG SPACE (astral)"),
];

/// REJECTED, half two: whitespace outside ASCII, and CONTROL characters. The
/// control rows are the reason this file exists rather than a corpus document.
/// U+0080-U+009F are Cc, are NOT Cf, and are not whitespace, so a rule written
/// as "non-ASCII and not Cf" admits fourteen control characters while excluding
/// every C0 one.
const NOT_URL_CHAR: &[(u32, &str)] = &[
    (0x0085, "NEXT LINE (Cc + White_Space)"),
    (0x00a0, "NO-BREAK SPACE (Zs)"),
    (0x1680, "OGHAM SPACE MARK (Zs)"),
    (0x2009, "THIN SPACE (Zs)"),
    (0x2028, "LINE SEPARATOR (Zl)"),
    (0x2029, "PARAGRAPH SEPARATOR (Zp)"),
    (0x202f, "NARROW NO-BREAK SPACE (Zs)"),
    (0x3000, "IDEOGRAPHIC SPACE (Zs)"),
    (0x0001, "START OF HEADING (Cc)"),
    (0x0008, "BACKSPACE (Cc)"),
    (0x001f, "UNIT SEPARATOR (Cc)"),
    (0x007f, "DELETE (Cc)"),
    (0x0080, "PADDING CHARACTER (Cc, C1)"),
    (0x009f, "APPLICATION PROGRAM COMMAND (Cc, C1)"),
];

/// The ASCII exclusions this ruling did NOT move. Listed so a later widening to
/// "any non-whitespace, non-control character" - the reading PART 3 explicitly
/// declines - cannot land green.
const ASCII_EXCLUDED: &[char] = &['"', '\\', '`', '{', '}', '|', '^', '<'];

#[test]
fn a_plain_ascii_autolink_links() {
    // The control that keeps the rest honest.
    assert!(
        is_autolink("<https://e.com/>\n"),
        "the baseline autolink stopped linking"
    );
}

#[test]
fn outside_ascii_a_non_whitespace_non_format_character_is_a_url_char() {
    let missed: Vec<String> = ADMITTED
        .iter()
        .filter(|(n, _)| !is_autolink(&in_body(cp(*n))))
        .map(|(n, name)| format!("U+{n:04X} {name}"))
        .collect();
    assert!(
        missed.is_empty(),
        "these characters should be admitted by url_char and are not: {missed:?}"
    );
    assert_eq!(ADMITTED.len(), 12, "the ADMITTED table lost or gained rows");
}

#[test]
fn a_format_character_is_not_a_url_char() {
    let linked: Vec<String> = FORMAT
        .iter()
        .filter(|(n, _)| is_autolink(&in_body(cp(*n))))
        .map(|(n, name)| format!("U+{n:04X} {name}"))
        .collect();
    assert!(
        linked.is_empty(),
        "a General_Category=Cf character opened an autolink - it is invisible, so \
         the rendered host is not the host that was linked: {linked:?}"
    );
    assert_eq!(FORMAT.len(), 21, "the FORMAT table lost or gained rows");
}

#[test]
fn whitespace_and_control_characters_are_not_url_chars() {
    let linked: Vec<String> = NOT_URL_CHAR
        .iter()
        .filter(|(n, _)| is_autolink(&in_body(cp(*n))))
        .map(|(n, name)| format!("U+{n:04X} {name}"))
        .collect();
    assert!(
        linked.is_empty(),
        "a whitespace or control character opened an autolink: {linked:?}"
    );
    assert_eq!(
        NOT_URL_CHAR.len(),
        14,
        "the NOT_URL_CHAR table lost or gained rows"
    );
}

#[test]
fn the_ascii_exclusions_did_not_move() {
    let linked: Vec<char> = ASCII_EXCLUDED
        .iter()
        .copied()
        .filter(|c| is_autolink(&format!("<https://e.com/a{c}b>\n")))
        .collect();
    assert!(
        linked.is_empty(),
        "an enumerated ASCII exclusion became a url_char: {linked:?}"
    );
    assert_eq!(
        ASCII_EXCLUDED.len(),
        8,
        "the ASCII_EXCLUDED table lost or gained rows"
    );
}

#[test]
fn a_closing_angle_bracket_ends_the_body_rather_than_joining_it() {
    // `>` belongs to the same exclusion and cannot be tested like the rest: it
    // is the construct's own terminator, so a document containing one still
    // produces an autolink - a SHORTER one. What must hold is that the
    // character never reaches the body.
    let html = carve::to_html("<https://e.com/a>b>\n");
    assert!(
        html.contains("<a href=\"https://e.com/a\">"),
        "the href swallowed the terminator: {html}"
    );
    assert!(
        html.contains("b&gt;"),
        "the remainder should be literal text after the autolink: {html}"
    );
}

#[test]
fn a_scheme_is_ascii_even_though_the_body_is_not() {
    assert!(
        !is_autolink("<\u{4f8b}://e.com/>\n"),
        "a non-ASCII scheme opened an autolink"
    );
    assert!(
        is_autolink("<https://\u{4f8b}.jp/>\n"),
        "the same character in the BODY must still link"
    );
}

#[test]
fn link_destination_is_a_different_production_and_still_admits_a_format_character() {
    let html = carve::to_html("[t](https://e\u{feff}.com/)\n");
    assert_eq!(
        html.trim(),
        "<p><a href=\"https://e\u{feff}.com/\">t</a></p>",
        "the inline destination lost the character"
    );
}

#[test]
fn every_codepoint_the_tables_name_was_actually_examined() {
    // Zero findings from zero rows reads exactly like a clean run. This asserts
    // the denominator: each row is rendered once, and this is the total the
    // tests above walked.
    let examined = ADMITTED.len() + FORMAT.len() + NOT_URL_CHAR.len() + ASCII_EXCLUDED.len();
    assert_eq!(
        examined, 55,
        "the number of characters this file examines changed"
    );
    let mut codepoints: Vec<u32> = ADMITTED
        .iter()
        .chain(FORMAT)
        .chain(NOT_URL_CHAR)
        .map(|(n, _)| *n)
        .collect();
    codepoints.sort_unstable();
    codepoints.dedup();
    assert_eq!(
        codepoints.len(),
        47,
        "a codepoint is listed twice, so one row tests nothing new"
    );
}

/// The corpus documents of category
/// `272-an-autolink-body-admits-non-ascii-and-excludes-format-characters`,
/// carried here as bytes so they are pinned before the submodule moves.
#[test]
fn the_corpus_shapes_render_as_the_fixtures_do() {
    let cases: &[(&str, &str)] = &[
        (
            "<https://\u{4f8b}.jp/>\n",
            "<p><a href=\"https://\u{4f8b}.jp/\">https://\u{4f8b}.jp/</a></p>",
        ),
        (
            "<https://example.com/caf\u{e9}>\n",
            "<p><a href=\"https://example.com/caf\u{e9}\">https://example.com/caf\u{e9}</a></p>",
        ),
        (
            "<https://example.com/\u{20ac}10>\n",
            "<p><a href=\"https://example.com/\u{20ac}10\">https://example.com/\u{20ac}10</a></p>",
        ),
        (
            "<https://e\u{feff}.com/>\n",
            "<p>&lt;https://e\u{feff}.com/&gt;</p>",
        ),
        (
            "<\u{feff}https://e.com/>\n",
            "<p>&lt;\u{feff}https://e.com/&gt;</p>",
        ),
        (
            "<https://e\u{200b}.com/>\n",
            "<p>&lt;https://e\u{200b}.com/&gt;</p>",
        ),
        (
            "<https://e\u{a0}.com/>\n",
            "<p>&lt;https://e&nbsp;.com/&gt;</p>",
        ),
        (
            "<https://example.com/\"q\">\n",
            "<p>&lt;https://example.com/\u{201c}q\u{201d}&gt;</p>",
        ),
        (
            "[t](https://e\u{feff}.com/)\n",
            "<p><a href=\"https://e\u{feff}.com/\">t</a></p>",
        ),
        (
            "<\u{4f8b}://example.com/>\n",
            "<p>&lt;\u{4f8b}://example.com/&gt;</p>",
        ),
    ];
    for (src, expected) in cases {
        assert_eq!(
            carve::to_html(src).trim(),
            *expected,
            "corpus shape {src:?} did not render as its fixture"
        );
    }
    assert_eq!(cases.len(), 10, "the corpus category has ten documents");
}
