//! PART 9 §25 caps nesting DEPTH. This pins what a level COSTS.
//!
//! Every recursive walk over the tree - parse, the derived `Clone` a render runs
//! on entry, the renderers, the AST serializer, the derived `Drop` - moves
//! `BlockNode` values by value. The enum's size is therefore a direct multiplier
//! on the stack a nesting level takes, and §25's cap of 200 levels multiplies it
//! again. On a host with a 1 MiB stack that product is the difference between an
//! engine that refuses and one that aborts (markup-carve/carve-wasm#44, #1119).
//!
//! `Figure` used to set that size at 472 bytes - it embeds a whole `Table` or
//! `CodeBlock` through `FigureTarget` - against 264 for the next largest
//! variant. So the rarest node kind in the language priced every walk over every
//! document, including documents with no figure in them. Boxing its target caps
//! the enum near the second-largest variant instead.

/// A ceiling, not an equality: the point is that no ONE variant may run away
/// with the size again, not that the number never moves. A field added to
/// `Table` may legitimately raise this a little. A whole node embedded in a
/// variant may not - box it instead.
#[test]
fn no_single_variant_sets_the_size_of_every_block_node() {
    let size = std::mem::size_of::<carve::ast::BlockNode>();
    assert!(
        size <= 288,
        "BlockNode is {size} bytes. Every recursive walk moves these by value, so this is \
         what a nesting level costs before anything else, and PART 9 §25's cap of 200 levels \
         multiplies it. Box the payload of whichever variant grew rather than raising this."
    );
}

/// The same statement for inline nodes, which the inline walks move by value the
/// same way. `InlineNode` is the larger of the two today at 280 bytes, so this
/// records the current value rather than improving on it: it is here so that a
/// variant embedding a whole block-level node cannot land unnoticed.
#[test]
fn no_single_variant_sets_the_size_of_every_inline_node() {
    let size = std::mem::size_of::<carve::ast::InlineNode>();
    assert!(
        size <= 296,
        "InlineNode is {size} bytes; see the note on BlockNode in this file."
    );
}
