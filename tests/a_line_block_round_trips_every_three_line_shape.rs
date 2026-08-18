//! Every three-line line block over an alphabet of verse line shapes survives
//! the writer (PART 11 §1).
//!
//! The seven corpus documents that came with carve#1333 and carve#1334 pin one
//! shape each. The rules they state are about the RELATION between a line and
//! the one after it - an empty line ends the stanza, a trailing space is
//! stripped, a run opened above swallows what follows - so the defects live in
//! COMBINATIONS, and a fixture suite can only ever hold a few of them. This
//! generates the combinations instead: twenty-two shapes in three positions, 10648
//! documents, asserted against the invariant rather than against a golden.
//!
//! Two of the three assertions here cannot fail on some of these documents - a
//! comment renders nothing, so a corrupted comment body keeps both the HTML and
//! idempotence - which is exactly why the tree comparison leads.

/// One verse body line each, chosen so that every clause the two rulings touch
/// has a shape that exercises it and a shape that must NOT trigger it.
///
/// `a %%` - a trailing comment with EMPTY content - was held out while
/// `restore_inline_comments` re-grafted a source line's trailing comment onto the
/// first formatted line equal to the part before it, so the plain `a` shape above
/// it picked up a second copy of a comment the `comment` node had already written
/// further down (carve-rs#1076). The writer carries the comment on its own, the
/// re-graft is gone, and the shape is back.
const SHAPES: &[&str] = &[
    "a",         // plain content
    "a ",        // a LONE trailing space, which PART 2 strips off a bare line
    "a  ",       // two columns, which §23 MEDIAL GAPS makes NBSP content
    "a\\",       // a hard break with no whitespace before it
    "a \\",      // the reported shape: a lone space held interior by the `\`
    "a  \\",     // the same with a run that needs no backslash
    "\\",        // a backslash alone: how a stanza carries an EMPTY verse line
    "a\\ ",      // an ESCAPED trailing column, which section 2a writes bare
    "a\\ \\",    // the same column held interior by the break after it
    "",          // a blank line, which ENDS the stanza
    "  ",        // whitespace only, which is a blank line too
    "%%",        // a comment line with empty content
    "%% c",      // a comment line
    "  %% c",    // indented, so it is ordinary verse text and not a comment
    " a",        // a leading run, preserved as content
    "a `b",      // opens a run that never closes
    "`",         // a bare opener
    "a `b` c",   // a run that closes on its own line
    "x %% c",    // a TRAILING comment, which is a different construct
    "a %%",      // a trailing comment with EMPTY content (carve-rs#1076)
    "a\tb",      // a tab, which makes the line's text unplaceable
    "/em/ *st*", // ordinary inline markup
];

fn document(lines: [&str; 3]) -> String {
    format!("::: |\n{}\n{}\n{}\n:::\n", lines[0], lines[1], lines[2])
}

#[test]
fn every_three_line_shape_survives_the_writer() {
    let mut tree_broken = Vec::new();
    let mut not_idempotent = Vec::new();
    let mut html_broken = Vec::new();
    let mut checked = 0usize;

    for first in SHAPES {
        for second in SHAPES {
            for third in SHAPES {
                let source = document([first, second, third]);
                let out = carve::to_carve(&source);
                checked += 1;

                if carve::parse(&out).children != carve::parse(&source).children {
                    tree_broken.push((source.clone(), out.clone()));
                    continue;
                }
                if carve::to_carve(&out) != out {
                    not_idempotent.push((source.clone(), out.clone()));
                    continue;
                }
                if carve::to_html(&out) != carve::to_html(&source) {
                    html_broken.push((source.clone(), out.clone()));
                }
            }
        }
    }

    assert_eq!(checked, SHAPES.len().pow(3));
    report("parse(fmt(x)) != parse(x)", &tree_broken);
    report("fmt(fmt(x)) != fmt(x)", &not_idempotent);
    report("to_html(fmt(x)) != to_html(x)", &html_broken);
}

fn report(what: &str, broken: &[(String, String)]) {
    if broken.is_empty() {
        return;
    }
    let shown: Vec<String> = broken
        .iter()
        .take(8)
        .map(|(source, out)| format!("  source: {source:?}\n  fmt:    {out:?}"))
        .collect();
    panic!(
        "{what} on {} of the generated documents; first {}:\n{}",
        broken.len(),
        shown.len(),
        shown.join("\n")
    );
}
