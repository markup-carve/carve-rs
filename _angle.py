import io

p = 'src/parse.rs'
s = io.open(p, encoding='utf-8').read()

old = """    let mut i = start;
    let href_start = i;
    // Per the grammar, an inline link destination ends at the first whitespace
    // or first `)` (no balanced-paren or escape rule). A `)` that needs to live
    // in a URL comes via a reference definition; the markdown renderer
    // percent-encodes it on the way out.
    while i < bytes.len()
        && bytes[i] != b' '
        && bytes[i] != b')'
        && bytes[i] != b'\\t'
        && bytes[i] != b'\\n'
    {
        i += 1;
    }
    if i == href_start {
        return None;
    }
    let href = std::str::from_utf8(&bytes[href_start..i]).ok()?.to_string();"""

new = """    let mut i = start;
    let href_start = i;
    // The ANGLE form (`<...>`) carries a destination a bare run cannot: it may
    // hold a parenthesis or a space, which is what lets a URL like
    // `https://x/Foo_(bar)` survive at all (carve#377). It cannot hold `<`, `>`
    // or a newline; if no closing `>` is found the bare scan below runs
    // instead, so an unclosed `<` stays ordinary content.
    let href = if bytes.get(i) == Some(&b'<') {
        let mut j = i + 1;
        while j < bytes.len() && bytes[j] != b'>' && bytes[j] != b'<' && bytes[j] != b'\\n' {
            j += 1;
        }
        if bytes.get(j) == Some(&b'>') {
            let inner = std::str::from_utf8(&bytes[i + 1..j]).ok()?.to_string();
            i = j + 1;
            Some(inner)
        } else {
            None
        }
    } else {
        None
    };
    let href = match href {
        Some(h) => h,
        None => {
            // Per the grammar, a BARE inline link destination ends at the first
            // whitespace or first `)` (no balanced-paren or escape rule).
            while i < bytes.len()
                && bytes[i] != b' '
                && bytes[i] != b')'
                && bytes[i] != b'\\t'
                && bytes[i] != b'\\n'
            {
                i += 1;
            }
            if i == href_start {
                return None;
            }
            std::str::from_utf8(&bytes[href_start..i]).ok()?.to_string()
        }
    };"""

assert old in s, 'destination scan not matched'
io.open(p, 'w', encoding='utf-8').write(s.replace(old, new, 1))
print('ok')
