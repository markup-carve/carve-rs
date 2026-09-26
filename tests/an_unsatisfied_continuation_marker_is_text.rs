//! A `+` AT A COLUMN NO CONTAINER'S MARKER COLUMN NAMES IS ORDINARY TEXT
//! (CARVE-P9-031, markup-carve/carve#2322, markup-carve/carve-rs#1973).
//!
//! carve-rs#1980 recovered the narrowest spelling by rewriting the collector's
//! dedent for a stranded `+`, guarded on the previously collected line being a
//! LIST MARKER. Two spellings fall outside that guard and still lost the
//! character: a sub-list item that has a body line of its own, and a second `+`
//! written under the first. Both take the marker DECISION rather than the
//! dedent, and that decision reads the frame's column.
//!
//! THE COLLECTOR'S DEDENT DESTROYS THE DOCUMENT COLUMN, so a below-column line
//! arrives in the nested re-parse spelled flush, indistinguishable from one
//! written at that frame's marker column. `carried_reach` is the answer the
//! enclosing collection already recorded, and both decision sites now ask it.
//!
//! Measured against carve-js `8e427ade`, the pin the spec repo carries.

use carve::to_html;

/// A SUB-LIST ITEM WITH A BODY LINE OF ITS OWN. The line above the `+` is `m`,
/// not a marker, so the dedent path does not fire and the marker decision got
/// the line.
#[test]
fn a_sub_item_with_its_own_body_line_keeps_the_marker_character() {
    assert_eq!(
        to_html("- x\n  - L\n    m\n +\n"),
        "<ul>\n  <li>x\n    <ul>\n      <li>L\nm\n+</li>\n    </ul>\n  </li>\n</ul>"
    );
}

/// TWO MARKERS, TWO CHARACTERS. The second one arrives with a `+` above it
/// rather than a marker, and it takes the item-lead lazy scan rather than the
/// list loop, so recovering the first left the second dropped.
#[test]
fn a_second_unsatisfied_marker_survives_too() {
    assert_eq!(
        to_html("- x\n  - L\n +\n +\n"),
        "<ul>\n  <li>x\n    <ul>\n      <li>L\n+\n+</li>\n    </ul>\n  </li>\n</ul>"
    );
}

/// A WIDE MARKER WIDENS THE BAND. `10. ` puts the sub-list's marker column at
/// 4, so columns 1, 2 and 3 all reach nothing - the divergence was never one
/// column wide by nature, only in the narrowest spelling.
#[test]
fn a_wide_outer_marker_widens_the_band() {
    for column in 1..=3 {
        let source = format!("10. x\n    - L\n{}+\n", " ".repeat(column));
        let html = to_html(&source);
        assert!(html.contains("\n+</li>"), "column {column}: {html}");
    }
}

/// The band's edges, and the columns that are marker columns.
///
/// Columns 0 and 2 ARE marker columns here - the outer list's and the
/// sub-list's - so the marker is satisfied, attaches nothing and renders
/// nothing. Column 1 reaches neither; columns 3 and 4 were already text.
#[test]
fn a_marker_column_still_consumes_the_marker() {
    for (column, keeps) in [(0, false), (1, true), (2, false), (3, true), (4, true)] {
        let source = format!("- x\n  - L\n{}+\n", " ".repeat(column));
        let html = to_html(&source);
        assert_eq!(html.contains("\n+</li>"), keeps, "column {column}: {html}");
    }
}

/// The same rule one level deeper and under a quote host.
#[test]
fn a_deeper_nest_and_a_quote_host_answer_the_same_way() {
    assert!(to_html("- x\n  - L\n    - M\n +\n").contains("<li>M\n+</li>"));
    assert!(to_html("- x\n  - L\n    - M\n   +\n").contains("<li>M\n+</li>"));
    assert!(to_html("> - x\n>   - L\n>  +\n").contains("<li>L\n+</li>"));
}

/// THE LIST LOOP'S OWN DECISION SITE, reached once the sub-list item has a
/// collected chunk behind it rather than a lead line.
///
/// THE CHARACTER REACHES THE OUTPUT AND THE PLACEMENT IS STILL WRONG: carve-js
/// folds it into the innermost open paragraph and this engine leaves it beside
/// the sub-list. The drop is what this ticket is about; the placement is the
/// same open remainder as the deeper spellings, and the `ZZZ` control shows the
/// lazy path itself is right.
#[test]
fn the_list_loop_keeps_the_character_where_the_dedent_path_cannot() {
    for source in [
        "- x\n  - L\n\n    m\n +\n",
        "- x\n  - L\n    m\n\n    n\n +\n",
        "- x\n  - L\n    - M\n      n\n +\n",
    ] {
        let html = to_html(source);
        assert!(html.contains('+'), "{source:?} -> {html}");
    }
    assert!(to_html("- x\n  - L\n\n    m\n ZZZ\n").contains("<p>m\nZZZ</p>"));
}

/// CONTROLS. Ordinary text in the same slot already survived, and without the
/// sub-list a one-space `+` already survived, so neither the position nor the
/// character is what the drop turned on. A satisfied marker still attaches its
/// flush-left block and still writes no character of its own.
#[test]
fn the_controls_are_unchanged() {
    assert!(to_html("- x\n  - L\n    m\n ZZZ\n").contains("<li>L\nm\nZZZ</li>"));
    assert!(to_html("- x\n +\n").contains("<li>x\n+</li>"));
    assert_eq!(
        to_html("- a\n+\nq\n"),
        "<ul>\n  <li>a\n    q\n  </li>\n</ul>"
    );
    assert_eq!(
        to_html("> - - p\n> +\n> ![z](i.png)\n"),
        "<blockquote>\n  <ul>\n    <li>\n      <ul>\n        <li>p</li>\n      </ul>\n      \
         <img src=\"i.png\" alt=\"z\">\n    </li>\n  </ul>\n</blockquote>"
    );
}
