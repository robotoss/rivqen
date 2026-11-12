// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `rivqen_proto::compute` against the reference values: the known answers of
//! `spec/rqp/examples/kat/`, the server fixtures `FX-RQ-P*` (every `full`
//! manifest and every `delta` body) and the client fixtures `FX-RQ-C*` with a
//! delta body whose outcome depends only on steps 3 to 7 of manifest-patch
//! section 2.4.
//!
//! The test kit reads every file (DL-146); `common` converts its data to the
//! core types. The fixtures name pages, not snapshots. [`analyze`] is a test
//! oracle for the simple block markup of these pages (one element per block,
//! no element of the same name inside it); it is not the markup analysis of
//! WP-06. Each use checks its result against a value of the fixture (the page
//! revision of the manifest, the base revision of a binding, the hash of a
//! changed block), so a wrong extraction fails the test and never passes it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod common;

use std::path::PathBuf;

use rivqen_integrity::{Binding, PageRevision, TemplateRevision, sha256, sha256_parts};
use rivqen_proto::compute::{
    OperationError, binding, next_blocks, next_page_revision, page_revision, policy_line,
    snapshot_page_revision,
};
use rivqen_proto::json;
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, MAX_BLOCK_BYTES, NONCE_BYTES, PatchOp, SnapshotMeta,
};
use rivqen_testkit::fixtures::{
    BlockData, ExpectedBody, Fixture, Members, SnapshotData, load_json,
};
use serde_json::Value;

use common::{canonical_url, nonce, protocol, snapshot_meta};

// ---------------------------------------------------------------------------
// Helpers

fn text<'a>(value: &'a Value, name: &str) -> &'a str {
    value[name]
        .as_str()
        .unwrap_or_else(|| panic!("member {name} is not a string"))
}

/// `policy` and `limits` of a manifest, a case or a snapshot, with member
/// presence kept.
fn members(
    object: &Value,
) -> (
    Option<rivqen_proto::types::Policy>,
    Option<rivqen_proto::types::Limits>,
) {
    common::members(&Members::from_json(object).unwrap_or_else(|e| panic!("{e}")))
}

fn descriptor(block: &Value) -> BlockDescriptor {
    common::descriptor(&BlockData {
        id: text(block, "id").to_owned(),
        format: text(block, "format").to_owned(),
        sha256: text(block, "sha256").to_owned(),
    })
}

/// A snapshot from a JSON object with `template_revision`, `page_revision`,
/// `blocks` and the optional `policy` and `limits`.
fn snapshot_of(object: &Value) -> SnapshotMeta {
    snapshot_meta(&SnapshotData::from_json(object).unwrap_or_else(|e| panic!("{e}")))
}

fn operations_of(envelope: &Value) -> Vec<PatchOp> {
    envelope["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|op| PatchOp {
            block_id: BlockId::parse(text(op, "block_id")).unwrap(),
            content: text(op, "content").to_owned(),
        })
        .collect()
}

/// `binding` and the operations of a delta body, read with the JSON reader
/// of the crate and without the schema (an empty list stays empty).
fn envelope_parts(body: &[u8]) -> (Binding, Vec<PatchOp>) {
    let json::Value::Object(members) = json::parse(body).unwrap() else {
        panic!("envelope is not an object");
    };
    let member = |name: &str| {
        &members
            .iter()
            .find(|m| m.name == name)
            .unwrap_or_else(|| panic!("no member {name}"))
            .value
    };
    let string = |value: &json::Value| match value {
        json::Value::String(s) => s.clone(),
        other => panic!("not a string: {other:?}"),
    };
    let json::Value::Array(operations) = member("operations") else {
        panic!("operations is not an array");
    };
    let operations = operations
        .iter()
        .map(|op| {
            let json::Value::Object(fields) = op else {
                panic!("operation is not an object");
            };
            let field = |name: &str| {
                string(
                    &fields
                        .iter()
                        .find(|m| m.name == name)
                        .unwrap_or_else(|| panic!("no member {name}"))
                        .value,
                )
            };
            PatchOp {
                block_id: BlockId::parse(&field("block_id")).unwrap(),
                content: field("content"),
            }
        })
        .collect();
    (
        Binding::parse(&string(member("binding"))).unwrap(),
        operations,
    )
}

/// One block of a page: descriptor and content bytes.
struct Block {
    descriptor: BlockDescriptor,
    content: Vec<u8>,
}

/// Template revision and blocks of a page (manifest-patch section 1.3, steps
/// 1 to 3) for the simple block markup of the fixture pages.
struct Analysis {
    template_revision: TemplateRevision,
    blocks: Vec<Block>,
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

fn analyze(page: &[u8]) -> Analysis {
    const MARK: &[u8] = b"data-rq-block=\"";
    let mut template = Vec::with_capacity(page.len());
    let mut blocks = Vec::new();
    let mut copied = 0;
    let mut at = 0;
    while let Some(mark) = find(page, MARK, at) {
        let tag_start = page[..mark].iter().rposition(|&b| b == b'<').unwrap();
        let name_end = page[tag_start..]
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap()
            + tag_start;
        let name = &page[tag_start + 1..name_end];
        let id_start = mark + MARK.len();
        let id_end = find(page, b"\"", id_start).unwrap();
        let start_tag_end = find(page, b">", id_end).unwrap();
        let start = start_tag_end + 1;
        let mut end_tag = b"</".to_vec();
        end_tag.extend_from_slice(name);
        let end = find(page, &end_tag, start).unwrap();
        let start_tag = &page[tag_start..start];
        let json = find(start_tag, b"type=\"application/json\"", 0).is_some()
            || find(start_tag, b"type=\"application/ld+json\"", 0).is_some();
        let content = page[start..end].to_vec();
        blocks.push(Block {
            descriptor: BlockDescriptor {
                id: BlockId::parse(core::str::from_utf8(&page[id_start..id_end]).unwrap()).unwrap(),
                format: if json {
                    BlockFormat::Json
                } else {
                    BlockFormat::Html
                },
                sha256: sha256(&content),
            },
            content,
        });
        template.extend_from_slice(&page[copied..start]);
        copied = end;
        at = end;
    }
    template.extend_from_slice(&page[copied..]);
    Analysis {
        template_revision: TemplateRevision::from_digest(sha256_parts(&[b"rqp-t1\n", &template])),
        blocks,
    }
}

fn snapshot_of_page(page: &[u8], members_of: &Value) -> (SnapshotMeta, Analysis) {
    let analysis = analyze(page);
    let (policy, limits) = members(members_of);
    let blocks: Vec<BlockDescriptor> = analysis
        .blocks
        .iter()
        .map(|b| b.descriptor.clone())
        .collect();
    let revision = page_revision(
        &analysis.template_revision,
        &blocks,
        policy.as_ref(),
        limits.as_ref(),
    );
    let snapshot = SnapshotMeta {
        template_revision: analysis.template_revision,
        page_revision: revision,
        blocks,
        policy,
        limits,
    };
    (snapshot, analysis)
}

/// Server fixtures whose expected body has the given kind.
fn server_fixtures(kind: &str) -> Vec<&'static Fixture> {
    protocol()
        .server
        .iter()
        .filter(|f| {
            matches!(
                (kind, f.server_body().unwrap()),
                ("full", ExpectedBody::Full { .. }) | ("delta", ExpectedBody::Delta(_))
            )
        })
        .collect()
}

fn kat_root() -> PathBuf {
    rivqen_testkit::repo_root()
        .join("spec")
        .join("rqp")
        .join("examples")
        .join("kat")
}

fn kat_file(relative: &str) -> Value {
    load_json(&kat_root().join(relative)).unwrap_or_else(|e| panic!("{e}"))
}

fn kat() -> Value {
    kat_file("kat.json")
}

// ---------------------------------------------------------------------------
// Known answers (spec/rqp/examples/kat)

#[test]
fn compute_page_revision_kat() {
    let kat = kat();
    let computed = &kat["computed"];
    let page = std::fs::read(kat_root().join(text(&kat, "document"))).unwrap();
    let analysis = analyze(&page);
    assert_eq!(
        analysis.template_revision.to_string(),
        text(computed, "template_revision")
    );
    let expected_blocks: Vec<BlockDescriptor> = computed["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(descriptor)
        .collect();
    let blocks: Vec<BlockDescriptor> = analysis
        .blocks
        .iter()
        .map(|b| b.descriptor.clone())
        .collect();
    assert_eq!(blocks, expected_blocks);

    for (manifest_member, expected_line) in [
        (&kat["manifest"], None),
        (
            &kat["with_policy_limits"]["manifest"],
            kat["with_policy_limits"]["policy_line"].as_str(),
        ),
    ] {
        let manifest = kat_file(manifest_member.as_str().unwrap());
        let (policy, limits) = members(&manifest);
        assert_eq!(
            policy_line(policy.as_ref(), limits.as_ref()).as_deref(),
            expected_line
        );
        let snapshot = SnapshotMeta {
            template_revision: TemplateRevision::parse(text(computed, "template_revision"))
                .unwrap(),
            page_revision: PageRevision::parse(text(&manifest, "page_revision")).unwrap(),
            blocks: expected_blocks.clone(),
            policy,
            limits,
        };
        assert_eq!(snapshot_page_revision(&snapshot), snapshot.page_revision);
        assert_eq!(
            page_revision(
                &snapshot.template_revision,
                &snapshot.blocks,
                policy.as_ref(),
                limits.as_ref()
            ),
            snapshot.page_revision
        );
    }
}

/// The KAT bindings: next revision and binding of each patch example.
#[test]
fn compute_next_revision_kat() {
    let kat = kat();
    let computed = &kat["computed"];
    let sets = [
        (&kat["manifest"], &kat["bindings"]),
        (
            &kat["with_policy_limits"]["manifest"],
            &kat["with_policy_limits"]["bindings"],
        ),
    ];
    let mut checked = 0;
    for (manifest_member, bindings) in sets {
        let manifest = kat_file(manifest_member.as_str().unwrap());
        let mut snapshot_json = computed.clone();
        snapshot_json["page_revision"] = manifest["page_revision"].clone();
        for member in ["policy", "limits"] {
            if let Some(value) = manifest.get(member) {
                snapshot_json[member] = value.clone();
            }
        }
        let snapshot = snapshot_of(&snapshot_json);
        assert_eq!(snapshot_page_revision(&snapshot), snapshot.page_revision);
        for entry in bindings.as_array().unwrap() {
            let patch = kat_file(text(entry, "patch"));
            let operations = operations_of(&patch);
            let next = next_page_revision(&snapshot, &operations, MAX_BLOCK_BYTES).unwrap();
            assert_eq!(next.to_string(), text(entry, "next_revision"));
            assert_eq!(
                snapshot.page_revision.to_string(),
                text(entry, "base_revision")
            );
            let value = binding(
                text(entry, "url"),
                &snapshot.page_revision,
                &nonce(text(entry, "nonce")),
                &next,
            );
            assert_eq!(value.to_string(), text(entry, "binding"));
            assert_eq!(value, Binding::parse(text(&patch, "binding")).unwrap());
            checked += 1;
        }
    }
    assert_eq!(checked, 3);
}

#[test]
fn compute_binding_kat() {
    let kat = kat();
    assert_eq!(nonce(text(&kat["nonce"], "n")).0, {
        let mut bytes = [0_u8; NONCE_BYTES];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::try_from(i).unwrap();
        }
        bytes
    });
    let entries = kat["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .chain(kat["with_policy_limits"]["bindings"].as_array().unwrap())
        .chain([&kat["binding_with_query"]]);
    for entry in entries {
        let value = binding(
            text(entry, "url"),
            &PageRevision::parse(text(entry, "base_revision")).unwrap(),
            &nonce(text(entry, "nonce")),
            &PageRevision::parse(text(entry, "next_revision")).unwrap(),
        );
        assert_eq!(value.to_string(), text(entry, "binding"));
        // The preimage of the KAT is the one of section 2.2.
        let preimage = text(entry, "preimage");
        assert_eq!(value, Binding::from_digest(sha256(preimage.as_bytes())));
    }
}

/// The test helper for U gives the KAT value (manifest-patch section 2.2,
/// "The canonical URL U").
#[test]
fn compute_canonical_url_helper_matches_kat() {
    let kat = kat();
    let entry = &kat["binding_with_query"];
    assert_eq!(canonical_url(text(entry, "url_input")), text(entry, "url"));
    assert_eq!(canonical_url(text(entry, "url")), text(entry, "url"));
    assert_eq!(
        canonical_url("http://Example.COM:80?q=1#f"),
        "http://example.com/?q=1"
    );
    assert_eq!(
        canonical_url("https://example.com:8443/a"),
        "https://example.com:8443/a"
    );
}

// ---------------------------------------------------------------------------
// Server fixtures FX-RQ-P*

#[test]
fn compute_page_revision_server_fixtures() {
    let fixtures = server_fixtures("full");
    assert_ne!(fixtures.len(), 0);
    for fixture in &fixtures {
        let name = &fixture.id;
        let manifest = &fixture.expected["body"]["manifest"];
        let (snapshot, _) = snapshot_of_page(fixture.page.as_deref().unwrap(), manifest);
        assert_eq!(
            snapshot.page_revision.to_string(),
            text(manifest, "page_revision"),
            "{name}"
        );
    }
}

#[test]
fn compute_binding_matches_server_fixtures() {
    let fixtures = server_fixtures("delta");
    assert_ne!(fixtures.len(), 0);
    for fixture in &fixtures {
        let ExpectedBody::Delta(delta) = fixture.server_body().unwrap() else {
            panic!("{}: not a delta", fixture.id);
        };
        let inputs = &delta.inputs;
        let value = binding(
            &inputs.url,
            &PageRevision::parse(&inputs.base_revision).unwrap(),
            &nonce(&inputs.nonce),
            &PageRevision::parse(&inputs.next_revision).unwrap(),
        );
        assert_eq!(value.to_string(), delta.binding, "{}", fixture.id);
    }
}

/// For each delta fixture: the snapshot of `client_page` (checked through B),
/// the operations of the changed blocks with their content from `page`
/// (checked through the hashes of the fixture) give R′ of `binding_inputs`.
#[test]
fn compute_next_revision_server_fixtures() {
    let fixtures = server_fixtures("delta");
    assert_ne!(fixtures.len(), 0);
    let none = Value::Object(serde_json::Map::new());
    for fixture in &fixtures {
        let name = &fixture.id;
        let case = &fixture.case;
        let ExpectedBody::Delta(delta) = fixture.server_body().unwrap() else {
            panic!("{name}: not a delta");
        };
        let inputs = &delta.inputs;
        let client_members = case.get("client_members").unwrap_or(&none);
        let (snapshot, _) =
            snapshot_of_page(fixture.client_page.as_deref().unwrap(), client_members);
        assert_eq!(
            snapshot.page_revision.to_string(),
            inputs.base_revision,
            "{name}: snapshot"
        );
        let (current, analysis) = snapshot_of_page(
            fixture.page.as_deref().unwrap(),
            case.get("server_config").unwrap_or(&none),
        );
        assert_eq!(
            current.page_revision.to_string(),
            inputs.next_revision,
            "{name}: current page"
        );
        let operations: Vec<PatchOp> = delta
            .blocks
            .iter()
            .map(|changed| {
                let block = analysis
                    .blocks
                    .iter()
                    .find(|b| b.descriptor.id.as_str() == changed.id)
                    .unwrap_or_else(|| panic!("{name}: no block {}", changed.id));
                assert_eq!(
                    block.descriptor.sha256.to_b64u(),
                    changed.sha256,
                    "{name}: {}",
                    changed.id
                );
                PatchOp {
                    block_id: block.descriptor.id.clone(),
                    content: String::from_utf8(block.content.clone()).unwrap(),
                }
            })
            .collect();
        let next = next_page_revision(&snapshot, &operations, MAX_BLOCK_BYTES).unwrap();
        assert_eq!(next.to_string(), inputs.next_revision, "{name}");
        // L′ is the block list of the current page.
        assert_eq!(
            next_blocks(&snapshot, &operations, MAX_BLOCK_BYTES).unwrap(),
            current.blocks,
            "{name}"
        );
    }
}

// ---------------------------------------------------------------------------
// Client fixtures FX-RQ-C* with a delta body

/// The fixture results that depend on steps 3 to 7 only, and the result of
/// those steps: the binding is equal, the binding differs, or step 3 fails.
enum StepResult {
    BindingEqual,
    BindingDiffers,
    Operations(OperationError),
}

fn step_result(name: &str, outcome: &str, error: Option<&str>) -> Option<StepResult> {
    match (name, outcome, error) {
        // Steps 8 and 9 come after the binding check: their fixtures pass step 7.
        (_, "patched", _) | (_, _, Some("PATCH_CONTENT_REJECTED")) => {
            Some(StepResult::BindingEqual)
        }
        (_, _, Some("INTEGRITY_ERROR")) => Some(StepResult::BindingDiffers),
        ("FX-RQ-C30-DELTA-ORDER", ..) => {
            Some(StepResult::Operations(OperationError::NotDocumentOrder {
                index: 1,
            }))
        }
        ("FX-RQ-C30-DELTA-DUPLICATE-BLOCK", ..) => {
            Some(StepResult::Operations(OperationError::DuplicateBlock {
                index: 1,
            }))
        }
        ("FX-RQ-C30-DELTA-UNKNOWN-BLOCK", ..) => {
            Some(StepResult::Operations(OperationError::UnknownBlock {
                index: 0,
            }))
        }
        ("FX-RQ-C30-DELTA-EMPTY-OPERATIONS", ..) => {
            Some(StepResult::Operations(OperationError::NoOperations))
        }
        // Transport, size, JSON and schema checks come before step 3.
        _ => None,
    }
}

#[test]
fn compute_next_revision_client_fixtures() {
    let mut checked = 0;
    for fixture in &protocol().client {
        let name = &fixture.id;
        let response = fixture.client_response().unwrap();
        let delta = response
            .headers
            .iter()
            .any(|h| h.is("content-type") && h.value == rivqen_proto::fields::DELTA_MEDIA_TYPE);
        let context = fixture.client_context_data().unwrap();
        let Some(data) = context.snapshot.as_ref().filter(|_| delta) else {
            continue;
        };
        let result = fixture.client_result().unwrap();
        let Some(step) = step_result(name, &result.outcome, result.error.as_deref()) else {
            continue;
        };
        let snapshot = snapshot_meta(data);
        assert_eq!(
            snapshot_page_revision(&snapshot),
            snapshot.page_revision,
            "{name}: snapshot"
        );
        let (sent, operations) = envelope_parts(fixture.response_body.as_deref().unwrap());
        let next = next_page_revision(&snapshot, &operations, MAX_BLOCK_BYTES);
        let computed = next.map(|next| {
            binding(
                &canonical_url(&context.final_url),
                &snapshot.page_revision,
                &nonce(context.nonce.as_deref().unwrap()),
                &next,
            )
        });
        match step {
            StepResult::BindingEqual => assert_eq!(computed, Ok(sent), "{name}"),
            StepResult::BindingDiffers => {
                assert_ne!(computed.expect(name), sent, "{name}");
            }
            StepResult::Operations(error) => assert_eq!(computed, Err(error), "{name}"),
        }
        checked += 1;
    }
    // 26 patched, 34 PATCH_CONTENT_REJECTED and 13 INTEGRITY_ERROR binding
    // comparisons and 4 operation failures. S1 ran 76 of these 77: the
    // host-case fixture needs U from common::canonical_url.
    assert_eq!(checked, 77);
}
