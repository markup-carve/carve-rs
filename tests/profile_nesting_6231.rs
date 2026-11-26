use carve::options::Profile;
use carve::{Options, RenderError};

fn opts(profile: Profile) -> Options {
    Options {
        profile,
        ..Options::default()
    }
}

#[test]
fn minimal_plain_list_has_no_loss_even_when_strict() {
    let options = Options {
        strict_losses: true,
        ..opts(Profile::Minimal)
    };
    let source = "- a\n- b";
    assert_eq!(
        carve::try_render(source, &options),
        Ok("<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>".into())
    );
    assert!(
        carve::render_with_diagnostics(source, &options)
            .diagnostics
            .iter()
            .all(|d| d.code != "profile-construct-degraded")
    );
}

#[test]
fn minimal_counts_lists_but_not_items_paragraphs_or_inlines() {
    let options = opts(Profile::Minimal);
    let at_bound = "- - a `b`";
    let beyond = "- - - a `b`";
    assert!(
        carve::render_with_diagnostics(at_bound, &options)
            .diagnostics
            .iter()
            .all(|d| d.code != "profile-construct-degraded")
    );
    let rendered = carve::render_with_diagnostics(beyond, &options);
    let losses: Vec<_> = rendered
        .diagnostics
        .iter()
        .filter(|d| d.code == "profile-construct-degraded")
        .collect();
    assert_eq!(losses.len(), 1);
    assert!(
        losses[0]
            .message
            .contains("Degraded list because it exceeds the nesting limit")
    );
    assert_eq!(
        carve::try_render(
            beyond,
            &Options {
                strict_losses: true,
                ..options
            }
        ),
        Err(RenderError::Losses(vec!["profile-construct-degraded"]))
    );
}

#[test]
fn comment_quote_at_bound_stays_and_type_exclusion_wins() {
    let at_bound = format!("{}plain", "> ".repeat(4));
    let comment = opts(Profile::Comment);
    assert!(
        carve::render_with_diagnostics(&at_bound, &comment)
            .diagnostics
            .iter()
            .all(|d| d.code != "profile-construct-degraded")
    );

    let minimal = opts(Profile::Minimal);
    let loss = carve::render_with_diagnostics("> plain", &minimal)
        .diagnostics
        .into_iter()
        .find(|d| d.code == "profile-construct-degraded")
        .expect("quote is excluded by type");
    assert!(loss.message.contains("does not allow: block_quote"));

    let nested_excluded = carve::render_with_diagnostics("- - > plain", &minimal);
    let loss = nested_excluded
        .diagnostics
        .iter()
        .find(|d| d.code == "profile-construct-degraded")
        .expect("quote is also past the list depth bound");
    assert!(loss.message.contains("does not allow: block_quote"));
}

#[test]
fn thousand_list_variants_obey_the_same_container_bound() {
    let options = opts(Profile::Minimal);
    let leaves = [
        "a", "b", "c `d`", "e *f*", "g /h/", "i _j_", "k ~l~", "m {+n+}", "o {^p^}", "q {,r,}",
    ];
    let mut checked = 0;
    for depth in 1..=10 {
        for leaf in leaves {
            for sibling in 0..10 {
                let source = format!("{}{}\n- tail{}", "- ".repeat(depth), leaf, sibling);
                let rendered = carve::render_with_diagnostics(&source, &options);
                let losses: Vec<_> = rendered
                    .diagnostics
                    .iter()
                    .filter(|d| d.code == "profile-construct-degraded")
                    .collect();
                assert_eq!(losses.len(), usize::from(depth > 2), "{source}");
                if let Some(loss) = losses.first() {
                    assert!(loss.message.contains("nesting limit"), "{source}");
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 1_000);
}
