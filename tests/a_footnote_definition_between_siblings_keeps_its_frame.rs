//! A footnote definition opens a frame, so unlike a comment or a link or
//! abbreviation definition it is not skipped between list or description
//! siblings (PART 9 §10 I5, carve#4873).

#[test]
fn a_blank_separated_note_between_items_splits_the_list() {
    assert_eq!(
        carve::to_html("- a\n\n[^f]: x\n\n- b\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<ul>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn a_note_between_a_term_and_a_description_ends_the_term() {
    assert_eq!(
        carve::to_html(":: t\n[^f]: a\n\n: d\n"),
        "<dl>\n  <dt>t</dt>\n</dl>\n<p>: d</p>"
    );
}

#[test]
fn a_note_between_description_entries_splits_the_list() {
    let two_lists =
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n<dl>\n  <dt>u</dt>\n  <dd>v</dd>\n</dl>";
    assert_eq!(carve::to_html(":: t\n: d\n[^g]: x\n:: u\n: v\n"), two_lists);
    assert_eq!(
        carve::to_html(":: t\n: d\n%%\n[^g]: x\n:: u\n: v\n"),
        two_lists
    );
    assert_eq!(
        carve::to_html("> :: t\n> : d\n> [^g]: x\n> :: u\n> : v\n"),
        "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>d</dd>\n  </dl>\n  <dl>\n    <dt>u</dt>\n    <dd>v</dd>\n  </dl>\n</blockquote>"
    );
}

#[test]
fn a_link_definition_between_items_still_keeps_one_list() {
    assert_eq!(
        carve::to_html("- a\n[r]: /u\n- b [x][r]\n"),
        "<ul>\n  <li>a</li>\n  <li>b <a href=\"/u\">x</a></li>\n</ul>"
    );
}
