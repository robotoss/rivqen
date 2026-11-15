// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Loader of the cache policy matrix `fixtures/cache/policy` (WP-07 S1,
//! T-04; DL-182), as data (DL-146).
//!
//! Owner: T-04. The row format is in `fixtures/cache/policy/README.md`.
//!
//! The matrix has exactly four files, one for each [`Kind`]: `lookup.json`,
//! `failure.json`, `store.json` and `legacy.json`. Each file is a JSON array
//! of rows. [`load_at`] reads the four files in the order of [`Kind::ALL`]
//! and gives typed rows. It is strict, so that a row cannot drop out of a
//! test unseen:
//!
//! - another `.json` file in the directory is an error (no kind runs it);
//! - a row of another kind, a member that the README does not define for the
//!   kind, and a missing member are errors;
//! - the ids follow `POL-<KIND>-<NNN>` and come in strictly ascending byte
//!   order in each file (so they are unique);
//! - `source` is an array of at least one string that is not empty.
//!
//! Every failure is a [`FixtureError::Shape`] (or `Io`, `Json`) that names the
//! file, and for a row also the row id (or the row index when the row has no
//! id).

use std::fs;
use std::path::{Path, PathBuf};

use rivqen_cache::partition::Credentials;
use rivqen_model::http::FieldLine;
use rivqen_model::time::UnixTime;
use serde_json::{Map, Value};

use crate::fixtures::{FixtureError, load_json};
use crate::fixtures_root;

/// The largest `ttl_seconds` of a manifest policy: one year (manifest and
/// patch section 1.4).
pub const MAX_TTL_SECONDS: u64 = 31_536_000;

/// The kind of a row; each kind has its own file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// `lookup.json`: freshness lifetime, current age and the use of an entry.
    Lookup,
    /// `failure.json`: a stale entry after a network or TLS error.
    Failure,
    /// `store.json`: store or not.
    Store,
    /// `legacy.json`: legacy `cache-offline`.
    Legacy,
}

impl Kind {
    /// The four kinds in the order of the README table.
    pub const ALL: [Self; 4] = [Self::Lookup, Self::Failure, Self::Store, Self::Legacy];

    /// The value of the member `kind`, for example `lookup`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lookup => "lookup",
            Self::Failure => "failure",
            Self::Store => "store",
            Self::Legacy => "legacy",
        }
    }

    /// The file name, for example `lookup.json`.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Lookup => "lookup.json",
            Self::Failure => "failure.json",
            Self::Store => "store.json",
            Self::Legacy => "legacy.json",
        }
    }

    /// The prefix of a row id, for example `POL-LOOKUP-`.
    #[must_use]
    pub const fn id_prefix(self) -> &'static str {
        match self {
            Self::Lookup => "POL-LOOKUP-",
            Self::Failure => "POL-FAILURE-",
            Self::Store => "POL-STORE-",
            Self::Legacy => "POL-LEGACY-",
        }
    }

    /// The members of a row of this kind (README, table "Row").
    const fn members(self) -> &'static [&'static str] {
        match self {
            Self::Lookup => &[
                "id",
                "source",
                "note",
                "kind",
                "response_fields",
                "request_time",
                "response_time",
                "now",
                "manifest",
                "expect",
            ],
            Self::Failure => &[
                "id",
                "source",
                "note",
                "kind",
                "response_fields",
                "request_time",
                "response_time",
                "now",
                "manifest",
                "failure",
                "sensitive",
                "expect",
            ],
            Self::Store => &[
                "id",
                "source",
                "note",
                "kind",
                "response_fields",
                "request_fields",
                "status",
                "credentials",
                "authenticated_cache",
                "expect",
            ],
            Self::Legacy => &[
                "id",
                "source",
                "note",
                "kind",
                "response_fields",
                "request_fields",
                "status",
                "credentials",
                "authenticated_cache",
                "cache_offline",
                "unavailable_time_ms",
                "expect",
            ],
        }
    }
}

/// One file of the matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyFile {
    /// The path of the file.
    pub path: PathBuf,
    /// The kind of every row of the file.
    pub kind: Kind,
    /// The rows in file order (ascending ids).
    pub rows: Vec<PolicyRow>,
}

/// One row: one case and its expected result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRow {
    /// `id`, for example `POL-LOOKUP-001`.
    pub id: String,
    /// `source`: the sections that decide the expected value.
    pub source: Vec<String>,
    /// `note`: what the row shows.
    pub note: String,
    /// The inputs and the expected result of the kind.
    pub case: Case,
}

/// The inputs and the expected result of a row, by kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Case {
    /// A row of `lookup.json`.
    Lookup(LookupCase),
    /// A row of `failure.json`.
    Failure(FailureCase),
    /// A row of `store.json`.
    Store(StoreCase),
    /// A row of `legacy.json`.
    Legacy(LegacyCase),
}

impl Case {
    /// The kind of the case.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::Lookup(_) => Kind::Lookup,
            Self::Failure(_) => Kind::Failure,
            Self::Store(_) => Kind::Store,
            Self::Legacy(_) => Kind::Legacy,
        }
    }
}

/// The times of RFC 9111 section 4.2.3 and `now` (DL-183).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Times {
    /// `request_time`.
    pub request_time: UnixTime,
    /// `response_time`.
    pub response_time: UnixTime,
    /// `now`.
    pub now: UnixTime,
}

/// `manifest`: the members of the RQP manifest policy that the row gives
/// (manifest and patch section 1.4). An absent member has its default there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestPolicy {
    /// `ttl_seconds`, 0 to [`MAX_TTL_SECONDS`].
    pub ttl_seconds: Option<u64>,
    /// `offline`.
    pub offline: Option<bool>,
}

/// The expected `use` of a lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedUse {
    /// `fresh`: show without revalidation.
    Fresh,
    /// `stale-while-revalidate`: show and revalidate in the background.
    StaleWhileRevalidate,
    /// `revalidate`: revalidate before showing.
    Revalidate,
}

/// `expect` of a lookup row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupExpect {
    /// `lifetime` in seconds.
    pub lifetime: u64,
    /// `age` in seconds; `None` when `now` is before `response_time`.
    pub age: Option<u64>,
    /// `use`.
    pub entry_use: ExpectedUse,
}

/// A row of `lookup.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupCase {
    /// `response_fields`: the field lines of the stored response.
    pub response_fields: Vec<FieldLine>,
    /// `request_time`, `response_time`, `now`.
    pub times: Times,
    /// `manifest`; `None` for `null`.
    pub manifest: Option<ManifestPolicy>,
    /// `expect`.
    pub expect: LookupExpect,
}

/// `failure` of a failure row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// `network`.
    Network,
    /// `tls`.
    Tls,
}

/// A row of `failure.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureCase {
    /// `response_fields`: the field lines of the stored response.
    pub response_fields: Vec<FieldLine>,
    /// `request_time`, `response_time`, `now`.
    pub times: Times,
    /// `manifest`; `None` for `null`.
    pub manifest: Option<ManifestPolicy>,
    /// `failure`.
    pub failure: FailureKind,
    /// `sensitive`: the app marked the route sensitive.
    pub sensitive: bool,
    /// `expect.serve`.
    pub serve: bool,
}

/// `expect.reason` of a store row with `store: false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedSkip {
    /// `no-store`.
    NoStore,
    /// `vary-any`.
    VaryAny,
    /// `credentialed`.
    Credentialed,
    /// `status`.
    Status,
}

/// `expect` of a store row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreExpect {
    /// `store: true`.
    Store,
    /// `store: false` with its `reason`.
    Skip(ExpectedSkip),
}

/// A row of `store.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreCase {
    /// `response_fields`.
    pub response_fields: Vec<FieldLine>,
    /// `request_fields`.
    pub request_fields: Vec<FieldLine>,
    /// `status`.
    pub status: u16,
    /// `credentials`.
    pub credentials: Credentials,
    /// `authenticated_cache`: the app allows the cache of authenticated pages.
    pub authenticated_cache: bool,
    /// `expect`.
    pub expect: StoreExpect,
}

/// `expect` of a legacy row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyExpect {
    /// `commit`.
    pub commit: bool,
    /// `remove_entry`.
    pub remove_entry: bool,
    /// `disable_key_ms`; `None` for `null`.
    pub disable_key_ms: Option<u64>,
}

/// A row of `legacy.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyCase {
    /// `response_fields`.
    pub response_fields: Vec<FieldLine>,
    /// `request_fields`.
    pub request_fields: Vec<FieldLine>,
    /// `status`.
    pub status: u16,
    /// `credentials`.
    pub credentials: Credentials,
    /// `authenticated_cache`.
    pub authenticated_cache: bool,
    /// `cache_offline` as text; `None` for an absent field.
    pub cache_offline: Option<String>,
    /// `unavailable_time_ms`: the `http` period.
    pub unavailable_time_ms: u64,
    /// `expect`.
    pub expect: LegacyExpect,
}

/// The four files of `fixtures/cache/policy`.
///
/// # Errors
/// See [`load_at`].
pub fn load() -> Result<Vec<PolicyFile>, FixtureError> {
    load_at(&fixtures_root().join("cache").join("policy"))
}

/// The four files of the directory `root`, in the order of [`Kind::ALL`].
///
/// # Errors
/// [`FixtureError::Io`] when the directory or one of the four files cannot
/// be read, [`FixtureError::Json`] when a file is not JSON, and
/// [`FixtureError::Shape`] when the directory has another `.json` file or a
/// row breaks a rule of the format (see the module text).
pub fn load_at(root: &Path) -> Result<Vec<PolicyFile>, FixtureError> {
    check_no_other_json(root)?;
    Kind::ALL
        .iter()
        .map(|&kind| load_file(&root.join(kind.file_name()), kind))
        .collect()
}

fn io_error(path: &Path, source: std::io::Error) -> FixtureError {
    FixtureError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn check_no_other_json(root: &Path) -> Result<(), FixtureError> {
    for entry in fs::read_dir(root).map_err(|e| io_error(root, e))? {
        let entry = entry.map_err(|e| io_error(root, e))?;
        let path = entry.path();
        let is_json = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
        let known = Kind::ALL
            .iter()
            .any(|kind| entry.file_name() == kind.file_name());
        if is_json && !known {
            return Err(FixtureError::Shape {
                path,
                what: "not a file of the policy matrix (no kind runs it)".to_owned(),
            });
        }
    }
    Ok(())
}

fn load_file(path: &Path, kind: Kind) -> Result<PolicyFile, FixtureError> {
    let value = load_json(path)?;
    let items = value.as_array().ok_or_else(|| FixtureError::Shape {
        path: path.to_path_buf(),
        what: "the file is not a JSON array of rows".to_owned(),
    })?;
    let mut rows: Vec<PolicyRow> = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let row = read_row(path, kind, index, item)?;
        if let Some(previous) = rows.last()
            && previous.id.as_bytes() >= row.id.as_bytes()
        {
            return Err(FixtureError::Shape {
                path: path.to_path_buf(),
                what: format!(
                    "row {}: the ids are not unique or not sorted (it follows {})",
                    row.id, previous.id
                ),
            });
        }
        rows.push(row);
    }
    Ok(PolicyFile {
        path: path.to_path_buf(),
        kind,
        rows,
    })
}

/// Reads the members of one row object and names the file and the row in
/// every error.
struct RowReader<'a> {
    path: &'a Path,
    label: String,
    object: &'a Map<String, Value>,
}

impl<'a> RowReader<'a> {
    fn shape(&self, what: impl AsRef<str>) -> FixtureError {
        FixtureError::Shape {
            path: self.path.to_path_buf(),
            what: format!("row {}: {}", self.label, what.as_ref()),
        }
    }

    fn member(&self, key: &str) -> Result<&'a Value, FixtureError> {
        self.object
            .get(key)
            .ok_or_else(|| self.shape(format!("`{key}` is missing")))
    }

    fn string(&self, key: &str) -> Result<String, FixtureError> {
        self.member(key)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| self.shape(format!("`{key}` is not a string")))
    }

    fn boolean(&self, key: &str) -> Result<bool, FixtureError> {
        self.member(key)?
            .as_bool()
            .ok_or_else(|| self.shape(format!("`{key}` is not a boolean")))
    }

    fn seconds(&self, key: &str) -> Result<u64, FixtureError> {
        self.member(key)?
            .as_u64()
            .ok_or_else(|| self.shape(format!("`{key}` is not a non-negative integer")))
    }

    fn time(&self, key: &str) -> Result<UnixTime, FixtureError> {
        self.seconds(key).map(UnixTime)
    }

    fn times(&self) -> Result<Times, FixtureError> {
        Ok(Times {
            request_time: self.time("request_time")?,
            response_time: self.time("response_time")?,
            now: self.time("now")?,
        })
    }

    fn status(&self) -> Result<u16, FixtureError> {
        self.member("status")?
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| (100..=599).contains(n))
            .ok_or_else(|| self.shape("`status` is not a status code (100 to 599)"))
    }

    fn fields(&self, key: &str) -> Result<Vec<FieldLine>, FixtureError> {
        let items = self
            .member(key)?
            .as_array()
            .ok_or_else(|| self.shape(format!("`{key}` is not an array of [name, value]")))?;
        items
            .iter()
            .map(|item| match item.as_array().map(Vec::as_slice) {
                Some([Value::String(name), Value::String(value)]) if !name.is_empty() => {
                    Ok(FieldLine::new(name, value))
                }
                _ => Err(self.shape(format!(
                    "`{key}` has an entry that is not [name, value] with a name"
                ))),
            })
            .collect()
    }

    fn credentials(&self) -> Result<Credentials, FixtureError> {
        match self.member("credentials")?.as_str() {
            Some("absent") => Ok(Credentials::Absent),
            Some("present") => Ok(Credentials::Present),
            Some("unknown") => Ok(Credentials::Unknown),
            _ => Err(self.shape("`credentials` is not absent, present or unknown")),
        }
    }

    fn manifest(&self) -> Result<Option<ManifestPolicy>, FixtureError> {
        let object = match self.member("manifest")? {
            Value::Null => return Ok(None),
            Value::Object(object) => object,
            _ => return Err(self.shape("`manifest` is not null or an object")),
        };
        if object.is_empty() {
            return Err(self.shape("`manifest` has no member (write null)"));
        }
        check_members(object, &["ttl_seconds", "offline"])
            .map_err(|name| self.shape(format!("`manifest` has an unknown member `{name}`")))?;
        let ttl_seconds = match object.get("ttl_seconds") {
            None => None,
            Some(value) => Some(
                value
                    .as_u64()
                    .filter(|n| *n <= MAX_TTL_SECONDS)
                    .ok_or_else(|| {
                        self.shape("`manifest.ttl_seconds` is not an integer from 0 to 31536000")
                    })?,
            ),
        };
        let offline = match object.get("offline") {
            None => None,
            Some(value) => Some(
                value
                    .as_bool()
                    .ok_or_else(|| self.shape("`manifest.offline` is not a boolean"))?,
            ),
        };
        Ok(Some(ManifestPolicy {
            ttl_seconds,
            offline,
        }))
    }

    /// `expect` as an object with exactly the members `required` and, when
    /// present, members of `optional`.
    fn expect(&self, required: &[&str], optional: &[&str]) -> Result<RowReader<'a>, FixtureError> {
        let object = self
            .member("expect")?
            .as_object()
            .ok_or_else(|| self.shape("`expect` is not an object"))?;
        let mut allowed = required.to_vec();
        allowed.extend_from_slice(optional);
        check_members(object, &allowed)
            .map_err(|name| self.shape(format!("`expect` has an unknown member `{name}`")))?;
        let reader = RowReader {
            path: self.path,
            label: format!("{} expect", self.label),
            object,
        };
        for key in required {
            reader.member(key)?;
        }
        Ok(reader)
    }
}

/// `Err(name)` with the first member of `object` that is not in `allowed`.
fn check_members<'o>(object: &'o Map<String, Value>, allowed: &[&str]) -> Result<(), &'o str> {
    match object.keys().find(|name| !allowed.contains(&name.as_str())) {
        Some(name) => Err(name),
        None => Ok(()),
    }
}

fn read_row(
    path: &Path,
    kind: Kind,
    index: usize,
    item: &Value,
) -> Result<PolicyRow, FixtureError> {
    let object = item.as_object().ok_or_else(|| FixtureError::Shape {
        path: path.to_path_buf(),
        what: format!("row {index} (by index): not an object"),
    })?;
    let label = match object.get("id").and_then(Value::as_str) {
        Some(id) => id.to_owned(),
        None => format!("{index} (by index)"),
    };
    let row = RowReader {
        path,
        label,
        object,
    };
    let id = row.string("id")?;
    if !valid_id(&id, kind) {
        return Err(row.shape(format!(
            "`id` is not {}<NNN> with three digits",
            kind.id_prefix()
        )));
    }
    let found = row.string("kind")?;
    if found != kind.name() {
        return Err(row.shape(format!(
            "`kind` is `{found}`, but the file holds `{}` rows",
            kind.name()
        )));
    }
    check_members(object, kind.members())
        .map_err(|name| row.shape(format!("unknown member `{name}` for kind {}", kind.name())))?;
    let source = row
        .member("source")?
        .as_array()
        .filter(|items| !items.is_empty())
        .and_then(|items| {
            items
                .iter()
                .map(|item| {
                    item.as_str()
                        .filter(|s| !s.trim().is_empty())
                        .map(str::to_owned)
                })
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| row.shape("`source` is not an array of one or more texts"))?;
    let note = row.string("note")?;
    if note.trim().is_empty() {
        return Err(row.shape("`note` is empty"));
    }
    let case = match kind {
        Kind::Lookup => Case::Lookup(read_lookup(&row)?),
        Kind::Failure => Case::Failure(read_failure(&row)?),
        Kind::Store => Case::Store(read_store(&row)?),
        Kind::Legacy => Case::Legacy(read_legacy(&row)?),
    };
    Ok(PolicyRow {
        id,
        source,
        note,
        case,
    })
}

fn valid_id(id: &str, kind: Kind) -> bool {
    id.strip_prefix(kind.id_prefix())
        .is_some_and(|digits| digits.len() == 3 && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn read_lookup(row: &RowReader<'_>) -> Result<LookupCase, FixtureError> {
    let expect = row.expect(&["lifetime", "age", "use"], &[])?;
    let age = match expect.member("age")? {
        Value::Null => None,
        _ => Some(expect.seconds("age")?),
    };
    let entry_use = match expect.member("use")?.as_str() {
        Some("fresh") => ExpectedUse::Fresh,
        Some("stale-while-revalidate") => ExpectedUse::StaleWhileRevalidate,
        Some("revalidate") => ExpectedUse::Revalidate,
        _ => {
            return Err(expect.shape("`use` is not fresh, stale-while-revalidate or revalidate"));
        }
    };
    Ok(LookupCase {
        response_fields: row.fields("response_fields")?,
        times: row.times()?,
        manifest: row.manifest()?,
        expect: LookupExpect {
            lifetime: expect.seconds("lifetime")?,
            age,
            entry_use,
        },
    })
}

fn read_failure(row: &RowReader<'_>) -> Result<FailureCase, FixtureError> {
    let expect = row.expect(&["serve"], &[])?;
    let failure = match row.member("failure")?.as_str() {
        Some("network") => FailureKind::Network,
        Some("tls") => FailureKind::Tls,
        _ => return Err(row.shape("`failure` is not network or tls")),
    };
    Ok(FailureCase {
        response_fields: row.fields("response_fields")?,
        times: row.times()?,
        manifest: row.manifest()?,
        failure,
        sensitive: row.boolean("sensitive")?,
        serve: expect.boolean("serve")?,
    })
}

fn read_store(row: &RowReader<'_>) -> Result<StoreCase, FixtureError> {
    let expect_reader = row.expect(&["store"], &["reason"])?;
    let store = expect_reader.boolean("store")?;
    let expect = match (store, expect_reader.object.get("reason")) {
        (true, None) => StoreExpect::Store,
        (true, Some(_)) => return Err(expect_reader.shape("`reason` with `store: true`")),
        (false, None) => return Err(expect_reader.shape("`reason` is missing")),
        (false, Some(reason)) => StoreExpect::Skip(match reason.as_str() {
            Some("no-store") => ExpectedSkip::NoStore,
            Some("vary-any") => ExpectedSkip::VaryAny,
            Some("credentialed") => ExpectedSkip::Credentialed,
            Some("status") => ExpectedSkip::Status,
            _ => {
                return Err(expect_reader
                    .shape("`reason` is not no-store, vary-any, credentialed or status"));
            }
        }),
    };
    Ok(StoreCase {
        response_fields: row.fields("response_fields")?,
        request_fields: row.fields("request_fields")?,
        status: row.status()?,
        credentials: row.credentials()?,
        authenticated_cache: row.boolean("authenticated_cache")?,
        expect,
    })
}

fn read_legacy(row: &RowReader<'_>) -> Result<LegacyCase, FixtureError> {
    let expect = row.expect(&["commit", "remove_entry", "disable_key_ms"], &[])?;
    let cache_offline = match row.member("cache_offline")? {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        _ => return Err(row.shape("`cache_offline` is not a string or null")),
    };
    let disable_key_ms = match expect.member("disable_key_ms")? {
        Value::Null => None,
        _ => Some(expect.seconds("disable_key_ms")?),
    };
    Ok(LegacyCase {
        response_fields: row.fields("response_fields")?,
        request_fields: row.fields("request_fields")?,
        status: row.status()?,
        credentials: row.credentials()?,
        authenticated_cache: row.boolean("authenticated_cache")?,
        cache_offline,
        unavailable_time_ms: row.seconds("unavailable_time_ms")?,
        expect: LegacyExpect {
            commit: expect.boolean("commit")?,
            remove_entry: expect.boolean("remove_entry")?,
            disable_key_ms,
        },
    })
}
