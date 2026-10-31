// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The RQP request fields of negotiation §3 through `rqp::request::parse`
//! (`req_*`): one test per row of the tables of §3.3 and §3.4, per server step of
//! §3.3, §3.4 and §3.9, and the rules of §3.2, §3.5, §3.6 and DL-089; and every
//! request of the `FX-RQ-P*` fixtures.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use rivqen_reference::model::{Field, Request};
use rivqen_reference::rqp::request::{
    MAX_RQ_CAPABILITIES_BYTES, MAX_RQ_HAVE_BYTES, MAX_RQ_NONCE_BYTES, parse,
};
use rivqen_reference::rqp::types::{EntityTag, Have, IfNoneMatch, RqpRequest};

const R1_A: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";
const R1_B: &str = "r1.QZWQveB3Od0paA9JtInsnGdvGe4b3NIqT4xsP6rdKMU";
const T1: &str = "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM";
const B24: &str = ":0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5:";
const NONCE: &str = ":AAECAwQFBgcICQoLDA0ODw==:";
const NONCE_BYTES: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

fn req(fields: &[(&str, &str)]) -> RqpRequest {
    parse(&Request {
        method: "GET".to_owned(),
        url: "https://shop.example.com/catalog".to_owned(),
        fields: fields.iter().map(|(n, v)| Field::new(n, v)).collect(),
    })
}

fn caps(lines: &[&str]) -> Vec<String> {
    let fields: Vec<(&str, &str)> = lines.iter().map(|v| ("Rq-Capabilities", *v)).collect();
    req(&fields).capabilities
}

fn have(lines: &[&str]) -> Option<Have> {
    let fields: Vec<(&str, &str)> = lines.iter().map(|v| ("Rq-Have", *v)).collect();
    req(&fields).have
}

fn nonce(lines: &[&str]) -> Option<[u8; 16]> {
    let fields: Vec<(&str, &str)> = lines.iter().map(|v| ("Rq-Nonce", *v)).collect();
    req(&fields).nonce
}

fn accepts(lines: &[&str]) -> bool {
    let fields: Vec<(&str, &str)> = lines.iter().map(|v| ("Accept", *v)).collect();
    req(&fields).accepts_delta
}

fn inm(lines: &[&str]) -> (IfNoneMatch, Option<String>) {
    let fields: Vec<(&str, &str)> = lines.iter().map(|v| ("If-None-Match", *v)).collect();
    let r = req(&fields);
    (r.if_none_match, r.base)
}

fn strong(opaque: &str) -> EntityTag {
    EntityTag {
        weak: false,
        opaque: opaque.to_owned(),
    }
}

fn weak(opaque: &str) -> EntityTag {
    EntityTag {
        weak: true,
        opaque: opaque.to_owned(),
    }
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn version(lines: &[&str]) -> bool {
    let fields: Vec<(&str, &str)> = lines.iter().map(|v| ("Rq-Version", *v)).collect();
    req(&fields).version_1
}

// --- No field -------------------------------------------------------------

#[test]
fn req_no_fields_gives_the_empty_request() {
    assert_eq!(
        req(&[]),
        RqpRequest {
            version_1: false,
            capabilities: vec![],
            accepts_delta: false,
            if_none_match: IfNoneMatch::Absent,
            base: None,
            have: None,
            nonce: None,
            has_range: false,
            other_precondition: false,
        }
    );
}

#[test]
fn req_field_names_are_case_insensitive() {
    let r = req(&[
        ("RQ-VERSION", "1"),
        ("rq-capabilities", "(block-patch)"),
        ("ACCEPT", "application/vnd.rivqen.delta+json"),
        ("if-NONE-match", &format!("\"{R1_A}\"")),
        ("Rq-HAVE", &format!("t={T1}")),
        ("rQ-nOnCe", NONCE),
        ("RANGE", "bytes=0-1"),
        ("IF-RANGE", "\"x\""),
    ]);
    assert!(r.version_1);
    assert_eq!(r.capabilities, strings(&["block-patch"]));
    assert!(r.accepts_delta);
    assert_eq!(r.base.as_deref(), Some(R1_A));
    assert!(r.have.is_some());
    assert_eq!(r.nonce, Some(NONCE_BYTES));
    assert!(r.has_range);
    assert!(r.other_precondition);
}

#[test]
fn req_other_fields_are_ignored() {
    assert_eq!(
        req(&[
            ("X-Rq-Version", "1"),
            ("Rq-Versions", "1"),
            ("Accepts", "*/*")
        ]),
        req(&[])
    );
}

// --- §3.2 Rq-Version ------------------------------------------------------

#[test]
fn req_version_integer_1() {
    assert!(version(&["1"]));
}

#[test]
fn req_version_parameters_are_ignored() {
    assert!(version(&["1;x=2;y"]));
}

#[test]
fn req_version_other_values_are_not_version_1() {
    // FX-RQ-P03, FX-RQ-P04 and other types.
    for v in [
        "2", "1.0", "\"1\"", "abc", "?1", "@1", ":AQ==:", "%\"1\"", "-1", "", "(1)",
    ] {
        assert!(!version(&[v]), "{v:?}");
    }
}

#[test]
fn req_version_parse_failure_is_not_version_1() {
    assert!(!version(&["1 2"]));
    assert!(!version(&["1;"]));
}

#[test]
fn req_version_two_lines_are_not_one_item() {
    // Combined: "1, 1", which is not an Item.
    assert!(!version(&["1", "1"]));
}

#[test]
fn req_version_value_is_trimmed() {
    assert!(version(&[" \t1\t "]));
}

// --- §3.3 Rq-Capabilities: the table --------------------------------------

#[test]
fn req_caps_row_single_token() {
    assert_eq!(caps(&["(block-patch)"]), strings(&["block-patch"]));
}

#[test]
fn req_caps_row_inner_list_parameter_ignored() {
    assert_eq!(
        caps(&["(block-patch realtime-ws);v=2"]),
        strings(&["block-patch", "realtime-ws"])
    );
}

#[test]
fn req_caps_row_empty_inner_list() {
    assert_eq!(caps(&["()"]), Vec::<String>::new());
}

#[test]
fn req_caps_row_bare_token_is_absent() {
    assert_eq!(caps(&["block-patch"]), Vec::<String>::new());
}

#[test]
fn req_caps_row_string_in_inner_list_is_absent() {
    assert_eq!(caps(&["(block-patch \"x\")"]), Vec::<String>::new());
}

#[test]
fn req_caps_row_two_members_is_absent() {
    assert_eq!(
        caps(&["(block-patch), (realtime-ws)"]),
        Vec::<String>::new()
    );
}

#[test]
fn req_caps_row_two_lines_is_absent() {
    assert_eq!(
        caps(&["(block-patch)", "(realtime-ws)"]),
        Vec::<String>::new()
    );
}

// --- §3.3 Rq-Capabilities: the steps --------------------------------------

/// A combined value of exactly `len` bytes: `(block-patch x-aaa…)`.
fn caps_of_len(len: usize) -> String {
    let fixed = "(block-patch x-)".len();
    format!("(block-patch x-{})", "a".repeat(len - fixed))
}

#[test]
fn req_caps_step1_limit_is_1024_bytes() {
    // FX-RQ-P22-CAPS-1024 and -1025.
    let at = caps_of_len(MAX_RQ_CAPABILITIES_BYTES);
    assert_eq!(at.len(), 1024);
    assert_eq!(caps(&[&at])[0], "block-patch");
    assert_eq!(
        caps(&[&caps_of_len(MAX_RQ_CAPABILITIES_BYTES + 1)]),
        Vec::<String>::new()
    );
}

#[test]
fn req_caps_step1_limit_is_after_the_trim_of_each_line() {
    // Two lines are always two List members (step 3), so the join length of
    // the limit is shown with Rq-Have; here: white space around a line does not
    // count.
    let at = caps_of_len(MAX_RQ_CAPABILITIES_BYTES);
    assert_eq!(caps(&[&format!("  {at}\t")])[0], "block-patch");
}

#[test]
fn req_caps_step2_parse_failure_is_absent() {
    assert_eq!(caps(&["(block-patch"]), Vec::<String>::new());
    assert_eq!(caps(&["(block-patch),"]), Vec::<String>::new());
}

#[test]
fn req_caps_step3_member_not_inner_list_is_absent() {
    assert_eq!(caps(&["\"block-patch\""]), Vec::<String>::new());
    assert_eq!(caps(&[""]), Vec::<String>::new(), "empty List");
    assert_eq!(caps(&["(block-patch 1)"]), Vec::<String>::new());
    assert_eq!(caps(&["(block-patch ?1)"]), Vec::<String>::new());
}

#[test]
fn req_caps_step4_item_parameters_are_ignored() {
    assert_eq!(
        caps(&["(block-patch;v=1 realtime-ws;x)"]),
        strings(&["block-patch", "realtime-ws"])
    );
}

#[test]
fn req_caps_step4_set_has_each_token_once() {
    assert_eq!(
        caps(&["(realtime-ws block-patch realtime-ws block-patch)"]),
        strings(&["realtime-ws", "block-patch"])
    );
}

#[test]
fn req_caps_step5_unknown_tokens_stay_without_effect() {
    // FX-RQ-P31-UNKNOWN-TOKENS: block-patch is still in the set.
    let c = caps(&["(block-patch manifest json-blocks x-future)"]);
    assert!(c.iter().any(|t| t == "block-patch"));
    assert_eq!(c.len(), 4);
}

// --- §3.4 Rq-Have: the table and the example -------------------------------

#[test]
fn req_have_row_t_is_a_token() {
    assert_eq!(have(&[&format!("t={T1}")]).unwrap().template, T1);
}

#[test]
fn req_have_row_b_is_a_byte_sequence_of_prefixes() {
    // The example of §3.4: 3 blocks, 24 bytes.
    let h = have(&[&format!("t={T1}, b={B24}")]).unwrap();
    let expected = STANDARD.decode("0Aqua3+/d6ReDehqKiXfLM8hC9ypGZS5").unwrap();
    assert_eq!(expected.len(), 24);
    assert_eq!(
        h,
        Have {
            template: T1.to_owned(),
            digests: Some(expected)
        }
    );
}

// --- §3.4 Rq-Have: the server steps ---------------------------------------

/// `t=…, b=…, x="aaa…"` of exactly `len` bytes (FX-RQ-P16).
fn have_of_len(len: usize) -> String {
    let fixed = format!("t={T1}, b={B24}, x=\"\"").len();
    format!("t={T1}, b={B24}, x=\"{}\"", "a".repeat(len - fixed))
}

#[test]
fn req_have_step1_limit_is_4096_bytes() {
    let at = have_of_len(MAX_RQ_HAVE_BYTES);
    assert_eq!(at.len(), 4096);
    assert!(have(&[&at]).is_some());
    assert_eq!(have(&[&have_of_len(MAX_RQ_HAVE_BYTES + 1)]), None);
}

#[test]
fn req_have_step1_limit_counts_the_join_and_not_the_trim() {
    let full = have_of_len(MAX_RQ_HAVE_BYTES);
    let (left, right) = full.split_at(full.find(", x=").unwrap());
    let right = right.strip_prefix(", ").unwrap();
    // left + ", " + right = 4096 bytes.
    assert!(have(&[left, right]).is_some());
    assert!(have(&[&format!(" {left}\t"), &format!("\t{right} ")]).is_some());
    let longer = right.replacen("x=\"", "x=\"a", 1);
    assert_eq!(left.len() + 2 + longer.len(), 4097);
    assert_eq!(have(&[left, &longer]), None);
}

#[test]
fn req_have_step2_parse_failure_is_absent() {
    // FX-RQ-P17-HAVE-PARSE-FAILURE.
    assert_eq!(have(&["t="]), None);
    assert_eq!(
        have(&[&format!("t={T1}, b=:AA=:")]),
        None,
        "bad padding fails the field"
    );
    assert_eq!(have(&[&format!("t={T1},")]), None);
}

#[test]
fn req_have_step3_t_missing_is_absent() {
    // FX-RQ-P15-HAVE-NO-T.
    assert_eq!(have(&[&format!("b={B24}")]), None);
    assert_eq!(have(&[""]), None);
}

#[test]
fn req_have_step3_t_not_a_token_is_absent() {
    // FX-RQ-P15-HAVE-T-STRING and other types.
    for t in [
        format!("\"{T1}\""),
        "1".into(),
        ":AA==:".into(),
        "(a)".into(),
    ] {
        assert_eq!(have(&[&format!("t={t}, b={B24}")]), None, "{t}");
    }
    assert_eq!(
        have(&[&format!("t, b={B24}")]),
        None,
        "bare key: Boolean true"
    );
}

#[test]
fn req_have_step3_t_parameters_are_ignored() {
    assert_eq!(
        have(&[&format!("t={T1};v=2;x, b={B24}")]).unwrap().template,
        T1
    );
}

#[test]
fn req_have_step5_b_missing_is_absent() {
    // FX-RQ-P10-DELTA-NO-B.
    assert_eq!(
        have(&[&format!("t={T1}")]),
        Some(Have {
            template: T1.to_owned(),
            digests: None
        })
    );
}

#[test]
fn req_have_step5_b_not_a_byte_sequence_is_absent() {
    // FX-RQ-P13-B-BOOLEAN, FX-RQ-P13-B-STRING and others.
    for b in ["?1", "\"x\"", "1", "x", "(:AAAAAAAAAAA=:)"] {
        let h = have(&[&format!("t={T1}, b={b}")]).unwrap();
        assert_eq!(h.template, T1);
        assert_eq!(h.digests, None, "{b}");
    }
    let h = have(&[&format!("t={T1}, b")]).unwrap();
    assert_eq!(h.digests, None, "bare key: Boolean true");
}

#[test]
fn req_have_step5_b_parameters_are_ignored() {
    let h = have(&[&format!("t={T1}, b={B24};x=1")]).unwrap();
    assert_eq!(h.digests.unwrap().len(), 24);
}

#[test]
fn req_have_step6_length_not_multiple_of_8_is_absent() {
    // FX-RQ-P12-B-23-BYTES.
    let h = have(&[&format!("t={T1}, b=:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=:")]).unwrap();
    assert_eq!(h.digests, None);
    for len in [1, 7, 9, 15, 17] {
        let b = STANDARD.encode(vec![0u8; len]);
        assert_eq!(
            have(&[&format!("t={T1}, b=:{b}:")]).unwrap().digests,
            None,
            "{len}"
        );
    }
}

#[test]
fn req_have_step6_length_multiple_of_8_is_kept() {
    // FX-RQ-P11-B-SHORT has 8 bytes; FX-RQ-P32 has 0 bytes. The 8 × N check is
    // lane T-06.
    for len in [0_u32, 8, 16, 2048] {
        let bytes: Vec<u8> = (0..len).map(|i| u8::try_from(i % 251).unwrap()).collect();
        let b = STANDARD.encode(&bytes);
        let h = have(&[&format!("t={T1}, b=:{b}:")]).unwrap();
        assert_eq!(h.digests, Some(bytes), "{len}");
    }
}

#[test]
fn req_have_step6_byte_sequence_variants_give_the_same_bytes() {
    // §3.4 note: no padding and non-zero pad bits give the same bytes.
    let a = have(&[&format!("t={T1}, b=:AAAAAAAAAAA=:")]).unwrap();
    let b = have(&[&format!("t={T1}, b=:AAAAAAAAAAA:")]).unwrap();
    let c = have(&[&format!("t={T1}, b=:AAAAAAAAAAB:")]).unwrap();
    assert_eq!(a.digests, Some(vec![0; 8]));
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn req_have_step7_other_members_are_ignored() {
    // FX-RQ-P42-NONCE-IN-HAVE: `n` is an unknown member, never a nonce.
    let r = req(&[("Rq-Have", &format!("t={T1}, b={B24}, n={NONCE}, z=(1 2);p"))]);
    assert_eq!(r.have.unwrap().digests.unwrap().len(), 24);
    assert_eq!(r.nonce, None);
}

#[test]
fn req_have_duplicate_key_last_wins() {
    // FX-RQ-P34-HAVE-DUPLICATE-KEY.
    let h = have(&[&format!("t=t1.X, b={B24}, t={T1}")]).unwrap();
    assert_eq!(h.template, T1);
    let h = have(&[&format!("t={T1}, b={B24}, b=?1")]).unwrap();
    assert_eq!(h.digests, None);
}

#[test]
fn req_have_two_lines_are_one_dictionary() {
    // FX-RQ-P33-HAVE-TWO-LINES.
    let h = have(&[&format!("t={T1}"), &format!("b={B24}")]).unwrap();
    assert_eq!(h.template, T1);
    assert_eq!(h.digests.unwrap().len(), 24);
}

// --- §3.5 If-None-Match and the base ---------------------------------------

#[test]
fn req_inm_absent() {
    assert_eq!(inm(&[]), (IfNoneMatch::Absent, None));
}

#[test]
fn req_inm_strong_page_revision_gives_the_base() {
    assert_eq!(
        inm(&[&format!("\"{R1_B}\"")]),
        (IfNoneMatch::Tags(vec![strong(R1_B)]), Some(R1_B.to_owned()))
    );
}

#[test]
fn req_inm_weak_page_revision_gives_the_base() {
    // FX-RQ-P09-DELTA-WEAK-BASE, H-52.
    assert_eq!(
        inm(&[&format!("W/\"{R1_B}\"")]),
        (IfNoneMatch::Tags(vec![weak(R1_B)]), Some(R1_B.to_owned()))
    );
}

#[test]
fn req_inm_star_is_any_without_base() {
    assert_eq!(inm(&["*"]), (IfNoneMatch::Any, None));
    assert_eq!(inm(&[" * "]), (IfNoneMatch::Any, None));
}

#[test]
fn req_inm_list_of_two_tags_has_no_base() {
    // FX-RQ-P19-TWO-TAGS, FX-RQ-P07-NOT-MODIFIED-LIST.
    let v = format!("\"{R1_B}\", \"{R1_A}\"");
    assert_eq!(
        inm(&[&v]),
        (IfNoneMatch::Tags(vec![strong(R1_B), strong(R1_A)]), None)
    );
}

#[test]
fn req_inm_two_lines_are_one_list() {
    let (tags, base) = inm(&[&format!("\"{R1_B}\""), &format!("W/\"{R1_A}\"")]);
    assert_eq!(tags, IfNoneMatch::Tags(vec![strong(R1_B), weak(R1_A)]));
    assert_eq!(base, None);
}

#[test]
fn req_inm_other_tags_have_no_base() {
    // FX-RQ-P05-APP-VALIDATOR, FX-RQ-P20-INM-OTHER-TAG, -TEMPLATE-REVISION.
    for opaque in [
        "app-1".to_owned(),
        "abc".to_owned(),
        T1.to_owned(),
        String::new(),
        format!("{R1_B}A"),
        R1_B[..R1_B.len() - 1].to_owned(),
        R1_B.replacen("r1.", "R1.", 1),
        R1_B.replacen('Q', "+", 1),
        R1_B.replacen('Q', "=", 1),
    ] {
        assert_eq!(
            inm(&[&format!("\"{opaque}\"")]),
            (IfNoneMatch::Tags(vec![strong(&opaque)]), None),
            "{opaque}"
        );
    }
}

#[test]
fn req_inm_list_elements_take_ows_and_empty_elements() {
    // RFC 9110 §5.6.1.2: empty list elements are ignored for the condition; the
    // base needs exactly one element (§3.5 item 2).
    let (tags, base) = inm(&[&format!(" , \"{R1_A}\" ,\t,  \"x\" ,")]);
    assert_eq!(tags, IfNoneMatch::Tags(vec![strong(R1_A), strong("x")]));
    assert_eq!(base, None);
    let (tags, base) = inm(&[&format!("\"{R1_A}\","), ""]);
    assert_eq!(tags, IfNoneMatch::Tags(vec![strong(R1_A)]));
    assert_eq!(base, None);
}

#[test]
fn req_inm_opaque_tag_characters() {
    // etagc = %x21 / %x23-7E / obs-text: commas and backslashes are opaque bytes.
    assert_eq!(
        inm(&["\"a,b\\\", \"é\""]),
        (IfNoneMatch::Tags(vec![strong("a,b\\"), strong("é")]), None)
    );
}

#[test]
fn req_inm_other_syntax_is_invalid() {
    // DL-089 item 3.
    for v in [
        "abc",
        "w/\"abc\"",
        "W/abc",
        "W/ \"abc\"",
        "\"abc",
        "abc\"",
        "\"a\"b\"",
        "\"a b\"",
        "\"a\tb\"",
        "\"abc\" x",
        "\"abc\"\"def\"",
        "*, \"abc\"",
        "\"abc\", *",
        "**",
        "",
        ",",
        " , , ",
    ] {
        assert_eq!(inm(&[v]), (IfNoneMatch::Invalid, None), "{v:?}");
    }
    assert_eq!(inm(&["*", "*"]), (IfNoneMatch::Invalid, None));
}

// --- §3.6 Accept ----------------------------------------------------------

#[test]
fn req_accept_delta_media_type() {
    assert!(accepts(&["text/html, application/vnd.rivqen.delta+json"]));
    assert!(accepts(&["application/vnd.rivqen.delta+json"]));
}

#[test]
fn req_accept_compares_ascii_case_insensitively() {
    assert!(accepts(&["Application/VND.Rivqen.Delta+JSON"]));
}

#[test]
fn req_accept_other_types_and_wildcards_do_not_count() {
    // FX-RQ-P23-ACCEPT-HTML, -WILDCARD, FX-RQ-P02-BROWSER.
    for v in [
        "text/html",
        "*/*",
        "application/*",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        "application/vnd.rivqen.delta",
        "application/vnd.rivqen.delta+jsonx",
        "xapplication/vnd.rivqen.delta+json",
        "application/json",
        "",
    ] {
        assert!(!accepts(&[v]), "{v:?}");
    }
    assert!(!req(&[]).accepts_delta);
}

#[test]
fn req_accept_q_zero_does_not_accept() {
    // FX-RQ-P23-ACCEPT-Q0.
    for q in ["0", "0.", "0.0", "0.00", "0.000"] {
        assert!(
            !accepts(&[&format!(
                "text/html, application/vnd.rivqen.delta+json;q={q}"
            )]),
            "{q}"
        );
    }
}

#[test]
fn req_accept_q_above_zero_accepts() {
    for q in ["1", "1.", "1.0", "1.000", "0.001", "0.5", "0.999", "0.1"] {
        assert!(
            accepts(&[&format!("application/vnd.rivqen.delta+json;q={q}")]),
            "{q}"
        );
    }
}

#[test]
fn req_accept_q_name_is_case_insensitive_and_takes_ows() {
    assert!(accepts(&["application/vnd.rivqen.delta+json ; Q=0.5"]));
    assert!(!accepts(&["application/vnd.rivqen.delta+json\t;\tQ=0"]));
}

#[test]
fn req_accept_invalid_qvalue_does_not_accept() {
    // DL-089 item 4.
    for q in [
        "2", "1.5", "1.001", "0.1234", "1.0000", "-1", "abc", "", "\"1\"", "0.5x", ".5", "01",
        "0,5",
    ] {
        assert!(
            !accepts(&[&format!("application/vnd.rivqen.delta+json;q={q}")]),
            "{q:?}"
        );
    }
}

#[test]
fn req_accept_q_is_the_weight_in_any_position() {
    // RFC 9110 §12.5.1: any parameter named "q" is the weight.
    assert!(accepts(&["application/vnd.rivqen.delta+json;q=0.5;v=1"]));
    assert!(!accepts(&["application/vnd.rivqen.delta+json;v=1;q=0"]));
    assert!(
        !accepts(&["application/vnd.rivqen.delta+json;q=1;q=1"]),
        "two weights"
    );
}

#[test]
fn req_accept_other_parameters_do_not_matter() {
    assert!(accepts(&[
        "application/vnd.rivqen.delta+json;v=2;x=\"a;b,c\""
    ]));
    assert!(accepts(&["application/vnd.rivqen.delta+json;"]));
    assert!(accepts(&["application/vnd.rivqen.delta+json; ;v=1"]));
    assert!(accepts(&["application/vnd.rivqen.delta+json;x=\"a\\\"b\""]));
}

#[test]
fn req_accept_malformed_element_accepts_nothing() {
    for v in [
        "application/vnd.rivqen.delta+json foo",
        "application/vnd.rivqen.delta+json;v",
        "application/vnd.rivqen.delta+json;v=",
        "application/vnd.rivqen.delta+json;=1",
        "application/vnd.rivqen.delta+json;v =1",
        "application/vnd.rivqen.delta+json;v= 1",
        "application/vnd.rivqen.delta+json;v=\"open",
        "application /vnd.rivqen.delta+json",
        "application/ vnd.rivqen.delta+json",
        "application vnd.rivqen.delta+json",
    ] {
        assert!(!accepts(&[v]), "{v:?}");
    }
}

#[test]
fn req_accept_elements_are_independent() {
    assert!(accepts(&[
        "text/html;;bad x, application/vnd.rivqen.delta+json"
    ]));
    assert!(accepts(&[
        "application/vnd.rivqen.delta+json;q=0, application/vnd.rivqen.delta+json"
    ]));
    assert!(accepts(&[", ,application/vnd.rivqen.delta+json,"]));
    // A comma inside a quoted string does not end the element.
    assert!(!accepts(&[
        "text/html;x=\", application/vnd.rivqen.delta+json\""
    ]));
    // An escaped quote (quoted-pair) does not end the quoted string: the
    // string stays open to the end, so the comma does not split the list.
    assert!(!accepts(&[
        "text/html;x=\"a\\\", application/vnd.rivqen.delta+json"
    ]));
    assert!(accepts(&[
        "text/html;x=\"a\\\\\", application/vnd.rivqen.delta+json"
    ]));
}

#[test]
fn req_accept_two_lines_are_one_list() {
    assert!(accepts(&["text/html", "application/vnd.rivqen.delta+json"]));
}

// --- §3.9 Rq-Nonce ---------------------------------------------------------

#[test]
fn req_nonce_step5_sixteen_bytes() {
    assert_eq!(nonce(&[NONCE]), Some(NONCE_BYTES));
    // FX-RQ-P42-NONCE-OTHER.
    let other = nonce(&[":8PHy8/T19vf4+fr7/P3+/w==:"]).unwrap();
    assert_eq!(URL_SAFE_NO_PAD.encode(other), "8PHy8_T19vf4-fr7_P3-_w");
}

#[test]
fn req_nonce_step1_no_line_is_no_nonce() {
    assert_eq!(nonce(&[]), None);
}

#[test]
fn req_nonce_step1_two_lines_are_no_nonce() {
    // FX-RQ-P42-NONCE-TWO-LINES.
    assert_eq!(nonce(&[NONCE, NONCE]), None);
    assert_eq!(nonce(&[NONCE, ""]), None);
}

#[test]
fn req_nonce_step1_limit_is_128_bytes() {
    // FX-RQ-P42-NONCE-AT-LIMIT and -TOO-LONG.
    let base = format!("{NONCE};x=\"\"");
    let at = format!(
        "{NONCE};x=\"{}\"",
        "a".repeat(MAX_RQ_NONCE_BYTES - base.len())
    );
    assert_eq!(at.len(), 128);
    assert_eq!(nonce(&[&at]), Some(NONCE_BYTES));
    let over = format!(
        "{NONCE};x=\"{}\"",
        "a".repeat(MAX_RQ_NONCE_BYTES + 1 - base.len())
    );
    assert_eq!(nonce(&[&over]), None);
}

#[test]
fn req_nonce_step1_limit_is_after_the_trim() {
    let base = format!("{NONCE};x=\"\"");
    let at = format!(
        "{NONCE};x=\"{}\"",
        "a".repeat(MAX_RQ_NONCE_BYTES - base.len())
    );
    assert_eq!(nonce(&[&format!("  \t{at}\t  ")]), Some(NONCE_BYTES));
}

#[test]
fn req_nonce_step2_parse_failure_is_no_nonce() {
    // FX-RQ-P42-NONCE-PARSE-FAILURE.
    assert_eq!(nonce(&[&format!("{NONCE}, {NONCE}")]), None);
    assert_eq!(
        nonce(&[":AAECAwQFBgcICQoLDA0ODw=:"]),
        None,
        "incomplete padding"
    );
    assert_eq!(nonce(&[":AAECAwQFBgcICQoLDA0ODw==:;"]), None);
}

#[test]
fn req_nonce_step3_other_types_are_no_nonce() {
    // FX-RQ-P42-NONCE-STRING, -BOOLEAN and the other bare item types.
    for v in [
        "\"AAECAwQFBgcICQoLDA0ODw\"",
        "?1",
        "AAECAwQFBgcICQoLDA0ODw",
        "1",
        "1.5",
        "@1",
        "%\"x\"",
    ] {
        assert_eq!(nonce(&[v]), None, "{v}");
    }
}

#[test]
fn req_nonce_step3_parameters_are_ignored() {
    // FX-RQ-P42-NONCE-PARAMETER.
    assert_eq!(nonce(&[&format!("{NONCE};x=1;y")]), Some(NONCE_BYTES));
}

#[test]
fn req_nonce_step4_other_lengths_are_no_nonce() {
    // FX-RQ-P42-NONCE-15-BYTES and -17-BYTES.
    assert_eq!(nonce(&[":AAECAwQFBgcICQoLDA0O:"]), None);
    assert_eq!(nonce(&[":AAECAwQFBgcICQoLDA0ODxA=:"]), None);
    assert_eq!(nonce(&["::"]), None);
}

#[test]
fn req_nonce_step5_padding_variants_give_the_same_bytes() {
    assert_eq!(nonce(&[":AAECAwQFBgcICQoLDA0ODw:"]), Some(NONCE_BYTES));
    assert_eq!(nonce(&[":AAECAwQFBgcICQoLDA0ODx==:"]), Some(NONCE_BYTES));
}

// --- §4 step 5.1 and DL-089 item 1 ------------------------------------------

#[test]
fn req_range_field_is_reported() {
    // FX-RQ-P24-RANGE.
    assert!(req(&[("Range", "bytes=0-99")]).has_range);
    assert!(req(&[("range", "")]).has_range);
    assert!(!req(&[("Content-Range", "bytes 0-1/2")]).has_range);
}

#[test]
fn req_other_preconditions_are_reported() {
    for name in ["If-Match", "If-Unmodified-Since", "If-Range", "if-match"] {
        let r = req(&[(name, "x")]);
        assert!(r.other_precondition, "{name}");
        assert!(!r.has_range);
    }
    assert!(!req(&[("If-Modified-Since", "x")]).other_precondition);
    assert!(!req(&[("If-None-Match", "*")]).other_precondition);
}

// --- Fixtures -------------------------------------------------------------

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rqp/protocol")
}

fn str_of<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or_else(|| panic!("{key}"))
}

#[test]
fn req_fixture_requests_parse() {
    let mut cases = 0;
    let mut deltas = 0;
    let mut not_modified = 0;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(fixture_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("FX-RQ-P"))
        })
        .collect();
    dirs.sort();
    for dir in dirs {
        let text = std::fs::read_to_string(dir.join("expected.json")).unwrap();
        let expected: serde_json::Value = serde_json::from_str(&text).unwrap();
        let id = str_of(&expected, "id").to_owned();
        let request = &expected["request"];
        let fields = request["headers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| Field::new(h[0].as_str().unwrap(), h[1].as_str().unwrap()))
            .collect();
        let method = str_of(request, "method").to_owned();
        let r = parse(&Request {
            method: method.clone(),
            url: str_of(request, "target").to_owned(),
            fields,
        });
        cases += 1;

        // A delta answers only a delta-eligible request (§4 step 5): every
        // condition that the request fields decide must hold, and B and N must be
        // the inputs of the expected binding.
        let body = &expected["body"];
        if body["kind"] == "delta" {
            deltas += 1;
            assert_eq!(method, "GET", "{id}");
            assert!(!r.has_range && !r.other_precondition, "{id}");
            assert!(r.version_1, "{id}");
            assert!(r.capabilities.iter().any(|c| c == "block-patch"), "{id}");
            assert!(r.accepts_delta, "{id}");
            let inputs = &body["binding_inputs"];
            assert_eq!(
                r.base.as_deref(),
                Some(str_of(inputs, "base_revision")),
                "{id}"
            );
            assert!(r.have.is_some(), "{id}");
            let n = r.nonce.unwrap_or_else(|| panic!("{id}: no nonce"));
            assert_eq!(URL_SAFE_NO_PAD.encode(n), str_of(inputs, "nonce"), "{id}");
        }
        // A 304 needs an If-None-Match that can match (§4 step 4).
        if expected["status"] == 304 {
            not_modified += 1;
            assert!(
                matches!(r.if_none_match, IfNoneMatch::Any | IfNoneMatch::Tags(_)),
                "{id}"
            );
        }
        // A Have that passed steps 1 to 3 keeps b only with L a multiple of 8.
        if let Some(Have {
            digests: Some(d), ..
        }) = &r.have
        {
            assert_eq!(d.len() % 8, 0, "{id}");
        }
    }
    assert_eq!(cases, 112);
    assert_eq!(deltas, 21);
    assert!(not_modified > 0);
}

#[test]
fn req_fixture_nonce_cases() {
    // §3.9: the FX-RQ-P42 cases whose name says no nonce.
    for id in [
        "FX-RQ-P42-NONCE-15-BYTES",
        "FX-RQ-P42-NONCE-17-BYTES",
        "FX-RQ-P42-NONCE-BOOLEAN",
        "FX-RQ-P42-NONCE-IN-HAVE",
        "FX-RQ-P42-NONCE-MISSING",
        "FX-RQ-P42-NONCE-PARSE-FAILURE",
        "FX-RQ-P42-NONCE-STRING",
        "FX-RQ-P42-NONCE-TOO-LONG",
        "FX-RQ-P42-NONCE-TWO-LINES",
    ] {
        let text = std::fs::read_to_string(fixture_dir().join(id).join("expected.json")).unwrap();
        let expected: serde_json::Value = serde_json::from_str(&text).unwrap();
        let fields = expected["request"]["headers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| Field::new(h[0].as_str().unwrap(), h[1].as_str().unwrap()))
            .collect();
        let r = parse(&Request {
            method: "GET".to_owned(),
            url: "https://shop.example.com/catalog".to_owned(),
            fields,
        });
        assert_eq!(r.nonce, None, "{id}");
        assert!(r.have.is_some(), "{id}");
    }
}
