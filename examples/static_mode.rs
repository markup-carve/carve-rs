//! Demo: render one document both ways - interactive HTML (live disclosures,
//! client-script diagrams, KaTeX-ready math) and static HTML (flattened
//! sections, diagrams/math degraded to source or a supplied build renderer).
//!
//! Run with:
//!
//!     cargo run --example static_mode
//!
//! carve-rs ships Details, Spoiler, FencedRender (mermaid / chart) and
//! MathBlock; it has no Tabs / CodeGroup extension (those are carve-js /
//! carve-php only), so this demo exercises the four it has plus core math.

use carve::{Details, FencedRender, MathBlock, Mode, Options, Spoiler, StaticRenderers};

const SRC: &str = "\
# Static render mode demo

::: details \"Build steps\"
Run `cargo build`, then `cargo test`.
:::

Plot twist: :spoiler[the butler did it].

::: spoiler \"Ending\"
Everyone lives happily ever after.
:::

``` mermaid
graph TD; A --> B
```

``` chart
{\"type\":\"bar\",\"data\":[1,2,3]}
```

``` math
\\int_0^1 x^2 \\, dx
```

Inline math: $`e^{i\\pi} + 1 = 0`.
";

fn extensions() -> (Details, Spoiler, FencedRender, FencedRender, MathBlock) {
    (
        Details::new(),
        Spoiler::new(),
        FencedRender::mermaid(),
        FencedRender::chart(),
        MathBlock::new(),
    )
}

fn main() {
    let (details, spoiler, mermaid, chart, math) = extensions();
    let exts: [&dyn carve::CarveExtension; 5] = [&details, &spoiler, &mermaid, &chart, &math];

    // 1) Interactive: live disclosures, client-hydration diagram elements,
    //    KaTeX-ready math.
    let mut interactive = Options::new().with_mode(Mode::Interactive);
    for ext in exts {
        interactive = interactive.with_extension(ext);
    }
    println!("===== INTERACTIVE (mode = interactive) =====\n");
    println!("{}", carve::to_html_with_options(SRC, &interactive));

    // 2) Static, NO build renderers: disclosures expand to inert <section>s,
    //    diagrams/math degrade to escaped source - never blank, fully
    //    self-contained for print / PDF / archival.
    let mut static_source = Options::new().with_mode(Mode::Static);
    for ext in exts {
        static_source = static_source.with_extension(ext);
    }
    println!("\n\n===== STATIC, source fallback (mode = static, no renderers) =====\n");
    println!("{}", carve::to_html_with_options(SRC, &static_source));

    // 3) Static WITH build renderers: a host injects build-time mermaid / chart
    //    / graphviz / math renderers (here stubs) so the static HTML is a finished artifact
    //    with no client scripts. This is the API carve-py will wrap: each
    //    renderer is a boxed closure keyed by extension on StaticRenderers.
    let mut static_ssr = Options::new()
        .with_mode(Mode::Static)
        .with_renderers(StaticRenderers {
            mermaid: Some(Box::new(|src: &str| {
                format!(
                    "<svg class=\"mermaid\" data-bytes=\"{}\"><!-- pre-rendered --></svg>",
                    src.len()
                )
            })),
            chart: Some(Box::new(|_src: &str| {
                "<img alt=\"chart\" src=\"chart.svg\">".to_string()
            })),
            graphviz: Some(Box::new(|_src: &str| {
                "<img alt=\"graphviz\" src=\"graph.svg\">".to_string()
            })),
            plantuml: Some(Box::new(|_src: &str| {
                "<img alt=\"plantuml\" src=\"uml.svg\">".to_string()
            })),
            math: Some(Box::new(|tex: &str, display: bool| {
                format!(
                    "<math display=\"{}\"><!-- {} --></math>",
                    display,
                    tex.trim()
                )
            })),
        });
    for ext in exts {
        static_ssr = static_ssr.with_extension(ext);
    }
    println!("\n\n===== STATIC, server-rendered (mode = static, with renderers) =====\n");
    println!("{}", carve::to_html_with_options(SRC, &static_ssr));
}
