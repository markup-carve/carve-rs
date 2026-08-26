//! Below a definition description's content column an invisible line is lazy
//! paragraph text OF THAT CONTAINER (markup-carve/carve#1809, §10 I5 DEFINITION
//! OWNERSHIP IS COLUMN-SCOPED; markup-carve/carve-rs#1443, corpus 430 and 431).
//!
//! §10 I5's missing half was WHICH container: "lazy paragraph text" names an
//! operation on an OPEN paragraph, so ending the description and emitting the
//! same characters one level out has not carried the sentence out, whatever the
//! bytes look like.
//!
//! THREE MECHANISMS HAD TO CHANGE HERE, and each alone leaves a different half
//! fold - which is why every row asserts the characters AND, where the kind has
//! one, the absence of the registration or the attachment:
//!
//!  1. this collector briefly BROKE on a definition-shaped line
//!     (markup-carve/carve-rs#1438 read the band the other way round, and
//!     carve#1809 overruled that reading), which put the link and footnote kinds
//!     at document level;
//!  2. `item_block_opener` counted a footnote definition and a block-attribute
//!     line as openers, and the below-column guard ejected them before the
//!     interrupt test ever saw the line - the amended BELOW THE BODY'S COLUMN
//!     THE BODY ENDS bullet is explicit that it is about OPENERS;
//!  3. `rebase_overindented_blocks` then rebased the folded line's RESIDUAL
//!     INDENT away, delivering it FLUSH inside the description where the block
//!     parser recognized its shape all over again: `{.k}` became the `dd`'s own
//!     floating attribute and attached to the paragraph below it.
//!
//! Both render entry points on every row: `to_html` opens with a layout fast
//! path and the CLI runs transforms, and a divergence on one path only is what a
//! single-path assertion cannot see.

fn html(source: &str) -> String {
    let convenience = carve::to_html(source);
    let cli = carve::try_to_html_with_options(source, &carve::Options::default())
        .expect("the default profile denies nothing");
    assert_eq!(
        convenience, cli,
        "the two render paths disagree on:\n{source}"
    );
    convenience
}

const ENDS: &str = "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>\n";

fn folds(line: &str) -> String {
    format!("<dl>\n  <dt>t</dt>\n  <dd>d\n{line}\ntail</dd>\n</dl>")
}

#[test]
fn every_kind_folds_at_both_columns_of_the_band() {
    // The band is two columns wide under a `:  ` body and the answer does not
    // move inside it. The plain line is in the set because it is what "folds as
    // text" means, and it folded from every column all along.
    for indent in [" ", "  "] {
        for line in ["[r]: /u", "[^f]: n", "{.k}", "*[A]: a", "x"] {
            assert_eq!(
                html(&format!(":: t\n:  d\n{indent}{line}\ntail\n")),
                folds(line),
                "indent {indent:?} line {line:?}"
            );
        }
    }
}

#[test]
fn nothing_registers_so_a_reference_below_stays_literal() {
    // The half-fold rows: characters on the page AND an entry in a symbol table
    // is the shape a bytes-only assertion passes.
    assert_eq!(
        html(":: t\n:  d\n  [r]: /u\ntail\n\nSee [text][r].\n"),
        format!("{}\n<p>See [text][r].</p>", folds("[r]: /u"))
    );
    assert_eq!(
        html(":: t\n:  d\n  [^f]: n\ntail\n\nSee[^f]\n"),
        format!("{}\n<p>See[^f]</p>", folds("[^f]: n"))
    );
    let abbr = html(":: t\n:  d\n  *[A]: a\ntail\n\nA here\n");
    assert_eq!(abbr, format!("{}\n<p>A here</p>", folds("*[A]: a")));
    assert!(!abbr.contains("<abbr"), "{abbr}");
}

#[test]
fn the_attribute_attaches_to_nothing_inside_the_description() {
    let out = html(":: t\n:  d\n  {.k}\ntail\n");
    assert_eq!(out, folds("{.k}"));
    assert!(!out.contains("class="), "{out}");
}

#[test]
fn control_a_comment_is_column_exempt_and_renders_nothing() {
    // Corpus 430-5. Waiving the comment with the rest would put its characters
    // on the page.
    assert_eq!(
        html(":: t\n:  d\n  %% c\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>d</dd>\n</dl>"
    );
    assert_eq!(
        html(":: t\n:  d\n  %% c\ntail\n"),
        format!("{ENDS}<p>tail</p>")
    );
}

#[test]
fn control_a_real_opener_below_the_column_still_ends_the_body() {
    // The half that must NOT move: a fix that folded everything below the column
    // fails here, and this is the row that separates the two readings.
    for indent in [" ", "  "] {
        for opener in ["> q", "# h", "| a |", "---", "::: note"] {
            let out = html(&format!(":: t\n:  d\n{indent}{opener}\n"));
            assert!(
                out.starts_with(ENDS.trim_end_matches('\n')),
                "indent {indent:?} opener {opener:?} did not end the body:\n{out}"
            );
        }
    }
}

#[test]
fn control_at_column_zero_the_line_acts() {
    // Corpus 431 and 431-4: the document's own opener column, where a definition
    // registers and a floating attribute attaches forward.
    assert_eq!(
        html(":: t\n:  d\n{.k}\ntail\n"),
        format!("{ENDS}<p class=\"k\">tail</p>")
    );
    assert_eq!(
        html(":: t\n:  d\n[r]: /u\n\nSee [text][r].\n"),
        format!("{ENDS}<p>See <a href=\"/u\">text</a>.</p>")
    );
}

#[test]
fn control_at_the_content_column_the_line_is_inside_the_description() {
    // Not this band. A definition at the column is collected, and an attribute
    // at the column is scoped to the description and dropped by §15 A4 - corpus
    // 329-a-floating-attribute-is-scoped-to-the-container-that-holds-it-5.
    assert_eq!(
        html(":: t\n:  d\n   [r]: /u\ntail\n\nSee [text][r].\n"),
        format!("{ENDS}<p>tail</p>\n<p>See <a href=\"/u\">text</a>.</p>")
    );
    assert_eq!(
        html(":: t\n:  d\n   {.k}\ntail\n"),
        format!("{ENDS}<p>tail</p>")
    );
}

#[test]
fn control_the_list_item_host_it_agrees_with() {
    // The host whose answer this is, in the same build: it was their
    // DISAGREEMENT that was the defect, so one host cannot record it.
    assert_eq!(
        html("- d\n [r]: /u\ntail\n"),
        "<ul>\n  <li>d\n[r]: /u\ntail</li>\n</ul>"
    );
    assert_eq!(
        html("- para\n  {.k}\n\n  more\n"),
        "<ul>\n  <li><p>para</p>\n    <p class=\"k\">more</p>\n  </li>\n</ul>"
    );
}
