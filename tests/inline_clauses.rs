//! Unit tests of the inline pass, one or more per clause. Each parses one
//! paragraph and compares a compact rendering of the tree.

use carve::ast::owned::Node;
use carve::inline::{ContentKind, InlineEnv, Join, Piece, parse};
use carve::source::Source;
use serde_json::Value;
use std::collections::HashSet;
use std::fmt::Write;

/// Options of one parse.
#[derive(Default)]
struct Opts<'a> {
    kind: Option<ContentKind>,
    keys: &'a [&'a str],
    abbrs: &'a [(&'a str, &'a str)],
    hard: bool,
    cell_join: bool,
}

fn run(input: &str, o: &Opts<'_>) -> Vec<Node> {
    let src = Source::new(input);
    let mut pieces = Vec::new();
    let mut at = 0;
    for (n, line) in input.split('\n').enumerate() {
        let lead = if o.kind == Some(ContentKind::LineBlockLine) {
            0
        } else {
            line.len() - line.trim_start().len()
        };
        let join = if n > 0 && o.cell_join {
            Join::Cell
        } else {
            Join::Line
        };
        pieces.push(Piece {
            start: at + lead,
            end: at + line.len(),
            join,
        });
        at += line.len() + 1;
    }
    let keys: HashSet<String> = o.keys.iter().map(|k| (*k).to_string()).collect();
    let mut abbrs: Vec<(String, String)> = o
        .abbrs
        .iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect();
    abbrs.sort_by(|a, b| a.0.cmp(&b.0));
    let arena = carve::Arena::new();
    let env = InlineEnv {
        src: &src,
        arena: &arena,
        link_keys: &keys,
        abbreviations: &abbrs,
        kind: o.kind.unwrap_or(ContentKind::Paragraph),
        hard_breaks: o.hard,
    };
    parse(&env, &pieces, &mut carve::inline::Scratch::default())
        .iter()
        .map(carve::ast::Node::owned)
        .collect()
}

/// Compact form: `type(field=value ...)[children]`, text as its quoted value.
fn show(nodes: &[Node]) -> String {
    let v = serde_json::to_value(nodes).unwrap_or_default();
    let mut out = String::new();
    walk_list(&v, &mut out);
    out
}

fn walk_list(v: &Value, out: &mut String) {
    for (i, n) in v.as_array().into_iter().flatten().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        walk(n, out);
    }
}

fn walk(n: &Value, out: &mut String) {
    let Some(m) = n.as_object() else { return };
    let ty = m.get("type").and_then(Value::as_str).unwrap_or("?");
    if ty == "text" {
        let _ = write!(
            out,
            "{:?}",
            m.get("value").and_then(Value::as_str).unwrap_or("")
        );
        return;
    }
    out.push_str(ty);
    let mut fields: Vec<String> = Vec::new();
    for (k, v) in m {
        if matches!(k.as_str(), "type" | "children" | "pos") {
            continue;
        }
        let shown = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        fields.push(format!("{k}={shown}"));
    }
    if !fields.is_empty() {
        let _ = write!(out, "({})", fields.join(" "));
    }
    if let Some(c) = m.get("children") {
        out.push('[');
        walk_list(c, out);
        out.push(']');
    }
}

fn p(input: &str) -> String {
    show(&run(input, &Opts::default()))
}

fn po(input: &str, o: &Opts<'_>) -> String {
    show(&run(input, o))
}

fn offsets(nodes: &[Node]) -> Vec<(String, usize, usize)> {
    nodes
        .iter()
        .map(|n| {
            let pos = n
                .pos()
                .map_or((usize::MAX, usize::MAX), |p| (p.start(), p.end()));
            (n.type_name().to_string(), pos.0, pos.1)
        })
        .collect()
}

#[test]
fn c2_i_002_trailing_spaces_dropped_escaped_space_kept() {
    assert_eq!(p("a  \nb"), "\"a\" soft_break \"b\"");
    assert_eq!(p("a\\ "), "\"a\" hard_break");
}

#[test]
fn c2_i_003_c2_i_103_lines_join_with_soft_break() {
    let nodes = run("line one\n  line two", &Opts::default());
    assert_eq!(show(&nodes), "\"line one\" soft_break \"line two\"");
    assert_eq!(
        offsets(&nodes).get(1),
        Some(&("soft_break".to_string(), 8, 11))
    );
}

#[test]
fn c2_i_005_atom_that_starts_first_wins() {
    assert_eq!(p("*a `b* c"), "strong[\"a `b\"] \" c\"");
    assert_eq!(p("`a %% b` x"), "code(value=a %% b) \" x\"");
}

#[test]
fn c2_i_007_bracket_construct_settles_before_delimiters() {
    assert_eq!(p("*a [b* c](/u)"), "\"*a \" link(href=/u)[\"b* c\"]");
    assert_eq!(p("*a [b* c] d"), "strong[\"a [b\"] \" c] d\"");
}

#[test]
fn c2_i_010_to_c2_i_012_backslash_escapes() {
    assert_eq!(p("\\*a\\* \\a C:\\dir"), "\"*a* \\\\a C:\\\\dir\"");
    assert_eq!(p("a\\ b"), "\"a\" non_breaking_space \"b\"");
}

#[test]
fn c2_i_013_no_escapes_in_code_and_destination_reads_parens() {
    assert_eq!(
        p("`a\\*b` [x](/a\\(b)"),
        "code(value=a\\*b) \" \" link(href=/a(b)[\"x\"]"
    );
}

#[test]
fn c2_i_014_to_c2_i_016_code_spans() {
    assert_eq!(
        p("`` a ` b `` ` x `"),
        "code(value= a ` b ) \" \" code(value= x )"
    );
    assert_eq!(p("`` `x` ``"), "code(value=`x`)");
    assert_eq!(p("`  `"), "code(value=  )");
}

#[test]
fn c2_i_017_unclosed_code_run_is_text() {
    assert_eq!(p("see `unclosed *x*  "), "\"see `unclosed \" strong[\"x\"]");
}

#[test]
fn c2_i_018_literal_span() {
    assert_eq!(
        p("!`a<b>`{.ipa} \\!`x`"),
        "literal_inline(attrs={\"classes\":[\"ipa\"]} value=a<b>) \" !\" code(value=x)"
    );
}

#[test]
fn c2_i_019_c2_i_020_math() {
    assert_eq!(
        p("$`x` $$`y` $5"),
        "math(display=false value=x) \" \" math(display=true value=y) \" $5\""
    );
    assert_eq!(p("$`a + b"), "\"$`a + b\"");
}

#[test]
fn c2_i_021_raw_inline_only_after_plain_code() {
    assert_eq!(
        p("`<br>`{=html} $`x`{=html}"),
        "raw_inline(format=html value=<br>) \" \" math(display=false value=x) \"{=html}\""
    );
}

#[test]
fn c2_i_023_line_comment_after_whitespace_only() {
    let nodes = run("a %% comment\nb and 50%% and a%%b", &Opts::default());
    assert_eq!(
        show(&nodes),
        "\"a\" comment(form=line value=comment) soft_break \"b and 50%% and a%%b\""
    );
    assert_eq!(
        offsets(&nodes).get(1),
        Some(&("comment".to_string(), 1, 12))
    );
}

#[test]
fn c2_i_024_line_comment_takes_the_closers_of_its_line() {
    assert_eq!(
        p("*a %% b*\nc* d"),
        "strong[\"a\" comment(form=line value=b*) soft_break \"c\"] \" d\""
    );
    assert_eq!(
        p("[a %% b](/u) c"),
        "\"[a\" comment(form=line value=b](/u) c)"
    );
}

#[test]
fn c2_i_025_c2_i_026_braced_comment_is_transparent() {
    assert_eq!(
        p("*bo{% c %}ld*"),
        "strong[\"bo\" comment(form=braced value=c) \"ld\"]"
    );
    assert_eq!(p("a {% oops"), "\"a {% oops\"");
    assert_eq!(
        p("a {% one\ntwo %} b"),
        "\"a \" comment(form=braced value=one\ntwo) \" b\""
    );
}

#[test]
fn c2_i_028_to_c2_i_034_attribute_block_parts() {
    assert_eq!(
        p("[s]{#a .b k=v} [s]{.a.b}"),
        "span(attrs={\"classes\":[\"b\"],\"id\":\"a\",\"keyValues\":[[\"k\",\"v\"]]})[\"s\"] \" [s]{.a.b}\""
    );
}

#[test]
fn c2_i_029_language_part_goes_to_lang() {
    assert_eq!(
        p("[s]{:en} `c`{:de}"),
        "span(lang=en)[\"s\"] \" \" code(lang=de value=c)"
    );
}

#[test]
fn c2_i_035_unattached_block_is_printed_as_written() {
    assert_eq!(
        p("word{k=\"v\"} and \"v\""),
        concat!(
            "\"word{k=\\\"v\\\"} and \" smart_punctuation(glyph=“ kind=left_double_quote source=\") \"v\" ",
            "smart_punctuation(glyph=” kind=right_double_quote source=\")",
        )
    );
}

#[test]
fn c2_i_036_c2_i_037_hosts_and_non_hosts() {
    assert_eq!(
        p("*b*{.c} @user{.c}"),
        "strong(attrs={\"classes\":[\"c\"]})[\"b\"] \" \" mention(kind=user target=user) \"{.c}\""
    );
}

#[test]
fn c2_i_039_glued_blocks_merge() {
    assert_eq!(
        p("[x]{#i}{.a}{.b}"),
        "span(attrs={\"classes\":[\"a\",\"b\"],\"id\":\"i\"})[\"x\"]"
    );
}

#[test]
fn c2_i_040_empty_block_only_after_a_span_pair() {
    assert_eq!(
        p("[s]{}{.b} *a*{} a {} b"),
        "span(attrs={\"classes\":[\"b\"]})[\"s\"] \" \" strong[\"a\"] \"{} a {} b\""
    );
}

#[test]
fn c2_i_043_brace_order() {
    assert_eq!(
        p("[s]{#id#} {_k=v_} {_a_}"),
        "\"[s]\" critic_comment(value=id) \" {_k=v_} \" underline[\"a\"]"
    );
}

#[test]
fn c2_i_047_c2_i_048_flanking() {
    assert_eq!(p("* a* and *a *"), "\"* a* and *a *\"");
    assert_eq!(p("a*b*c"), "\"a*b*c\"");
}

#[test]
fn c2_i_049_double_delimiter_is_text() {
    assert_eq!(p("**bold** //x//"), "\"**bold** //x//\"");
}

#[test]
fn c2_i_050_joining_mark_blocks_a_delimiter() {
    let s = p("*\u{301}a* and *a*");
    assert_eq!(s.matches("strong").count(), 1, "{s}");
    assert!(s.starts_with("\"*"), "{s}");
}

#[test]
fn c2_i_051_closer_pairs_nearest_and_removes_above() {
    assert_eq!(p("/a /b/ c/"), "emphasis[\"a \" emphasis[\"b\"] \" c\"]");
    assert_eq!(p("*a /b* c/"), "strong[\"a /b\"] \" c/\"");
}

#[test]
fn c2_i_053_own_mark_between_word_characters() {
    assert_eq!(
        p("the /etc/ dir and /usr/local/"),
        "\"the \" emphasis[\"etc\"] \" dir and /usr/local/\""
    );
}

#[test]
fn c2_i_057_mark_before_brace_without_braced_opener() {
    assert_eq!(
        p("/a/} and *a*}"),
        "emphasis[\"a\"] \"} and \" strong[\"a\"] \"}\""
    );
    assert_eq!(p("**}"), "\"**}\"");
}

#[test]
fn c2_i_059_first_pair_closed_wins() {
    assert_eq!(p("{+a {*b+} c*}"), "insert[\"a {*b\"] \" c*}\"");
    assert_eq!(
        p("*bold {/note*/} more*"),
        "strong[\"bold {/note\"] \"/} more*\""
    );
}

#[test]
fn c2_i_060_failed_braced_opener_is_text() {
    assert_eq!(p("{*unclosed and *x*"), "\"{*unclosed and \" strong[\"x\"]");
}

#[test]
fn c2_i_061_glued_braced_pair_is_text() {
    assert_eq!(p("{**} and {==}"), "\"{**} and {==}\"");
}

#[test]
fn c2_i_062_replacement_is_two_nodes() {
    assert_eq!(p("{-old-}{+new+}"), "delete[\"old\"] insert[\"new\"]");
}

#[test]
fn c2_i_063_editorial_comment() {
    assert_eq!(
        p("{# note *x* #} {##}"),
        "critic_comment(value= note *x* ) \" {##}\""
    );
}

#[test]
fn c2_i_066_to_c2_i_069_link_tails() {
    assert_eq!(
        p("[a](/u \"T\") [a]()"),
        "link(href=/u tooltip=T)[\"a\"] \" \" link(href=)[\"a\"]"
    );
    assert_eq!(p("[a](/u v)"), "\"[a](/u v)\"");
    assert_eq!(p("[a](/u(1)"), "\"[a](/u(1)\"");
}

#[test]
fn c2_i_071_links_nest_in_the_tree() {
    assert_eq!(p("[[x](/y)](/z)"), "link(href=/z)[link(href=/y)[\"x\"]]");
}

#[test]
fn c2_i_072_reference_forms() {
    let o = Opts {
        keys: &["ref"],
        ..Default::default()
    };
    assert_eq!(
        po("[t][ref] [ref][] [ref] [no]", &o),
        concat!(
            "link(key=ref namespace=link refForm=full)[\"t\"] \" \" link(key=ref namespace=link ",
            "refForm=collapsed)[\"ref\"] \" \" link(key=ref namespace=link refForm=shortcut)[\"ref\"] \" ",
            "[no]\"",
        )
    );
    assert_eq!(p("[ ][]"), "\"[ ][]\"");
}

#[test]
fn c2_i_073_autolinks() {
    assert_eq!(
        p("<a@b.com> <a@b> <h:x>"),
        "autolink(href=mailto:a@b.com text=a@b.com) \" <a@b> \" autolink(href=h:x)"
    );
}

#[test]
fn c2_i_074_image_alt_is_source() {
    assert_eq!(
        p("![*alt*](/i.png){.c} ![x]"),
        "image(alt=*alt* attrs={\"classes\":[\"c\"]} src=/i.png) \" ![x]\""
    );
}

#[test]
fn c2_i_076_label_is_taken_whole() {
    assert_eq!(
        p("[x][a`b]"),
        "link(key=a`b namespace=link refForm=full)[\"x\"]"
    );
}

#[test]
fn c2_i_078_caret_label_is_a_footnote() {
    assert_eq!(
        p("[x][^n]"),
        "\"[x]\" footnote_ref(key=n namespace=footnote)"
    );
}

#[test]
fn c2_i_079_c2_i_080_key_folds_and_wraps() {
    assert_eq!(
        p("[a][no\npe]"),
        "link(key=no pe namespace=link refForm=full)[\"a\"]"
    );
}

#[test]
fn c2_i_087_footnote_reference_takes_no_tail() {
    assert_eq!(
        p("[^n](/u)"),
        "footnote_ref(key=n namespace=footnote) \"(/u)\""
    );
}

#[test]
fn c2_i_089_inline_footnote() {
    assert_eq!(
        p("c^[a *b*] d^[x"),
        "\"c\" inline_footnote[\"a \" strong[\"b\"]] \" d^[x\""
    );
}

#[test]
fn c2_i_090_to_c2_i_093_sigils() {
    assert_eq!(
        p("é@user _@user x@u"),
        "\"é@user _\" mention(kind=user target=user) \" x@u\""
    );
    assert_eq!(p("#release-1.0. C#"), "tag(name=release-1.0) \". C#\"");
    assert_eq!(p(":+1: :-: 10:30:00"), "symbol(name=+1) \" :-: 10:30:00\"");
}

#[test]
fn c2_i_094_inline_extension() {
    assert_eq!(
        p(":note[a] :note[b"),
        "inline_extension(name=note)[\"a\"] \" :note[b\""
    );
}

#[test]
fn c2_i_096_cross_reference() {
    assert_eq!(
        p("</#intro>{.c}"),
        "xref(key=intro namespace=heading) \"{.c}\""
    );
}

#[test]
fn c2_i_098_to_c2_i_100_abbreviations() {
    let o = Opts {
        abbrs: &[("HTML", "H")],
        ..Default::default()
    };
    assert_eq!(
        po("HTML HTMLs *HTML* `HTML`", &o),
        concat!(
            "abbreviation(expansion=H key=HTML namespace=abbr) \" HTMLs \" strong[abbreviation(expansion=H ",
            "key=HTML namespace=abbr)] \" \" code(value=HTML)",
        )
    );
    assert_eq!(po("{*HTML*}s", &o), "strong[\"HTML\"] \"s\"");
}

#[test]
fn c2_i_101_citation_pair_is_text() {
    let o = Opts {
        keys: &["@doe"],
        ..Default::default()
    };
    assert_eq!(
        po("[@doe]", &o),
        "\"[\" mention(kind=user target=doe) \"]\""
    );
}

#[test]
fn c2_i_102_backslash_at_line_end_is_hard_break() {
    let nodes = run("a\\\nb", &Opts::default());
    assert_eq!(show(&nodes), "\"a\" hard_break \"b\"");
    assert_eq!(
        offsets(&nodes).get(1),
        Some(&("hard_break".to_string(), 1, 3))
    );
    let end = run("a\\", &Opts::default());
    assert_eq!(
        offsets(&end).get(1),
        Some(&("hard_break".to_string(), 1, 2))
    );
}

#[test]
fn c2_i_104_quotes() {
    assert_eq!(
        p("\"a\" it's '90s"),
        concat!(
            "smart_punctuation(glyph=“ kind=left_double_quote source=\") \"a\" smart_punctuation(glyph=” ",
            "kind=right_double_quote source=\") \" it\" smart_punctuation(glyph=’ kind=right_single_quote ",
            "source=') \"s \" smart_punctuation(glyph=’ kind=right_single_quote source=') \"90s\"",
        )
    );
}

#[test]
fn c2_i_104_quotes_follow_local_neighbors() {
    let s = p("'tis 'em' 'n'");
    assert_eq!(s.matches("left_single_quote").count(), 3, "{s}");
}

#[test]
fn c2_i_104_single_quotes_do_not_pair() {
    let s = p("'I said 'yes' twice'");
    assert_eq!(s.matches("left_single_quote").count(), 2, "{s}");
}

#[test]
fn c2_i_111_dash_runs() {
    let s = p("1--10 a---b a----b");
    assert_eq!(s.matches("en_dash").count(), 3, "{s}");
    assert_eq!(s.matches("em_dash").count(), 1, "{s}");
}

#[test]
fn c2_i_115_c2_i_116_arrows_need_exact_runs() {
    assert_eq!(p("a--->b").matches("em_dash").count(), 1);
    assert!(p("a --> b").contains("rightwards_arrow"));
    assert!(p("<=>").contains("left_right_double_arrow"));
    assert!(!p("x -> y").contains("smart"));
}

#[test]
fn c2_i_117_plus_minus_needs_its_exact_run() {
    assert!(p("5 +- 1").contains("plus_minus"));
    assert!(!p("C++-style").contains("plus_minus"));
}

#[test]
fn c2_i_118_depth_cap_makes_the_opener_text() {
    let n = 201;
    let input = format!("{}x{}", "{^".repeat(n), "^}".repeat(n));
    let s = p(&input);
    assert_eq!(s.matches("superscript").count(), 200);
    assert!(s.contains("\"{^x\""), "{s}");
    assert!(
        s.ends_with("] \"^}\""),
        "{}",
        &s[s.len().saturating_sub(40)..]
    );
}

#[test]
fn c2_i_118_capped_bracket_closer_is_text() {
    let input = format!("{}[x](u)", "[".repeat(200));
    assert!(!p(&input).contains("link["));
}

#[test]
fn c2_i_119_failed_openers_are_text() {
    assert_eq!(
        p("[open and (x and {*y and {% z and ^[w and :ext[v and <http://u"),
        "\"[open and (x and {*y and {% z and ^[w and :ext[v and <http://u\""
    );
}

#[test]
fn c2_b_184_cell_join_is_space_in_text_line_feed_in_code() {
    let o = Opts {
        kind: Some(ContentKind::Cell),
        cell_join: true,
        ..Default::default()
    };
    assert_eq!(po("a\nb", &o), "\"a b\"");
    assert_eq!(po("`a\nb`", &o), "code(value=a\nb)");
    assert!(run("a\nb", &o).iter().all(|n| n.pos().is_none()));
}

#[test]
fn c2_b_223_unclosed_code_in_a_cell_is_text() {
    let o = Opts {
        kind: Some(ContentKind::Cell),
        ..Default::default()
    };
    assert_eq!(po("`abc", &o), "\"`abc\"");
}

#[test]
fn c2_i_102_cell_final_backslash_is_hard_break() {
    let o = Opts {
        kind: Some(ContentKind::Cell),
        ..Default::default()
    };
    assert_eq!(po(" a\\  ", &o), "\"a\" hard_break");
}

#[test]
fn c2_b_224_escaped_pipe_in_cell_destination() {
    let o = Opts {
        kind: Some(ContentKind::Cell),
        ..Default::default()
    };
    assert_eq!(
        po("[x](u\\|v) `a\\|b`", &o),
        "link(href=u|v)[\"x\"] \" \" code(value=a\\|b)"
    );
}

#[test]
fn c2_b_135_line_block_line_keeps_spaces_and_reads_comment_as_text() {
    let o = Opts {
        kind: Some(ContentKind::LineBlockLine),
        ..Default::default()
    };
    assert_eq!(po("  a %% b", &o), "\"  a %% b\"");
}

#[test]
fn c2_b_207_caption_mark() {
    let o = Opts {
        kind: Some(ContentKind::Caption),
        ..Default::default()
    };
    assert_eq!(
        po("Figure #: the *a #* #1 #", &o),
        "\"Figure \" caption_number \": the \" strong[\"a #\"] \" #1 #\""
    );
    assert_eq!(po("#tag #", &o), "tag(name=tag) \" \" caption_number");
}

#[test]
fn c2_a_068_hard_break_block() {
    let o = Opts {
        hard: true,
        ..Default::default()
    };
    assert_eq!(po("a\nb", &o), "\"a\" hard_break \"b\"");
}

#[test]
fn c2_a_040_adjacent_text_merges_and_non_adjacent_omits_pos() {
    let nodes = run("a\\*b", &Opts::default());
    assert_eq!(offsets(&nodes), vec![("text".to_string(), 0, 4)]);
}

#[test]
fn c2_b_154_heading_continuation_breaks() {
    let o = Opts {
        kind: Some(ContentKind::Heading),
        ..Default::default()
    };
    assert_eq!(
        po("A\\\nB\nC", &o),
        "\"A\" hard_break \"B\" soft_break \"C\""
    );
}

#[test]
fn pathological_inputs_stay_linear() {
    let big: usize = std::env::var("BIG")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000);
    let pats: Vec<String> = std::env::var("PATS").map_or_else(
        |_| {
            [
                "[", "[^a ", "*a ", "{%", "{#", "`", "<a", "{k=\"", "](", "][", "/a/b", ":a[",
                "^[", "{*", "*}",
            ]
            .iter()
            .map(|s| (*s).to_string())
            .collect()
        },
        |v| v.split(',').map(String::from).collect(),
    );
    for pat in &pats {
        let input = pat.repeat(big);
        let t = std::time::Instant::now();
        let _ = run(&input, &Opts::default());
        eprintln!("{pat}: {:?}", t.elapsed());
        assert!(t.elapsed().as_secs() < 2, "{pat}: {:?}", t.elapsed());
    }
}
