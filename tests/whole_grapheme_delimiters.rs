use carve::{parse, render_html};

#[test]
fn a_decorated_delimiter_is_text() {
    for (source, expected) in [
        ("*️⃣abc*", "<p>*️⃣abc*</p>"),
        ("*a*️⃣", "<p>*a*️⃣</p>"),
        ("*a*́ b*", "<p><strong>a*́ b</strong></p>"),
        ("_︎x _y_", "<p>_︎x <u>y</u></p>"),
        ("~a~‍b~", "<p><s>a~‍b</s></p>"),
        (
            "*x* *️⃣ *y*",
            "<p><strong>x</strong> *️⃣ <strong>y</strong></p>",
        ),
        ("{*️⃣abc*}", "<p>{*️⃣abc*}</p>"),
        ("/*️a*/", "<p><em>*️a*</em></p>"),
        ("/*a*/́ b*/", "<p><strong><em>a*/́ b</em></strong></p>"),
    ] {
        assert_eq!(render_html(&parse(source)).unwrap(), expected, "{source}");
    }
}
