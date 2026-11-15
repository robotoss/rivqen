// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Partition rules (cache identity section 3; H-60 to H-68, H-81, DL-038,
//! DL-039, DL-053). Owner: WP-07 S1 T-01.
//!
//! Contract (architect): the public items of this file. Every function fails
//! closed: an input that is not known gives the partition of the request.

use rivqen_model::ids::PartitionId;

use crate::key::Partition;

/// What the client knows about the credentials of a request (H-62).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credentials {
    /// The client knows that no cookie, no `Authorization` field, no HTTP
    /// authentication and no client certificate was used.
    Absent,
    /// A credential was used.
    Present,
    /// The client cannot tell. Counts as [`Credentials::Present`].
    Unknown,
}

/// The facts of one exchange that decide the partition of its result (cache
/// identity rule 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // one flag per condition of rule 3
pub struct StoreFacts {
    /// The app turned the `public` partition on (H-81).
    pub public_enabled: bool,
    /// The app declares the origin public.
    pub origin_public: bool,
    /// The response came through a redirect, also to the same URL (H-67).
    pub redirected: bool,
    /// The native SDK saw all header fields of the response (H-64).
    pub fields_visible: bool,
    /// The response has the `Cache-Control` directive `private`.
    pub private: bool,
    /// The response has an explicit `Cache-Control: public`.
    pub explicit_public: bool,
    /// The response has a `Set-Cookie` or `Set-Cookie2` field (H-63).
    pub set_cookie: bool,
    /// The credentials of the request.
    pub credentials: Credentials,
    /// The manifest has `"mode": "public"`. Always `false` in legacy mode
    /// (CL-05).
    pub manifest_public: bool,
}

/// The partition for the snapshot of a full document.
#[must_use]
pub fn store_partition(request: PartitionId, facts: &StoreFacts) -> Partition {
    // Stub (T-01): the fail-closed answer.
    let _ = facts;
    Partition::Scoped(request)
}

/// The partition for the result of a delta on a snapshot in `snapshot`
/// (H-65): an explicit `public` of the delta response does not count.
#[must_use]
pub fn delta_partition(snapshot: Partition, request: PartitionId, facts: &StoreFacts) -> Partition {
    // Stub (T-01): the fail-closed answer.
    let _ = (snapshot, facts);
    Partition::Scoped(request)
}

/// True when a request in the partition `request` may read, offer or
/// revalidate an entry in `entry` (cache identity rules 2 and 3, DL-053).
#[must_use]
pub fn readable(entry: Partition, request: PartitionId, public_enabled: bool) -> bool {
    // Stub (T-01): the fail-closed answer.
    let _ = (entry, request, public_enabled);
    false
}

/// True when `host` is `cleared_domain` or a subdomain of it (cache
/// identity rule 5, H-60). Both are lower-case ASCII host names.
#[must_use]
pub fn site_matches(host: &str, cleared_domain: &str) -> bool {
    // Stub (T-01): clears more, never less.
    let _ = (host, cleared_domain);
    true
}
