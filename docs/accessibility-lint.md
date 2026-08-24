# Accessibility linting

`lint_accessibility` reports structural accessibility problems directly from
Carve source.

```rust
use carve::lint_accessibility;

for diagnostic in lint_accessibility("# One\n\n### Three\n\n![](/map.png)\n") {
    eprintln!("{}: {}", diagnostic.rule, diagnostic.message);
}
```

The draft currently reports:

- `a11y/image-alt` for an image with empty alternative text;
- `a11y/heading-jump` when a heading skips a level.

Diagnostics carry severity and source offsets. This lets CLIs, editors, CI,
and bindings present the same finding instead of implementing separate regex
checks that disagree about Carve structure.

The practical value is early feedback while authors can still fix the source,
plus a foundation for a `wcag` profile. This is not a WCAG-conformance claim.
Nested-container traversal, an explicit decorative-image marker, profile
configuration, JSON output, and further rules remain draft work.
