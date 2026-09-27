use carve::{Options, SocialLinkResolverInput};
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let target = args.next().ok_or("expected a render target")?;
    let source = fs::read_to_string(args.next().ok_or("expected an input path")?)?;
    let mention = |input: &SocialLinkResolverInput<'_>| {
        Ok(match input.name {
            "alice" => Some("/people/42".to_owned()),
            "unsafe" => Some("javascript:alert(1)".to_owned()),
            _ => None,
        })
    };
    let tag = |input: &SocialLinkResolverInput<'_>| {
        Ok((input.name == "release").then(|| "/collections/stable".to_owned()))
    };
    let options = Options::default()
        .with_mention_resolver(&mention)
        .with_tag_resolver(&tag);
    let output = match target.as_str() {
        "html" => carve::to_html_with_options(&source, &options),
        "markdown" => carve::to_markdown_with_options(&source, &options),
        "plain" => carve::to_plain_text_with_options(&source, &options),
        "ansi" => carve::to_ansi_with_options(&source, &options),
        "carve" => carve::to_carve_with_options(&source, &options),
        _ => return Err(format!("unknown render target: {target}").into()),
    };
    print!("{output}");
    Ok(())
}
