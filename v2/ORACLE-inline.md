# Oracle against the clauses (inline pass)

Inputs where the TypeScript oracle contradicts a clause. The Rust pass stays
on the clause for each.

## OR-INL-001 Trailing spaces of a line inside a code span

Clause C2-I-002: "Spaces and tabs at the end of a line of the content are not
part of it, on the last line as on the others."

```carve
`a  
  b` x
```

Oracle: `code.value` is `"a  \nb"` (the spaces before the line break kept).
The clause removes them before any inline form is read, so Rust gives
`"a\nb"`. The same holds for math, literal spans, braced comments and
editorial comments. This is the most frequent difference in the generated
differential (about one input in a thousand).
