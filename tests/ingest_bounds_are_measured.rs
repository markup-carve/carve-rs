//! An ingested tree is bounded by what its payload COST, not by what the
//! payload CLAIMS it cost (carve-rs#811).
//!
//! `srcByteLength` arrives inside the payload. Two separate caps were sized
//! from it: the three expansion budgets, and the profile's `max_length` inside
//! the library helper `prepare_document_for_render`. Each let the payload
//! choose the size of the guard that was supposed to bound it.
//!
//! The CLI already reasons correctly and says why, at `src/main.rs`:
//!
//! > The document's own `srcByteLength` cannot stand in for it - that number
//! > arrives inside the payload, so a hostile tree can claim 0 and render
//! > anything. Measured on the payload, which is also the form a host storing
//! > trees actually receives.
//!
//! That is the model these follow. The library helper documents itself as
//! "used by `--from-json`, where parsing has happened in another process but
//! render restrictions must still apply", so a host following the documented
//! `from_json` -> `prepare_document_for_render` -> `render_*` pipeline is the
//! reachable case.

use carve::extensions::table_of_contents::TocPlacement;

/// A tree of `headings` headings, each followed by a `::: toc` block, as JSON.
///
/// Output is (toc blocks) x (headings), so the budget is the only thing between
/// the payload and an amplification that grows with the payload.
fn toc_payload(headings: usize) -> String {
    let mut source = String::new();
    for i in 0..headings {
        source.push_str(&format!(
            "# Heading number {i} is reasonably long so the toc entry costs bytes\n\n::: toc\n:::\n\n"
        ));
    }
    carve::to_json(&carve::parse(&source))
}

/// Rewrite the payload's own claim about how big its source was.
fn with_claim(payload: &str, claim: usize) -> String {
    let needle = "\"srcByteLength\":";
    let at = payload
        .rfind(needle)
        .expect("payload carries srcByteLength");
    let value_at = at + needle.len();
    let end = value_at
        + payload[value_at..]
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(payload.len() - value_at);
    let rewritten = format!("{}{claim}{}", &payload[..value_at], &payload[end..]);

    // The generator, checked before it is trusted: a rewrite that silently
    // matched nothing would make every case below pass for the wrong reason.
    assert!(
        rewritten.contains(&format!("\"srcByteLength\":{claim}")),
        "the claim was not rewritten"
    );
    assert_eq!(
        carve::from_json(&rewritten)
            .expect("rewritten payload decodes")
            .source_len,
        claim,
        "the decoded tree does not carry the rewritten claim"
    );
    rewritten
}

fn render_with_toc(payload: &str) -> String {
    let doc = carve::from_json(payload).expect("decode");
    let toc = TocPlacement::new();
    let options = carve::Options::new().with_extension(&toc);
    let prepared =
        carve::prepare_document_for_render(doc, &options, carve::Mode::Interactive, true)
            .expect("no profile, so no violation");
    carve::render_html_with_options(&prepared, &options).expect("render")
}

#[test]
fn a_nine_digit_claim_does_not_buy_an_unbounded_expansion_budget() {
    let honest = toc_payload(800);
    let spoofed = with_claim(&honest, 1_000_000_000);

    let honest_html = render_with_toc(&honest);
    let spoofed_html = render_with_toc(&spoofed);

    // Before this fix the same rewrite took a 1.07 MB payload to 39.6 MB of
    // HTML, 288x. What remains has to be bought in bytes.
    assert!(
        spoofed_html.len() < spoofed.len() * 16,
        "claimed length still sizes the budget: {} bytes of HTML from {} bytes of payload",
        spoofed_html.len(),
        spoofed.len()
    );
    assert!(!honest_html.is_empty());
}

#[test]
fn the_index_budget_is_sized_the_same_way() {
    // One rule, three spellings. A fix that only reached the table of contents
    // would have left the other two open, so each budget gets a case rather
    // than one case standing in for all of them.
    let mut source = String::new();
    for i in 0..400 {
        source.push_str(&format!(":index[term number {i} in this document] "));
    }
    source.push_str("\n\n");
    for _ in 0..60 {
        source.push_str("::: index\n:::\n\n");
    }
    let honest = carve::to_json(&carve::parse(&source));
    let spoofed = with_claim(&honest, 1_000_000_000);

    let render = |payload: &str| {
        let doc = carve::from_json(payload).expect("decode");
        let index = carve::extensions::Index::new();
        let options = carve::Options::new().with_extension(&index);
        let prepared =
            carve::prepare_document_for_render(doc, &options, carve::Mode::Interactive, true)
                .expect("no profile, so no violation");
        carve::render_html_with_options(&prepared, &options).expect("render")
    };

    // Identical: both land on the budget's 1 MB floor, where before the fix the
    // claim would have bought a budget nine digits wide.
    assert_eq!(render(&honest).len(), render(&spoofed).len());
    assert!(!render(&honest).is_empty());
}

#[test]
fn the_abbreviation_budget_is_sized_the_same_way() {
    // The third spelling, and the one that is always on - no extension has to be
    // wired for it.
    //
    // Pinned at the seam rather than through an amplification, because this
    // crate has no abbreviation amplification to measure: `Abbreviation` carries
    // its `expansion` on every occurrence, so a tree that renders megabytes of
    // expansions already cost megabytes to send. What was defective there is the
    // BASIS, not the output, so the basis is what this asks about.
    let expansion = "HyperText Markup Language ".repeat(6000);
    let source = format!("*[HTML]: {expansion}\n\n{}\n", "HTML ".repeat(20));

    let mut doc = carve::parse(&source);
    // Past the budget's 1 MB floor, or the ceiling would have nothing to take
    // away.
    assert!(doc.source_len > 125_000);
    let options = carve::Options::new();
    let without_ceiling = carve::render_html_with_options(&doc, &options)
        .expect("render")
        .len();

    doc.ingest_payload_len = 50_000;
    assert_eq!(doc.expansion_budget_len(), 50_000);
    let with_ceiling = carve::render_html_with_options(&doc, &options)
        .expect("render")
        .len();

    assert!(
        with_ceiling < without_ceiling,
        "the abbreviation budget ignored the ceiling: {with_ceiling} vs {without_ceiling}"
    );
}

#[test]
fn the_claim_is_still_read_exactly_as_written() {
    // The budget stops trusting `srcByteLength`; the decoder does not rewrite
    // it. A reader that repaired the field would have changed the record.
    let spoofed = with_claim(&toc_payload(2), 1_000_000_000);
    let doc = carve::from_json(&spoofed).expect("decode");

    assert_eq!(doc.source_len, 1_000_000_000);
    assert!(carve::to_json(&doc).contains("\"srcByteLength\":1000000000"));
}

#[test]
fn the_budget_basis_is_the_smaller_of_the_claim_and_the_payload() {
    let honest = toc_payload(2);
    let honest_doc = carve::from_json(&honest).expect("decode");
    assert_eq!(
        honest_doc.expansion_budget_len(),
        honest_doc.source_len,
        "an honest payload is bounded by its own claim, which is the smaller one"
    );

    let spoofed = with_claim(&honest, 1_000_000_000);
    let spoofed_doc = carve::from_json(&spoofed).expect("decode");
    assert_eq!(spoofed_doc.expansion_budget_len(), spoofed.len());
    assert!(spoofed_doc.expansion_budget_len() < spoofed_doc.source_len);
}

#[test]
fn a_parsed_document_is_bounded_by_its_own_measured_source() {
    // Nothing on the parse path changes: the parser measured the input, so
    // there is no second number and no ceiling to apply.
    let source = "# Title\n\nSome text.\n";
    let doc = carve::parse(source);

    assert_eq!(doc.source_len, source.len());
    assert_eq!(doc.expansion_budget_len(), source.len());
    assert_eq!(doc.untrusted_input_len(), source.len());
}

#[test]
fn the_profile_max_length_is_checked_against_the_payload() {
    // The library helper kept the spoofable check where the CLI beside it does
    // not: `Profile::minimal()` caps input at 10,000 bytes and used to accept a
    // 353 KB payload that claimed to be zero bytes long.
    let payload = with_claim(&toc_payload(300), 0);
    // Against the cap itself rather than a number chosen by hand, so the case
    // cannot start passing vacuously the day the profile's limit moves.
    let cap = carve::Profile::minimal().max_length();
    assert!(
        payload.len() > cap,
        "the sample must exceed the profile's limit of {cap}; got {}",
        payload.len()
    );

    let doc = carve::from_json(&payload).expect("decode");
    assert_eq!(
        doc.source_len, 0,
        "the tree claims to have come from nothing"
    );
    assert_eq!(doc.untrusted_input_len(), payload.len());

    let options = carve::Options::new().with_profile(carve::Profile::minimal());
    let refused = carve::prepare_document_for_render(doc, &options, carve::Mode::Interactive, true);

    assert!(
        refused.is_err(),
        "a payload past the profile's max_length must be refused however it describes itself"
    );
    let violations = refused.unwrap_err().violations;
    assert_eq!(violations[0].reason, "max_length_exceeded");
    let description = violations[0]
        .reason_description
        .clone()
        .expect("the refusal says what was measured");
    assert!(
        description.contains(&payload.len().to_string()),
        "the refusal must name the measured payload, not the claim: {description}"
    );
}

/// An extension that records whether it ran, and clears the measurement while
/// it is at it.
///
/// `before_render` takes the document by value and hands one back, so a hook
/// CAN rewrite `ingest_payload_len`. That is the point: a cap read after the
/// hooks would be a cap whose own input the pipeline gets to rewrite.
struct ClearsTheMeasurement {
    ran: std::cell::Cell<bool>,
}

impl carve::CarveExtension for ClearsTheMeasurement {
    fn name(&self) -> &'static str {
        "clears-the-measurement"
    }

    fn before_render(
        &self,
        mut doc: carve::Document,
        _ctx: &carve::BeforeRenderContext<'_>,
    ) -> carve::Document {
        self.ran.set(true);
        doc.ingest_payload_len = 0;
        doc
    }
}

#[test]
fn the_cap_is_answered_before_any_hook_walks_the_tree() {
    // Two things at once. A hook cannot clear the number the cap is read from,
    // because the cap is read first; and the hooks that traverse and allocate
    // from the tree - the table of contents, the index - never run on a payload
    // that is going to be refused, which is the work the cap exists to prevent.
    let payload = with_claim(&toc_payload(300), 0);
    let cap = carve::Profile::minimal().max_length();
    assert!(payload.len() > cap);

    let doc = carve::from_json(&payload).expect("decode");
    let extension = ClearsTheMeasurement {
        ran: std::cell::Cell::new(false),
    };
    let options = carve::Options::new()
        .with_profile(carve::Profile::minimal())
        .with_extension(&extension);

    let refused = carve::prepare_document_for_render(doc, &options, carve::Mode::Interactive, true);

    assert!(refused.is_err(), "a hook must not be able to widen the cap");
    assert!(
        !extension.ran.get(),
        "the hooks ran on a payload the profile was going to refuse"
    );
}

#[test]
fn a_hook_still_runs_when_the_payload_is_inside_the_cap() {
    // The mirror, so the ordering above cannot pass by never running hooks.
    let payload = carve::to_json(&carve::parse("a short comment\n"));
    let doc = carve::from_json(&payload).expect("decode");
    let extension = ClearsTheMeasurement {
        ran: std::cell::Cell::new(false),
    };
    let options = carve::Options::new()
        .with_profile(carve::Profile::minimal())
        .with_extension(&extension);

    assert!(
        carve::prepare_document_for_render(doc, &options, carve::Mode::Interactive, true).is_ok()
    );
    assert!(
        extension.ran.get(),
        "an accepted payload still runs its hooks"
    );
}

#[test]
fn a_small_payload_still_passes_the_same_profile() {
    // The mirror, so the bound above cannot pass by refusing everything.
    let payload = carve::to_json(&carve::parse("a short comment\n"));
    assert!(payload.len() < 10_000);

    let doc = carve::from_json(&payload).expect("decode");
    let options = carve::Options::new().with_profile(carve::Profile::minimal());

    assert!(
        carve::prepare_document_for_render(doc, &options, carve::Mode::Interactive, true).is_ok(),
        "a payload inside the profile's limit must still render"
    );
}

#[test]
fn a_parsed_document_is_still_bounded_by_its_source_under_a_profile() {
    // The parse path through the same helper: `source_len` was measured there,
    // so it is what the cap is checked against and nothing changes.
    let doc = carve::parse(&"x".repeat(20_000));
    let options = carve::Options::new().with_profile(carve::Profile::minimal());

    assert!(
        carve::prepare_document_for_render(doc, &options, carve::Mode::Interactive, true).is_err(),
        "an oversize parsed document must still be refused"
    );
}

#[test]
fn the_corpus_ingests_with_the_budget_parsing_would_have_given() {
    // On a worker with an ample stack, like the other corpus sweeps
    // (tests/ast_json.rs): the parser, the encoder and the decoder all use one
    // native frame per level, and a DEBUG build's frames are several times a
    // release build's, so the corpus's deepest documents overflow the default
    // 2 MiB thread stack there while passing comfortably in release. This is
    // about the budget, not about stack headroom.
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(corpus_budget_inner)
        .expect("thread spawns")
        .join()
        .expect("the sweep finishes");
}

fn corpus_budget_inner() {
    // "It does not affect legitimate input" is a claim about all of them. An
    // encoded tree is bigger than the source it came from, so the ceiling never
    // binds on a document this crate produced.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus");
    let mut checked = 0usize;
    let mut binding: Vec<String> = Vec::new();

    let entries = std::fs::read_dir(&dir).expect("the corpus was not found");
    for entry in entries {
        let path = entry.expect("read dir").path();
        if path.extension().and_then(|e| e.to_str()) != Some("crv") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("read");
        let json = carve::to_json(&carve::parse(&source));
        let doc = match carve::from_json(&json) {
            Ok(doc) => doc,
            Err(_) => continue,
        };
        checked += 1;
        if doc.expansion_budget_len() < doc.source_len {
            binding.push(path.file_name().unwrap().to_string_lossy().to_string());
        }
    }

    assert!(checked > 400, "only {checked} corpus documents were read");
    assert!(
        binding.is_empty(),
        "{} corpus documents would have their budget cut on ingest: {:?}",
        binding.len(),
        &binding[..binding.len().min(8)]
    );
}
