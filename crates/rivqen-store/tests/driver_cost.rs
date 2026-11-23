// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//! The cost of one `Replace` commit of a 100 KiB page and of one read on
//! the reference driver (ADR-004 "Verification", R-4 of WP-07 S2). A
//! measurement, not a check: it is ignored by default. Run it with
//! `cargo test -p rivqen-store --release --test driver_cost -- --ignored --nocapture`.
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

use rivqen_cache::driver::StorageDriver;
use rivqen_cache::key::Partition;
use rivqen_cache::txn::{CacheTransaction, Expect, Preconditions, TxnOp, TxnOutcome, Writer};
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_model::safe::PageBytes;
use rivqen_model::time::UnixTime;
use rivqen_store::crypto::HostKeys;
use rivqen_store::driver::{FILES_DIR, FileStore, schema_dir};
use rivqen_testkit::store::contract::sample_entry;

const SAMPLES: usize = 50;
const PAGE_BYTES: usize = 100 * 1024;
const NOW: UnixTime = UnixTime(1_800_000_000);

fn median(mut times: Vec<Duration>) -> (Duration, Duration, Duration, Duration) {
    times.sort();
    (
        times[times.len() / 2],
        times[times.len() * 9 / 10],
        times[0],
        times[times.len() - 1],
    )
}

fn dir_bytes(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .map(|entries| -> u64 {
            entries
                .map(|entry| {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        dir_bytes(&path)
                    } else {
                        fs::metadata(&path).unwrap().len()
                    }
                })
                .sum()
        })
        .unwrap_or_default()
}

#[test]
#[ignore = "measurement; run with --ignored --nocapture"]
fn driver_cost_replace_and_read_of_a_100_kib_page() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rivqen-store-driver-cost");
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    let partition = Partition::Scoped(PartitionId::from_bytes([3; 32]));
    let mut store = FileStore::open(&dir, HostKeys::new().unwrap()).unwrap();
    store.recover(NOW).unwrap();

    let mut commits = Vec::with_capacity(SAMPLES);
    let mut reads = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        let key = CacheKey::from_bytes([u8::try_from(index).unwrap(); 32]);
        let (meta, mut body) = sample_entry(partition, key, "shop.example.com", "cost", 0);
        body.page = PageBytes::new(vec![b'x'; PAGE_BYTES]);
        let pre = Preconditions {
            entry: Expect::Any,
            writer: Writer::Maintenance,
            partition_epoch: store.partition_epoch(partition).unwrap(),
            site_epoch: store.site_epoch().unwrap(),
        };
        let txn = CacheTransaction {
            op: TxnOp::Replace {
                meta: Box::new(meta),
                body: Box::new(body),
            },
            pre,
        };
        let started = Instant::now();
        let outcome = store.apply(txn).unwrap();
        commits.push(started.elapsed());
        assert!(matches!(outcome, TxnOutcome::Applied { .. }));

        let started = Instant::now();
        let entry = store.read(partition, &key, NOW).unwrap().unwrap();
        reads.push(started.elapsed());
        assert_eq!(entry.body.page.len(), PAGE_BYTES);
    }
    let files = dir_bytes(&schema_dir(&dir).join(FILES_DIR));
    let total = dir_bytes(&schema_dir(&dir));
    drop(store);
    fs::remove_dir_all(&dir).unwrap();

    let (median_commit, p90_commit, min_commit, max_commit) = median(commits);
    let (median_read, p90_read, min_read, max_read) = median(reads);
    eprintln!(
        "Replace of a {PAGE_BYTES}-byte page, {SAMPLES} samples: median {median_commit:?}, p90 {p90_commit:?}, min {min_commit:?}, max {max_commit:?}"
    );
    eprintln!(
        "read of it, {SAMPLES} samples: median {median_read:?}, p90 {p90_read:?}, min {min_read:?}, max {max_read:?}"
    );
    eprintln!("store of {SAMPLES} entries: files {files} bytes, schema directory {total} bytes");
}
