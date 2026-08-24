# Reversible AST patches

`create_reversible_ast_patch` creates forward and inverse operations together
with document preconditions.

```rust
use carve::{apply_reversible_ast_patch, create_reversible_ast_patch, parse};

let before = parse("Before.\n");
let after = parse("After.\n");
let patch = create_reversible_ast_patch(&before, &after)?;
let accepted = apply_reversible_ast_patch(&before, &patch, false)?;
let rejected = apply_reversible_ast_patch(&accepted, &patch, true)?;
# Ok::<(), carve::AstPatchError>(())
```

Preconditions reject a patch applied to the wrong document. Keeping the inverse
with the forward patch makes accept, reject, and undo deterministic rather than
requiring an application to reconstruct old values after the fact.

This is the first useful primitive for editorial review, transaction history,
and ProseMirror integration. The draft uses stable FNV fingerprints over the
position-independent AST. A versioned wire format, source-preserving patches,
stable editor identities, and comment anchors remain follow-up work.
