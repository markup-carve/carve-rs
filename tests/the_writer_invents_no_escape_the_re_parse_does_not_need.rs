//! PART 11 §2's OTHER HALF: the writer escapes a character IF AND ONLY IF
//! omitting the escape would change the re-parse.
//!
//! The "if" half is covered several times over - `render_carve.rs` sweeps the
//! corpus for `to_html(fmt(x)) == to_html(x)`, for idempotency and for a clean
//! re-parse, and `corpus_canonical_form.rs` pins the exact bytes of the
//! documents the spec ships a `.fmt` for. NOTHING measured the "only if" half,
//! and nothing above CAN: a tree comparison has to forgive escaping or §1
//! contradicts §2, and an over-escaped document renders identically, re-parses
//! cleanly and is happily idempotent. An invented escape passes every one of
//! them.
//!
//! That is not hypothetical. Two carve-php writer defects of exactly this
//! shape, a doubled caret (markup-carve/carve-php#1520) and a half-formed
//! braced pair (markup-carve/carve-php#1522), both reached a human reading
//! output, because no automated check could see them.
//!
//! THE MEASUREMENT. For each corpus document take `to_carve`, then remove each
//! backslash on its own; a backslash whose removal leaves BOTH the render and
//! the canonical tree unchanged is an escape the re-parse never needed. The
//! same count is taken on the SOURCE and subtracted, so an escape the author
//! wrote and the writer merely carried through is not charged to the writer.
//!
//! THE READING WHEN THIS WAS SEEDED, at spec `d164b12`: 72 invented escapes
//! across 28 of 1341 documents - the same 28 slugs with the same 28 counts,
//! character for character, that markup-carve/carve-php#1549 and
//! markup-carve/carve-js#1286 measured. THREE independently written writers
//! landing on the same 72 is what markup-carve/carve#1507 asked for a ruling on:
//! the debt was not any one engine's escape table, it was the shape all three
//! writers chose.
//!
//! THE READING NOW: 57 across 24, after PART 11 SECTION 2b bounded an escalation
//! to the smallest unit that fails (markup-carve/carve#1516). The three engines
//! still agree character for character. What did NOT retire is the two causes
//! named on the ratchet below, and neither of them is the document scope.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// The two causes measured HERE, one of which every ratchet entry must name.
///
/// They were classified against this engine rather than inherited: the
/// escalation branch in `render_carve_once` was instrumented to report, per
/// document, whether the minimal and conservative passes agreed and which form
/// it returned. An entry belonging to none of them is a cause nobody has looked
/// at yet, which is a finding rather than a resident.
///
/// TWO HAVE BEEN RETIRED BY WORK. `escalation: ` went when PART 11 SECTION 2b
/// narrowed the fallback from the document to the failing unit, and
/// `unit scope: ` went when SECTION 2's test was taken per opener occurrence
/// inside that unit (markup-carve/carve#1533). What is left is the two causes
/// that are not scope questions at all.
const IDLE_ESCAPE_CAUSES: &[&str] = &["opener run: ", "minimal class: "];

/// THE DEBT, NOT A BLESSING: documents where the writer emits an escape the
/// re-parse does not need, with the exact count of invented escapes.
///
/// It is a shrink-only ratchet. An entry may be lowered or deleted as the
/// writer improves, and NOTHING may be added or raised. A count that goes up is
/// a regression and fails; a count that goes down fails too, so the entry is
/// tightened rather than left as slack a later defect could spend - which is
/// the whole difference between this and an allowlist.
///
/// Every entry carries a reason naming the characters escaped for nothing,
/// because an entry nobody can explain is the next thing to investigate.
///
/// `unit scope` WAS THE 20-DOCUMENT CAUSE AND IS GONE
/// (markup-carve/carve#1533). PART 11 SECTION 4's strategy had one knob per
/// unit, so a unit that failed was written conservatively IN FULL and every
/// candidate in it was escaped beside the one that needed it. SECTION 2 takes
/// the decision per OPENER OCCURRENCE, and the writer now runs the same halving
/// search one level finer: 20 documents and 47 escapes retired, `\\{.note}`
/// where the unit-scoped form wrote `\\{\\.note\\}`.
///
/// OPENER RUN, three documents: SECTION 2's THE UNIT IS THE OPENER requires the
/// WHOLE opener run escaped - `\\#\\# H` and not `\\## H`, `\\*\\*\\*` and not
/// `\\***`. The sweep below removes ONE backslash at a time, so it reads the
/// second `\\#` as idle: with the first still there no heading forms either way.
/// These entries are a floor this measurement cannot go below while SECTION 2
/// says what it says, and they are here to be seen rather than to be fixed. The
/// occurrence search is why they are a floor rather than an accident: it offers
/// a RUN back whole, so the half-escaped run SECTION 2 forbids is not a state
/// it can reach.
///
/// MINIMAL CLASS, the other two: both passes agree, so nothing escalated, and
/// the escape is still idle - once because a lone authored backslash before a
/// non-escapable character is written back doubled where the bare one re-parses
/// the same, once because the writer's own cell padding retired an authored
/// escape it then kept.
const IDLE_ESCAPE_RATCHET: &[(&str, usize, &str)] = &[
    (
        "72-escape-coverage-2",
        4,
        "minimal class: a lone authored backslash before a non-escapable character is written back doubled, and bare it re-parses the same - `\\` x2, `a`, `«`",
    ),
    (
        "103-heading-marker-column-zero-2",
        2,
        "opener run: the heading opener `##` is escaped in full, and removing either backslash alone still leaves a paragraph",
    ),
    (
        "132-thematic-break-requires-contiguous-markers-3",
        3,
        "opener run: the break opener `***` is escaped in full, and removing any one backslash alone still leaves a paragraph",
    ),
    (
        "390-a-table-cell-s-marker-run-ends-at-a-space-5",
        1,
        "minimal class: an authored `\\=` is kept after the writer's own cell padding retired it - padded, the `=` no longer starts the cell",
    ),
    (
        "396-an-idle-escape-does-not-spread-from-the-block-that-needed-one",
        2,
        "opener run: the heading opener `##` is escaped in full, and removing either backslash alone still leaves a paragraph",
    ),
];

/// The reading this commit measured, pinned where a reader can find it.
///
/// IT MOVED WITH THE CORPUS ONCE AND WITH THE WRITER ONCE. The document
/// markup-carve/carve#1516 ADDED - `396-an-idle-escape-does-not-spread-from-
/// the-block-that-needed-one`, `  ## H` plus a second paragraph - is a THIRD
/// opener-run case, so the reading went to 25 / 59 where PART 11 SECTION 2b's
/// prose still says 2 opener-run documents and 24 / 57 overall; nothing
/// regressed there, the SECTION 2b count was simply taken on a pin that
/// predated its own corpus case. SECTION 2's per-OPENER-OCCURRENCE test then
/// retired the 47 that were one unit written conservatively in full
/// (markup-carve/carve#1533), and what is left is 12 across 5.
///
/// carve-js and carve-php measure the same 5 with the same counts, and the
/// three writers agree byte for byte on all 1358 corpus documents.
const MEASURED_ESCAPES: usize = 12;
const MEASURED_DOCUMENTS: usize = 5;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus")
}

fn corpus_sources() -> Vec<(String, String)> {
    let dir = corpus_dir();
    if !dir.exists() {
        panic!(
            "Spec corpus not found at {}.\n\
             Did you initialize the submodule?\n  git submodule update --init",
            dir.display()
        );
    }
    let mut out: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("crv") {
                return None;
            }
            let slug = path.file_stem()?.to_str()?.to_string();
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            Some((slug, source))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn allowed_for(slug: &str) -> usize {
    IDLE_ESCAPE_RATCHET
        .iter()
        .find(|(name, _, _)| *name == slug)
        .map_or(0, |(_, count, _)| *count)
}

/// Key-order-insensitive, position-free view of an AST-JSON tree.
///
/// `pos` and `srcByteLength` say where the text sat rather than what it says,
/// and removing a backslash shifts every offset after it - compared, they would
/// report a difference on EVERY escape and the count would be a structural
/// zero. They are the only offset-bearing fields the wire format has today; a
/// future one would silently make this measurement too lenient, which is what
/// the footnote document in the self-check below is there to catch.
///
/// `escaped_text` is folded into `text` and adjacent text runs are merged,
/// because an escape is exactly what this comparison is deciding: without it
/// every backslash would split one text node into three and read as
/// load-bearing.
///
/// NOT INTO `attrs`. It holds named slots rather than nodes, and an author can
/// spell an attribute `type`, `pos` or `srcByteLength` - descending would
/// rename or delete an ATTRIBUTE. Attributes are content, so they compare
/// verbatim.
fn canonical(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Array(items) => {
            let mut out: Vec<Value> = Vec::with_capacity(items.len());
            for raw in items {
                let child = canonical(raw);
                if is_text_run(&child) {
                    if let Some(last) = out.last_mut() {
                        if is_text_run(last) {
                            let merged = format!(
                                "{}{}",
                                last["value"].as_str().unwrap_or_default(),
                                child["value"].as_str().unwrap_or_default()
                            );
                            last["value"] = Value::String(merged);
                            continue;
                        }
                    }
                }
                out.push(child);
            }
            Value::Array(out)
        }
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, raw) in map {
                if key == "pos" || key == "srcByteLength" {
                    continue;
                }
                let child = if key == "attrs" {
                    raw.clone()
                } else {
                    canonical(raw)
                };
                out.insert(key.clone(), child);
            }
            if out.get("type").and_then(Value::as_str) == Some("escaped_text") {
                out.insert("type".to_string(), Value::String("text".to_string()));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn is_text_run(node: &serde_json::Value) -> bool {
    let Some(map) = node.as_object() else {
        return false;
    };
    map.len() == 2
        && map.get("type").and_then(serde_json::Value::as_str) == Some("text")
        && map.contains_key("value")
}

/// The render and the canonical tree of a document as one comparable string, or
/// `None` when the document does not encode at all.
///
/// Both halves are needed. The tree comparison forgives escaping - it has to,
/// or §1 contradicts §2 - so on its own it would call EVERY escape idle. The
/// render is what still separates an escape that changes the document from one
/// that changes nothing.
fn fingerprint(source: &str) -> Option<String> {
    let html = carve::to_html(source);
    let json = carve::try_to_json(&carve::parse(source)).ok()?;
    let tree: serde_json::Value = serde_json::from_str(&json).ok()?;
    Some(format!("{html}\u{0}{}", canonical(&tree)))
}

/// A document's IDLE escapes, counted PER ESCAPED CHARACTER - §2's "only if".
///
/// Each backslash is removed on its own and the document re-measured. One whose
/// removal leaves both the render and the canonical tree unchanged is counted
/// under the character it was escaping. A removal that leaves a document the
/// encoder refuses is not idle: the fingerprint is `None`, which matches
/// nothing.
fn idle_escapes(source: &str) -> BTreeMap<char, usize> {
    let mut idle: BTreeMap<char, usize> = BTreeMap::new();
    let Some(base) = fingerprint(source) else {
        return idle;
    };
    for (i, _) in source.char_indices().filter(|(_, c)| *c == '\\') {
        let mut without = String::with_capacity(source.len() - 1);
        without.push_str(&source[..i]);
        without.push_str(&source[i + 1..]);
        if fingerprint(&without).as_deref() != Some(base.as_str()) {
            continue;
        }
        let escaped = source[i + 1..].chars().next().unwrap_or('\u{0}');
        *idle.entry(escaped).or_insert(0) += 1;
    }
    idle
}

/// The idle escapes the WRITER added, over the ones the author already had.
///
/// THE SUBTRACTION IS PER CHARACTER AND CLAMPED AT ZERO PER CHARACTER. A
/// document-wide total would let the writer pay for a newly invented escape
/// with an unrelated one it retired - drop two of the author's idle `.`
/// escapes, invent an idle `|`, and the net is negative while a new defect is
/// on the page. Per character, the invented `|` still counts. Clamping per
/// character is what keeps that sound: retiring an author's escape is §2's job,
/// not credit.
///
/// BOTH READINGS WERE TAKEN before this was seeded, and on this corpus they
/// agree exactly - 28 documents and 72 escapes, per character and as a document
/// total (`invented_idle_escapes_by_document_total` is the second one, kept so
/// the difference between them stays demonstrable). The per-character one is
/// kept because it is the one that stays honest when they stop agreeing.
///
/// THE RESIDUAL BLIND SPOT: what is left is a FLOOR, not an exact count. Two
/// idle escapes of the SAME character, one retired and one invented elsewhere
/// in the same document, still cancel - the count is keyed by character, not by
/// position. Positional matching would close it, and nothing in the corpus
/// exercises it today.
fn invented_idle_escapes_between(source: &str, formatted: &str) -> usize {
    if !source.contains('\\') && !formatted.contains('\\') {
        return 0;
    }
    let authored = idle_escapes(source);
    let mut invented = 0;
    for (escaped, count) in idle_escapes(formatted) {
        invented += count.saturating_sub(authored.get(&escaped).copied().unwrap_or(0));
    }
    invented
}

/// The same reading as a DOCUMENT TOTAL rather than per character.
///
/// Not what the sweep gates on - it is the reading the ratchet deliberately
/// does NOT use, kept so the test below can show what it misses.
fn invented_idle_escapes_by_document_total(source: &str, formatted: &str) -> isize {
    if !source.contains('\\') && !formatted.contains('\\') {
        return 0;
    }
    let authored: usize = idle_escapes(source).values().sum();
    let formatted_total: usize = idle_escapes(formatted).values().sum();
    formatted_total as isize - authored as isize
}

fn invented_idle_escapes(source: &str) -> usize {
    invented_idle_escapes_between(source, &carve::to_carve(source))
}

#[test]
fn the_sweep_reads_a_corpus_that_is_actually_there() {
    // A glob that quietly matches nothing is how a checker reports success
    // having compared nothing (markup-carve/carve#671). The ratchet alone would
    // not catch it: with no documents, every entry is simply never visited.
    let found = corpus_sources().len();
    assert!(found > 1000, "found {found} corpus documents");
}

#[test]
fn the_writer_invents_no_escape_the_re_parse_does_not_need() {
    // On a thread with room, for the same reason the formatter sweep in
    // `render_carve.rs` needs one: the corpus holds a document nested to the
    // parser's cap, and one debug-build frame per level overflows the 2 MiB a
    // test thread gets (carve-rs#530).
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(the_writer_invents_no_escape_the_re_parse_does_not_need_inner)
        .expect("thread spawns")
        .join()
        .expect("the sweep finishes");
}

fn the_writer_invents_no_escape_the_re_parse_does_not_need_inner() {
    let mut wrong = Vec::new();
    for (slug, source) in corpus_sources() {
        let allowed = allowed_for(&slug);
        let invented = invented_idle_escapes(&source);
        if invented == allowed {
            continue;
        }
        // BOTH DIRECTIONS FAIL. Up is a regression; down is a stale entry to
        // lower, so the slack cannot quietly re-fill with a later defect.
        wrong.push(if invented > allowed {
            format!(
                "{slug}: the writer invented {invented} escape(s) the re-parse does not need, \
                 and the ratchet allows {allowed}. PART 11 §2 escapes a character only if \
                 omitting it would change the re-parse. The ratchet may only shrink, so this \
                 is a regression to fix, not an entry to raise."
            )
        } else {
            format!(
                "{slug}: the ratchet entry is stale - it records {allowed} invented escape(s) \
                 and the writer now emits {invented}. Lower the entry to {invented} (or delete \
                 it at 0) so the debt cannot grow back into the slack."
            )
        });
    }
    assert!(
        wrong.is_empty(),
        "the idle-escape ratchet disagrees with the writer:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn every_ratchet_entry_names_a_real_document_a_count_and_a_cause() {
    // An entry with an empty reason is a build failure rather than an entry: a
    // list nobody has to justify is how a ratchet quietly becomes the allowlist
    // that hides the problem.
    let corpus: Vec<String> = corpus_sources().into_iter().map(|(slug, _)| slug).collect();
    let mut seen: Vec<&str> = Vec::new();
    for (slug, count, reason) in IDLE_ESCAPE_RATCHET {
        assert!(
            corpus.iter().any(|name| name == slug),
            "the ratchet names a document the corpus does not have: {slug}"
        );
        assert!(
            *count > 0,
            "a ratchet entry records no invented escape, so it is not debt: {slug}"
        );
        assert!(
            !reason.trim().is_empty(),
            "the ratchet entry for {slug} has no reason, and an entry nobody can explain is \
             the next thing to investigate"
        );
        assert!(
            IDLE_ESCAPE_CAUSES.iter().any(|c| reason.starts_with(c)),
            "the ratchet entry for {slug} names no measured cause: {reason}"
        );
        assert!(
            !seen.contains(slug),
            "the ratchet names {slug} twice, and only one of the two counts would be read"
        );
        seen.push(slug);
    }
}

#[test]
fn the_ratchet_is_the_reading_this_commit_measured_and_only_ever_less() {
    // REDUNDANT BY DESIGN. The per-document sweep is an EQUALITY, so raising an
    // entry already fails as stale and adding one for a clean document already
    // fails at 0 - the shrink-only rule is enforced entry by entry, not by this
    // ceiling. What this adds is a single place the reading is written down, so
    // a reader does not have to sum 28 numbers, and one line that moves when
    // the debt does.
    let total: usize = IDLE_ESCAPE_RATCHET.iter().map(|(_, count, _)| count).sum();
    assert!(
        total <= MEASURED_ESCAPES,
        "the ratchet totals {total} invented escapes, over the {MEASURED_ESCAPES} measured"
    );
    assert!(
        IDLE_ESCAPE_RATCHET.len() <= MEASURED_DOCUMENTS,
        "the ratchet names {} documents, over the {MEASURED_DOCUMENTS} measured",
        IDLE_ESCAPE_RATCHET.len()
    );
}

#[test]
fn the_idle_sweep_sees_an_invented_escape_and_keeps_a_needed_one() {
    // THE SWEEP CAN FAIL, and it fails on exactly what §2 forbids. Without this
    // the whole check could be a count that is structurally always zero - the
    // shape markup-carve/carve#755 catalogs.

    // Idle: mid-line, a `>` is text with or without the backslash.
    assert_eq!(idle_escapes("a \\> b\n"), BTreeMap::from([('>', 1)]));

    // Needed: at column zero, bare it opens a quote.
    assert_eq!(idle_escapes("\\> a\n"), BTreeMap::new());

    // And the count is backslashes that do nothing, not backslashes.
    assert_eq!(idle_escapes("a > b\n"), BTreeMap::new());
    assert_eq!(idle_escapes("a b\n"), BTreeMap::new());
}

#[test]
fn the_idle_sweep_is_not_blinded_by_a_position_bearing_field() {
    // The one way this measurement could go quietly lenient: an offset-bearing
    // field reaching the fingerprint. Removing a backslash shifts every offset
    // after it, so a document carrying one would report a difference for EVERY
    // escape and count none of them idle. A footnote is the field that bit the
    // writer's own comparison (`footnote_def_pos`), so the idle `>` has to
    // still be visible with one on the page.
    assert_eq!(
        idle_escapes("a \\> b[^x]\n\n[^x]: c\n"),
        BTreeMap::from([('>', 1)])
    );
}

#[test]
fn the_count_is_per_character_so_a_retired_escape_cannot_pay_for_an_invented_one() {
    // The same total, a different character, is still one invented escape.
    assert_eq!(invented_idle_escapes_between("a \\. b\n", "a \\| b\n"), 1);
    assert_eq!(invented_idle_escapes_between("a \\. b\n", "a \\. b\n"), 0);

    // Two retired, one invented. Per character that is the one invented escape;
    // the document total reads -1 and reports nothing, which is why the sweep
    // does not gate on it.
    assert_eq!(
        invented_idle_escapes_between("a \\. b \\. c\n", "a . b . c \\| d\n"),
        1
    );
    assert_eq!(
        invented_idle_escapes_by_document_total("a \\. b \\. c\n", "a . b . c \\| d\n"),
        -1
    );
}
