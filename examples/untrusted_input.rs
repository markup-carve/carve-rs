//! Rendering input you did not author.
//!
//! Run with: `cargo run --example untrusted_input`
//!
//! Carve's normative hardening is always on and needs no configuration here:
//! dangerous URL schemes are blanked, event-handler attributes are dropped, and
//! the bidi override/isolate characters behind Trojan Source are removed from
//! rendered text. Raw passthrough is the deliberate exception - a `=html` block
//! renders verbatim by design - so it is the one thing untrusted input has to
//! switch off.

fn main() {
    let untrusted = concat!(
        "# Heading\n\n",
        "```=html\n<script>alert(1)</script>\n```\n\n",
        "[click](javascript:alert(2)) and [span]{onclick=\"alert(3)\"}\n"
    );

    // The baseline, with nothing configured: the javascript: URL is already
    // blanked and the onclick attribute already dropped. Only the raw block
    // survives, because that is what a raw block is for.
    println!("default:\n{}\n", carve::to_html(untrusted));

    // For untrusted input: escape raw passthrough, and restrict which
    // constructs are allowed at all.
    let options = carve::Options::new()
        .with_raw_html(false)
        .with_profile(carve::Profile::comment());

    // try_* rather than to_html_with_options. The infallible wrappers are
    // `try_...().unwrap_or_default()`, so a profile rejection - input past
    // max_length, or a denied construct when the action is Error - comes back as
    // an EMPTY STRING, which a caller cannot tell from a document that
    // legitimately rendered to nothing.
    match carve::try_to_html_with_options(untrusted, &options) {
        Ok(html) => println!("safe + comment profile:\n{html}\n"),
        Err(violations) => eprintln!("rejected: {violations}"),
    }

    // What a rejection looks like: the comment profile caps input length.
    let oversize = "x".repeat(20_000);
    let minimal = carve::Options::new().with_profile(carve::Profile::minimal());
    match carve::try_to_html_with_options(&oversize, &minimal) {
        Ok(_) => unreachable!("20 KB is past the minimal profile's cap"),
        Err(violations) => println!("rejected as expected: {violations}"),
    }

    // The same input through the infallible wrapper: no error, no output.
    let silent = carve::to_html_with_options(&oversize, &minimal);
    assert!(silent.is_empty());
    println!("infallible wrapper on the same rejection: {silent:?} (this is the trap)");
}
