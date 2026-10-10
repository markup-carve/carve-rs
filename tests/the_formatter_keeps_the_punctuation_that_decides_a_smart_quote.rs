//! CARVE-P3-023 closes a quote written after a dash when closing punctuation
//! follows it. Escaping that punctuation takes it out of the set the rule
//! reads, so the quote comes back as an OPENER and `fmt` changes what the
//! document renders (carve-rs#2455).

/// The punctuation that decides the direction is written bare.
const CLOSING_PUNCTUATION: [&str; 5] = [
    "end-\".\n",
    "end-'!\n",
    "end-\";\n",
    "end-\")\n",
    // The corpus row this was found on.
    "end-\"\n\nend-\".\n\nend-'!\n",
];

/// The four rules of CARVE-P3-023, and the escapes around them.
const CONTROLS: [&str; 13] = [
    "end-\"\n",
    "end-\" x\n",
    "said -\"quoted text\"\n",
    "'tis the season\n",
    "a 'n' b\n",
    "'outer 'tis inner'\n",
    "x 'unclosed to the end\n",
    "x---{%%}\" y\n",
    "\"quoted\" text\n",
    "the cat's paw\n",
    "he said \"\n",
    "the '90s\n",
    "end-\"!`code`\n",
];

#[test]
fn the_punctuation_after_a_closing_quote_is_written_bare() {
    for source in CLOSING_PUNCTUATION {
        assert_eq!(carve::to_carve(source), source, "{source:?}");
    }
}

#[test]
fn formatting_keeps_the_quote_direction_and_is_idempotent() {
    for source in CLOSING_PUNCTUATION.iter().chain(CONTROLS.iter()) {
        let formatted = carve::to_carve(source);
        assert_eq!(
            carve::to_html(&formatted),
            carve::to_html(source),
            "{source:?}"
        );
        assert_eq!(carve::to_carve(&formatted), formatted, "{source:?}");
    }
}
