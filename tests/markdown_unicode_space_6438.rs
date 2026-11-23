//! carve#6438: the Markdown export trims only ASCII spaces, tabs and line feeds at a block's end; a no-break
//! space or other non-ASCII space is content and is kept (CARVE-P11-055).

use carve::api::{self, Options};
use carve::export::{self, ExportOptions};

fn md(source: &str) -> String {
    let tree = api::parse(source, &Options::default()).unwrap();
    export::to_markdown(&tree, &ExportOptions::default())
        .unwrap()
        .output
}

#[test]
fn a_non_ascii_space_ending_a_block_is_kept() {
    let cases = [
        (
            "A trailing no-break space\u{a0}\n",
            "A trailing no-break space\u{a0}\n",
        ),
        ("- a\u{a0}\n- b\n", "- a\u{a0}\n- b\n"),
        ("# a\u{a0}\n", "# a\u{a0}\n"),
        ("> a\u{3000}\n", "> a\u{3000}\n"),
        (" \u{feff} \n", "\u{feff}\n"),
        ("a \u{2002}\t\n", "a \u{2002}\n"),
    ];
    for (source, expected) in cases {
        assert_eq!(md(source), expected, "{source:?}");
    }
}

#[test]
fn a_table_cell_and_a_footnote_keep_it() {
    assert!(md("| a\u{a0} |\n| b |\n").contains("a\u{a0} |"));
    assert!(md("x[^f]\n\n[^f]: a\u{a0}\n").contains("a\u{a0}\n"));
}
