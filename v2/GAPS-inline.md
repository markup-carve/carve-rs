# Gaps of the inline chapter (0.2 Rust engine, inline pass)

Each entry: the nearest clause, a minimal input, the readings, what the
oracle does, what the Rust pass does. Code paths carry the gap id in a comment.

## INL-001 `autolink.text` is unspecified

Nearest: C2-A-050 (`autolink`: `href`, `text`?), C2-I-073.

```carve
<https://a.b> <a@b.com>
```

Readings: `text` always holds the body; `text` only where it differs from
`href`; `text` never (a renderer derives it). The oracle writes `text` only
for the email form (`text: "a@b.com"`, `href: "mailto:a@b.com"`). Rust
follows the oracle.

## INL-002 A soft break spans the dropped spaces around it

Nearest: C2-A-152 (a break owns its line ending), C2-I-103 (spaces before and
after are dropped).

```carve
a  
  b
```

Readings: the break spans the line ending only (offsets 3-4); the break also
spans the trailing spaces and the next line's indent (1-6). The oracle gives
1-6 for a paragraph line (from behind the last kept character to the first
character of the next line). Rust follows the oracle: the break runs from the
trimmed end of its piece to the start of the next piece.

## INL-003 A line comment's span starts at the spaces it drops

Nearest: C2-I-023 ("the spaces before it are dropped with it"), C2-A-151.

```carve
a %% c
```

The oracle's comment spans 1-6, the text `a` 0-1. Rust follows the oracle.
The clause says the spaces are dropped but not who owns them in the tree.

## INL-004 A failed reference label leaves the pair to the shortcut test

Nearest: C2-I-072, C2-I-077, C2-I-079.

```carve
[x][a %% b] and [x][ ]

[x]: /u
```

Readings: the label fails, so the pair is text (C2-I-077: "the brackets are
text"); or the label fails and the pair is tested as a shortcut. The oracle
makes `[x]` a shortcut reference in both cases. Rust follows the oracle. A
label starting with `@` or `^` keeps the pair as text (C2-I-078, C2-I-125),
in the oracle as in Rust.

## INL-005 C2-B-224 and code spans in a cell

Nearest: C2-B-224 ("an escaped pipe is a literal pipe in cell text before
inline parsing"), C2-B-169, C2-I-015.

```carve
| `a\|b` |
```

Readings: "before inline parsing" makes the code value `a|b`; the list of
C2-B-224 (link and image destinations, image label, reference label) is
exhaustive and code keeps `a\|b`. The oracle keeps `a\|b`. Rust follows the
oracle: `\|` reads as `|` in a link destination, an image alt and a
reference or footnote label of a cell; in text `\|` is an ordinary escape.

## INL-006 C2-B-223 against C2-I-017 (unclosed code span in a cell)

Nearest: C2-B-223 ("an unclosed code span at the end of a cell is text") and
C2-I-017 ("a code span never fails").

```carve
| `abc | x |
```

The two clauses contradict for cells. The oracle makes the backtick text.
Rust follows C2-B-223 for `ContentKind::Cell` (also for the `!`, `$` and
`$$` prefixed forms, which the clause does not name; the prefix stays text).
C2-I-017 should name the exception.

## INL-007 Abbreviation boundaries across atoms

Nearest: C2-I-099 ("an element edge is no boundary of its own"), C2-I-100.

```carve
`x`HTML and HTML{% c %}s and {*HTML*}s

*[HTML]: H
```

Readings: the neighbor across an atom is the atom's first or last character;
an atom is a boundary. The oracle matches `HTML` after the code span (the
atom is a boundary), does not match before `{% c %}s` (a braced comment is
transparent, C2-I-026) and does not match in `{*HTML*}s` (the element edge
is transparent). Rust does the same.

## INL-008 `caption_number` has a source character but no `pos`

Nearest: C2-A-158 ("`caption_number`, which has no source text").

```carve
![a](i.png)
^ Figure #: x
```

The mark `#` is source text, so the reason C2-A-158 gives does not hold.
The oracle omits `pos` on the caption number (and, oddly, on the text nodes
next to it, which Rust does not copy: C2-A-138 requires their `pos`). Rust
omits `pos` on the caption number only.

## INL-009 What counts toward the depth cap for delimiters

Nearest: C2-I-118 ("a delimiter opener is text when the block depth, the
open delimiter entries and the bracket constructs around it reach the cap").

Readings for a delimiter inside a bracket construct: the open entries are
those of its own construct's stack; or also the entries open outside the
construct at that point (which may never pair). Rust counts the block depth,
the bracket constructs around the delimiter and the open entries of the
delimiter's own construct. A bracket construct that ends up past the cap
after pairing (inside many delimiter spans) is written as its source text
around its children. Not compared with the oracle.

## INL-010 Positions in a cell joined by a continuation row

Nearest: C2-A-158 ("the inline nodes of a cell that a continuation row
joined" may omit `pos`).

```carve
| `a |
+ b` |
```

Readings: only the nodes that cross the join omit `pos`; every inline node of
the cell omits it. The oracle omits `pos` on every inline node of such a
cell. Rust follows the oracle.

## INL-011 A line comment before a line break owns the trailing spaces

Nearest: C2-A-074 (the value drops trailing spaces), C2-I-002, C2-A-151.

```carve
a %% x  
b
```

The oracle's comment spans 1-8 (trailing spaces included) and the soft break
8-9; on the last line of the content the comment ends before the trailing
spaces (`a  %%  ` gives 1-5). Rust follows the oracle in both cases.

## INL-012 Exact runs of smart typography: which neighbors count

Nearest: C2-I-116 ("an arrow ... needs its exact run"), C2-I-117 ("`+-` ...
next to another `+` or `-` it is text").

```carve
--+- and -+- and {--->-} and {+x+-+} and {-+-x-}
```

Readings: the neighbor is the source character; or the neighbor is a
character of the same text that no earlier replacement took. The oracle:
`--+-` is an en dash and `±` (the replaced `-` does not count), `-+-` is
text, `{--->-}` holds an arrow (the opener's mark does not count),
`{+x+-+}` holds `+-` as text (the closer's `+` counts after `+-`), and an
arrow before a closer mark (`{-x<---}`) is an arrow. Rust follows the oracle:
the character before a run counts only when it is unreplaced text of the
same text run; after a run, `+-` reads the source and an arrow reads only its
own text.

## INL-013 Abbreviations at the edges of an inline footnote

Nearest: C2-I-099 ("an element edge is no boundary of its own").

```carve
word^[HT]

*[HT]: Hyper
```

Read literally, `d` before the footnote blocks the match. The oracle matches
`HT`: the edges of an inline footnote are boundaries (its content is a note
of its own). Rust follows the oracle.

## INL-014 Runs that are not replaced take no characters

Nearest: C2-I-115 ("the longest match is taken, among these runs and the
runs of C2-I-117"), `smart-punctuation.json` rows with `replaced: false`.

```carve
a {->=}
```

Readings: `->` is a run of the longest match that is then left as text, so
`>=` is never seen; or the rows with `replaced: false` only document what
stays text. The oracle writes `≥`. Rust skips the unreplaced rows.

## INL-015 `{=name}` after a code span: which code span counts as closed

Nearest: C2-I-021, C2-I-022, C2-I-045.

Rust reads the raw marker only after a closed plain code span (no `!`, `$`
or `$$` prefix), as the clauses say. The oracle agrees on the fixture.

## INL-016 The lookup key of a label that holds `\|` outside a cell

Nearest: C2-I-079 ("a backslash stays in the key"), C2-B-224.

Outside a cell `[a\|b]` keeps the backslash in its key; inside a cell the
key is `a|b`. Rust does that; the clauses do not say it outright.
