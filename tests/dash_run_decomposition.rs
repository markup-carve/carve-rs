//! A run of N hyphens collapses to em/en dashes via the canonical djot
//! allocation (carve-js/carve-php/oracle `allocateDashes`): all em when N is
//! divisible by 3, all en when divisible by 2, otherwise as many em-dashes as
//! fit with the remainder as en-dashes - a remainder of 1 trades one em for two
//! en, so a run of 2+ hyphens NEVER leaves a literal hyphen behind.
//!
//! Regression: carve-rs previously used a fixed longest-match table capped at
//! six hyphens (`------` -> em em), applied greedily. That left a stray literal
//! hyphen at N = 7, 13, ... (N cong 1 mod 6) and mis-allocated at N = 8, 10, 13,
//! diverging from carve-js/carve-php/oracle (which all agree). See the ladder.

const EM: &str = "\u{2014}"; // em dash
const EN: &str = "\u{2013}"; // en dash

/// Wrap a glyph sequence in the `x ... y` paragraph the ladder cases use.
fn html_para(dashes: &str) -> String {
    format!("<p>x {dashes} y</p>")
}

fn run_of(n: usize) -> String {
    format!("x {} y\n", "-".repeat(n))
}

#[test]
fn full_ladder_matches_canonical_allocation() {
    // N -> expected glyph sequence, byte-for-byte with carve-js/carve-php/oracle.
    let expected: [(usize, String); 13] = [
        (1, "-".to_string()),                  // lone hyphen stays literal
        (2, EN.to_string()),                   // en
        (3, EM.to_string()),                   // em
        (4, format!("{EN}{EN}")),              // en en
        (5, format!("{EM}{EN}")),              // em en
        (6, format!("{EM}{EM}")),              // em em
        (7, format!("{EM}{EN}{EN}")),          // em en en  (was em em + stray `-`)
        (8, format!("{EN}{EN}{EN}{EN}")),      // en*4      (was em em en)
        (9, format!("{EM}{EM}{EM}")),          // em*3
        (10, format!("{EN}{EN}{EN}{EN}{EN}")), // en*5      (was em em en en)
        (11, format!("{EM}{EM}{EM}{EN}")),     // em*3 en
        (12, format!("{EM}{EM}{EM}{EM}")),     // em*4
        (13, format!("{EM}{EM}{EM}{EN}{EN}")), // em*3 en en (was em*4 + stray `-`)
    ];

    for (n, seq) in &expected {
        assert_eq!(
            carve::to_html(&run_of(*n)),
            html_para(seq),
            "HTML dash run N={n} must be the canonical allocation"
        );
    }
}

#[test]
fn no_trailing_literal_hyphen_at_the_regression_counts() {
    // N cong 1 mod 6 (7, 13, 19, ...) used to leave a stray `-`. None may now.
    for n in [7usize, 13, 19] {
        let html = carve::to_html(&run_of(n));
        let inner = html.trim_start_matches("<p>x ").trim_end_matches(" y</p>");
        assert!(
            !inner.contains('-'),
            "N={n} left a literal hyphen: {html:?}"
        );
        // Byte length must be exactly a whole number of 3-byte dash glyphs.
        assert_eq!(
            inner.len() % 3,
            0,
            "N={n} not a clean em/en split: {html:?}"
        );
    }
}

#[test]
fn arrow_operator_still_wins_at_its_own_position() {
    // `-->` is the CANONICAL rightwards arrow since markup-carve/carve#1442, so
    // the doubled run is an arrow rather than an en dash plus `>`. Three or more
    // hyphens are still a run: `--->` is not `-->`, so the allocation takes all
    // three and the `>` is literal.
    assert_eq!(carve::to_html("x --> y\n"), "<p>x → y</p>");
    assert_eq!(carve::to_html("x ---> y\n"), format!("<p>x {EM}&gt; y</p>"));
    // A long run swallows the trailing hyphen atomically, so `->` never forms:
    // `------->` is a run of 7 (em en en) followed by a literal `>`.
    assert_eq!(
        carve::to_html("x -------> y\n"),
        format!("<p>x {EM}{EN}{EN}&gt; y</p>")
    );
    // `<-->` is now ONE token, the canonical bidirectional arrow
    // (markup-carve/carve#1442), rather than `<-` followed by `->`.
    assert_eq!(carve::to_html("x <--> y\n"), "<p>x ↔ y</p>");
    // `<--->` is not `<-->`, so tokenizing runs left to right from where the
    // longest match at each position leaves off: `<-` (deprecated but still an
    // arrow), then `-->` (canonical). Never a dash run - the run pass defers to
    // an arrow that starts at its position.
    assert_eq!(carve::to_html("x <---> y\n"), "<p>x ←→ y</p>");
}

#[test]
fn non_html_renderers_stay_byte_parity_with_html() {
    // The Markdown / plain / ANSI renderers must emit the same dash glyphs as
    // the HTML path (renderer parity). Check the previously divergent counts.
    for n in [7usize, 8, 10, 11, 13] {
        let src = run_of(n);
        let html = carve::to_html(&src);
        let inner = html
            .trim_start_matches("<p>x ")
            .trim_end_matches(" y</p>")
            .to_string();
        let want = format!("x {inner} y");
        assert_eq!(carve::to_markdown(&src).trim_end(), want, "markdown N={n}");
        assert_eq!(carve::to_plain_text(&src).trim_end(), want, "plain N={n}");
    }
}
