# Migrating into Carve

HTML, Markdown, Djot and BBCode all convert into Carve. Only the HTML importer drops anything, and only it takes a mode, an adapter and a loss report.

HTML migration is available through `html_to_ast` and `html_to_carve`. Both
return the value plus ordered loss diagnostics and accept safe, semantic, and
trusted-roundtrip policies. The CLI equivalent is:

```sh
carve migrate --from html --mode safe --report report.json input.html
```

A `<math>` element is read for the TeX it already carries: a `<semantics>`
annotation declaring `application/x-tex`, `text/x-tex` or `LaTeX`, else
`alttext` with the assumption reported. There is no MathML-to-TeX converter
here by decision (carve#1210 D6), so an element carrying neither is dropped
with a warning in `safe` and `semantic` and preserved verbatim in
`roundtrip` - its children are a token stream, and concatenating them reads
`<mfrac><mn>1</mn><mn>2</mn></mfrac>` as `12` rather than as one half.

`--adapter word` and `--adapter google-docs` add one recognition the `generic`
default does not risk: footnote-shaped HTML. A word processor writes a note as
a body anchor and a definition block that link to each other, and none of them
uses the `doc-noteref` / `doc-endnotes` roles a Carve engine writes, so under
`generic` a note arrives as a literal link beside an orphaned list. Under those
two adapters the pair is matched through the fragment each anchor addresses and
written back as a footnote reference and definition, whatever the ids are
called - Word's `_ftnref1`/`_ftn1`, Google Docs' `ftnt_ref1`/`ftnt1`,
LibreOffice's `sdfootnote1anc`/`sdfootnote1sym` and Pandoc's `fnref1`/`fn1` all
pair by the same rule. Back-links, the marker anchors they sit on, and the rule
separating the notes from the body are generated navigation and are dropped. A
reference whose target is missing stays a link, and a definition nothing
references stays ordinary content rather than becoming a definition that renders
as nothing. Name the adapter only for input you know came from that editor: on
arbitrary HTML a mutually linked anchor pair is not proof of a footnote, which
is why `generic` stays out.

Markdown migration is `markdown_to_ast` and `markdown_to_carve`, or
`carve migrate --from markdown input.md`. It parses the source to a tree and
writes it canonically, so the output is the document rather than the author's
spelling: a setext heading comes back as `#`, an indented code block as a
fence. There is no mode or report, because nothing is dropped - the
`--mode`/`--adapter`/`--report` options belong to HTML.

Djot migration is `djot_to_carve`, or `carve migrate --from djot input.dj`. It
rewrites the delimiters that differ between the two languages, and like
Markdown it has no mode or report.

BBCode migration is `bbcode_to_carve`, or
`carve migrate --from bbcode input.bbcode`. It converts forum formatting,
links, images, quotes, lists, code, spoilers and tables while keeping ordinary
Carve-looking source text literal. Inputs above 256 KiB are rejected because
the compatibility rewrite pipeline makes several bounded passes over a post.

---

[Back to the README](../README.md)
