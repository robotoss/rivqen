// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Contract of the legacy session (WP-08 S2): what the stubs and the product
//! both guarantee.
#![cfg(feature = "legacy")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use rivqen_cache::txn::{Epoch, Expect, TxnOp, Writer};
use rivqen_model::ids::{CacheKey, PartitionId};
use rivqen_session::legacy::record::{WriteBasis, remove};
use rivqen_session::legacy::types::{DEFAULT_UNAVAILABLE_TIME_MS, DIFF_MAX_AGE_MS, Effect};

#[test]
fn session_constants_of_the_rules() {
    assert_eq!(DIFF_MAX_AGE_MS, 30_000);
    assert_eq!(DEFAULT_UNAVAILABLE_TIME_MS, 21_600_000);
}

/// DL-220: every write of a session names the request partition.
#[test]
fn session_write_names_the_request_partition() {
    let basis = WriteBasis {
        partition: PartitionId::from_bytes([5; 32]),
        key: CacheKey::from_bytes([6; 32]),
        partition_epoch: Epoch(3),
        site_epoch: Epoch(4),
        expect: Expect::Any,
    };
    let txn = remove(&basis);
    assert_eq!(
        txn.pre.writer,
        Writer::request(PartitionId::from_bytes([5; 32]), Epoch(3))
    );
    assert_eq!(txn.pre.site_epoch, Epoch(4));
    assert!(matches!(txn.op, TxnOp::Remove { .. }));
}

#[test]
fn session_effect_debug_has_no_content() {
    let text = format!(
        "{:?}",
        Effect::Message("{\"result\":\"secret\"}".to_owned())
    );
    assert!(!text.contains("secret"), "{text}");
    let text = format!("{:?}", Effect::StoreCookies(vec!["sid=secret".to_owned()]));
    assert!(!text.contains("secret"), "{text}");
}

mod debug_texts {
    use rivqen_cache::key::Partition;
    use rivqen_cache::txn::{CacheTransaction, Epoch, Expect, TxnOp};
    use rivqen_integrity::legacy::sha1_hex;
    use rivqen_model::http::FieldLine;
    use rivqen_model::ids::{CacheKey, PartitionId};
    use rivqen_model::safe::Secret;
    use rivqen_model::time::{Instant, UnixTime};
    use rivqen_proto_legacy::message::ResultCode;
    use rivqen_session::legacy::record::{NewRecord, WriteBasis, commit};
    use rivqen_session::legacy::types::{
        DocumentHead, Effect, ExchangeId, Input, InterceptId, LookupResult, OpenContext,
        RequestSpec, TransportError, VaryKey,
    };

    const SECRET: &str = "secret-value";

    fn basis() -> WriteBasis {
        WriteBasis {
            partition: PartitionId::from_bytes([1; 32]),
            key: CacheKey::from_bytes([2; 32]),
            partition_epoch: Epoch(1),
            site_epoch: Epoch(2),
            expect: Expect::Any,
        }
    }

    fn txn(op: TxnOp) -> Effect {
        Effect::Cache(Box::new(CacheTransaction {
            op,
            pre: basis().preconditions(),
        }))
    }

    /// Each type writes its kind, ids, codes, counts and lengths, and no
    /// content (DL-227).
    #[test]
    #[allow(clippy::too_many_lines)] // one table of every input and effect
    fn session_debug_texts_are_exact() {
        let fields = vec![FieldLine::new("set-cookie", SECRET)];
        assert_eq!(
            format!("{:?}", VaryKey(Secret::new([9; 32]))),
            "VaryKey(..)"
        );
        let url = rivqen_url::parse("https://a.example/p?q=secret-value").unwrap();
        let context = OpenContext {
            url,
            partition: PartitionId::from_bytes([1; 32]),
            anonymous: true,
            network_valid: false,
            platform_fields: fields.clone(),
            vary_key: VaryKey(Secret::new([9; 32])),
        };
        let text = format!("{context:?}");
        assert!(text.starts_with("OpenContext { url: ParsedUrl"), "{text}");
        assert!(
            text.ends_with("anonymous: true, network_valid: false, platform_fields: 1, .. }"),
            "{text}"
        );
        assert!(!text.contains(SECRET), "{text}");
        let lookup = LookupResult {
            entry: None,
            disabled: true,
            partition_epoch: Epoch(3),
            site_epoch: Epoch(4),
        };
        assert_eq!(
            format!("{lookup:?}"),
            "LookupResult { entry: None, disabled: true, partition_epoch: Epoch(3), site_epoch: Epoch(4) }"
        );
        let inputs = [
            (Input::Lookup(Box::new(lookup)), "Lookup(LookupResult {"),
            (Input::Cookie(Some(SECRET.to_owned())), "Cookie(Some(12))"),
            (
                Input::ResponseHead {
                    exchange: ExchangeId(1),
                    status: 200,
                    fields: fields.clone(),
                },
                "ResponseHead { exchange: ExchangeId(1), status: 200, fields: 1 }",
            ),
            (
                Input::BodyChunk {
                    exchange: ExchangeId(1),
                    bytes: SECRET.as_bytes().to_vec(),
                },
                "BodyChunk { exchange: ExchangeId(1), len: 12 }",
            ),
            (
                Input::BodyEnd {
                    exchange: ExchangeId(2),
                },
                "BodyEnd { exchange: ExchangeId(2) }",
            ),
            (
                Input::TransportFailed {
                    exchange: ExchangeId(3),
                    error: TransportError::Tls,
                },
                "TransportFailed { exchange: ExchangeId(3), error: Tls }",
            ),
            (Input::BindClient, "BindClient"),
            (Input::ClientReady, "ClientReady"),
            (
                Input::Intercept(InterceptId(5)),
                "Intercept(InterceptId(5))",
            ),
            (Input::GetDiff, "GetDiff"),
            (Input::PageFinished, "PageFinished"),
            (Input::Refresh, "Refresh"),
            (Input::Destroy, "Destroy"),
            (Input::Tick, "Tick"),
        ];
        for (input, start) in inputs {
            let text = format!("{input:?}");
            assert!(text.starts_with(start), "{text}");
            assert!(!text.contains(SECRET), "{text}");
        }
        let spec = RequestSpec {
            exchange: ExchangeId(1),
            target: "/p?q=secret-value".to_owned(),
            fields: fields.clone(),
        };
        assert_eq!(
            format!("{spec:?}"),
            "RequestSpec { exchange: ExchangeId(1), target_len: 17, fields: 1 }"
        );
        let head = DocumentHead {
            status: 200,
            fields: fields.clone(),
        };
        assert_eq!(
            format!("{head:?}"),
            "DocumentHead { status: 200, fields: 1 }"
        );
        let effects = [
            (
                Effect::Lookup {
                    partition: Partition::Public,
                    key: CacheKey::from_bytes([2; 32]),
                },
                "Lookup { partition: Public, key: ",
            ),
            (Effect::ReadCookie, "ReadCookie"),
            (Effect::Send(spec), "Send(RequestSpec {"),
            (
                Effect::StoreCookies(vec![SECRET.to_owned()]),
                "StoreCookies(1)",
            ),
            (Effect::LoadUrl, "LoadUrl"),
            (
                Effect::LoadData {
                    head: head.clone(),
                    body: SECRET.as_bytes().to_vec(),
                },
                "LoadData { head: DocumentHead { status: 200, fields: 1 }, len: 12 }",
            ),
            (
                Effect::AnswerNull(InterceptId(4)),
                "AnswerNull(InterceptId(4))",
            ),
            (
                Effect::AnswerDocument {
                    intercept: InterceptId(4),
                    head,
                },
                "AnswerDocument { intercept: InterceptId(4), head: DocumentHead { status: 200, fields: 1 } }",
            ),
            (
                Effect::StreamChunk {
                    intercept: InterceptId(4),
                    bytes: SECRET.as_bytes().to_vec(),
                },
                "StreamChunk { intercept: InterceptId(4), len: 12 }",
            ),
            (
                Effect::StreamEnd {
                    intercept: InterceptId(4),
                    complete: false,
                },
                "StreamEnd { intercept: InterceptId(4), complete: false }",
            ),
            (Effect::Message(SECRET.to_owned()), "Message(12)"),
            (
                Effect::Result {
                    src: ResultCode::FirstLoad,
                    final_code: ResultCode::NotModified,
                },
                "Result { src: FirstLoad, final_code: NotModified }",
            ),
            (
                Effect::ScheduleTimer(Instant(7)),
                "ScheduleTimer(Instant(7))",
            ),
            (Effect::Closed, "Closed"),
        ];
        for (effect, start) in effects {
            let text = format!("{effect:?}");
            assert!(text.starts_with(start), "{text}");
            assert!(!text.contains(SECRET), "{text}");
        }
    }

    /// `Effect::Cache` names the kind of its transaction only.
    #[test]
    fn session_debug_of_a_cache_effect_names_the_kind() {
        let partition = Partition::Scoped(PartitionId::from_bytes([1; 32]));
        let key = CacheKey::from_bytes([2; 32]);
        let page = "<title>secret-value</title>";
        let record = NewRecord {
            host: "a.example",
            vary: &[],
            page,
            template: "{title}",
            data: &[],
            fields: &[],
            etag: sha1_hex(page.as_bytes()),
            template_tag: sha1_hex(b"{title}"),
            response_fields: &[],
            request_time: UnixTime(1),
            response_time: UnixTime(2),
        };
        let cases = [
            (
                Effect::Cache(Box::new(commit(&basis(), &record))),
                "Cache(Replace)",
            ),
            (txn(TxnOp::Remove { partition, key }), "Cache(Remove)"),
            (
                txn(TxnOp::DisableKey {
                    partition,
                    key,
                    until: UnixTime(9),
                }),
                "Cache(DisableKey)",
            ),
            (
                txn(TxnOp::RevokePartition { partition }),
                "Cache(RevokePartition)",
            ),
            (
                txn(TxnOp::ClearSite {
                    domain: Some(SECRET.to_owned()),
                }),
                "Cache(ClearSite)",
            ),
        ];
        for (effect, text) in cases {
            assert_eq!(format!("{effect:?}"), text);
        }
    }
}
