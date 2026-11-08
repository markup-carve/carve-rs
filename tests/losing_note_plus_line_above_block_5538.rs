//! A losing duplicate `+` line is read as if it were not there, also when it
//! stands directly above the block or names another defined label (PART 9R R2,
//! carve#5474 ruling, carve#5538).

fn html(source: &str) -> String {
    carve::to_html(source)
}

fn note(out: &str, n: usize) -> &str {
    let open = format!("<li id=\"fn{n}\">");
    let start = out.find(&open).expect("note") + open.len();
    &out[start..start + out[start..].find("</li>").expect("note end")]
}

#[test]
fn block_directly_below_the_losing_line_goes_to_the_later_note() {
    let out = html("[^m]: +\n[^n]: +\n[^n]: +\n  para\n\nsee[^m][^n]\n");
    assert!(!note(&out, 1).contains("para"), "{out}");
    assert!(
        note(&out, 2).contains("<p>para<a href=\"#fnref2\""),
        "{out}"
    );
}

#[test]
fn repeated_losing_lines_directly_above_the_block() {
    let out = html("[^m]: +\n[^n]: +\n[^n]: +\n[^n]: +\n  para\n\nsee[^m][^n]\n");
    assert!(
        note(&out, 2).contains("<p>para<a href=\"#fnref2\""),
        "{out}"
    );
}

#[test]
fn losing_line_of_the_earlier_note_after_a_blank() {
    let out = html("[^m]: +\n[^n]: +\n[^m]: +\n\n  para\n\nsee[^m][^n]\n");
    assert!(!note(&out, 1).contains("para"), "{out}");
    assert!(
        note(&out, 2).contains("<p>para<a href=\"#fnref2\""),
        "{out}"
    );
}

#[test]
fn losing_line_of_another_defined_label_after_a_blank() {
    let out = html("[^k]: x\n[^n]: +\n[^k]: +\n\n  # h\n\nsee[^k][^n]\n");
    assert!(note(&out, 2).contains("<h1 id=\"h\">h</h1>"), "{out}");
    assert!(!out.starts_with("<section id=\"h\">"), "{out}");
}

#[test]
fn losing_line_followed_by_a_blank_keeps_its_reading() {
    let out = html("[^n]: +\n[^n]: +\n\n  para\n\nsee[^n]\n");
    assert!(
        note(&out, 1).contains("<p>para<a href=\"#fnref1\""),
        "{out}"
    );
    assert!(!out.contains("<p>+"), "{out}");
}
