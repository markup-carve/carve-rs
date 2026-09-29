use carve::{parse, render_ansi};

#[test]
fn ansi_code_payload_keeps_every_line() {
    for payload in ["", "\n", "\n\n", "a\n", "a\n\n", "a\n\n\n", "\na\n\n"] {
        let source = format!("```\n{payload}```\n");
        let expected = if payload.is_empty() {
            "\n".to_string()
        } else {
            payload
                .strip_suffix('\n')
                .unwrap()
                .split('\n')
                .map(|line| format!("\x1b[97m  {line}\x1b[0m\n"))
                .collect::<String>()
        };
        assert_eq!(
            render_ansi(&parse(&source)).unwrap(),
            expected,
            "{source:?}"
        );
    }
}
