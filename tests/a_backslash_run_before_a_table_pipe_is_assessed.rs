//! cmark-gfm drops exactly one backslash before a table pipe and keeps the
//! pipe in the cell, whatever the run length; pulldown splits the cell on an
//! even run. Every run from 1 to 6 is assessed completely.

use carve::migrate_markdown;

#[test]
fn every_run_length_is_assessed_without_a_fallback() {
    let shapes = [
        "| x{b}|y | z |\n|---|---|\n",
        "| <https://e.com/a{b}|b> | z |\n|---|---|\n",
        "| https://e.com/a{b}|b | z |\n|---|---|\n",
        "| [l](https://e.com/a{b}|b) | z |\n|---|---|\n",
        "| `x{b}|y` | z |\n|---|---|\n",
    ];
    for shape in shapes {
        for count in 1..=6 {
            let source = shape.replace("{b}", &"\\".repeat(count));
            let codes: Vec<String> = migrate_markdown(&source)
                .report
                .diagnostics
                .iter()
                .map(|row| row.code.clone())
                .collect();
            assert!(
                !codes.iter().any(|code| code == "fidelity-unverified"),
                "{source:?}: {codes:?}"
            );
            assert!(
                codes.iter().any(|code| code == "markdown-table-cell"),
                "{source:?}"
            );
        }
    }
}
