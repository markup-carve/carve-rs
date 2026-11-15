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

## OR-INL-002 An unclosed backtick run in a cell

Clauses C2-I-014 and C2-I-119: an unclosed backtick run is text, the whole
run. C2-B-223 makes an unclosed code span at the end of a cell text.

```carve
| a ``x`y | b |
```

Oracle: only the first backtick of the run is text; the second one opens a
one-backtick code span closed by the later backtick (`"a `"`, code `"x"`,
`"y"`). Rust keeps the whole two-backtick run as text, so the cell is the
single text `"a ``x`y"`.

## OR-INL-003 No abbreviations in table cells

Clause C2-I-098: abbreviations apply to every text of the inline content.

```carve
| HT |

*[HT]: Hyper
```

Oracle: the cell text stays plain `HT`, no `abbreviation` node. Rust applies
the definitions to cells as to any other content. The fixture comparison
passes an empty abbreviation list for cells so the rest of the cell can be
compared.
