#[test]
fn attached_heading_and_table_leave_no_lazy_paragraph_claim() {
    for (block, expected) in [
        ("# h", "<h1 id=\"h\">h</h1>"),
        (
            "| a |",
            "<table>\n        <tbody>\n          <tr><td>a</td></tr>\n        </tbody>\n      </table>",
        ),
    ] {
        let source = format!("- see[^n]\n\n  [^n]: +\n  {block}\nend\n");
        let html = carve::to_html(&source);
        assert!(html.contains("<p>end</p>"), "{source:?}: {html}");
        assert!(html.contains(expected), "{source:?}: {html}");
        assert!(!html.contains("<p># h"), "{source:?}: {html}");
        assert!(!html.contains("<p>| a |"), "{source:?}: {html}");
    }
}

#[test]
fn attached_paragraph_then_body_floor_heading_are_separate_blocks() {
    let source = "- see[^n]\n  [^n]: +\n  x\n    # z\n\nsee[^n]\n";
    let html = carve::to_html(source);
    assert!(
        html.contains("<p>x</p>\n      <h1 id=\"z\">z</h1>"),
        "{html}"
    );
    assert!(!html.contains("<p>x\n# z"), "{html}");
}

#[test]
fn later_heading_at_body_floor_belongs_to_host_item() {
    let source = "- a\n  [^n]: +\n  # h\n    # h\n\nsee[^n]\n";
    let html = carve::to_html(source);
    assert!(html.contains("<li>a\n    <h1 id=\"h\">h</h1>"), "{html}");
    assert!(
        html.contains("<li id=\"fn1\">\n      <h1 id=\"h-2\">h</h1>"),
        "{html}"
    );
}

#[test]
fn invisible_comment_before_attachment_does_not_choose_its_block_kind() {
    for comment in ["  %% c", "  %%%\n%%%"] {
        let heading = format!("- see[^n]\n  [^n]: +\n{comment}\n  # h\n- sibling\n");
        let html = carve::to_html(&heading);
        assert!(
            html.contains("<li id=\"fn1\">\n      <h1 id=\"h\">h</h1>"),
            "{html}"
        );
        assert!(!html.contains("<li>see\n    <h1"), "{html}");

        let paragraph = format!("- see[^n]\n\n  [^n]: +\n{comment}\n  x\n- sibling\n");
        let html = carve::to_html(&paragraph);
        assert!(html.contains("<li><p>see"), "{html}");
        assert!(html.contains("<li><p>sibling</p></li>"), "{html}");
    }
}
