//! A bare span holding its own delimiter between two word characters does not
//! form, and the entry stays on the stack through the closer that declines it
//! (markup-carve/carve#4810, superseding ruling; PART 9 R2). Expected HTML is the
//! spec model's.

use carve::to_html;

#[test]
fn a_bare_span_holding_its_own_delimiter_does_not_form() {
    for (source, expected) in [
        ("*x /*y*b*/\n", "<p>*x <em>*y*b*</em></p>"),
        ("*x*z /*y*/\n", "<p>*x*z <em>*y*</em></p>"),
        ("/x/z /*y*/\n", "<p>/x/z /<strong>y</strong>/</p>"),
        ("*x*z *y*\n", "<p>*x*z *y*</p>"),
        ("*a*b* *c*\n", "<p>*a*b* <strong>c</strong></p>"),
        ("/a/b *c/ d*\n", "<p>/a/b <strong>c/ d</strong></p>"),
        ("*x*y /*a*/ z*\n\n*a*\n", "<p>*x*y <em>*a*</em> z*</p>\n<p><strong>a</strong></p>"),
        ("/*y*b*/\n", "<p><strong><em>y*b</em></strong></p>"),
        ("*x a*b *c.*(e)* /k/\n", "<p>*x a*b *c.*(e)* <em>k</em></p>"),
        ("(/x a/b /c./(e)/\n", "<p>(/x a/b /c./(e)/</p>"),
        ("_x y a_b _c._(e)__\n", "<p>_x y a_b _c._(e)__</p>"),
        ("- .*xb*x/*}*/x~\n", "<ul>\n  <li>.*xb*x/*}*/x~</li>\n</ul>"),
        ("*x a*b z* *c*\n", "<p>*x a*b z* <strong>c</strong></p>"),
        ("/my/var/\n", "<p>/my/var/</p>"),
        ("_my_var_\n", "<p>_my_var_</p>"),
        ("*my*var*\n", "<p>*my*var*</p>"),
        ("~my~var~\n", "<p>~my~var~</p>"),
        ("=my=var=\n", "<p>=my=var=</p>"),
        ("{/my/var/}\n", "<p><em>my/var</em></p>"),
        ("/etc/\n", "<p><em>etc</em></p>"),
        ("_my_var_ and *a*b* and /usr/local/ and /api/v1/ x\n\n/etc/ and _a_\n", "<p>_my_var_ and *a*b* and /usr/local/ and /api/v1/ x</p>\n<p><em>etc</em> and <u>a</u></p>"),
        ("see /usr/local/ and the /api/v1/ route\n", "<p>see /usr/local/ and the /api/v1/ route</p>"),
        ("_@a_b_\n", "<p><u><span class=\"mention\"><strong>@a_b</strong></span></u></p>"),
        ("/x `a/b` y/\n", "<p><em>x <code>a/b</code> y</em></p>"),
        ("/a\\/b/\n", "<p><em>a/b</em></p>"),
        ("/*a/b*/\n", "<p><strong><em>a/b</em></strong></p>"),
        ("*@a*b*\n", "<p>*<span class=\"mention\"><strong>@a</strong></span>*b*</p>"),
        ("/#a/b/\n", "<p>/<span class=\"tag\"><strong>#a</strong></span>/b/</p>"),
        ("_#a_b_ x\n", "<p><u><span class=\"tag\"><strong>#a_b</strong></span></u> x</p>"),
        ("_x @a_b y_\n", "<p><u>x <span class=\"mention\"><strong>@a_b</strong></span> y</u></p>"),
        ("*x #a*b y*\n", "<p>*x <span class=\"tag\"><strong>#a</strong></span>*b y*</p>"),
        ("_[a_b](u)_\n", "<p><u><a href=\"u\">a_b</a></u></p>"),
        ("_<https://a_b.c>_\n", "<p><u><a href=\"https://a_b.c\">https://a_b.c</a></u></p>"),
        ("/a{=b/c=}d/\n", "<p><em>a<mark>b/c</mark>d</em></p>"),
        ("_a {_b_c_} d_\n", "<p><u>a <u>b_c</u> d</u></p>"),
        ("/a [x/y] b/\n", "<p><em>a [x/y] b</em></p>"),
        ("=a=b=\n", "<p>=a=b=</p>"),
        ("~a~b~ c\n", "<p>~a~b~ c</p>"),
        ("_a_é_\n", "<p>_a_é_</p>"),
        ("_1_2_\n", "<p>_1_2_</p>"),
    ] {
        assert_eq!(to_html(source).trim(), expected, "{source:?}");
    }
}
