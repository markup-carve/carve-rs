//! `type` on `<input>` is an ENUMERATED attribute, and HTML matches an
//! enumerated keyword ASCII case-insensitively.
//!
//! `<input type="CHECKBOX">` is a checkbox to every browser, so an importer
//! that compares the value exactly reads a real task list as an ordinary
//! bullet and the task state leaves the document with nothing said.
//!
//! All three engines compared exactly, so nothing diverged and no cross-engine
//! gate could see it - which is why this is pinned per SPELLING rather than on
//! the one uppercase shape that prompted it. A fix tested only on `CHECKBOX`
//! still misses `Checkbox`.

use carve::{html_to_carve, HtmlImportOptions};

/// Every spelling of the keyword this has to answer for, and one it must not.
const SPELLINGS: [&str; 5] = ["checkbox", "CHECKBOX", "Checkbox", "chEckBox", "cHECKBOx"];

fn migrated(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

#[test]
fn a_task_item_is_read_whatever_case_its_type_is_written_in() {
    // EVERY spelling is EVALUATED. A loop that asserts in place stops at its
    // first panic, so the spellings after it never run - they did not pass,
    // they were never measured, and a fix covering `CHECKBOX` alone would look
    // like a fix covering all five.
    let mut wrong = Vec::new();
    for spelling in SPELLINGS {
        let unchecked = migrated(&format!("<ul><li><input type=\"{spelling}\"> a</li></ul>"));
        if unchecked != "- [ ] a\n" {
            wrong.push(format!("type=\"{spelling}\" -> {unchecked:?}"));
        }
        let checked = migrated(&format!(
            "<ul><li><input type=\"{spelling}\" checked> a</li></ul>"
        ));
        if checked != "- [x] a\n" {
            wrong.push(format!("type=\"{spelling}\" checked -> {checked:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "these must read as task items:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn a_recognized_checkbox_is_consumed_whatever_case_it_was_written_in() {
    // Recognizing the checkbox has to also CONSUME it, or the item carries
    // both a `[ ]` marker and a report claiming the element was lost. The
    // lowercase spelling already had this; the others reach it here.
    let mut wrong = Vec::new();
    for spelling in SPELLINGS {
        let result = html_to_carve(
            &format!("<ul><li><input type=\"{spelling}\"> a</li></ul>"),
            &HtmlImportOptions::default(),
        )
        .expect("import");
        if !result.report.diagnostics.is_empty() {
            wrong.push(format!(
                "type=\"{spelling}\" -> {:?}",
                result
                    .report
                    .diagnostics
                    .iter()
                    .map(|d| d.message.clone())
                    .collect::<Vec<_>>()
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "a consumed checkbox reports nothing lost:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn a_non_checkbox_input_is_still_not_a_task_item() {
    // The control, and it holds on both sides of the fix. A loose match - a
    // prefix test, a `contains` - would turn every text input at the head of an
    // item into a task marker.
    for value in ["text", "TEXT", "radio", "checkboxes", "acheckbox", ""] {
        let back = migrated(&format!("<ul><li><input type=\"{value}\"> a</li></ul>"));
        assert!(
            !back.contains("[ ]"),
            "type=\"{value}\" must not read as a task marker, got {back:?}"
        );
    }
}

#[test]
fn the_fold_is_ascii_so_a_kelvin_sign_is_not_a_k() {
    // `eq_ignore_ascii_case` folds only `A-Z`, which is what HTML's rule says.
    // A Unicode fold turns U+212A KELVIN SIGN into `k`, so `CHEC<U+212A>BOX`
    // would become the exact keyword - and no browser reads that as a checkbox.
    let back = migrated("<ul><li><input type=\"CHEC\u{212A}BOX\"> a</li></ul>");
    assert!(
        !back.contains("[ ]"),
        "a Kelvin sign is not an ASCII K, got {back:?}"
    );
}
