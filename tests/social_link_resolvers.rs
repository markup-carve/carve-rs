use std::cell::Cell;

use carve::{Options, SocialLinkKind, SocialLinkResolverInput};

#[test]
fn resolvers_are_authoritative_and_receive_context() {
    let calls = Cell::new(0);
    let tenant = 42_u64;
    let mention = |input: &SocialLinkResolverInput<'_>| -> Result<Option<String>, String> {
        calls.set(calls.get() + 1);
        assert_eq!(input.kind, SocialLinkKind::Mention);
        assert_eq!(
            input.context.and_then(|value| value.downcast_ref::<u64>()),
            Some(&tenant)
        );
        Ok(match input.name {
            "alice" => Some("/people/42".to_string()),
            "unsafe" => Some("javascript:alert(1)".to_string()),
            _ => None,
        })
    };
    let tag = |input: &SocialLinkResolverInput<'_>| -> Result<Option<String>, String> {
        Ok((input.name == "release").then(|| "/collections/stable".to_string()))
    };
    let options = Options::new()
        .with_mention_url("/fallback/{name}")
        .with_tag_url("/fallback-tag/{name}")
        .with_mention_resolver(&mention)
        .with_tag_resolver(&tag)
        .with_social_context(&tenant);

    assert_eq!(
        carve::to_html_with_options("@alice @missing #release @unsafe", &options),
        "<p><a class=\"mention\" href=\"/people/42\">@alice</a> <span class=\"mention\"><strong>@missing</strong></span> <a class=\"tag\" href=\"/collections/stable\">#release</a> <span class=\"mention\"><strong>@unsafe</strong></span></p>"
    );
    assert_eq!(calls.get(), 3);
}

#[test]
fn resolver_errors_render_the_inert_form() {
    let resolver = |_input: &SocialLinkResolverInput<'_>| -> Result<Option<String>, String> {
        Err("lookup failed".to_string())
    };
    let options = Options::new().with_mention_resolver(&resolver);
    assert_eq!(
        carve::to_html_with_options("@alice", &options),
        "<p><span class=\"mention\"><strong>@alice</strong></span></p>"
    );
}

#[test]
fn resolver_receives_ast_attributes() {
    let document = carve::from_json(
        r##"{"type":"document","children":[{"type":"paragraph","children":[{"type":"mention","user":"alice","attrs":{"id":"lead","order":["#id"]}}]}],"srcByteLength":0}"##,
    )
    .expect("decode attributed mention");
    let resolver = |input: &SocialLinkResolverInput<'_>| -> Result<Option<String>, String> {
        assert_eq!(
            input.attrs.and_then(|attrs| attrs.id.as_deref()),
            Some("lead")
        );
        Ok(Some("/people/42".to_string()))
    };
    let options = Options::new().with_mention_resolver(&resolver);
    assert_eq!(
        carve::render_html_with_options(&document, &options).expect("render attributed mention"),
        "<p><a class=\"mention\" href=\"/people/42\" id=\"lead\">@alice</a></p>"
    );
}
