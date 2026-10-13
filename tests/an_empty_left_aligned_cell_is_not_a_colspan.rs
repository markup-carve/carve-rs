//! `|< |` is a glued alignment run ended by a space (PART 9 §5 T11), so it is
//! an empty left-aligned cell. Only a lone `<` with no run, `|<|` or `| < |`,
//! is a colspan marker.

use carve::{migrate_markdown, to_carve, to_html};

fn rows(source: &str) -> String {
    to_html(source)
        .lines()
        .filter(|line| line.contains("<tr>"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_glued_left_marker_and_a_space_is_an_empty_left_aligned_cell() {
    assert_eq!(
        rows("| a |< |\n"),
        "<tr><td>a</td><td style=\"text-align: left;\"></td></tr>"
    );
    assert_eq!(
        rows("|< |\n"),
        "<tr><td style=\"text-align: left;\"></td></tr>"
    );
}

#[test]
fn a_lone_left_marker_is_still_a_colspan() {
    for source in ["| a |<|\n", "| a | < |\n", "| a | <|\n"] {
        assert_eq!(
            rows(source),
            "<tr><td colspan=\"2\">a</td></tr>",
            "{source:?}"
        );
    }
}

#[test]
fn the_writer_spelling_reads_back_as_written() {
    assert_eq!(to_carve("| a |< |\n"), "| a |< |\n");
}

#[test]
fn a_left_aligned_blank_markdown_row_round_trips() {
    let result = migrate_markdown("| A | B |\n|:--|--:|\n|   |   |\n| 1 | 2 |\n");
    assert_eq!(result.value, "|=< A |=> B |\n|< |> |\n|< 1 |> 2 |\n");
    assert_eq!(to_carve(&result.value), result.value);
    assert!(rows(&result.value).contains(
        "<tr><td style=\"text-align: left;\"></td><td style=\"text-align: right;\"></td></tr>"
    ));
}
