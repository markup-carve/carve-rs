# Djot image alternative text

Discarded Djot attributes or comments can leave a blank line in an image's alternative text. For example:

```djot
![a
{.x}
b](img)
```

Djot's reference renderer produces `a`, two line breaks, and `b`. Carve image labels cannot cross a blank paragraph separator. Import reports `structure-unspellable` at the image's source line. The exported source may render as literal image notation instead of an image.

Spaces and tabs immediately before an image-label line break also receive this warning. Carve removes that trailing whitespace, changing the `alt` value. Whitespace inside discarded attributes or comments does not trigger the warning.

Ordinary single line breaks, escaped braces, and verbatim brace text are preserved. The warning describes export loss; it does not reconstruct the original value.

Use `carve::migrate_djot(source).report.diagnostics`, or `carve migrate --from djot --report report.json input.dj`.

Djot's reference HTML can also contain duplicate `alt` attributes when an image has an explicit `alt` attribute. Import preserves that behavior, with the computed label value first.
