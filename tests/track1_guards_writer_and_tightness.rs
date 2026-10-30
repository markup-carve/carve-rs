use carve::{to_carve, to_html};

fn html(source: &str, expected: &str) {
    assert_eq!(to_html(source).trim(), expected, "{source:?}");
}

fn round_trips(source: &str) -> String {
    let written = to_carve(source);
    assert_eq!(
        to_html(&written),
        to_html(source),
        "{source:?} -> {written:?}"
    );
    assert_eq!(to_carve(&written), written, "{source:?}");
    written
}

// carve#4021: a second `_` is not pushed while the first is open (E3), so it guards.
#[test]
fn a_second_guard_of_a_held_kind_leaves_its_childs_block_as_text() {
    html(
        "*_a _~b~{t=*} z*\n",
        "<p><strong>_a _~b~{t=</strong>} z*</p>",
    );
    html(
        "*_~a~{title=\"*_\"} _~a~{title=\"*_\"} z*\n",
        "<p><strong>_<s title=\"*_\">a</s> _~a~{title=“</strong>_”} z*</p>",
    );
}

// carve#4022
#[test]
fn a_third_guarded_pair_opens_behind_the_closed_outer_span() {
    html(
        "*_~a~{title=\"*_\"} _~a~{title=\"*_\"} _~a~{title=\"*_\"} z*\n",
        "<p><strong>_<s title=\"*_\">a</s> _~a~{title=“</strong><u>“} _~a~{title=“*</u>”} z*</p>",
    );
}

#[test]
fn a_closed_earlier_pair_holds_no_kind() {
    html(
        "*_a_ _~b~{t=*} z*\n",
        "<p><strong><u>a</u> _<s t=\"*\">b</s> z</strong></p>",
    );
}

// carve#4050
#[test]
fn a_combined_opener_behind_an_open_guard_opens() {
    html("_/*~*/\n", "<p>_<strong><em>~</em></strong></p>");
}

// carve#4042 option C: the comment is invisible to pairing.
#[test]
fn a_comment_in_a_combined_shape_ends_at_its_closer_when_the_token_splits() {
    html(
        "/x /*a %% b*/ y\n",
        "<p><em>x /<strong>a</strong></em> y</p>",
    );
    html(
        "*x /*`a` %% b*/ y\n",
        "<p><strong>x /</strong><code>a</code>*/ y</p>",
    );
    html(
        "/x *y /*a %% b*/ y\n",
        "<p><em>x <strong>y /*a</strong></em> y</p>",
    );
    html("/*a %% b*/ y\n", "<p><strong><em>a</em></strong> y</p>");
}

// carve#4109: the attribute run renders nothing, so the blank above it still loosens.
#[test]
fn an_attribute_line_before_a_paragraph_keeps_the_blank_that_loosens_the_item() {
    let loose = "<ul>\n  <li><p>a</p>\n    <ul>\n      <li>b</li>\n    </ul>\n    <p x=\"\">p</p>\n  </li>\n</ul>";
    html("- a\n  - b\n\n  {x}\n  p\n", loose);
    html("- a\n  - b\n\n  {x}\n    p\n", loose);
}

#[test]
fn an_attribute_line_without_a_blank_keeps_the_item_tight() {
    html(
        "- a\n  - b\n  {x}\n  p\n",
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n    <p x=\"\">p</p>\n  </li>\n</ul>",
    );
}

// carve#4043
#[test]
fn a_description_note_leaves_the_next_entry_to_the_list() {
    html(
        ":: m\n: [^c]: a\n: b\n",
        "<dl>\n  <dt>m</dt>\n  <dd></dd>\n  <dd>b</dd>\n</dl>",
    );
    html(
        ":: m\n: [^c]: a\n:: t\n: b\n",
        "<dl>\n  <dt>m</dt>\n  <dd></dd>\n  <dt>t</dt>\n  <dd>b</dd>\n</dl>",
    );
}

// carve-rs#3569
#[test]
fn a_fence_in_a_div_in_a_description_opens_a_code_block() {
    html(
        ":: m\n: n\n  :::\n  ```\n",
        "<dl>\n  <dt>m</dt>\n  <dd>\n    <p>n</p>\n    <div>\n      <pre><code></code></pre>\n    </div>\n  </dd>\n</dl>",
    );
}

// carve#4034
#[test]
fn a_task_items_marker_line_sub_list_writes_past_the_task_box() {
    assert_eq!(round_trips("* [ ] - :::\n"), "* [ ] - :::\n\n        :::\n");
    for source in [
        "* [x] - :::\n  a\n",
        "- [ ] 1. :::\n",
        "- [ ] - - :::\n",
        "> * [ ] - :::\n",
        "* [ ] - > a\n\n        > b\n",
        "* [ ] - a\n\n        b\n",
        "* [ ] - a\n        - b\n",
        "- - [ ] - :::\n",
    ] {
        round_trips(source);
    }
}

#[test]
fn a_block_of_the_task_item_itself_stays_at_its_column() {
    assert_eq!(round_trips("* [ ] - a\n\n  b\n"), "* [ ] - a\n\n  b\n");
    assert_eq!(round_trips("- - :::\n"), "- - :::\n\n    :::\n");
}

// carve#4023
#[test]
fn a_region_marker_stays_adjacent_to_the_list_above_it() {
    for source in [
        "%% #region requirements\n- PHP 8.3 or later\n- Composer 2.7\n%% #endregion requirements\n",
        "- x\n  - y\n%% #endregion r\n",
        "1. a\n%% #endregion r\n",
    ] {
        assert_eq!(to_carve(source), source);
    }
    assert_eq!(
        to_carve("- a\n\n%% #endregion r\n"),
        "- a\n\n%% #endregion r\n"
    );
}
