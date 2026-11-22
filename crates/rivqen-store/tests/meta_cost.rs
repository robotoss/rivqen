// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! The cost of one committed metadata transaction with `synchronous=FULL`
//! (ADR-004 "Verification", R-4 of WP-07 S2). A measurement, not a check:
//! it is ignored by default. Run it with
//! `cargo test -p rivqen-store --release --test meta_cost -- --ignored --nocapture`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing
)]

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use rivqen_cache::key::Partition;
use rivqen_cache::txn::EntryVersion;
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::time::UnixTime;
use rivqen_store::meta::{MetaDb, Row};

const SAMPLES: usize = 101;

#[test]
#[ignore = "measurement; run with --ignored --nocapture"]
fn meta_cost_of_one_committed_transaction() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rivqen-store-meta-cost");
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    let mut db = MetaDb::open(&dir).unwrap();
    let partition = Partition::Scoped(PartitionId::from_bytes([3; 32]));

    // The metadata part of a `Replace`: a version, the row, the epochs.
    let mut times: Vec<Duration> = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        let key = CacheKey::from_bytes([u8::try_from(index % 251).unwrap(); 32]);
        let started = Instant::now();
        db.transaction(|txn| {
            txn.partition_epoch(partition)?;
            txn.site_epoch()?;
            let version = txn.next_version()?;
            txn.put_row(&Row {
                partition,
                key,
                host: "shop.example.com".to_owned(),
                version: EntryVersion(version.0),
                file_id: [7; 16],
                content_bytes: 100 * 1024,
                stored_at: UnixTime(1_700_000_000),
                last_used_at: UnixTime(1_700_000_000),
                sealed: vec![0x5A; 600],
            })
        })
        .unwrap();
        times.push(started.elapsed());
    }
    drop(db);
    fs::remove_dir_all(&dir).unwrap();

    times.sort();
    let median = times[SAMPLES / 2];
    let p90 = times[SAMPLES * 9 / 10];
    eprintln!(
        "meta commit, synchronous=FULL, {SAMPLES} samples: median {median:?}, p90 {p90:?}, min {:?}, max {:?}",
        times[0],
        times[SAMPLES - 1]
    );
}
