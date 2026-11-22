// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The partition rules of cache identity section 3 and the rows of its
//! verification table (section 4). The rows name the protocol fixtures that
//! show the same case (`fixtures/rqp/protocol/FX-RQ-C24-*`, `-C27-*`); the
//! fixture run itself is WP-08.

#![allow(clippy::too_many_lines)] // one table per function

use rivqen_cache::key::Partition;
use rivqen_cache::partition::{
    Credentials, StoreFacts, delta_partition, readable, site_matches, store_partition, valid_host,
};
use rivqen_model::ids::PartitionId;

const REQUEST: PartitionId = PartitionId::from_bytes([0x0A; 32]);
const OTHER: PartitionId = PartitionId::from_bytes([0x0B; 32]);

/// Every condition of rule 3 holds, with absent credentials and no explicit
/// `public`. Each row changes what its case names.
const BASE: StoreFacts = StoreFacts {
    public_enabled: true,
    origin_public: true,
    redirected: false,
    fields_visible: true,
    private: false,
    explicit_public: false,
    set_cookie: false,
    credentials: Credentials::Absent,
    manifest_public: true,
};

const PUBLIC: Partition = Partition::Public;
const SCOPED: Partition = Partition::Scoped(REQUEST);

#[test]
fn partition_store_table() {
    let rows: [(&str, StoreFacts, Partition); 19] = [
        // Section 4 rows.
        (
            "FX-RQ-C24-PUBLIC-NOT-ENABLED: setting off, explicit public",
            StoreFacts {
                public_enabled: false,
                explicit_public: true,
                credentials: Credentials::Present,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C24-PUBLIC-NO-DIRECTIVE: cookies, no public",
            StoreFacts {
                credentials: Credentials::Present,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C24-PUBLIC-CREDENTIALS-UNKNOWN: no public",
            StoreFacts {
                credentials: Credentials::Unknown,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C24-PUBLIC-SET-COOKIE: no credentials, Set-Cookie",
            StoreFacts {
                set_cookie: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C24-PUBLIC-EXPLICIT-SET-COOKIE: public and Set-Cookie",
            StoreFacts {
                explicit_public: true,
                set_cookie: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C24-PUBLIC-FIELDS-HIDDEN: public, no credentials",
            StoreFacts {
                explicit_public: true,
                fields_visible: false,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C24-PUBLIC-SET-COOKIE2: public and Set-Cookie2",
            StoreFacts {
                explicit_public: true,
                set_cookie: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C24-PUBLIC-REDIRECT: public, no credentials",
            StoreFacts {
                explicit_public: true,
                redirected: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C24-PUBLIC-REDIRECT-SAME-URL: public, no credentials",
            StoreFacts {
                explicit_public: true,
                redirected: true,
                ..BASE
            },
            SCOPED,
        ),
        // The other C24 fixtures.
        (
            "FX-RQ-C24-PUBLIC-EXPLICIT: cookies and public",
            StoreFacts {
                explicit_public: true,
                credentials: Credentials::Present,
                ..BASE
            },
            PUBLIC,
        ),
        ("FX-RQ-C24-PUBLIC-NO-CREDENTIALS: no public", BASE, PUBLIC),
        (
            "FX-RQ-C24-PUBLIC-PRIVATE-WINS: public and private",
            StoreFacts {
                explicit_public: true,
                private: true,
                ..BASE
            },
            SCOPED,
        ),
        // Rule 3 conditions without a fixture of their own.
        (
            "public with unknown credentials (H-61: public or no credentials)",
            StoreFacts {
                explicit_public: true,
                credentials: Credentials::Unknown,
                ..BASE
            },
            PUBLIC,
        ),
        (
            "public and no credentials",
            StoreFacts {
                explicit_public: true,
                ..BASE
            },
            PUBLIC,
        ),
        (
            "origin not declared public",
            StoreFacts {
                origin_public: false,
                explicit_public: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "manifest without mode public; legacy mode always (CL-05)",
            StoreFacts {
                manifest_public: false,
                explicit_public: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "private without public, no credentials",
            StoreFacts {
                private: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "setting off, every other condition holds",
            StoreFacts {
                public_enabled: false,
                explicit_public: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "nothing holds",
            StoreFacts {
                public_enabled: false,
                origin_public: false,
                redirected: true,
                fields_visible: false,
                private: true,
                explicit_public: false,
                set_cookie: true,
                credentials: Credentials::Unknown,
                manifest_public: false,
            },
            SCOPED,
        ),
    ];
    for (name, facts, expected) in rows {
        assert_eq!(store_partition(REQUEST, &facts), expected, "{name}");
    }
}

#[test]
fn partition_delta_table() {
    let rows: [(&str, Partition, StoreFacts, Partition); 16] = [
        // Section 4 rows (H-65, DL-039, DL-040 I-2, H-67).
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-STAYS-PUBLIC",
            PUBLIC,
            BASE,
            PUBLIC,
        ),
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-CREDENTIALS-UNKNOWN",
            PUBLIC,
            StoreFacts {
                credentials: Credentials::Unknown,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-SET-COOKIE",
            PUBLIC,
            StoreFacts {
                set_cookie: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-PUBLIC-DIRECTIVE: cookies and public",
            PUBLIC,
            StoreFacts {
                credentials: Credentials::Present,
                explicit_public: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-FIELDS-HIDDEN",
            PUBLIC,
            StoreFacts {
                fields_visible: false,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-NOT-PUBLIC-ORIGIN",
            PUBLIC,
            StoreFacts {
                origin_public: false,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-PRIVATE",
            PUBLIC,
            StoreFacts {
                private: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-SET-COOKIE2",
            PUBLIC,
            StoreFacts {
                set_cookie: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-REDIRECT",
            PUBLIC,
            StoreFacts {
                redirected: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-REDIRECT-HOST-CASE",
            PUBLIC,
            StoreFacts {
                redirected: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "FX-RQ-C27-PUBLIC-SNAPSHOT-NOT-ENABLED-DELTA: setting off",
            PUBLIC,
            StoreFacts {
                public_enabled: false,
                ..BASE
            },
            SCOPED,
        ),
        // Negotiation section 6.3 item 2.6: the policy of S has mode public.
        (
            "snapshot policy without mode public",
            PUBLIC,
            StoreFacts {
                manifest_public: false,
                ..BASE
            },
            SCOPED,
        ),
        (
            "public with unknown credentials does not count for a delta",
            PUBLIC,
            StoreFacts {
                explicit_public: true,
                credentials: Credentials::Unknown,
                ..BASE
            },
            SCOPED,
        ),
        // A scoped snapshot never moves to public.
        (
            "scoped snapshot, every condition holds",
            SCOPED,
            StoreFacts {
                explicit_public: true,
                ..BASE
            },
            SCOPED,
        ),
        (
            "snapshot of another partition gives the partition of the request",
            Partition::Scoped(OTHER),
            BASE,
            SCOPED,
        ),
        (
            "scoped snapshot, nothing holds",
            SCOPED,
            StoreFacts {
                public_enabled: false,
                credentials: Credentials::Present,
                ..BASE
            },
            SCOPED,
        ),
    ];
    for (name, snapshot, facts, expected) in rows {
        assert_eq!(
            delta_partition(snapshot, REQUEST, &facts),
            expected,
            "{name}"
        );
    }
}

#[test]
fn partition_readable_scoped_entry_only_for_its_own_partition() {
    for enabled in [false, true] {
        assert!(readable(Partition::Scoped(REQUEST), REQUEST, enabled));
        assert!(!readable(Partition::Scoped(OTHER), REQUEST, enabled));
        assert!(!readable(Partition::Scoped(REQUEST), OTHER, enabled));
    }
}

#[test]
fn partition_readable_public_entry_only_with_the_setting_on() {
    // DL-053; FX-RQ-C27-PUBLIC-SNAPSHOT-NOT-ENABLED-304 and -DELTA.
    assert!(!readable(Partition::Public, REQUEST, false));
    assert!(!readable(Partition::Public, OTHER, false));
    assert!(readable(Partition::Public, REQUEST, true));
    assert!(readable(Partition::Public, OTHER, true));
}

#[test]
fn partition_readable_ids_that_differ_in_one_byte_are_not_shared() {
    let mut bytes = [0x0A; 32];
    bytes[31] = 0x0B;
    let near = PartitionId::from_bytes(bytes);
    assert!(!readable(Partition::Scoped(near), REQUEST, true));
}

#[test]
fn partition_site_equal_host_matches() {
    assert!(site_matches("example.com", "example.com"));
    assert!(site_matches("a", "a"));
}

#[test]
fn partition_site_subdomain_matches_at_a_label_boundary() {
    assert!(site_matches("shop.example.com", "example.com"));
    assert!(site_matches("a.b.shop.example.com", "example.com"));
    assert!(site_matches("a.b.shop.example.com", "shop.example.com"));
    assert!(!site_matches("notexample.com", "example.com"));
    assert!(!site_matches("shop-example.com", "example.com"));
    assert!(!site_matches("example.com.attacker.net", "example.com"));
}

#[test]
fn partition_site_parent_and_other_hosts_do_not_match() {
    assert!(!site_matches("example.com", "shop.example.com"));
    assert!(!site_matches("example.org", "example.com"));
    assert!(!site_matches("example.co", "example.com"));
    assert!(!site_matches("xample.com", "example.com"));
    assert!(!site_matches("", "example.com"));
}

#[test]
fn partition_site_empty_domain_matches_nothing() {
    assert!(!site_matches("example.com", ""));
    assert!(!site_matches("", ""));
    assert!(!site_matches("example.com", "."));
    assert!(!site_matches("example.com.", "."));
    assert!(!site_matches(".", "."));
}

#[test]
fn partition_site_clears_more_for_other_forms() {
    // ASCII case does not count.
    assert!(site_matches("Shop.EXAMPLE.com", "example.com"));
    assert!(site_matches("shop.example.com", "Example.COM"));
    // One trailing dot of either name.
    assert!(site_matches("shop.example.com.", "example.com"));
    assert!(site_matches("example.com", "example.com."));
    assert!(site_matches("example.com.", "example.com."));
    // One leading dot of the domain (cookie `Domain` form).
    assert!(site_matches("example.com", ".example.com"));
    assert!(site_matches("shop.example.com", ".example.com"));
    assert!(!site_matches("notexample.com", ".example.com"));
}

#[test]
fn partition_site_strips_at_most_one_trailing_dot() {
    assert!(!site_matches("example.com", "example.com.."));
    assert!(!site_matches("example.com..", "example.com"));
}

#[test]
fn partition_site_strips_every_leading_dot_of_the_domain() {
    // A site clear must clear more, never less: `..example.com` is the same
    // site as `.example.com`.
    assert!(site_matches("example.com", "..example.com"));
    assert!(site_matches("shop.example.com", "..example.com"));
    assert!(site_matches("shop.example.com", "...example.com."));
    assert!(site_matches("Shop.Example.com.", "....EXAMPLE.COM"));
    assert!(!site_matches("notexample.com", "..example.com"));
    assert!(!site_matches("example.org", "..example.com"));
}

#[test]
fn partition_valid_host_lower_case_names() {
    for host in [
        "example.com",
        "shop.example.com",
        "a",
        "localhost",
        "a-b.c0.example",
        "-a.b-",
        "0.example",
        "xn--bcher-kva.example",
    ] {
        assert!(valid_host(host), "{host:?}");
    }
}

#[test]
fn partition_valid_host_refuses_a_port() {
    for host in [
        "shop.example.com:443",
        "shop.example.com:",
        "192.0.2.1:80",
        "[::1]:443",
        "[2001:db8::1]:8443",
    ] {
        assert!(!valid_host(host), "{host:?}");
    }
}

#[test]
fn partition_valid_host_refuses_upper_case() {
    for host in [
        "Shop.example.com",
        "SHOP.EXAMPLE.COM",
        "shop.example.Com",
        "[2001:DB8::1]",
        "[::A]",
    ] {
        assert!(!valid_host(host), "{host:?}");
    }
}

#[test]
fn partition_valid_host_refuses_an_idn_in_unicode() {
    for host in [
        "b\u{fc}cher.example",
        "\u{4f8b}\u{3048}.jp",
        "shop.\u{0435}xample.com",
    ] {
        assert!(!valid_host(host), "{host:?}");
    }
}

#[test]
fn partition_valid_host_refuses_empty_and_empty_labels() {
    for host in [
        "",
        ".",
        "..",
        "example.com.",
        ".example.com",
        "shop..example.com",
        "[]",
        "[",
        "]",
    ] {
        assert!(!valid_host(host), "{host:?}");
    }
}

#[test]
fn partition_valid_host_refuses_the_url_form_and_other_bytes() {
    for host in [
        "https://shop.example.com/",
        "https://shop.example.com",
        "shop.example.com/cart",
        "user@shop.example.com",
        "shop example.com",
        "shop_example.com",
        "shop.example.com\0",
        "*.example.com",
        "shop.example.com?x",
        "shop.example.com#x",
        "%73hop.example.com",
    ] {
        assert!(!valid_host(host), "{host:?}");
    }
}

#[test]
fn partition_valid_host_label_and_total_length_bounds() {
    let label_63 = "a".repeat(63);
    let label_64 = "a".repeat(64);
    assert!(valid_host(&label_63));
    assert!(valid_host(&format!("{label_63}.com")));
    assert!(!valid_host(&label_64));
    assert!(!valid_host(&format!("{label_64}.com")));
    // 4 labels of 63 and one dot each: 255 bytes; cut to 253 and 254.
    let name_253 = format!("{label_63}.{label_63}.{label_63}.{}", "a".repeat(61));
    assert_eq!(name_253.len(), 253);
    assert!(valid_host(&name_253));
    let name_254 = format!("{name_253}a");
    assert!(!valid_host(&name_254));
}

#[test]
fn partition_valid_host_ipv4_literal() {
    // A dotted-decimal IPv4 literal has the form of a host name.
    for host in ["192.0.2.1", "127.0.0.1", "0.0.0.0", "255.255.255.255"] {
        assert!(valid_host(host), "{host:?}");
    }
    for host in ["192.0.2.1.", ".192.0.2.1", "192..2.1"] {
        assert!(!valid_host(host), "{host:?}");
    }
}

#[test]
fn partition_valid_host_ipv6_literal() {
    for host in [
        "[::1]",
        "[::]",
        "[2001:db8::1]",
        "[2001:db8::]",
        "[::ffff:c000:201]",
        "[2001:db8:0:0:0:0:0:1]",
        "[2001:0db8:0000:0000:0000:0000:0000:0001]",
        "[1:2:3:4:5:6:7::]",
        "[::2:3:4:5:6:7:8]",
        "[1:2:3::6:7:8]",
    ] {
        assert!(valid_host(host), "{host:?}");
    }
    for host in [
        // No brackets.
        "::1",
        "2001:db8::1",
        // One bracket.
        "[::1",
        "::1]",
        // Two `::`.
        "[1::2::3]",
        "[:::1]",
        // Seven groups without `::`; nine groups.
        "[1:2:3:4:5:6:7]",
        "[1:2:3:4:5:6:7:8:9]",
        // Eight groups and `::`.
        "[1:2:3:4::5:6:7:8]",
        "[1:2:3:4:5:6:7:8::]",
        // A group of 5 digits; a single leading or trailing colon.
        "[2001:db8::10000]",
        "[:1:2:3:4:5:6:7]",
        "[1:2:3:4:5:6:7:]",
        // Not hexadecimal; embedded IPv4; a zone.
        "[2001:db8::g]",
        "[::ffff:192.0.2.1]",
        "[fe80::1%25eth0]",
        "[fe80::1%eth0]",
        // A host name in brackets.
        "[example.com]",
    ] {
        assert!(!valid_host(host), "{host:?}");
    }
}

#[test]
fn partition_valid_host_is_matched_by_a_site_clear_of_itself() {
    for host in [
        "shop.example.com",
        "192.0.2.1",
        "[2001:db8::1]",
        "xn--bcher-kva.example",
    ] {
        assert!(valid_host(host), "{host:?}");
        assert!(site_matches(host, host), "{host:?}");
    }
    // The refused forms are those a site clear of the site could miss.
    assert!(!site_matches("shop.example.com:443", "example.com"));
    assert!(!valid_host("shop.example.com:443"));
}

#[test]
fn partition_site_domain_of_dots_only_matches_nothing() {
    for domain in ["..", "...", "....", "....."] {
        assert!(!site_matches("example.com", domain), "{domain:?}");
        assert!(!site_matches("", domain), "{domain:?}");
        assert!(!site_matches(".", domain), "{domain:?}");
        assert!(!site_matches("..", domain), "{domain:?}");
    }
}
