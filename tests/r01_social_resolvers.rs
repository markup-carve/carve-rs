//! C2-H-126: the `resolveMention`, `resolveTag` and `context` fields of
//! `social`, as `Options::social_resolvers`. Expected HTML is carve-js next2.
use carve::options::{Options, Social, SocialKind, SocialResolver, SocialResolvers};
use serde_json::json;
use std::sync::{Arc, Mutex};

fn render(source: &str, social: Option<Social>, resolvers: SocialResolvers) -> String {
    let opts = Options {
        social,
        social_resolvers: Some(resolvers),
        ..Options::default()
    };
    carve::api::render(source, &opts).unwrap()
}

#[test]
fn a_resolver_is_final_and_gets_kind_name_and_context() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let html = render(
        "@alice @bob #news\n",
        Some(Social {
            mention_url: Some("/t/{name}".into()),
            tag_url: None,
        }),
        SocialResolvers {
            resolve_mention: Some(SocialResolver::new(move |q| {
                log.lock()
                    .unwrap()
                    .push((q.kind, q.name.to_string(), q.context.cloned()));
                (q.name == "alice").then(|| format!("/u/{}", q.name))
            })),
            resolve_tag: Some(SocialResolver::new(|q| {
                assert_eq!(q.kind.as_str(), "tag");
                Some(format!("/tag/{}", q.name))
            })),
            context: Some(json!({"site": 1})),
        },
    );
    assert_eq!(
        html,
        "<p><a class=\"mention\" href=\"/u/alice\">@alice</a> <span class=\"mention\"><strong>@bob</strong></span> <a class=\"tag\" href=\"/tag/news\">#news</a></p>"
    );
    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            (
                SocialKind::Mention,
                "alice".to_string(),
                Some(json!({"site": 1}))
            ),
            (
                SocialKind::Mention,
                "bob".to_string(),
                Some(json!({"site": 1}))
            ),
        ]
    );
}

#[test]
fn an_empty_url_or_a_panic_is_the_inert_span() {
    let span = "<p><span class=\"mention\"><strong>@alice</strong></span></p>";
    let empty = SocialResolvers {
        resolve_mention: Some(SocialResolver::new(|_| Some(String::new()))),
        ..SocialResolvers::default()
    };
    assert_eq!(render("@alice\n", None, empty), span);
    let panics = SocialResolvers {
        resolve_mention: Some(SocialResolver::new(|_| panic!("host error"))),
        ..SocialResolvers::default()
    };
    assert_eq!(render("@alice\n", None, panics), span);
}

#[test]
fn inside_a_link_the_resolver_is_not_called() {
    let html = render(
        "[see @alice](/x)\n",
        None,
        SocialResolvers {
            resolve_mention: Some(SocialResolver::new(|_| panic!("called inside a link"))),
            ..SocialResolvers::default()
        },
    );
    assert_eq!(
        html,
        "<p><a href=\"/x\">see <span class=\"mention\"><strong>@alice</strong></span></a></p>"
    );
}

#[test]
fn a_template_serves_the_pair_without_a_resolver() {
    let html = render(
        "#tag @al.ice\n",
        Some(Social {
            mention_url: None,
            tag_url: Some("/t/{name}".into()),
        }),
        SocialResolvers {
            resolve_mention: Some(SocialResolver::new(|q| Some(format!("/u/{}", q.name)))),
            ..SocialResolvers::default()
        },
    );
    assert_eq!(
        html,
        "<p><a class=\"tag\" href=\"/t/tag\">#tag</a> <a class=\"mention\" href=\"/u/al.ice\">@al.ice</a></p>"
    );
}
