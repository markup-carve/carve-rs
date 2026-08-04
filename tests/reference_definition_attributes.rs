//! PART 9R R1 + §16: a TRAILING attribute block on a link reference definition
//! attaches to the DEFINITION and transfers to every link that resolves the
//! label, with the link's own attributes overriding per key (carve#604).
//!
//! PART 9R already carried the semantics -- its symbol table is
//! `linkDefs : label -> (url, title?, attrs?)` and R1 already said definition
//! attributes transfer. What was missing was the PRODUCTION: there was no way
//! to write the `attrs` the rule consumes, so the feature was unreachable
//! rather than undecided (markup-carve/carve#612 added the slot).

#[test]
fn definition_attributes_transfer_to_the_link() {
    assert_eq!(
        carve::to_html("[Example][ex]\n\n[ex]: https://example.com {.external}\n"),
        "<p><a href=\"https://example.com\" class=\"external\">Example</a></p>"
    );
}

/// The point of the construct: a definition exists so a destination is written
/// once. Before this, attributing one used ten times meant repeating the
/// attribute ten times.
#[test]
fn every_link_resolving_the_label_gets_them() {
    assert_eq!(
        carve::to_html("[A][ex] [B][ex]\n\n[ex]: /u {.e}\n"),
        "<p><a href=\"/u\" class=\"e\">A</a> <a href=\"/u\" class=\"e\">B</a></p>"
    );
}

/// "Override per key" is §15 A3's merge, the one stacked attribute lists
/// already use: a repeated id or key takes the LAST value (the link's) and
/// classes ACCUMULATE across the two lists. A rule where the link's class
/// REPLACED the definition's would make this the only place in Carve where
/// stacking classes drops one.
#[test]
fn the_link_wins_the_key_and_classes_accumulate() {
    assert_eq!(
        carve::to_html("[Example][ex]{.internal #b}\n\n[ex]: /u {.external #a}\n"),
        "<p><a href=\"/u\" class=\"external internal\" id=\"b\">Example</a></p>"
    );
}

/// A3 accumulates ACROSS lists; inside one attribute block a repeated class
/// still collapses, exactly as on an inline link.
#[test]
fn a_repeated_class_within_one_block_still_deduplicates() {
    assert_eq!(
        carve::to_html("[x][r]{.a .a}\n\n[r]: /u\n"),
        "<p><a href=\"/u\" class=\"a\">x</a></p>"
    );
}

/// The production is `space, attributes`, so a brace run touching the
/// destination stays part of it.
#[test]
fn a_brace_run_with_no_space_stays_in_the_destination() {
    assert_eq!(
        carve::to_html("[x][r]\n\n[r]: /u{.x}\n"),
        "<p><a href=\"/u{.x}\">x</a></p>"
    );
}

/// Widening the parse must not change what counts as a definition: this was a
/// definition with the trailing junk ignored, and still is.
#[test]
fn a_junk_tail_is_still_a_definition() {
    assert_eq!(
        carve::to_html("[x][r]\n\n[r]: /u junk here\n"),
        "<p><a href=\"/u\">x</a></p>"
    );
}

/// The block is SCANNED rather than matched on `{[^}]*}`: a value may hold a
/// `}` inside quotes, and stopping at the first brace drops every attribute on
/// the line silently.
#[test]
fn a_closing_brace_inside_a_quoted_value_survives() {
    assert_eq!(
        carve::to_html("[x][r]\n\n[r]: /u {data-x=\"}\" .a}\n"),
        "<p><a href=\"/u\" data-x=\"}\" class=\"a\">x</a></p>"
    );
}

#[test]
fn an_invalid_block_is_ignored_rather_than_breaking_the_definition() {
    assert_eq!(
        carve::to_html("[x][r]\n\n[r]: /u {!!!}\n"),
        "<p><a href=\"/u\">x</a></p>"
    );
}

/// A floating attribute line ABOVE a definition floats PAST it to the next
/// visible block (§15 A2a). Different constructs, both well-defined, and this
/// is the case that shows they do not compete.
#[test]
fn an_attribute_line_above_the_definition_still_floats_past_it() {
    assert_eq!(
        carve::to_html("{.a}\n[ex]: /u {.b}\n\n[E][ex] and text\n"),
        "<p class=\"a\"><a href=\"/u\" class=\"b\">E</a> and text</p>"
    );
}
