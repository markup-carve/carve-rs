# File inclusion

## Using includes

`{{ path }}` include directives (spec §19) are a **processor-level, opt-in**
feature. The parser never touches the filesystem and does not know the directive
exists, so with no resolver configured `{{ … }}` stays literal text.

The CLI enables inclusion for file inputs, with the containment root defaulting
to the **directory of the input document**, never the process working
directory. Stdin has no path context and therefore no inferable root, so
directives stay literal unless `--include-root` names one:

```bash
carve book/main.crv                       # root defaults to book/
carve --include-root . book/main.crv      # widen the root to the project
carve --include-root ./book < main.crv    # required to enable includes on stdin
```

`FileSystemResolver::new` refuses a root spec that is not **absolute** (spec
§19): a relative one has no base the specification names, and every
canonicalizer resolves it against the process working directory, which is
exactly what §19 forbids the root defaulting to. The relative spellings above
keep working because the CLI expands them in its own argument parsing before
constructing the resolver; an embedder that wants the same convenience does the
same.

```
{{ chapters/intro.crv }}              include a whole file
{{ chapters/intro.crv #setup }}       include one heading subtree, or the one
                                      block whose {#setup} id matches
{{ notes.crv @lines:10-25 }}          include a physical line range
{{ notes.crv @region:demo }}          include lines between named region markers
{{ src/main.rs @code:rust }}          include a file as one code block
{{ src/main.rs @region:demo @code }}  include a region as a code block, no language
{{ chapters/intro.crv @shift:2 }}     shift included heading levels by +2
{{ chapters/intro.crv @shift:auto }}  shift to sit under the heading in scope
```

`#section` matches an id exactly, case included: `#plan` does not select
`{#Plan}`. A heading with that id (explicit or generated) selects its section,
which ends with its container when the heading sits inside one. Otherwise the
first block with that explicit id is included alone, at any depth. Ids on list
items, table cells, inline elements and footnotes select nothing.

`#section`, `@lines` and `@region` are selection options; a directive uses at
most one. A region is the lines between a `#region name` marker line and its
matching `#endregion name` line; the marker lines are excluded. Which marker
lines count depends on the include mode. A Carve include recognizes only the
`%%` line comment, `%% #region name` and `%% #endregion name`, so the markers
never render in the file that holds them. A code include (`@code`) also
recognizes `#region name`, `// #region name`, `-- #region name`,
`/* #region name */` and `<!-- #region name -->`; a block comment's closer
must end the same line. The keywords are case-sensitive, a `%%%` line is a
comment fence and never a marker, and CRLF, CR and LF all end a line for
`@lines` and `@region`. `carve lint` reports a source-style or mixed-case marker
line in a Carve file as `region-marker-renders-as-text`.

`@code` or `@code:lang` turns the selected source (the whole file, an `@lines`
range or an `@region` region) into one code block. The payload is not parsed,
so a `{{ … }}` or a fence line inside it is content. A leading byte order mark
is dropped and every line ends with a line feed; an empty selection gives an
empty block. `@code:lang` sets the fence language; a bare `@code` sets none and
nothing is inferred from the file extension. A block attribute line above the
directive applies to the code block as it would to an authored fence, so a
class, a `title` or a callout-reading extension works the same. `@code` cannot
combine with `#section` (`include-selection-conflict`), `@shift` has no effect
on it, and a code include inside a paragraph is `include-block-in-inline`.

`select_include_region(source, name, mode)` resolves `@region:name` in a file's
text with the expander's own scanner and returns the 1-based content lines, so
an editor jumping to a region follows the same marker rules.

Every failure (a missing file, binary content,
conflicting selectors, an invalid or missing region, a
cycle, the depth limit, the size budget) emits a warning on stderr and leaves
the directive **literal**; inclusion never silently drops a directive. A
rejected directive has **no observable side effects**: the output is
byte-identical to the same document with that directive written as literal text
from the start, so a rejected include can never renumber an id or footnote
label that a later, successful include claims.

`--report-includes FILE` writes the dependency list (spec I11) as JSON, the way
`--report-losses` writes the render-loss report; `-` sends it to stderr. Each
entry carries `id`, `resolved`, and `denial` where the resolver named a class.
A target that is simply not there is named by where it WOULD be, so the watch
lands on the file the author meant; one denied by containment keeps the
directive's spelling. The list is **not** capped the way the warnings are, so a
host can key file watching off it even for a document with hundreds of refused
directives:

```bash
carve --report-includes deps.json book/main.crv > main.html
```

```json
{"dependencies":[{"id":"/book/intro.crv","resolved":true},{"id":"/book/gone.crv","resolved":false,"denial":"not-found"}]}
```

`carve fmt` **preserves** a well-formed directive verbatim rather than escaping
its braces, so formatting a document never breaks its includes. A run that is
not a well-formed directive (`{{ oops`, or an unknown `@option`) is ordinary
text and is escaped as such. To keep a literal `{{ … }}` out of the include
processor's way, put it in a code span or fence, where directives are inert.

Paths are checked against the root by **canonicalizing and then testing
containment**, not by banning `..` lexically. A `../shared/glossary.crv` whose
target stays inside the root resolves; symlinks out of the root, symlinked
directory components, and absolute paths outside the root are denied.

From the library, expansion is a pass between parse and render:

```rust
use carve::{expand_includes, parse, render_html, FileSystemResolver, IncludeOptions};

let source = std::fs::read_to_string("book/main.crv")?;
let resolver = FileSystemResolver::new("/srv/book")?;
let options = IncludeOptions::new()
    .with_resolver(&resolver)
    .with_source_path("book/main.crv");

let result = expand_includes(parse(&source), &source, &options);
for warning in &result.warnings {
    eprintln!("{}: {}", warning.rule, warning.message);
}
// Every target touched, resolved or not: hosts key file watchers off this.
// A refused one carries the class the resolver gave, such as `outside-root`.
for dependency in &result.dependencies {
    let denial = dependency.denial.map(|d| d.as_str()).unwrap_or("-");
    println!("{} (resolved: {}, denial: {denial})", dependency.id, dependency.resolved);
}
// Bytes charged against the byte budget: every target the resolver handed
// back, INCLUDING one refused for breaking the budget. A target is resolved
// before its size is known, so the budget bounds the expanded output rather
// than the work; this is the counter of what was read.
println!("charged {} bytes", result.charged_bytes);
let html = render_html(&result.doc);
```

Implement `IncludeResolver` for a virtual filesystem, an in-memory map, or any
other source; `FileSystemResolver` is the ready-made one for trusted hosts. A
resolver returns `Ok(IncludeResolved)` or `Err(IncludeDenial)`, the error naming
why it refused:

```rust
use carve::{IncludeContext, IncludeDenial, IncludeResolved, IncludeResolver};

struct MapResolver(std::collections::HashMap<String, String>);

impl IncludeResolver for MapResolver {
    fn resolve(
        &self,
        path: &str,
        _ctx: &IncludeContext<'_>,
    ) -> Result<IncludeResolved, IncludeDenial> {
        self.0
            .get(path)
            .map(|source| IncludeResolved::with_id(source.clone(), path))
            .ok_or(IncludeDenial::NotFound)
    }
}
```

A child is parsed as a document of its own, so hand it the extensions the parent
was parsed with through `IncludeOptions::with_extension`. Otherwise syntax an
extension adds, a `[[Page]]` wikilink say, works in the parent and stays literal
in the child.

`prepare_doc_with_includes` runs expansion and then the same extension-hook and
profile pipeline the `to_*` entry points use, so included content is subject to
exactly the same sanitization as content the author typed directly. It also
parses each child with its `Options` extensions, with no second list to pass.

Source-position remapping for included spans (spec I4) is not implemented in any
engine yet; warnings carry the identity of the file they arose in instead.

### One self-contained file

`carve flatten` writes the document back as Carve with every include expanded in
place; the deliberate opposite of `carve fmt`, which leaves directives alone so
formatting returns the author's document. Flattening is for handing the document
to something that has no filesystem behind it: a web editor, a paste box, a
colleague.

```bash
carve flatten book/main.crv > one-file.crv
carve flatten --include-root ./book < main.crv
```

Two things it changes beyond inlining, both reported on stderr: the output is
CANONICAL Carve, so formatting is normalized rather than preserved, and
colliding explicit ids (on any element) and footnote labels are renamed (two
files that were never in one document together can each define `intro`). Each
renamed copy gets its own `-N`, the least one no id in the assembled document
uses, so a child that writes a colliding `intro` twice yields `intro-2` and
`intro-3`. Ids compare exactly: `Intro` and `intro` do not collide. The renames
are written into the source, so the flattened file renders exactly like the
expanded original.

### Turning the filesystem off entirely

`FileSystemResolver` is behind the default-on `fs` feature. Build with
`--no-default-features` and the crate carries no code that opens a file at all,
which is what a sandboxed, WASM or browser-hosted embedder wants: the guarantee
is then structural rather than a matter of not configuring a resolver. The CLI's
include path compiles out with it, so `--include-root` is refused rather than
silently ignored. Such a build can still expand includes from a resolver the
host writes itself (over an in-memory map, a virtual filesystem, `fetch`), since
a resolver is just a function.

`FileSystemResolver` reads at most 4 MiB per target (`DEFAULT_MAX_FILE_BYTES`),
adjustable with `with_max_file_bytes`. The expansion byte budget cannot stand in
for this: it charges a target only once its source is in hand, so without a cap
one oversized file is read into memory in full before expansion refuses it.
