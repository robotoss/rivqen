// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `rivqen_proto::compute` against the reference values: the known answers of
//! `spec/rqp/examples/kat/`, the server fixtures `FX-RQ-P*` (every `full`
//! manifest and every `delta` body) and the client fixtures `FX-RQ-C*` with a
//! delta body whose outcome depends only on steps 3 to 7 of manifest-patch
//! section 2.4.
//!
//! The fixtures name pages, not snapshots. [`analyze`] is a test oracle for
//! the simple block markup of these pages (one element per block, no element
//! of the same name inside it); it is not the markup analysis of WP-06. Each
//! use checks its result against a value of the fixture (the page revision of
//! the manifest, the base revision of a binding, the hash of a changed block),
//! so a wrong extraction fails the test and never passes it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::fs;
use std::path::{Path, PathBuf};

use rivqen_integrity::{
    Binding, PageRevision, Sha256Digest, TemplateRevision, b64u_encode, sha256, sha256_parts,
};
use rivqen_proto::compute::{
    OperationError, binding, next_blocks, next_page_revision, page_revision, policy_line,
    snapshot_page_revision,
};
use rivqen_proto::types::{
    BlockDescriptor, BlockFormat, BlockId, Limits, Mode, NONCE_BYTES, Nonce, PatchOp, Policy,
    SnapshotMeta,
};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Helpers

fn repo_root() -> PathBuf {
    rivqen_testkit::fixtures_root().join("..")
}

fn protocol_root() -> PathBuf {
    rivqen_testkit::fixtures_root().join("rqp").join("protocol")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_slice(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn text<'a>(value: &'a Value, name: &str) -> &'a str {
    value[name]
        .as_str()
        .unwrap_or_else(|| panic!("member {name} is not a string"))
}

fn u32_of(value: &Value) -> u32 {
    u32::try_from(value.as_u64().expect("an integer")).expect("a u32")
}

/// `policy` and `limits` of a manifest, a case or a snapshot, with member
/// presence kept: a member that is absent stays `None`.
fn members(object: &Value) -> (Option<Policy>, Option<Limits>) {
    let policy = object.get("policy").map(|p| {
        let p = p.as_object().expect("policy is an object");
        Policy {
            mode: p.get("mode").map(|m| match m.as_str() {
                Some("private") => Mode::Private,
                Some("public") => Mode::Public,
                other => panic!("mode {other:?}"),
            }),
            ttl_seconds: p.get("ttl_seconds").map(u32_of),
            offline: p.get("offline").map(|o| o.as_bool().expect("a boolean")),
        }
    });
    let limits = object.get("limits").map(|l| {
        let l = l.as_object().expect("limits is an object");
        Limits {
            max_patch_bytes: l.get("max_patch_bytes").map(u32_of),
        }
    });
    (policy, limits)
}

fn format_of(text: &str) -> BlockFormat {
    match text {
        "html" => BlockFormat::Html,
        "json" => BlockFormat::Json,
        other => panic!("format {other}"),
    }
}

fn descriptor(block: &Value) -> BlockDescriptor {
    BlockDescriptor {
        id: BlockId::parse(text(block, "id")).unwrap(),
        format: format_of(text(block, "format")),
        sha256: Sha256Digest::from_b64u(text(block, "sha256")).unwrap(),
    }
}

/// A snapshot from a JSON object with `template_revision`, `page_revision`,
/// `blocks` and the optional `policy` and `limits`.
fn snapshot_of(object: &Value) -> SnapshotMeta {
    let (policy, limits) = members(object);
    SnapshotMeta {
        template_revision: TemplateRevision::parse(text(object, "template_revision")).unwrap(),
        page_revision: PageRevision::parse(text(object, "page_revision")).unwrap(),
        blocks: object["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(descriptor)
            .collect(),
        policy,
        limits,
    }
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

/// The 16 nonce bytes of the text N (22 base64url characters, no padding).
fn nonce_of(n: &str) -> Nonce {
    let mut bits: u32 = 0;
    let mut count = 0;
    let mut bytes = Vec::with_capacity(NONCE_BYTES);
    for c in n.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => panic!("nonce text {n}"),
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push(u8::try_from((bits >> count) & 0xFF).unwrap());
        }
    }
    let nonce = Nonce(bytes.try_into().expect("16 bytes"));
    // The decoder is right when the encoder of the crate writes N again.
    assert_eq!(b64u_encode(&nonce.0), n);
    nonce
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

fn read_page(name: &str) -> Vec<u8> {
    let path = protocol_root().join("pages").join(format!("{name}.html"));
    fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn snapshot_of_page(page: &str, members_of: &Value) -> (SnapshotMeta, Analysis) {
    let analysis = analyze(&read_page(page));
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

/// Every fixture directory with the given name prefix, in name order.
fn fixture_dirs(prefix: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(protocol_root())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix))
        })
        .collect();
    dirs.sort();
    dirs
}

fn name_of(dir: &Path) -> String {
    dir.file_name().unwrap().to_string_lossy().into_owned()
}

/// Server fixtures whose expected body has the given kind.
fn server_fixtures(kind: &str) -> Vec<(String, Value, Value)> {
    fixture_dirs("FX-RQ-P")
        .into_iter()
        .filter_map(|dir| {
            let expected = read_json(&dir.join("expected.json"));
            (expected["body"]["kind"] == kind)
                .then(|| (name_of(&dir), read_json(&dir.join("case.json")), expected))
        })
        .collect()
}

fn kat_root() -> PathBuf {
    repo_root()
        .join("spec")
        .join("rqp")
        .join("examples")
        .join("kat")
}

fn kat() -> Value {
    read_json(&kat_root().join("kat.json"))
}

fn kat_file(relative: &str) -> Value {
    read_json(&kat_root().join(relative))
}

// ---------------------------------------------------------------------------
// Known answers (spec/rqp/examples/kat)

#[test]
fn compute_page_revision_kat() {
    let kat = kat();
    let computed = &kat["computed"];
    let page = fs::read(kat_root().join(text(&kat, "document"))).unwrap();
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
            let next = next_page_revision(&snapshot, &operations).unwrap();
            assert_eq!(next.to_string(), text(entry, "next_revision"));
            assert_eq!(
                snapshot.page_revision.to_string(),
                text(entry, "base_revision")
            );
            let value = binding(
                text(entry, "url"),
                &snapshot.page_revision,
                &nonce_of(text(entry, "nonce")),
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
    assert_eq!(nonce_of(text(&kat["nonce"], "n")).0, {
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
            &nonce_of(text(entry, "nonce")),
            &PageRevision::parse(text(entry, "next_revision")).unwrap(),
        );
        assert_eq!(value.to_string(), text(entry, "binding"));
        // The preimage of the KAT is the one of section 2.2.
        let preimage = text(entry, "preimage");
        assert_eq!(value, Binding::from_digest(sha256(preimage.as_bytes())));
    }
}

// ---------------------------------------------------------------------------
// Server fixtures FX-RQ-P*

#[test]
fn compute_page_revision_server_fixtures() {
    let fixtures = server_fixtures("full");
    assert_ne!(fixtures.len(), 0);
    for (name, case, expected) in &fixtures {
        let manifest = &expected["body"]["manifest"];
        let (snapshot, _) = snapshot_of_page(text(case, "page"), manifest);
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
    for (name, _, expected) in &fixtures {
        let body = &expected["body"];
        let inputs = &body["binding_inputs"];
        let value = binding(
            text(inputs, "url"),
            &PageRevision::parse(text(inputs, "base_revision")).unwrap(),
            &nonce_of(text(inputs, "nonce")),
            &PageRevision::parse(text(inputs, "next_revision")).unwrap(),
        );
        assert_eq!(value.to_string(), text(body, "binding"), "{name}");
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
    for (name, case, expected) in &fixtures {
        let body = &expected["body"];
        let inputs = &body["binding_inputs"];
        let client_members = case.get("client_members").unwrap_or(&none);
        let (snapshot, _) = snapshot_of_page(text(case, "client_page"), client_members);
        assert_eq!(
            snapshot.page_revision.to_string(),
            text(inputs, "base_revision"),
            "{name}: snapshot"
        );
        let (current, analysis) = snapshot_of_page(
            text(case, "page"),
            case.get("server_config").unwrap_or(&none),
        );
        assert_eq!(
            current.page_revision.to_string(),
            text(inputs, "next_revision"),
            "{name}: current page"
        );
        let operations: Vec<PatchOp> = body["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|changed| {
                let id = text(changed, "id");
                let block = analysis
                    .blocks
                    .iter()
                    .find(|b| b.descriptor.id.as_str() == id)
                    .unwrap_or_else(|| panic!("{name}: no block {id}"));
                assert_eq!(
                    block.descriptor.sha256.to_b64u(),
                    text(changed, "sha256"),
                    "{name}: {id}"
                );
                PatchOp {
                    block_id: block.descriptor.id.clone(),
                    content: String::from_utf8(block.content.clone()).unwrap(),
                }
            })
            .collect();
        let next = next_page_revision(&snapshot, &operations).unwrap();
        assert_eq!(next.to_string(), text(inputs, "next_revision"), "{name}");
        // L′ is the block list of the current page.
        assert_eq!(
            next_blocks(&snapshot, &operations).unwrap(),
            current.blocks,
            "{name}"
        );
    }
}

// ---------------------------------------------------------------------------
// Client fixtures FX-RQ-C* with a delta body

/// Client fixtures whose final URL is not in the canonical form; U needs the
/// URL parser of WP-08.
const NOT_CANONICAL_FINAL_URL: &[&str] = &["FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-REDIRECT-HOST-CASE"];

/// The fixture results that depend on steps 3 to 7 only, and the result of
/// those steps: the binding is equal, the binding differs, or step 3 fails.
enum StepResult {
    BindingEqual,
    BindingDiffers,
    Operations(OperationError),
}

fn step_result(name: &str, result: &Value) -> Option<StepResult> {
    match (name, result["outcome"].as_str(), result["error"].as_str()) {
        // Steps 8 and 9 come after the binding check: their fixtures pass step 7.
        (_, Some("patched"), _) | (_, _, Some("PATCH_CONTENT_REJECTED")) => {
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
    let mut not_canonical = Vec::new();
    for dir in fixture_dirs("FX-RQ-C") {
        let name = name_of(&dir);
        let expected = read_json(&dir.join("expected.json"));
        let delta = expected["response"]["headers"]
            .as_array()
            .is_some_and(|headers| {
                headers
                    .iter()
                    .any(|h| h[0] == "content-type" && h[1] == "application/vnd.rivqen.delta+json")
            });
        let context = &expected["context"];
        if !delta || context["snapshot"].is_null() {
            continue;
        }
        let Some(step) = step_result(&name, &expected["result"]) else {
            continue;
        };
        let final_url = text(context, "final_url");
        if final_url.bytes().any(|b| b.is_ascii_uppercase()) {
            not_canonical.push(name);
            continue;
        }
        let snapshot = snapshot_of(&context["snapshot"]);
        assert_eq!(
            snapshot_page_revision(&snapshot),
            snapshot.page_revision,
            "{name}: snapshot"
        );
        let envelope = read_json(&dir.join(text(&expected["response"], "body")));
        let operations = operations_of(&envelope);
        let next = next_page_revision(&snapshot, &operations);
        let computed = next.map(|next| {
            binding(
                final_url,
                &snapshot.page_revision,
                &nonce_of(text(context, "nonce")),
                &next,
            )
        });
        let sent = Binding::parse(text(&envelope, "binding")).unwrap();
        match step {
            StepResult::BindingEqual => assert_eq!(computed, Ok(sent), "{name}"),
            StepResult::BindingDiffers => {
                assert_ne!(computed.expect(&name), sent, "{name}");
            }
            StepResult::Operations(error) => assert_eq!(computed, Err(error), "{name}"),
        }
        checked += 1;
    }
    assert_eq!(not_canonical, NOT_CANONICAL_FINAL_URL);
    assert!(checked > 0);
}
