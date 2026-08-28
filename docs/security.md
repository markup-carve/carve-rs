# Untrusted input


The normative hardening is always on and needs no configuration: dangerous URL
schemes are blanked, event-handler attributes like `onclick` are dropped, and the
bidi override/isolate characters behind Trojan Source are removed from rendered
text.

Raw passthrough is the deliberate exception. A ` ```=html ` block or a
`` `…`{=html} `` span renders **verbatim** by design, so it is the one thing input
you did not author has to switch off:

```rust
let options = carve::Options::new()
    .with_raw_html(false)                        // escape =html, do not emit it
    .with_profile(carve::Profile::comment());    // full | article | comment | minimal

let html = carve::try_to_html_with_options(untrusted, &options)?;
```

Use the `try_*` entry points here, not `to_html_with_options`. The infallible
wrappers are `try_…().unwrap_or_default()`, so a profile rejection - input past
`max_length`, or a denied construct when the profile's action is `Error` - comes
back as an **empty string**, which a caller cannot tell from a document that
legitimately rendered to nothing.

`Profile` also carries a link policy; pair it with
`Options::with_profile_base_host` so the policy can tell internal links from
external ones.

An untrusted **AST payload** is bounded the same way, through the same profile:

```rust
let doc = carve::from_json(untrusted_payload)?;
let prepared = carve::prepare_document_for_render(
    doc,
    &options,
    carve::Mode::Interactive,
    true,
)?;
let html = carve::render_html_with_options(&prepared, &options)?;
```

`prepare_document_for_render` is where the profile applies on this path, and the
caps there are sized from the payload's measured length rather than from the
`srcByteLength` it carries - that number arrives inside the payload, so a hostile
tree could otherwise claim to have come from nothing and render anything, or
claim a gigabyte and widen its own expansion budget. `Document::source_len`
still reports the claim as written; `untrusted_input_len()` and
`expansion_budget_len()` report what may be trusted to size a cap.

`from_json` replaces every U+0000 a string value carries with U+FFFD, before it
reads that value for anything else - the same replacement `normalize_source`
performs on Carve source, so an ingested document renders like the same document
written as source (PART 12 section 21). A raw control byte in JSON text stays a
syntax error, which is RFC 8259 rather than a Carve rule.

Runnable version of all of the above, including what a rejection looks like:
`cargo run --example untrusted_input`. Full recipe, defaults and threat model:
[Security](https://markup-carve.github.io/carve/security).

---

[Back to the README](../README.md)
