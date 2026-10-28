#[test]
fn hosted_note_releases_block_openers_to_the_list() {
    let source = "- a\n  [^n]: note\n   q\n    | a |\n";
    let html = carve::to_html(source);
    assert!(html.contains("<table>"), "{html}");
    assert!(html.contains("<li>a"), "{html}");
}

#[test]
fn hosted_note_releases_a_child_list_marker() {
    let source = "- see[^n]\n\n  [^n]: a\n  - li\n";
    let html = carve::to_html(source);
    assert!(html.contains("<li>li</li>"), "{html}");
}

#[test]
fn attached_heading_closes_the_note_claim() {
    let source = "- see[^n]\n\n  [^n]: +\n  # h\nend\n";
    let html = carve::to_html(source);
    assert!(html.contains("<p>end</p>"), "{html}");
}

#[test]
fn nested_note_keeps_a_below_host_marker_as_lazy_text() {
    let source = "- - a\n    [^n]: x\n    q\n   - li\n\nsee[^n]\n";
    let html = carve::to_html(source);
    assert!(!html.contains("<li>li</li>"), "{html}");
    assert!(html.contains("q\n- li"), "{html}");
}

#[test]
fn below_host_openers_remain_lazy_note_text() {
    for opener in ["# h", "| a |"] {
        let source = format!("- a\n  [^n]: x\n  q\n {opener}\n\nsee[^n]\n");
        let html = carve::to_html(&source);
        assert!(html.contains(&format!("q\n{opener}")), "{source}\n{html}");
    }
}

#[test]
fn formatter_keeps_the_quoted_note_boundary() {
    let source = "- > - para\n  >   [^f]: n\n  tail\n";
    let formatted = carve::to_carve(source);
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}

#[test]
fn djot_import_keeps_text_after_an_empty_task_note() {
    let source = "[^1]: + [x]\ntext\n\nt[^1]\n";
    let imported = carve::djot_to_carve(source);
    let html = carve::to_html(&imported);
    assert!(html.contains("<p>text</p>"), "{imported}\n{html}");
}

#[test]
fn unused_note_heading_is_not_a_crossref_target() {
    assert_eq!(
        carve::to_html("[^n]: # H\n\n</#H>\n").trim(),
        "<p>&lt;/#H&gt;</p>"
    );
    assert!(carve::to_html("[^n]: # H\n\nsee[^n] and </#H>\n").contains("href=\"#H\""),);
}
