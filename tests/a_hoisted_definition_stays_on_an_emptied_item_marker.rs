//! Regression for carve-rs#1144.
//!
//! Collection hoists definitions to the document. When a definition was the
//! only content of a nested marker-line item, the item's source position must
//! put it back on that marker line instead of spelling the empty item with `+`.

const SHAPES: &[(&str, &str, &str)] = &[
    (
        "two levels and following colon",
        "* * [d]: u\n  :\n",
        "* * [d]: u\n  :\n",
    ),
    (
        "two levels and following text",
        "* * [d]: u\n  x\n",
        "* * [d]: u\n  x\n",
    ),
    (
        "hyphen markers",
        "- - [d]: u\n  tail\n",
        "- - [d]: u\n  tail\n",
    ),
    (
        "footnote definition",
        "* * [^f]: n\n  :\n",
        "* * [^f]: n\n  :\n",
    ),
    ("no following content", "* * [d]: u\n", "* * [d]: u\n"),
    // A TOP-LEVEL emptied item keeps `- +`, the older canonical form that corpus
    // fixtures 16-reference-link-4 and
    // 117-footnote-definition-inside-a-container-is-collected-2 pin. It
    // round-trips there because nothing follows at a shallower column for the
    // marker to capture, and carve-js and carve-php draw the line in the same
    // place. A first draft registered every emptied item regardless of depth,
    // which rewrote both fixtures' form and made the bytes disagree with the
    // other two engines.
    (
        "empty sibling item",
        "- a\n- [d]: u\n",
        "- a\n- +\n\n[d]: u\n",
    ),
    (
        "inside a div",
        "::: n\n* * [d]: u\n  :\n:::\n",
        "::: n\n* * [d]: u\n  :\n:::\n",
    ),
    (
        "three levels",
        "* * * [d]: u\n    :\n",
        "* * * [d]: u\n    :\n",
    ),
    (
        "following continuation line",
        "* * [d]: u\n  :\n  more\n",
        "* * [d]: u\n  :\n  more\n",
    ),
];

#[test]
fn every_emptied_marker_line_has_the_cross_engine_canonical_spelling() {
    for (name, source, expected) in SHAPES {
        let formatted = carve::to_carve(source);
        assert_eq!(&formatted, expected, "wrong canonical bytes for {name}");
        assert_eq!(
            carve::to_carve(&formatted),
            formatted,
            "fmt(fmt(x)) != fmt(x) for {name}"
        );
        assert_eq!(
            carve::to_html(&formatted),
            carve::to_html(source),
            "to_html(fmt(x)) != to_html(x) for {name}"
        );
    }
}

#[test]
fn every_definition_still_resolves_after_the_round_trip() {
    // The half the byte assertions cannot see: a writer that kept the
    // definition's TEXT but lost the hoist would satisfy every spelling above
    // and leave the reference dead. Asserted on the rendered output rather than
    // on bytes - where the writer places a hoisted definition relative to a
    // following use site is its own question, and not this test's.
    for (name, shape, _) in SHAPES {
        let (use_site, resolved) = if name == &"footnote definition" {
            ("\n[^f]\n", "role=\"doc-noteref\"")
        } else {
            ("\n[use][d]\n", "href=\"u\"")
        };
        let source = format!("{shape}{use_site}");
        let formatted = carve::to_carve(&source);
        let rendered = carve::to_html(&formatted);

        assert!(
            rendered.contains(resolved),
            "the definition stopped resolving after fmt for {name}: {rendered}"
        );
        assert_eq!(
            carve::to_html(&source),
            rendered,
            "to_html(fmt(x)) != to_html(x) with a use site for {name}"
        );
        assert_eq!(
            carve::to_carve(&formatted),
            formatted,
            "fmt(fmt(x)) != fmt(x) with a use site for {name}"
        );
    }
}

#[test]
fn a_three_level_marker_definition_keeps_following_content_in_the_middle_item() {
    assert_eq!(
        carve::to_html("* * * [d]: u\n    :\n"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>\n",
            "        <ul>\n",
            "          <li></li>\n",
            "        </ul>\n",
            "        :\n",
            "      </li>\n",
            "    </ul>\n",
            "  </li>\n",
            "</ul>",
        )
    );
}

#[test]
fn a_four_level_marker_definition_keeps_following_content_in_the_parent_item() {
    assert_eq!(
        carve::to_html("* * * * [d]: u\n      :\n"),
        concat!(
            "<ul>\n",
            "  <li>\n",
            "    <ul>\n",
            "      <li>\n",
            "        <ul>\n",
            "          <li>\n",
            "            <ul>\n",
            "              <li></li>\n",
            "            </ul>\n",
            "            :\n",
            "          </li>\n",
            "        </ul>\n",
            "      </li>\n",
            "    </ul>\n",
            "  </li>\n",
            "</ul>",
        )
    );
}
