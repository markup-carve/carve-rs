import io

p = 'src/render_carve.rs'
s = io.open(p, encoding='utf-8').read()

old = """            with_block_attrs(
                &code.attrs,
                &format!(
                    "{fence}{info}\\n{}\\n{fence}",
                    protect_verbatim(&code.content)
                ),
            )"""

new = """            // The opener's quoted title is resolved onto `attrs.title` at parse
            // time so it reaches every consumer, but the fence carries it too -
            // emitting both says it twice and re-parses with an attribute ORDER
            // slot the source never had (carve#369). The fence is the authored
            // spelling, so it wins.
            let attrs = match (&code.title, &code.attrs) {
                (Some(title), Some(a)) if a.key_values.get("title") == Some(title) => {
                    without_key(a, "title")
                }
                _ => code.attrs.clone(),
            };
            with_block_attrs(
                &attrs,
                &format!(
                    "{fence}{info}\\n{}\\n{fence}",
                    protect_verbatim(&code.content)
                ),
            )"""

assert old in s, 'code block arm not matched'
s = s.replace(old, new, 1)

anchor = "fn with_block_attrs(attrs: &Option<Attrs>, body: &str) -> String {"
helper = '''/// A copy of `attrs` without one key-value, dropping the slot from `order`.
/// Returns `None` when the removal leaves nothing to render.
fn without_key(attrs: &Attrs, key: &str) -> Option<Attrs> {
    let mut next = attrs.clone();
    next.key_values.remove(key);
    next.order.retain(|slot| !matches!(slot, AttrSlot::KeyValue(k) if k == key));
    if next.id.is_none() && next.classes.is_empty() && next.key_values.is_empty() {
        return None;
    }
    Some(next)
}

'''
assert anchor in s
s = s.replace(anchor, helper + anchor, 1)
io.open(p, 'w', encoding='utf-8').write(s)
print('ok')
