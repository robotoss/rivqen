// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Large inputs at the limits: the reader finishes them in one pass. The test
//! asserts results, not times; `cargo test --release -- --nocapture` prints
//! the times for a manual check.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::print_stderr
)]

use std::fmt::Write as _;
use std::time::Instant;

use rivqen_proto::envelope::{EnvelopeError, parse_envelope};
use rivqen_proto::json::{JsonRule, Reader, Value, parse};
use rivqen_proto::types::MAX_BLOCK_BYTES;

const FIVE_MIB: usize = 5 * 1024 * 1024;

fn drain(text: &[u8]) -> Result<usize, JsonRule> {
    let mut reader = Reader::new(text, 16).map_err(|e| e.rule())?;
    let mut n = 0;
    while reader.next_event().map_err(|e| e.rule())?.is_some() {
        n += 1;
    }
    Ok(n)
}

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    eprintln!("{label}: {:?}", start.elapsed());
    out
}

#[test]
fn json_reads_five_mib_inputs_in_one_pass() {
    // Many small values.
    let mut zeros = String::from("[0");
    while zeros.len() < FIVE_MIB {
        zeros.push_str(",0");
    }
    zeros.push(']');
    let events = timed("5 MiB of integers", || drain(zeros.as_bytes())).unwrap();
    assert_eq!(events, zeros.matches('0').count() + 2);

    // Many distinct member names with a long common prefix (duplicate check).
    let prefix = "k".repeat(64);
    let mut members = String::from("{");
    let mut i = 0_u32;
    while members.len() < FIVE_MIB {
        if i > 0 {
            members.push(',');
        }
        write!(members, "\"{prefix}{i}\":0").unwrap();
        i += 1;
    }
    members.push('}');
    let events = timed("5 MiB of member names", || drain(members.as_bytes())).unwrap();
    assert_eq!(events, 2 * i as usize + 2);
    // The last name again is found as a duplicate.
    let dup = format!("{},\"{prefix}0\":1}}", &members[..members.len() - 1]);
    assert_eq!(
        timed("duplicate at the end", || drain(dup.as_bytes())),
        Err(JsonRule::Duplicate)
    );

    // One long string with escapes.
    let long = format!("\"{}\"", "a\\n".repeat(FIVE_MIB / 3));
    let Value::String(s) = timed("5 MiB string", || parse(long.as_bytes())).unwrap() else {
        panic!("string expected");
    };
    assert_eq!(s.len(), 2 * (FIVE_MIB / 3));
}

#[test]
fn envelope_stops_at_the_first_schema_failure() {
    // 5 MiB of extra array items after 256 operations: the check stops at item
    // 257 and keeps at most 256 operations.
    let op = r#"{"block_id":"a","content":""}"#;
    let mut body = String::from(
        r#"{"binding":"d1.orrx-_-Iflnnjs7DvvuOOdxW3FzbZpU_PTlR0jdWw54","operations":["#,
    );
    while body.len() < FIVE_MIB {
        body.push_str(op);
        body.push(',');
    }
    body.push_str(op);
    body.push_str("]}");
    match timed("5 MiB envelope", || {
        parse_envelope(body.as_bytes(), usize::MAX, None)
    }) {
        Err(EnvelopeError::Schema(v)) => assert_eq!(v.instance_path(), "/operations"),
        other => panic!("maxItems expected, got {other:?}"),
    }
    // A content of maxBlockBytes + 1 bytes passes the parse; the size check of
    // check 10 is separate.
    let big = format!(
        r#"{{"binding":"d1.orrx-_-Iflnnjs7DvvuOOdxW3FzbZpU_PTlR0jdWw54","operations":[{{"block_id":"a","content":"{}"}}]}}"#,
        "\u{e9}".repeat(MAX_BLOCK_BYTES / 2 + 1)
    );
    let env = parse_envelope(big.as_bytes(), usize::MAX, None).unwrap();
    assert_eq!(env.operations[0].content.len(), MAX_BLOCK_BYTES + 2);
}
