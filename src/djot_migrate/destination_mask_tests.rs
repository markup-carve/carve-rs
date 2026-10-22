use super::mask_djot_forms;

#[test]
fn opaque_parentheses_keep_following_text_visible() {
    for target in ["u`)`z", "u{a=\")\"}x"] {
        let source = format!("[t]({target}) and ~x~ later");
        assert_eq!(
            mask_djot_forms(&source, true, false, None, &[]),
            format!("[t]{} and ~x~ later", " ".repeat(target.len() + 2))
        );
    }
}

#[test]
fn quote_boundary_requires_space_or_line_ending() {
    for (heads, boundary) in [
        (&["># heading", ">***", ">{.c}", ">:::"][..], false),
        (&["> # heading", "> ***", "> {.c}", "> :::", ">"][..], true),
    ] {
        for head in heads {
            let source = format!("{head}\n[r]: /a`b");
            let mask = super::mask_djot_opaque(
                &source,
                super::OpaqueOptions {
                    unclosed_code: false,
                    ..super::OpaqueOptions::default()
                },
            );
            let expected = if boundary {
                source.replace("/a`b", "    ")
            } else {
                source
            };
            assert_eq!(mask, expected.into_bytes(), "{head}");
        }
    }
}

#[test]
fn list_hosted_block_boundaries_remain_opaque() {
    for head in [
        "- ",
        "1. ",
        ": ",
        "- [ ] ",
        "- # H",
        "- [a]: /x",
        "- {.c}",
        "- :::",
    ] {
        let source = format!("{head}\n[r]: /a`b\n[s]: /c`d");
        let mask = super::mask_djot_opaque(
            &source,
            super::OpaqueOptions {
                unclosed_code: false,
                ..super::OpaqueOptions::default()
            },
        );
        let expected = source
            .replace("/a`b", "    ")
            .replace("/c`d", "    ")
            .replace("/x", "  ");
        assert_eq!(mask, expected.into_bytes(), "{head}");
    }
}

#[test]
fn definitions_do_not_join_code_across_paragraphs() {
    let source = "[r]: /x\n\na `b\n\nc` d";
    let mask = super::mask_djot_opaque(
        source,
        super::OpaqueOptions {
            unclosed_code: false,
            ..super::OpaqueOptions::default()
        },
    );
    assert_eq!(mask, source.replace("/x", "  ").into_bytes());
}

#[test]
fn disabling_inline_forms_keeps_destination_visible() {
    let source = "[t](u~x~)";
    assert_eq!(mask_djot_forms(source, false, false, None, &[]), source);
}
