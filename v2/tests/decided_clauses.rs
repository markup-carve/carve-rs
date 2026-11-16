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
    assert_eq!(cases().len(), 49);
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
