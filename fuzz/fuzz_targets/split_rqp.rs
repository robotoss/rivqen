// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! RQP markup analysis: `split_rqp` with the default limits on any bytes.
//! Invariants: no panic; on `Ok` the blocks are in document order, do not
//! overlap, lie inside the input, number at most 256, and the template (the
//! input without the block contents) with the contents gives the input back.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rivqen_template::limits::TemplateLimits;
use rivqen_template::rqp::split_rqp;

const MAX_BLOCKS: usize = 256;

fuzz_target!(|data: &[u8]| {
    let limits = TemplateLimits::default();
    let Ok(document) = split_rqp(data, &limits) else {
        return;
    };
    assert!(document.blocks.len() <= MAX_BLOCKS);
    let mut template = Vec::new();
    let mut cursor = 0_usize;
    for block in &document.blocks {
        assert!(block.start <= block.end, "block range is reversed");
        assert!(block.end <= data.len(), "block ends outside the input");
        assert!(cursor <= block.start, "blocks overlap or are out of order");
        template.extend_from_slice(&data[cursor..block.start]);
        cursor = block.end;
    }
    template.extend_from_slice(&data[cursor..]);
    // Put the contents back at the cut points.
    let mut rebuilt = Vec::with_capacity(data.len());
    let mut at = 0_usize;
    let mut cut = 0_usize;
    for block in &document.blocks {
        let len = block.start - cut;
        rebuilt.extend_from_slice(&template[at..at + len]);
        rebuilt.extend_from_slice(&data[block.start..block.end]);
        at += len;
        cut = block.end;
    }
    rebuilt.extend_from_slice(&template[at..]);
    assert_eq!(rebuilt, data);
});
