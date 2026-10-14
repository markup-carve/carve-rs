//! A `+` continuation on a span marker joins the cell the T5 walk resolves the
//! marker into (markup-carve/carve#3097); a blocked marker is a cell of its
//! own and keeps its fragment (markup-carve/carve#3151). Expected output is
//! the spec checker's.

#[test]
fn caret_after_the_origin_row() {
    assert_eq!(
        carve::to_html("| a | b |\n| ^ | c |\n+ more | d |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"2\">a more</td><td>b</td></tr>\n    <tr><td>c d</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn continuation_before_the_caret_row() {
    assert_eq!(
        carve::to_html("| a | b |\n+ more | d |\n| ^ | c |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"2\">a more</td><td>b d</td></tr>\n    <tr><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn chain_of_carets() {
    assert_eq!(
        carve::to_html("| a | b |\n| ^ | c |\n| ^ | e |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"3\">a x</td><td>b</td></tr>\n    <tr><td>c</td></tr>\n    <tr><td>e y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn colspan_marker() {
    assert_eq!(
        carve::to_html("| a | < |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td colspan=\"2\">a x y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn rowspan_and_colspan_block() {
    assert_eq!(
        carve::to_html("| a | < | c |\n| ^ | ^ | d |\n+ x | y | z |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"2\" colspan=\"2\">a x y</td><td>c</td></tr>\n    <tr><td>d z</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn gfm_head_rowspan() {
    assert_eq!(
        carve::to_html("| A |\n|---|\n| ^ |\n+ x |\n").trim(),
        "<table>\n  <tbody>\n    <tr><th scope=\"col\" rowspan=\"2\">A x</th></tr>\n    <tr></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn blocked_colspan_marker() {
    assert_eq!(
        carve::to_html("| a | b |\n| < | c |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    <tr><td>x</td><td>c y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn blocked_caret_in_the_first_row() {
    assert_eq!(
        carve::to_html("| ^ | b |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td>x</td><td>b y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn two_origins_in_one_row() {
    assert_eq!(
        carve::to_html("| a | b | c |\n| ^ | d | e |\n| f | ^ | g |\n+ x | y | z |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"2\">a</td><td>b</td><td>c</td></tr>\n    <tr><td rowspan=\"2\">d y</td><td>e</td></tr>\n    <tr><td>f x</td><td>g z</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn blocked_caret_under_a_wide_cell() {
    assert_eq!(
        carve::to_html("| a | < |\n| ^ | b |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td colspan=\"2\">a</td></tr>\n    <tr><td>x</td><td>b y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn two_continuation_lines_on_one_caret() {
    assert_eq!(
        carve::to_html("|= h |= i |\n| a | b |\n| ^ | c |\n+ more | |\n+ again | d |\n").trim(),
        "<table>\n  <thead>\n    <tr><th scope=\"col\">h</th><th scope=\"col\">i</th></tr>\n  </thead>\n  <tbody>\n    <tr><td rowspan=\"2\">a more again</td><td>b</td></tr>\n    <tr><td>c d</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn blocked_colspan_marker_under_a_rowspan() {
    assert_eq!(
        carve::to_html("| a | b |\n| ^ | c |\n| < | d |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td rowspan=\"2\">a</td><td>b</td></tr>\n    <tr><td>c</td></tr>\n    <tr><td>x</td><td>d y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_marker_widening_a_blocked_cell_joins_its_fragment() {
    assert_eq!(
        carve::to_html("| ^ | < |\n+ x | y |\n").trim(),
        "<table>\n  <tbody>\n    <tr><td colspan=\"2\">x y</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn blocked_markers_keep_their_continuations_when_formatted() {
    let cases = [
        "| a | b |\n| < | c |\n+ x | y |\n",
        "| ^ | b |\n+ x | y |\n",
        "| a | < |\n| ^ | b |\n+ x | y |\n",
        "| a | b |\n| ^ | c |\n| < | d |\n+ x | y |\n",
        "| ^ | < |\n+ x | y |\n",
    ];
    for source in cases {
        let formatted = carve::to_carve(source);
        assert_eq!(
            carve::to_html(&formatted),
            carve::to_html(source),
            "{source}"
        );
        assert_eq!(carve::to_carve(&formatted), formatted, "{source}");
    }
}

#[test]
fn blocked_marker_continuation_keeps_its_line() {
    let source = "| ^ | b |\n+   | y |\n+ z | w |\n";
    let formatted = "| ^ | b |\n+ | y |\n+ z | w |\n";
    assert_eq!(carve::to_carve(source), formatted);
    assert_eq!(carve::to_carve(formatted), formatted);
}

#[test]
fn a_later_first_fragment_has_no_leading_space() {
    for source in [
        "| ^ | b |\n+   | y |\n+ z | w |\n",
        "| | b |\n+   | y |\n+ z | w |\n",
    ] {
        assert!(carve::to_html(source).contains("<td>z</td>"), "{source}");
        assert!(carve::to_plain_text(source).starts_with('z'), "{source}");
        assert!(carve::to_markdown(source).contains("| z |"), "{source}");
    }
}

#[test]
fn a_wrap_at_the_start_of_a_link_still_separates_text() {
    let source = "| see[ |\n+ docs](u) |\n";
    assert!(
        carve::to_html(source).contains("see<a href=\"u\"> docs</a>"),
        "{}",
        carve::to_html(source)
    );
}
