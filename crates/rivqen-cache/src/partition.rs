// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Partition rules (cache identity section 3; H-60 to H-68, H-81, DL-038,
//! DL-039, DL-053). Owner: WP-07 S1 T-01.
//!
//! Contract (architect): the public items of this file. Every function fails
//! closed: an input that is not known gives the partition of the request.
//! [`valid_host`] (DL-205) fails closed too: a host in any other form is not
//! valid, and the driver stores no entry for it.

use rivqen_model::ids::PartitionId;

use crate::key::Partition;

/// The longest host name in bytes, without a trailing dot (RFC 1035
/// section 2.3.4).
const MAX_HOST_NAME: usize = 253;

/// The longest label of a host name in bytes (RFC 1035 section 2.3.4).
const MAX_LABEL: usize = 63;

/// The number of 16-bit groups of an IPv6 address.
const IPV6_GROUPS: usize = 8;

/// The most hexadecimal digits in one group of an IPv6 address.
const MAX_GROUP_DIGITS: usize = 4;

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
/// and every leading dot of the domain (the cookie `Domain` form, also
/// written twice) are ignored. An empty domain (also a domain of dots
/// only) matches nothing; a clear of all sites is a clear without a domain.
#[must_use]
pub fn site_matches(host: &str, cleared_domain: &str) -> bool {
    let domain = strip_trailing_dot(cleared_domain.as_bytes());
    let domain = strip_leading_dots(domain);
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

/// True when `host` is in a form that [`site_matches`] always reads the
/// same way as a site clear does (DL-205; R-CACHE-09, S1 security review
/// L4). A driver refuses a `Replace` with any other host
/// ([`crate::txn::Refused::InvalidHost`]).
///
/// Valid forms:
///
/// - A lower-case ASCII host name: labels of `a` to `z`, `0` to `9` and
///   `-`, each 1 to 63 bytes, at most 253 bytes in total, no empty label
///   (so no leading or trailing dot), no port. A dotted-decimal IPv4
///   literal (`192.0.2.1`) has this form too. An internationalized name is
///   valid only in its ASCII (`xn--`) form; the adapter applies IDNA
///   (cache identity section 2).
/// - An IPv6 literal in brackets, only in the one text form that the
///   WHATWG URL serializer and RFC 5952 section 4 write (`[2001:db8::1]`,
///   `[::1]`, `[::]`): lower-case hexadecimal digits; each group without a
///   leading zero (`0` itself is a group); the longest run of two or more
///   zero groups written as `::`, the first one when two runs are equally
///   long; no `::` for one zero group; no other `::`. [`site_matches`]
///   compares bytes, so a second form of one address could escape a site
///   clear (DL-208 a). An embedded IPv4 form (`[::ffff:192.0.2.1]`) and a
///   zone (`%25`) are not valid.
///
/// Not valid, for example: `Shop.example.com`, `shop.example.com:443`,
/// `shop.example.com.`, a host with non-ASCII characters, an empty text,
/// `https://shop.example.com/`, `[2001:db8:0:0:0:0:0:1]`, `[2001:0db8::1]`,
/// `[2001:db8::1:1:1:1:1]`. The time is linear in the length of `host`;
/// the function allocates nothing.
#[must_use]
pub fn valid_host(host: &str) -> bool {
    match host.as_bytes() {
        [b'[', address @ .., b']'] => ipv6_address(address),
        name => host_name(name),
    }
}

/// A lower-case ASCII host name of [`valid_host`].
fn host_name(name: &[u8]) -> bool {
    name.len() <= MAX_HOST_NAME
        && name.split(|&byte| byte == b'.').all(|label| {
            (1..=MAX_LABEL).contains(&label.len())
                && label
                    .iter()
                    .all(|&byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

/// For each group of an IPv6 address: `true` when the group is zero.
type ZeroGroups = [bool; IPV6_GROUPS];

/// An IPv6 address of [`valid_host`], without its brackets, in the one
/// form of RFC 5952 section 4: every group written in its shortest form,
/// and `::` exactly at the run of zero groups that
/// [`compressed_run`] names, or no `::` when it names none.
fn ipv6_address(text: &[u8]) -> bool {
    // The groups that `::` stands for keep this value: zero.
    let mut zeros: ZeroGroups = [true; IPV6_GROUPS];
    let written = match text.windows(2).position(|pair| pair == b"::") {
        None => {
            if ipv6_groups(text, &mut zeros) != Some(IPV6_GROUPS) {
                return false;
            }
            None
        }
        Some(gap) => {
            let (Some(head), Some(tail)) = (text.get(..gap), text.get(gap.saturating_add(2)..))
            else {
                return false;
            };
            let mut tail_zeros: ZeroGroups = [false; IPV6_GROUPS];
            let (Some(before), Some(after)) = (
                ipv6_groups(head, &mut zeros),
                ipv6_groups(tail, &mut tail_zeros),
            ) else {
                return false;
            };
            // The groups that `::` stands for. A `run` of 0 or 1 never equals
            // the result of `compressed_run`, so the check below refuses it.
            let Some(run) = IPV6_GROUPS.checked_sub(before.saturating_add(after)) else {
                return false;
            };
            // The tail groups are the last `after` groups of the address.
            let (Some(target), Some(source)) = (
                zeros.get_mut(before.saturating_add(run)..),
                tail_zeros.get(..after),
            ) else {
                return false;
            };
            for (group, &zero) in target.iter_mut().zip(source) {
                *group = zero;
            }
            Some((before, run))
        }
    };
    written == compressed_run(zeros)
}

/// Reads `text`: groups of 1 to 4 lower-case hexadecimal digits with one `:`
/// between two groups, each group without a leading zero (`0` itself is a
/// group). Marks the zero groups in `zeros` from its first place on.
/// Returns the number of groups: `Some(0)` for an empty text; `None` for any
/// other form (also a second `::`) and for more groups than `zeros` holds.
fn ipv6_groups(text: &[u8], zeros: &mut ZeroGroups) -> Option<usize> {
    if text.is_empty() {
        return Some(0);
    }
    let mut count: usize = 0;
    for group in text.split(|&byte| byte == b':') {
        let digits = (1..=MAX_GROUP_DIGITS).contains(&group.len())
            && group
                .iter()
                .all(|&byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        // The shortest form: a leading `0` only in the group `0`.
        let shortest = group.len() == 1 || group.first() != Some(&b'0');
        if !digits || !shortest {
            return None;
        }
        *zeros.get_mut(count)? = group == b"0";
        count = count.checked_add(1)?;
    }
    Some(count)
}

/// The run of zero groups that RFC 5952 section 4.2 writes as `::`: the
/// longest run of two or more zero groups, the first one when two runs are
/// equally long. `(first group, length)`, or `None` when no run has two
/// groups.
fn compressed_run(zeros: ZeroGroups) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut start = 0;
    let mut length: usize = 0;
    for (index, zero) in zeros.into_iter().enumerate() {
        if zero {
            if length == 0 {
                start = index;
            }
            length = length.saturating_add(1);
            let longer = best.is_none_or(|(_, best_length)| length > best_length);
            if length >= 2 && longer {
                best = Some((start, length));
            }
        } else {
            length = 0;
        }
    }
    best
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

/// `name` without every leading `.`.
fn strip_leading_dots(name: &[u8]) -> &[u8] {
    let dots = name.iter().take_while(|&&b| b == b'.').count();
    name.get(dots..).unwrap_or_default()
}
