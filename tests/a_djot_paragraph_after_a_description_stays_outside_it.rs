//! A `%%` line is erased before layout (carve#5012), so the Djot importer
//! attaches a paragraph after an item's definition list with `+` instead
//! (carve-rs#3828).

fn html(carve: &str) -> String {
    carve::to_html(carve).split('\n').map(str::trim).collect()
}

#[test]
fn a_paragraph_after_a_description_is_attached_with_a_plus() {
    for (djot, written, expected) in [
        (
            "- : outer\n\n   : inner\n\n    body\n\n  outside\n",
            "     :  body\n+\noutside\n",
            "</dl></dd></dl>outside</li></ul>",
        ),
        (
            "- : outer\n\n    body\n\n  mid\n\n  : t2\n\n    b2\n\n  end\n- next\n",
            "  :  body\n+\nmid\n",
            "</dl>mid<dl><dt>t2</dt><dd>b2</dd></dl>end</li><li>next</li></ul>",
        ),
        (
            "> - : outer\n>\n>     body\n>\n>   outside\n",
            ">   :  body\n> +\n> outside\n",
            "<dd>body</dd></dl>outside</li></ul></blockquote>",
        ),
    ] {
        let carve = carve::djot_to_carve(djot);
        assert!(carve.contains(written), "{djot:?} -> {carve}");
        assert!(
            html(&carve).contains(expected),
            "{djot:?} -> {}",
            html(&carve)
        );
    }
}
