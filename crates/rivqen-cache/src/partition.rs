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
///
/// [`Partition::Public`] only when every condition of cache identity rule 3
/// holds (H-61 to H-64, H-67, H-81, DL-038): the setting is on, the origin is
/// declared public, no redirect, all fields seen, no `private`, no
/// `Set-Cookie` or `Set-Cookie2`, the manifest has `"mode": "public"`, and
/// the response has an explicit `public` or the credentials are known to be
/// absent. Else the partition of the request.
#[must_use]
pub fn store_partition(request: PartitionId, facts: &StoreFacts) -> Partition {
    let credentials_allow = facts.explicit_public || known_absent(facts.credentials);
    if shared_conditions(facts) && credentials_allow {
        Partition::Public
    } else {
        Partition::Scoped(request)
    }
}

/// The partition for the result of a delta on a snapshot in `snapshot`
/// (H-65): an explicit `public` of the delta response does not count.
///
/// [`Partition::Public`] only when the snapshot is in `public`, the
/// conditions of [`store_partition`] other than the explicit `public` hold
/// for the delta response (`manifest_public` is the policy of the snapshot,
/// negotiation section 6.3 item 2.6), and the credentials are known to be
/// absent. A snapshot in a scoped partition never moves to `public`.
#[must_use]
pub fn delta_partition(snapshot: Partition, request: PartitionId, facts: &StoreFacts) -> Partition {
    let stays_public = matches!(snapshot, Partition::Public)
        && shared_conditions(facts)
        && known_absent(facts.credentials);
    if stays_public {
        Partition::Public
    } else {
        Partition::Scoped(request)
    }
}

/// True when a request in the partition `request` may read, offer or
/// revalidate an entry in `entry` (cache identity rules 2 and 3, DL-053).
///
/// A scoped entry: only for its own partition. A `public` entry: only with
/// the setting on. With both readable, the session prefers the entry of the
/// partition of the request (DL-039); that choice is not made here.
#[must_use]
pub fn readable(entry: Partition, request: PartitionId, public_enabled: bool) -> bool {
    match entry {
        Partition::Scoped(owner) => owner == request,
        Partition::Public => public_enabled,
    }
}

/// True when `host` is `cleared_domain` or a subdomain of it (cache
/// identity rule 5, H-60). Both are lower-case ASCII host names.
///
/// The match is on a label boundary: `shop.example.com` matches
/// `example.com`, `notexample.com` does not. A site clear must clear more,
/// never less, so the function also accepts what a caller may give in
/// another form: ASCII case does not count, one trailing dot of each name
/// and one leading dot of the domain (the cookie `Domain` form) are
/// ignored. An empty domain (also `.`) matches nothing; a clear of all
/// sites is a clear without a domain.
#[must_use]
pub fn site_matches(host: &str, cleared_domain: &str) -> bool {
    let domain = strip_trailing_dot(cleared_domain.as_bytes());
    let domain = domain.strip_prefix(b".").unwrap_or(domain);
    if domain.is_empty() {
        return false;
    }
    let host = strip_trailing_dot(host.as_bytes());
    let Some(cut) = host.len().checked_sub(domain.len()) else {
        return false;
    };
    let Some((head, tail)) = host.split_at_checked(cut) else {
        return false;
    };
    tail.eq_ignore_ascii_case(domain) && (head.is_empty() || head.ends_with(b"."))
}

/// The conditions of rule 3 that a full document and a delta share: all but
/// the choice between an explicit `public` and absent credentials.
const fn shared_conditions(facts: &StoreFacts) -> bool {
    facts.public_enabled
        && facts.origin_public
        && !facts.redirected
        && facts.fields_visible
        && !facts.private
        && !facts.set_cookie
        && facts.manifest_public
}

/// True only for credentials that the client knows are absent; unknown
/// counts as present (H-62).
const fn known_absent(credentials: Credentials) -> bool {
    matches!(credentials, Credentials::Absent)
}

/// `name` without one trailing `.`.
fn strip_trailing_dot(name: &[u8]) -> &[u8] {
    name.strip_suffix(b".").unwrap_or(name)
}
