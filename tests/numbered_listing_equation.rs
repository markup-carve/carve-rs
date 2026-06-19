//! Numbered cross-references for code listings and display-math equations
//! (markup-carve/carve#87). A caption after a fenced code block or a standalone
//! display-math block wraps it in a `<figure>` and joins the per-label
//! numbering + `</#id>` crossref machinery, just like figures and tables.

const FENCE: &str = "```python\nx = 1\n```";

#[test]
fn listing_caption_wraps_in_figure() {
    let out = carve::to_html(&format!("{FENCE}\n^ Listing 1: example"));
    assert!(out.contains("<figure>"), "{out}");
    assert!(
        out.contains("<pre><code class=\"language-python\">x = 1\n</code></pre>"),
        "{out}"
    );
    assert!(
        out.contains("<figcaption>Listing 1: example</figcaption>"),
        "{out}"
    );
}

#[test]
fn code_block_without_caption_is_bare_pre() {
    let out = carve::to_html(FENCE);
    assert!(!out.contains("<figure>"), "{out}");
    assert!(
        out.contains("<pre><code class=\"language-python\">x = 1\n</code></pre>"),
        "{out}"
    );
}

#[test]
fn listing_number_placeholder_and_crossref() {
    let out = carve::to_html(&format!(
        "{{#lst-a}}\n{FENCE}\n^ Listing #: example\n\nSee </#lst-a>."
    ));
    assert!(
        out.contains("<figcaption>Listing 1: example</figcaption>"),
        "{out}"
    );
    assert!(
        out.contains("See <a href=\"#lst-a\">Listing 1</a>."),
        "{out}"
    );
}

#[test]
fn listing_counter_is_per_label() {
    let out = carve::to_html(&format!(
        "{FENCE}\n^ Listing #: one\n\n{FENCE}\n^ Listing #: two"
    ));
    assert!(out.contains("Listing 1: one"), "{out}");
    assert!(out.contains("Listing 2: two"), "{out}");
}

const EQ: &str = "$$`E = mc^2`";

#[test]
fn equation_caption_wraps_in_figure() {
    let out = carve::to_html(&format!("{EQ}\n^ Equation 1: mass-energy"));
    assert!(out.contains("<figure>"), "{out}");
    assert!(
        out.contains("<span class=\"math display\">\\[E = mc^2\\]</span>"),
        "{out}"
    );
    assert!(
        out.contains("<figcaption>Equation 1: mass-energy</figcaption>"),
        "{out}"
    );
}

#[test]
fn display_math_without_caption_is_bare_paragraph() {
    let out = carve::to_html(EQ);
    assert!(!out.contains("<figure>"), "{out}");
    assert!(
        out.contains("<span class=\"math display\">\\[E = mc^2\\]</span>"),
        "{out}"
    );
}

#[test]
fn inline_math_or_trailing_prose_is_not_wrapped() {
    assert!(!carve::to_html("Energy is $`E=mc^2` here.\n^ Equation #: x").contains("<figure>"));
    assert!(!carve::to_html(&format!("{EQ} and more text\n^ Equation #: x")).contains("<figure>"));
}

#[test]
fn equation_number_placeholder_and_crossref() {
    let out = carve::to_html(&format!(
        "{{#eq-e}}\n{EQ}\n^ Equation #: mass-energy\n\nSee </#eq-e>."
    ));
    assert!(out.contains("<figure id=\"eq-e\">"), "{out}");
    assert!(
        out.contains("See <a href=\"#eq-e\">Equation 1</a>."),
        "{out}"
    );
}

#[test]
fn equation_counter_is_per_label() {
    let out = carve::to_html(&format!(
        "{EQ}\n^ Equation #: one\n\n$$`a+b`\n^ Equation #: two"
    ));
    assert!(out.contains("Equation 1: one"), "{out}");
    assert!(out.contains("Equation 2: two"), "{out}");
}

#[test]
fn indented_standalone_equation_is_recognized() {
    let out = carve::to_html(&format!("   {EQ}\n^ Equation #: indented"));
    assert!(
        out.contains("<figcaption>Equation 1: indented</figcaption>"),
        "{out}"
    );
    assert!(
        out.contains("<span class=\"math display\">\\[E = mc^2\\]</span>"),
        "{out}"
    );
}
