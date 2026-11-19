//! PROVISIONAL standard extensions of `carve::api::extensions`: the fenced
//! renderers, `mathBlock`, `codeGroup` and `codeCallouts`, ported from the
//! carve-js reference (carve-js#5068, test/extensions-fenced-renderers.test.ts).

#![allow(clippy::format_in_format_args)] // expected HTML is assembled from parts

use carve::api::extensions::{
    self, Bundle, CodeGroupMode, CodeGroupOptions, FencedRenderOptions, MathBlockOptions,
    PresetBuilders, PresetOptions, Tag,
};
use carve::api::{self, Carve, Mode, Options, WriteOptions};
use serde_json::Value;
use std::sync::Arc;

fn host(bundle: Bundle) -> Carve {
    let mut carve = Carve::new();
    carve.register(bundle).unwrap();
    carve
}

fn html_in(source: &str, bundle: Bundle, mode: Mode) -> String {
    let opts = Options {
        mode,
        ..Options::default()
    };
    host(bundle).render(source, &opts).unwrap()
}

fn html(source: &str, bundle: Bundle) -> String {
    html_in(source, bundle, Mode::Interactive)
}

fn codes(source: &str, bundle: Bundle, mode: Mode) -> Vec<String> {
    let opts = Options {
        mode,
        ..Options::default()
    };
    host(bundle)
        .render_with_diagnostics(source, &opts)
        .unwrap()
        .diagnostics
        .into_iter()
        .map(|e| e.code)
        .collect()
}

fn core(source: &str) -> String {
    api::render(source, &Options::default()).unwrap()
}

fn fenced(language: &[&str]) -> FencedRenderOptions {
    FencedRenderOptions {
        language: language.iter().map(|w| (*w).to_string()).collect(),
        ..FencedRenderOptions::default()
    }
}

fn preset() -> PresetOptions {
    PresetOptions::default()
}

#[test]
fn text_mode_escapes_amp_and_lt_but_keeps_gt() {
    assert_eq!(
        html("```d2\na -> b & <c\n```", extensions::d2(preset())),
        r#"<pre class="d2" role="img" aria-label="d2">a -> b &amp; &lt;c</pre>"#
    );
}

#[test]
fn graphviz_claims_dot_and_graphviz() {
    for word in ["dot", "graphviz"] {
        assert_eq!(
            html(
                &format!("```{word}\na -> b\n```"),
                extensions::graphviz(preset())
            ),
            r#"<pre class="graphviz" role="img" aria-label="graphviz">a -> b</pre>"#
        );
    }
}

#[test]
fn json_mode_wraps_the_body_in_a_script_and_guards_the_close() {
    assert_eq!(
        html(
            "```vega-lite\n{\"mark\": \"bar\"}\n```",
            extensions::vega_lite(preset())
        ),
        r#"<div class="vega-lite" role="img" aria-label="vega-lite"><script type="application/json">{"mark": "bar"}</script></div>"#
    );
    let guarded = html(
        "```chart\n{\"x\": \"</script>\"}\n```",
        extensions::chart(preset()),
    );
    assert_eq!(
        guarded,
        r#"<div class="chart" role="img" aria-label="chart"><script type="application/json">{"x": "<\/script>"}</script></div>"#
    );
    assert_eq!(guarded.matches("</script>").count(), 1);
}

#[test]
fn an_unclaimed_language_stays_core() {
    let src = "```python\nprint(1)\n```";
    assert_eq!(html(src, extensions::d2(preset())), core(src));
}

#[test]
fn the_css_class_precedes_the_author_classes_c2_h_010() {
    assert_eq!(
        html(
            "{#c1 .tall data-theme=dark}\n```d2\na -> b\n```",
            extensions::d2(preset())
        ),
        r#"<pre id="c1" class="d2 tall" data-theme="dark" role="img" aria-label="d2">a -> b</pre>"#
    );
}

#[test]
fn an_event_handler_attribute_is_dropped_c2_h_014() {
    assert!(
        !html(
            "{#c1 onclick=\"alert(1)\"}\n```d2\na\n```",
            extensions::d2(preset())
        )
        .contains("onclick")
    );
}

#[test]
fn an_authored_name_and_role_win_and_no_role_without_a_name() {
    assert_eq!(
        html(
            "{aria-label=\"Deploy flow\"}\n```d2\na\n```",
            extensions::d2(preset())
        ),
        r#"<pre class="d2" aria-label="Deploy flow" role="img">a</pre>"#
    );
    assert_eq!(
        html("{role=figure}\n```d2\na\n```", extensions::d2(preset())),
        r#"<pre class="d2" role="figure" aria-label="d2">a</pre>"#
    );
    let unnamed = FencedRenderOptions {
        label: Some(String::new()),
        ..fenced(&["d2"])
    };
    assert_eq!(
        html("```d2\na\n```", extensions::fenced_render(unnamed).unwrap()),
        r#"<pre class="d2">a</pre>"#
    );
}

#[test]
fn figure_tag_and_class_options() {
    let figure = FencedRenderOptions {
        wrap_in_figure: true,
        ..fenced(&["d2"])
    };
    assert_eq!(
        html("```d2\na\n```", extensions::fenced_render(figure).unwrap()),
        r#"<figure class="d2-figure"><pre class="d2" role="img" aria-label="d2">a</pre></figure>"#
    );
    let div = FencedRenderOptions {
        css_class: Some("diagram".into()),
        tag: Some(Tag::Div),
        ..fenced(&["d2"])
    };
    assert_eq!(
        html("```d2\na\n```", extensions::fenced_render(div).unwrap()),
        r#"<div class="diagram" role="img" aria-label="diagram">a</div>"#
    );
}

#[test]
fn abc_wavedrom_and_plantuml_are_text_presets() {
    assert_eq!(
        html("```abc\nCDEF\n```", extensions::abc(preset())),
        r#"<pre class="abc" role="img" aria-label="abc">CDEF</pre>"#
    );
    assert_eq!(
        html("```wavedrom\n{}\n```", extensions::wavedrom(preset())),
        r#"<pre class="wavedrom" role="img" aria-label="wavedrom">{}</pre>"#
    );
    assert_eq!(
        html(
            "```puml\nA <|-- B\nC --> D\n```",
            extensions::plantuml(preset())
        ),
        "<pre class=\"plantuml\" role=\"img\" aria-label=\"plantuml\">A &lt;|-- B\nC --> D</pre>"
    );
}

#[test]
fn mermaid_equals_its_factory() {
    let src = "```mermaid\ngraph TD; A-->B\n```";
    assert_eq!(
        html(src, extensions::mermaid(preset())),
        html(
            src,
            extensions::fenced_render(fenced(&["mermaid"])).unwrap()
        )
    );
}

#[test]
fn an_empty_language_is_refused() {
    assert!(extensions::fenced_render(fenced(&[""])).is_err());
    assert!(extensions::fenced_render(fenced(&[])).is_err());
}

#[test]
fn presets_register_every_fence_word() {
    let all = extensions::presets(PresetBuilders::default());
    let words: Vec<&str> = all.matchers.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        words,
        [
            "mermaid",
            "d2",
            "dot",
            "graphviz",
            "wavedrom",
            "abc",
            "plantuml",
            "puml",
            "vega-lite",
            "chart"
        ]
    );
    assert_eq!(
        html("```dot\ndigraph { a -> b }\n```", all),
        r#"<pre class="graphviz" role="img" aria-label="graphviz">digraph { a -> b }</pre>"#
    );
}

#[test]
fn c2_x_014_a_second_claim_of_one_word_fails_at_registration() {
    let both = Bundle::merge([
        extensions::mermaid(preset()),
        extensions::fenced_render(FencedRenderOptions {
            css_class: Some("m".into()),
            ..fenced(&["mermaid"])
        })
        .unwrap(),
    ]);
    let err = Carve::new().register(both).unwrap_err();
    assert!(err.to_string().contains("duplicate"), "{err}");
}

#[test]
fn c2_x_022_c2_x_024_the_claim_carries_fallback_and_source() {
    let carve = host(extensions::d2(preset()));
    let tree = carve
        .parse("{#c1}\n```d2\na\n```", &Options::default())
        .unwrap();
    let doc: Value = serde_json::from_str(&api::encode_tree(&tree)).unwrap();
    let node = &doc["children"][0];
    assert_eq!(node["type"], "block_extension");
    assert_eq!(node["name"], "carve.fencedRender.d2");
    assert_eq!(node["version"], "1");
    assert_eq!(node["attrs"]["id"], "c1");
    assert_eq!(node["props"]["language"], "d2");
    assert_eq!(
        node["source"],
        serde_json::json!({"form": "fence", "name": "d2", "body": "a\n"})
    );
    assert_eq!(node["fallback"][0]["type"], "code_block");
    assert_eq!(node["fallback"][0]["value"], "a\n");
    assert_eq!(node["fallback"][0]["syntax"], "d2");
    assert!(node["fallback"][0].get("attrs").is_none());
    assert_eq!(
        doc["extensions"],
        serde_json::json!([{"name": "carve.fencedRender.d2", "version": "1", "required": false, "slots": []}])
    );
    let again = api::decode_tree(&api::encode_tree(&tree)).unwrap();
    assert_eq!(api::encode_tree(&again), api::encode_tree(&tree));
}

#[test]
fn c2_x_027_the_fence_is_written_back() {
    let src = "{#c1 .x}\n```mermaid\ngraph TD; A-->B\n```\n";
    let carve = host(extensions::mermaid(preset()));
    let tree = carve.parse(src, &Options::default()).unwrap();
    assert_eq!(carve.write(&tree, &WriteOptions::default()).unwrap(), src);
}

#[test]
fn c2_x_035_without_a_renderer_the_code_block_and_a_loss() {
    let only = || extensions::d2(preset()).without_renderers();
    assert_eq!(
        html("```d2\na\n```", only()),
        "<pre><code class=\"language-d2\">a\n</code></pre>"
    );
    assert!(
        codes("```d2\na\n```", only(), Mode::Interactive)
            .contains(&"extension-fallback-rendered".to_string())
    );
    assert_eq!(
        html("{#c1 .x}\n```d2\na\n```", only()),
        "<div id=\"c1\" class=\"x\">\n  <pre><code class=\"language-d2\">a\n</code></pre>\n</div>"
    );
}

#[test]
fn c2_x_052_static_mode_without_a_build_writes_the_fallback() {
    assert_eq!(
        html_in(
            "```mermaid\na\n```",
            extensions::mermaid(preset()),
            Mode::Static
        ),
        "<pre><code class=\"language-mermaid\">a\n</code></pre>"
    );
}

#[test]
fn c2_x_052_static_mode_with_a_build_writes_its_output() {
    let built = || {
        extensions::mermaid(PresetOptions {
            build: Some(Arc::new(|s: &str| {
                format!(r#"<svg data-n="{}"></svg>"#, s.len())
            })),
            ..preset()
        })
    };
    assert_eq!(
        html_in("{#m}\n```mermaid\nabc\n```", built(), Mode::Static),
        r#"<div id="m" class="mermaid" role="img" aria-label="mermaid"><svg data-n="3"></svg></div>"#
    );
    assert!(
        !codes("```mermaid\nabc\n```", built(), Mode::Static)
            .contains(&"extension-fallback-rendered".to_string())
    );
}

#[test]
fn presets_pass_each_build_to_its_preset_only() {
    let bundle = || {
        extensions::presets(PresetBuilders {
            chart: Some(Arc::new(|_: &str| r#"<img alt="c">"#.to_string())),
            ..PresetBuilders::default()
        })
    };
    assert_eq!(
        html_in("```chart\n{}\n```", bundle(), Mode::Static),
        r#"<div class="chart" role="img" aria-label="chart"><img alt="c"></div>"#
    );
    assert_eq!(
        html_in("```d2\na\n```", bundle(), Mode::Static),
        "<pre><code class=\"language-d2\">a\n</code></pre>"
    );
}

fn math() -> Bundle {
    extensions::math_block(MathBlockOptions::default())
}

#[test]
fn math_block_renders_display_math() {
    assert_eq!(
        html("```math\nx^2\n```", math()),
        r#"<div class="math display">\[x^2\]</div>"#
    );
    assert_eq!(
        html("```math\na<b & c>d\n```", math()),
        r#"<div class="math display">\[a&lt;b &amp; c&gt;d\]</div>"#
    );
}

#[test]
fn math_block_takes_another_language_and_is_inert_unregistered() {
    let tex = extensions::math_block(MathBlockOptions {
        language: Some("tex".into()),
        ..MathBlockOptions::default()
    });
    assert_eq!(
        html("```tex\nx\n```", tex),
        r#"<div class="math display">\[x\]</div>"#
    );
    assert_eq!(html("```js\nx\n```", math()), core("```js\nx\n```"));
    assert_eq!(
        core("```math\nx\n```"),
        "<pre><code class=\"language-math\">x\n</code></pre>"
    );
}

#[test]
fn math_block_keeps_the_author_attributes_after_its_classes() {
    assert_eq!(
        html("{#e1 .big data-n=1}\n```math\nx\n```", math()),
        r#"<div id="e1" class="math display big" data-n="1">\[x\]</div>"#
    );
}

#[test]
fn math_block_static_mode() {
    let built = extensions::math_block(MathBlockOptions {
        build: Some(Arc::new(|tex: &str, display: bool| {
            format!(
                r#"<math display="{}">{tex}</math>"#,
                if display { "block" } else { "inline" }
            )
        })),
        ..MathBlockOptions::default()
    });
    assert_eq!(
        html_in("```math\nx\n```", built, Mode::Static),
        r#"<div class="math display"><math display="block">x</math></div>"#
    );
    assert_eq!(
        html_in("```math\nx\n```", math(), Mode::Static),
        "<pre><code class=\"language-math\">x\n</code></pre>"
    );
}

const GROUP: &str = "::: code-group\n```js [NPM]\nnpm i\n```\n\n```sh\nyarn add\n```\n:::\n";

fn pre(language: &str, value: &str) -> String {
    format!("<pre><code class=\"language-{language}\">{value}\n</code></pre>")
}

fn group(options: CodeGroupOptions) -> Bundle {
    extensions::code_group(options)
}

#[test]
fn code_group_css_mode() {
    assert_eq!(
        html(GROUP, group(CodeGroupOptions::default())),
        format!(
            "{}{}{}{}{}{}",
            r#"<div class="code-group" role="group" aria-label="Code examples">"#,
            r#"<input type="radio" name="codegroup-1" id="codegroup-1-tab-1" class="code-group-radio" checked><label for="codegroup-1-tab-1" class="code-group-label">NPM</label>"#,
            r#"<input type="radio" name="codegroup-1" id="codegroup-1-tab-2" class="code-group-radio"><label for="codegroup-1-tab-2" class="code-group-label">sh</label>"#,
            format!(
                r#"<div class="code-group-panel" role="group" aria-label="NPM">{}</div>"#,
                pre("js", "npm i")
            ),
            format!(
                r#"<div class="code-group-panel" role="group" aria-label="sh">{}</div>"#,
                pre("sh", "yarn add")
            ),
            "</div>"
        )
    );
}

#[test]
fn code_group_aria_mode() {
    let aria = CodeGroupOptions {
        mode: CodeGroupMode::Aria,
        ..CodeGroupOptions::default()
    };
    assert_eq!(
        html(GROUP, group(aria)),
        format!(
            "{}{}{}{}{}</div>",
            r#"<div class="code-group" role="tablist" aria-label="Code examples">"#,
            r#"<button type="button" role="tab" id="codegroup-1-tab-1" aria-selected="true" aria-controls="codegroup-1-panel-1" class="code-group-label">NPM</button>"#,
            r#"<button type="button" role="tab" id="codegroup-1-tab-2" aria-selected="false" aria-controls="codegroup-1-panel-2" class="code-group-label" tabindex="-1">sh</button>"#,
            format!(
                r#"<div role="tabpanel" id="codegroup-1-panel-1" aria-labelledby="codegroup-1-tab-1" class="code-group-panel">{}</div>"#,
                pre("js", "npm i")
            ),
            format!(
                r#"<div role="tabpanel" id="codegroup-1-panel-2" aria-labelledby="codegroup-1-tab-2" class="code-group-panel" hidden>{}</div>"#,
                pre("sh", "yarn add")
            ),
        )
    );
}

#[test]
fn code_group_selects_the_first_marked_block() {
    let out = html(
        "::: code-group\n```js\na\n```\n\n{selected}\n```sh\nb\n```\n:::\n",
        group(CodeGroupOptions::default()),
    );
    assert!(
        out.contains(r#"id="codegroup-1-tab-2" class="code-group-radio" checked>"#),
        "{out}"
    );
    assert!(!out.contains("selected"), "{out}");
}

#[test]
fn code_group_panel_labels() {
    let out = html(
        "::: code-group\n```js \"Script\"\na\n```\n\n```\nb\n```\n:::\n",
        group(CodeGroupOptions::default()),
    );
    assert!(out.contains(r#"aria-label="Script""#), "{out}");
    assert!(out.contains(r#"aria-label="Code 2""#), "{out}");
}

#[test]
fn code_group_keeps_author_attributes_and_an_authored_name() {
    let out = html(
        &format!("{{#g .wide aria-label=\"Install\"}}\n{GROUP}"),
        group(CodeGroupOptions::default()),
    );
    assert!(
        out.starts_with(
            r#"<div id="g" class="code-group wide" aria-label="Install" role="group">"#
        ),
        "{out}"
    );
}

#[test]
fn code_group_name_from_labels_or_options() {
    let mut opts = Options::default();
    opts.labels.insert("codeGroup".into(), "Beispiele".into());
    let out = host(group(CodeGroupOptions::default()))
        .render(GROUP, &opts)
        .unwrap();
    assert!(out.contains(r#"aria-label="Beispiele""#), "{out}");
    let named = CodeGroupOptions {
        group_label: Some("Shells".into()),
        ..CodeGroupOptions::default()
    };
    assert!(html(GROUP, group(named)).contains(r#"aria-label="Shells""#));
}

#[test]
fn code_group_numbers_groups_and_avoids_authored_ids() {
    let out = html(
        &format!("[x]{{#codegroup-1}}\n\n{GROUP}\n{GROUP}"),
        group(CodeGroupOptions::default()),
    );
    assert!(out.contains(r#"name="codegroup-2""#), "{out}");
    assert!(out.contains(r#"name="codegroup-3""#), "{out}");
}

#[test]
fn code_group_highlighter() {
    let lit = CodeGroupOptions {
        highlighter: Some(Arc::new(|code: &str, lang: Option<&str>| {
            let l = lang.unwrap_or("none");
            format!("<x-{l}>{code}</x-{l}>")
        })),
        ..CodeGroupOptions::default()
    };
    assert!(html(GROUP, group(lit)).contains("<x-js>npm i</x-js>"));
}

#[test]
fn code_group_leaves_other_bodies_a_core_div() {
    for src in [
        "::: code-group\nText.\n\n```js\na\n```\n:::\n",
        "::: code-group\nText.\n:::\n",
    ] {
        assert_eq!(html(src, group(CodeGroupOptions::default())), core(src));
    }
}

#[test]
fn code_group_static_mode() {
    assert_eq!(
        html_in(GROUP, group(CodeGroupOptions::default()), Mode::Static),
        format!(
            "{}{}{}</div>",
            r#"<div class="code-group" role="group" aria-label="Code examples">"#,
            format!(
                r#"<section class="code-group-panel"><h3 class="code-group-label">NPM</h3>{}</section>"#,
                pre("js", "npm i")
            ),
            format!(
                r#"<section class="code-group-panel"><h3 class="code-group-label">sh</h3>{}</section>"#,
                pre("sh", "yarn add")
            ),
        )
    );
}

#[test]
fn code_group_fallback_and_write_back() {
    let only = || group(CodeGroupOptions::default()).without_renderers();
    assert_eq!(
        html(GROUP, only()),
        format!("{}\n{}", pre("js", "npm i"), pre("sh", "yarn add"))
    );
    let carve = host(only());
    let tree = carve.parse(GROUP, &Options::default()).unwrap();
    assert_eq!(carve.write(&tree, &WriteOptions::default()).unwrap(), GROUP);
}

const SAMPLE: &str = "```js\nlet a = 1 <1>\nlet b = 2 <2>\n```\n<1> First\n<2> Second *b*\n";

fn callouts(source: &str) -> String {
    html(source, extensions::code_callouts())
}

#[test]
fn callouts_render_bubbles_and_bind_the_list() {
    assert_eq!(
        callouts(SAMPLE),
        "<pre><code class=\"language-js\">let a = 1 <b class=\"callout\" data-callout=\"1\">1</b>\nlet b = 2 <b class=\"callout\" data-callout=\"2\">2</b>\n</code></pre>\n<ol class=\"callouts\"><li value=\"1\">First</li><li value=\"2\">Second <strong>b</strong></li></ol>"
    );
}

#[test]
fn callouts_keep_the_number_and_escape_the_code() {
    assert_eq!(
        callouts("```\nif a < b && c <7>\n```\n<7> Seven\n"),
        "<pre><code>if a &lt; b &amp;&amp; c <b class=\"callout\" data-callout=\"7\">7</b>\n</code></pre>\n<ol class=\"callouts\"><li value=\"7\">Seven</li></ol>"
    );
}

#[test]
fn callouts_only_a_trailing_marker_counts() {
    assert_eq!(
        callouts("```\na <1> b\n```\n"),
        "<pre><code>a &lt;1&gt; b\n</code></pre>"
    );
}

#[test]
fn callouts_bind_no_list_without_a_marker_or_a_non_item_line() {
    let src = "```\na\n```\n<1> First\n";
    assert_eq!(callouts(src), core(src));
    assert!(
        callouts("```\na <1>\n```\n<1> First\nplain\n").contains("<p>&lt;1&gt; First\nplain</p>")
    );
}

#[test]
fn callouts_carry_authored_attributes() {
    assert_eq!(
        callouts("{#c}\n```\na <1>\n```\n\n{#l .x}\n<1> One\n"),
        "<pre id=\"c\"><code>a <b class=\"callout\" data-callout=\"1\">1</b>\n</code></pre>\n<ol id=\"l\" class=\"callouts x\"><li value=\"1\">One</li></ol>"
    );
}

#[test]
fn callouts_work_inside_containers() {
    assert!(
        callouts("> ```\n> a <1>\n> ```\n> <1> One\n")
            .contains(r#"<ol class="callouts"><li value="1">One</li></ol>"#)
    );
    assert!(
        callouts("- ```\n  a <1>\n  ```\n")
            .contains(r#"<b class="callout" data-callout="1">1</b>"#)
    );
}

#[test]
fn callouts_static_mode_equals_interactive() {
    assert_eq!(
        html_in(SAMPLE, extensions::code_callouts(), Mode::Static),
        callouts(SAMPLE)
    );
}

#[test]
fn callouts_are_inert_unregistered_and_without_renderers() {
    assert!(core(SAMPLE).contains("<p>&lt;1&gt; First"));
    assert_eq!(
        html(SAMPLE, extensions::code_callouts().without_renderers()),
        core(SAMPLE)
    );
}

#[test]
fn the_group_composes() {
    let all = Bundle::merge([
        extensions::presets(PresetBuilders::default()),
        math(),
        group(CodeGroupOptions::default()),
        extensions::code_callouts(),
    ]);
    let src = format!("```mermaid\nA <1>\n```\n\n{GROUP}\n```math\nx\n```\n");
    let out = html(&src, all);
    assert!(
        out.contains(r#"<pre class="mermaid" role="img" aria-label="mermaid">A &lt;1></pre>"#),
        "{out}"
    );
    assert!(out.contains(r#"<div class="code-group""#), "{out}");
    assert!(
        out.contains(r#"<div class="math display">\[x\]</div>"#),
        "{out}"
    );
}

#[test]
fn a_code_group_keeps_a_panel_with_callouts_and_its_selection() {
    let all = Bundle::merge([
        group(CodeGroupOptions::default()),
        extensions::code_callouts(),
    ]);
    let out = html(
        "::: code-group\n```js\na\n```\n\n{selected}\n```sh\nb <1>\n```\n:::\n",
        all,
    );
    assert!(
        out.contains(r#"id="codegroup-1-tab-2" class="code-group-radio" checked>"#),
        "{out}"
    );
    assert!(out.contains("<div class=\"code-group-panel\" role=\"group\" aria-label=\"sh\"><pre><code class=\"language-sh\">b <b class=\"callout\" data-callout=\"1\">1</b>\n</code></pre></div>"), "{out}");
    assert!(!out.contains("selected"), "{out}");
}

#[test]
fn the_json_boundary_selects_them_by_name() {
    let out = api::json::call(
        r#"{"call":"render","source":"```mermaid\na\n```\n\n```math\nx\n```","options":{"extensions":["carve.fencedRender.mermaid","carve.mathBlock"],"renderers":{"carve.fencedRender.mermaid":"standard","carve.mathBlock":"standard"}}}"#,
    );
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v["ok"]["html"],
        "<pre class=\"mermaid\" role=\"img\" aria-label=\"mermaid\">a</pre>\n<div class=\"math display\">\\[x\\]</div>"
    );
    let fallback = api::json::call(
        r#"{"call":"render","source":"```dot\na\n```","options":{"extensions":["carve.fencedRender.dot"]}}"#,
    );
    let v: Value = serde_json::from_str(&fallback).unwrap();
    assert_eq!(
        v["ok"]["html"],
        "<pre><code class=\"language-dot\">a\n</code></pre>"
    );
}
