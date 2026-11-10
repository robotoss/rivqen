// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `Debug` of a digest and of the identifiers writes a short prefix only: a
//! full unkeyed block hash in a log allows guess-testing of the block content
//! (H-60). `Display` of an identifier is the wire text and stays full.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use proptest::prelude::*;
use rivqen_integrity::{Binding, PageRevision, Sha256Digest, TemplateRevision, sha256};

/// The block hash of "Shop" (manifest-patch section 1.1).
const HASH: &str = "0Aqua3-_d6RWbY3zaAXXrpc4lNqfUz7JZRazHgKLces";
const TEMPLATE_REVISION: &str = "t1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM";
const PAGE_REVISION: &str = "r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8";
const BINDING: &str = "d1.orrx-_-Iflnnjs7DvvuOOdxW3FzbZpU_PTlR0jdWw54";

#[test]
fn integrity_debug_prints_a_prefix_only() {
    let digest = Sha256Digest::from_b64u(HASH).unwrap();
    assert_eq!(format!("{digest:?}"), "Sha256Digest(0Aqua3-_...)");
    let template = TemplateRevision::parse(TEMPLATE_REVISION).unwrap();
    assert_eq!(format!("{template:?}"), "TemplateRevision(t1.86FtOeBe...)");
    let page = PageRevision::parse(PAGE_REVISION).unwrap();
    assert_eq!(format!("{page:?}"), "PageRevision(r1.RdZd06nC...)");
    let binding = Binding::parse(BINDING).unwrap();
    assert_eq!(format!("{binding:?}"), "Binding(d1.orrx-_-I...)");
    // Display is the full wire text.
    assert_eq!(template.to_string(), TEMPLATE_REVISION);
    assert_eq!(page.to_string(), PAGE_REVISION);
    assert_eq!(binding.to_string(), BINDING);
    // A container of digests (as SnapshotMeta) shows no full hash either.
    let shown = format!("{:?}", [digest, digest]);
    assert!(!shown.contains(HASH), "{shown}");
    assert!(!shown.contains(&HASH[..9]), "{shown}");
}

proptest! {
    /// For any digest, Debug writes exactly the first 8 of the 43 characters.
    #[test]
    fn integrity_debug_never_writes_more_than_8_characters(data in prop::collection::vec(any::<u8>(), 0..64)) {
        let digest = sha256(&data);
        let full = digest.to_b64u();
        prop_assert_eq!(format!("{digest:?}"), format!("Sha256Digest({}...)", &full[..8]));
        let page = PageRevision::from_digest(digest);
        prop_assert_eq!(format!("{page:?}"), format!("PageRevision(r1.{}...)", &full[..8]));
        prop_assert_eq!(page.to_string(), format!("r1.{full}"));
    }
}
