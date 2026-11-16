mod common;

use serde_json::Value;
use std::sync::OnceLock;

fn cases() -> &'static Vec<Value> {
    static CASES: OnceLock<Vec<Value>> = OnceLock::new();
    CASES.get_or_init(|| {
        include_str!("data/clause-cases.jsonl")
            .lines()
            .map(common::parse_json)
            .collect()
    })
}

fn check(id: &str, clause: &str) {
    let case = cases().iter().find(|c| c["caseId"] == id).unwrap();
    assert_eq!(case["clause"], clause);
    let input = case["input"].as_str().unwrap();
    let opts = common::options_of(&case["opts"]);
    assert_eq!(
        carve2::render(input, &opts),
        case["expectedHtml"],
        "HTML: {id}"
    );
    let arena = carve2::Arena::new();
    let doc = carve2::parse_resolved_in(input, &opts, &arena).unwrap();
    let got = common::parse_json(&carve2::to_json(&doc));
    assert_eq!(
        common::sorted(&got),
        common::sorted(&case["expectedTree"]),
        "tree: {id}"
    );
}

macro_rules! clause_case {
    ($name:ident, $id:literal, $clause:literal) => {
        #[test]
        fn $name() {
            check($id, $clause);
        }
    };
}

#[test]
fn decided_clause_table_has_a_test_for_each_row() {
    assert_eq!(cases().len(), 91);
}

clause_case!(c2_b_130_r1, "R1", "C2-B-130");
clause_case!(c2_b_130_r2, "R2", "C2-B-130");
clause_case!(c2_b_122_r3, "R3", "C2-B-122");
clause_case!(c2_b_225_r4, "R4", "C2-B-225");
clause_case!(c2_h_165_r5, "R5", "C2-H-165");
clause_case!(c2_h_165_r6, "R6", "C2-H-165");
clause_case!(c2_h_149_r7, "R7", "C2-H-149");
clause_case!(c2_a_049_r9, "R9", "C2-A-049");
clause_case!(c2_b_171_r10, "R10", "C2-B-171");
clause_case!(c2_b_127_r12, "R12", "C2-B-127");
clause_case!(c2_b_043_r14, "R14", "C2-B-043");
clause_case!(c2_h_165_r15, "R15", "C2-H-165");
clause_case!(c2_b_197_r16, "R16", "C2-B-197");
clause_case!(c2_i_079_r17, "R17", "C2-I-079");
clause_case!(c2_i_078_r18, "R18", "C2-I-078");
clause_case!(c2_i_077_r19, "R19", "C2-I-077");
clause_case!(c2_i_024_r20, "R20", "C2-I-024");
clause_case!(c2_i_076_r21, "R21", "C2-I-076");
clause_case!(c2_i_068_r22, "R22", "C2-I-068");
clause_case!(c2_b_127_r23, "R23", "C2-B-127");
clause_case!(c2_h_024_p1, "P1", "C2-H-024");
clause_case!(c2_i_033_p3, "P3", "C2-I-033");
clause_case!(c2_i_076_p4, "P4", "C2-I-076");
clause_case!(c2_b_084_p5, "P5", "C2-B-084");
clause_case!(c2_i_118_n1, "N1", "C2-I-118");
clause_case!(c2_i_118_n2, "N2", "C2-I-118");
clause_case!(c2_b_190_nested_marker, "S7-B-190-nested-marker", "C2-B-190");
clause_case!(
    c2_b_174_first_row_mark,
    "S7-B-174-first-row-mark",
    "C2-B-174"
);
clause_case!(c2_h_165_n3, "N3", "C2-H-165");
clause_case!(c2_h_166_n4, "N4", "C2-H-166");
clause_case!(c2_i_002_a_o1_space, "A-O1-space", "C2-I-002");
clause_case!(c2_i_002_a_o1_tab, "A-O1-tab", "C2-I-002");
clause_case!(c2_i_102_a_o5, "A-O5", "C2-I-102");
clause_case!(c2_h_069_a_o3, "A-O3", "C2-H-069");
clause_case!(c2_b_119_a_o6, "A-O6", "C2-B-119");
clause_case!(c2_h_165_a_o7, "A-O7", "C2-H-165");
clause_case!(c2_b_084_a_o9_space, "A-O9-space", "C2-B-084");
clause_case!(c2_b_084_a_o9_tab, "A-O9-tab", "C2-B-084");
clause_case!(c2_b_084_a_o9_b, "A-O9-b", "C2-B-084");
clause_case!(c2_h_051_c2_off, "C-2-off", "C2-H-051");
clause_case!(c2_h_051_c2_on, "C-2-on", "C2-H-051");
clause_case!(c2_h_054_c2_latex, "C-2-latex", "C2-H-054");
clause_case!(c2_h_178_c5, "C-5", "C2-H-178");
clause_case!(c2_a_182_e_g1_a, "E-G1-a", "C2-A-182");
clause_case!(c2_a_182_e_g1_b, "E-G1-b", "C2-A-182");
clause_case!(c2_a_086_e_g2_a, "E-G2-a", "C2-A-086");
clause_case!(c2_a_086_e_g2_b, "E-G2-b", "C2-A-086");
clause_case!(c2_i_017_g_s1, "G-S1", "C2-I-017");
clause_case!(c2_b_119_g_s4_a, "G-S4-a", "C2-B-119");
clause_case!(c2_b_101_g_s4_b, "G-S4-b", "C2-B-101");
clause_case!(c2_i_111_g_s4_c, "G-S4-c", "C2-I-111");
clause_case!(c2_b_015_s7, "S7-B-015", "C2-B-015");
clause_case!(c2_b_084_s7, "S7-B-084", "C2-B-084");
clause_case!(c2_b_137_s7, "S7-B-137", "C2-B-137");
clause_case!(c2_b_173_s7, "S7-B-173", "C2-B-173");
clause_case!(c2_b_174_s7, "S7-B-174", "C2-B-174");
clause_case!(c2_b_184_s7, "S7-B-184", "C2-B-184");
clause_case!(c2_b_209_s7, "S7-B-209", "C2-B-209");
clause_case!(c2_b_228_s7, "S7-B-228", "C2-B-228");
clause_case!(c2_b_229_s7, "S7-B-229", "C2-B-229");
clause_case!(c2_b_206_s7, "S7-B-206", "C2-B-206");
clause_case!(c2_i_017_s7, "S7-I-017", "C2-I-017");
clause_case!(c2_i_104_s7, "S7-I-104", "C2-I-104");
clause_case!(c2_i_119_s7, "S7-I-119", "C2-I-119");
clause_case!(c2_a_082_s7, "S7-A-082", "C2-A-082");
clause_case!(c2_h_016_s7, "S7-H-016", "C2-H-016");
clause_case!(
    c2_h_062_nested_math_caption,
    "S7-H-062-nested-math-caption",
    "C2-H-062"
);
clause_case!(c2_b_148_lazy_caption, "S7-B-148-lazy-caption", "C2-B-148");
clause_case!(
    c2_b_206_repeat_invalid,
    "S7-B-206-repeat-invalid",
    "C2-B-206"
);
clause_case!(
    c2_i_053_comment_neighbor,
    "S7-I-053-comment-neighbor",
    "C2-I-053"
);
clause_case!(
    c2_b_167_block_comment_tail,
    "S7-B-167-block-comment-tail",
    "C2-B-167"
);
clause_case!(
    c2_h_019_hard_break_comment,
    "S7-H-019-hard-break-comment",
    "C2-H-019"
);
clause_case!(
    c2_h_019_two_braced_comments,
    "S7-H-019-two-braced-comments",
    "C2-H-019"
);
clause_case!(c2_b_117_title_tab, "S7-B-117-title-tab", "C2-B-117");
clause_case!(
    c2_a_025_duplicate_footnote_walk,
    "S7-A-025-duplicate-footnote-walk",
    "C2-A-025"
);
clause_case!(c2_b_024_comment_lazy, "S7-B-024-comment-lazy", "C2-B-024");
clause_case!(c2_i_118_fallback_room, "S7-I-118-fallback-room", "C2-I-118");
clause_case!(c2_i_118_fallback_cap, "S7-I-118-fallback-cap", "C2-I-118");

clause_case!(c2_b_228_example_258, "S7-syntax-blocks-258", "C2-B-228");
clause_case!(c2_b_120_example_254, "S7-syntax-blocks-254", "C2-B-120");
clause_case!(c2_b_137_example_257, "S7-syntax-blocks-257", "C2-B-137");
clause_case!(c2_b_197_example_255, "S7-syntax-blocks-255", "C2-B-197");
clause_case!(c2_b_205_example_259, "S7-syntax-blocks-259", "C2-B-205");
clause_case!(c2_b_172_example_252, "S7-syntax-blocks-252", "C2-B-172");
clause_case!(c2_b_178_example_260, "S7-syntax-blocks-260", "C2-B-178");
clause_case!(c2_b_174_example_251, "S7-syntax-blocks-251", "C2-B-174");
clause_case!(c2_b_174_example_253, "S7-syntax-blocks-253", "C2-B-174");
clause_case!(c2_h_165_example_53, "S7-html-53", "C2-H-165");
clause_case!(c2_h_034_example_54, "S7-html-54", "C2-H-034");
clause_case!(c2_b_137_example_256, "S7-syntax-blocks-256", "C2-B-137");
clause_case!(c2_b_171_glued_prefix, "S7-B-171-glued-prefix", "C2-B-171");
