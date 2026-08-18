//! PART 9: the checkbox is a property of the ITEM, not of its first block.
//!
//! It is written directly after the `<li>` opener whatever the marker line
//! goes on to open, and nothing about that block reaches it. Only the CONTENT
//! moves - it sits beside the checkbox when the first block renders inline,
//! and on its own indented line below it when it does not.
//!
//! The HTML serializer decided the checkbox's placement from the part that
//! followed it, so every non-paragraph lead wrote it on a line of its own at
//! column 0: outside the indentation every other child of an `<li>` gets, and
//! with the block's own indent run trailing it on the same line. carve-js
//! `7cd66e0` and carve-php `8a28c20` write it on the opener line in every
//! spelling; spec corpus 363 pins three leads and both states
//! (carve-rs#1102, markup-carve/carve#1381).

use carve::to_html;

/// Every first-block kind a task item's marker line can open. A paragraph is
/// the control: it was the one lead that was already right, so it has to stay
/// byte-identical too.
const LEADS: &[(&str, &str)] = &[
    ("paragraph", "a"),
    ("blockquote", "> q"),
    ("heading", "# h"),
    ("thematic break", "---"),
    ("table row", "| a |"),
    ("sublist", "- inner"),
    ("code fence", "```js\n  x\n  ```"),
    ("colon-fence div", "::: note\n  b\n  :::"),
];

fn render(state: char, lead: &str) -> String {
    to_html(&format!("- [{state}] {lead}\n"))
}

#[test]
fn the_checkbox_sits_on_the_li_opener_line_for_every_lead() {
    for state in [' ', 'x'] {
        let expected = if state == 'x' {
            "<li><input type=\"checkbox\" checked disabled> "
        } else {
            "<li><input type=\"checkbox\" disabled> "
        };
        for (name, lead) in LEADS {
            let html = render(state, lead);
            assert!(
                html.contains(expected),
                "[{state}] with a {name} lead must write the checkbox on the \
                 `<li>` opener line, got:\n{html}"
            );
        }
    }
}

#[test]
fn no_checkbox_is_ever_written_at_the_start_of_a_line() {
    // The defect's own signature: the one element in the document written at
    // column 0 inside a nested structure. Nothing may open a line with it, at
    // column 0 or at any indentation.
    for state in [' ', 'x'] {
        for (name, lead) in LEADS {
            let html = render(state, lead);
            for line in html.lines() {
                assert!(
                    !line.trim_start().starts_with("<input type=\"checkbox\""),
                    "[{state}] with a {name} lead opened a line with the \
                     checkbox: {line:?}\nin:\n{html}"
                );
            }
        }
    }
}

#[test]
fn only_the_content_moves_between_an_inline_and_a_block_lead() {
    // The two shapes named in the grammar note, byte for byte. A paragraph
    // keeps the content beside the checkbox; a quote puts it on its own
    // indented line and leaves the opener line otherwise untouched - the
    // trailing space the checkbox carries included.
    assert_eq!(
        to_html("- [ ] a\n"),
        "<ul>\n  <li><input type=\"checkbox\" disabled> a</li>\n</ul>"
    );
    assert_eq!(
        to_html("- [ ] > q\n"),
        "<ul>\n  <li><input type=\"checkbox\" disabled> \n    \
         <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn corpus_363_renders_byte_for_byte() {
    // The spec document itself: three leads, both states, one list.
    assert_eq!(
        to_html("- [ ] > q\n- [x] # h\n- [ ] ---\n"),
        concat!(
            "<ul>\n",
            "  <li><input type=\"checkbox\" disabled> \n",
            "    <blockquote><p>q</p></blockquote>\n",
            "  </li>\n",
            "  <li><input type=\"checkbox\" checked disabled> \n",
            "    <h1 id=\"h\">h</h1>\n",
            "  </li>\n",
            "  <li><input type=\"checkbox\" disabled> \n",
            "    <hr>\n",
            "  </li>\n",
            "</ul>"
        )
    );
}

#[test]
fn a_plain_item_still_writes_no_checkbox() {
    // The near miss a naive reading of the fix would also change: an item with
    // no task marker has an empty checkbox string, and pushing it must not
    // add anything to the opener line.
    assert_eq!(
        to_html("- > q\n"),
        "<ul>\n  <li>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
    assert_eq!(to_html("- a\n"), "<ul>\n  <li>a</li>\n</ul>");
}
