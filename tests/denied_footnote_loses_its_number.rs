//! A footnote a profile took away leaves nothing numbered behind it.
//!
//! `parse` numbers footnotes and `apply_profile` runs after it, so denying the
//! footnote family took the definition away while leaving every reference
//! numbered for a document that no longer existed. The HTML was already right -
//! finding no definition, the renderer emits the literal `[^a]` - so the tree
//! and the output disagreed: one said "numbered reference", the other "text"
//! (carve-rs#641).
//!
//! THE ISSUE DIAGNOSED THIS WRONG, and the wrong diagnosis is worth keeping.
//! It reported that renumbering after `apply_profile` had been tried and did not
//! help, and concluded the filter must be leaving the label in `footnote_defs`
//! for a definition it removed. The filter has cleared `footnote_defs` on this
//! path since #499. What actually happened is the second half of this fix: the
//! numbering pass `continue`s past an unresolved reference WITHOUT clearing its
//! number, so a renumber on its own leaves the old value exactly in place - the
//! same symptom, from the opposite cause. Reverting either half alone shows it.
//!
//! `the_filter_clears_the_definitions_it_denied` pins the premise, so a future
//! reader does not have to take that on trust either.
//!
//! carve-js had the same defect from the same cause and fixed it the same way
//! (carve-js#698); the outputs below were compared against it directly.

use carve::profile::Profile;

const DENIED: &str = "Text[^a].\n\n[^a]: note\n";
/// References and an inline note in one document - where "clear every number"
/// and "renumber" stop agreeing.
const MIXED: &str = "a[^x] b ^[inline] c[^x]\n\n[^x]: note\n";

fn deny_footnotes() -> Profile {
    Profile::full().deny_block(&["footnote"])
}

/// Every footnote-ish node's `number`, in document order (`None` when absent).
///
/// Read off the SERIALIZED tree rather than by matching block variants: a walk
/// that only knew about top-level paragraphs would miss the reference nested in
/// a block quote below and silently report one number too few, which is the
/// shape of check that reports success because it looked nowhere.
fn numbers(src: &str, profile: Profile) -> Vec<Option<usize>> {
    let doc = carve::parse(src);
    let filtered = carve::profile_filter::apply_profile(doc, &profile, None)
        .expect("the profile is in collect mode, not error mode")
        .doc;
    let json = carve::ast_json::to_json(&filtered);

    let mut found = Vec::new();
    for marker in ["\"type\":\"footnote_ref\"", "\"type\":\"inline_footnote\""] {
        for (start, _) in json.match_indices(marker) {
            found.push((start, number_in_object_at(&json, start)));
        }
    }
    // `match_indices` groups by marker, so restore document order by offset.
    found.sort_by_key(|(start, _)| *start);
    found.into_iter().map(|(_, number)| number).collect()
}

/// The `number` of the JSON object containing byte offset `from`, if it has one.
///
/// Scans forward only to that object's closing brace, so a `number` belonging to
/// a LATER sibling cannot be read as this node's.
fn number_in_object_at(json: &str, from: usize) -> Option<usize> {
    let mut depth = 0i32;
    let tail = &json[from..];
    for (offset, ch) in tail.char_indices() {
        match ch {
            '{' | '[' => depth += 1,
            '}' | ']' => {
                if depth == 0 {
                    return None; // end of this object, no number in it
                }
                depth -= 1;
            }
            _ => {}
        }
        if depth == 0 && tail[offset..].starts_with("\"number\":") {
            let rest = &tail[offset + "\"number\":".len()..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            return digits.parse().ok();
        }
    }
    None
}

fn html(src: &str, profile: Profile) -> String {
    let options = carve::Options {
        profile: Some(profile),
        ..Default::default()
    };
    carve::to_html_with_options(src, &options)
}

#[test]
fn a_denied_definition_leaves_the_reference_unnumbered() {
    assert_eq!(numbers(DENIED, deny_footnotes()), vec![None]);
}

#[test]
fn the_rendered_output_agrees_with_the_tree() {
    // The invariant the whole fix is about: both say "this is text".
    let out = html(DENIED, deny_footnotes());
    assert_eq!(out.trim(), "<p>Text[^a].</p>", "{out}");
}

#[test]
fn an_inline_note_renumbers_from_one() {
    // The case that separates renumbering from blanket-clearing: the references
    // leave the sequence, so the inline note is 1 rather than 2.
    assert_eq!(numbers(MIXED, deny_footnotes()), vec![None, Some(1), None]);
    let out = html(MIXED, deny_footnotes());
    assert!(out.contains("<sup>1</sup>"), "{out}");
    assert!(!out.contains("<sup>2</sup>"), "{out}");
}

#[test]
fn the_filter_clears_the_definitions_it_denied() {
    // The premise the fix rests on, asserted rather than assumed. If a denied
    // definition were left in `footnote_defs`, the renumber above would resolve
    // against it and hand back the number it was supposed to remove.
    let doc = carve::parse(DENIED);
    let filtered = carve::profile_filter::apply_profile(doc, &deny_footnotes(), None)
        .expect("collect mode")
        .doc;
    assert!(
        filtered.footnote_defs.is_empty(),
        "a denied definition survived: {:?}",
        filtered.footnote_defs.keys().collect::<Vec<_>>()
    );
}

#[test]
fn nothing_changes_without_a_profile() {
    // The boundary. Every assertion above would also pass if numbering had been
    // broken outright.
    assert_eq!(numbers(DENIED, Profile::full()), vec![Some(1)]);
    assert_eq!(
        numbers(MIXED, Profile::full()),
        vec![Some(1), Some(2), Some(1)]
    );
    assert!(html(DENIED, Profile::full()).contains("<sup>1</sup>"));
}

#[test]
fn a_profile_that_denies_no_footnote_leaves_the_numbers_alone() {
    // The renumber runs on every filtered document, so an unrelated denial must
    // not disturb what it finds.
    let src = "see[^a] ~~struck~~\n\n[^a]: note\n";
    assert_eq!(
        numbers(src, Profile::full().deny_inline(&["delete"])),
        vec![Some(1)]
    );
}

#[test]
fn a_reference_dropped_inside_a_denied_container_renumbers_the_rest() {
    // The path a check on the removed node's own type would miss: the block
    // quote is what the profile denies, and `[^a]` is merely inside it.
    let src = "> q[^a]\n\nafter[^b]\n\n[^a]: one\n[^b]: two\n";
    let profile = Profile::full().deny_block(&["block_quote"]);

    assert_eq!(
        numbers(src, Profile::full()),
        vec![Some(1), Some(2)],
        "unfiltered numbering moved"
    );
    // `[^b]` was 2 and is now the only note left.
    let filtered = numbers(src, profile.clone());
    assert_eq!(filtered.last(), Some(&Some(1)), "{filtered:?}");
    assert!(html(src, profile).contains("href=\"#fn1\""));
}
