// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The fixture ledger of the RQP protocol client fixtures (DL-145).
//!
//! The data file `ledger/rqp-client.txt` lists each client fixture with its owner WP,
//! a one-word reason and the checks that WP-05 runs on it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

const EMBEDDED: &str = include_str!("../ledger/rqp-client.txt");

/// The work package that checks a fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Owner {
    /// `rivqen-proto` (this WP).
    Wp05,
    /// Template engine.
    Wp06,
    /// Session and diff.
    Wp08,
}

/// A check of DL-145 that WP-05 runs on a fixture besides its outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Check {
    /// The expected request carries `rq-*` fields: compare the request lines.
    RequestFields,
    /// The outcome is `patched`: the binding is a positive control.
    Binding,
}

/// One line of the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    /// Fixture id (directory name).
    pub id: String,
    /// The owner WP.
    pub owner: Owner,
    /// One word: why this WP owns the fixture.
    pub reason: String,
    /// The extra checks of WP-05.
    pub checks: Vec<Check>,
}

/// The ledger does not parse or does not match the fixtures.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LedgerError {
    /// A line of the data file is malformed.
    Line {
        /// Line number, from 1.
        line: usize,
        /// What is wrong.
        message: String,
    },
    /// Two lines have the same id.
    Duplicate(String),
    /// A fixture has no line in the ledger.
    MissingEntry(String),
    /// A line has no fixture.
    UnknownEntry(String),
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Line { line, message } => write!(f, "ledger line {line}: {message}"),
            Self::Duplicate(id) => write!(f, "ledger has two entries for {id}"),
            Self::MissingEntry(id) => write!(f, "fixture {id} has no ledger entry"),
            Self::UnknownEntry(id) => write!(f, "ledger entry {id} has no fixture"),
        }
    }
}

impl std::error::Error for LedgerError {}

/// The parsed ledger.
#[derive(Debug, Clone)]
pub struct Ledger {
    entries: BTreeMap<String, LedgerEntry>,
}

impl Ledger {
    /// The ledger committed in `ledger/rqp-client.txt`.
    ///
    /// # Errors
    /// [`LedgerError`] when the data file is malformed.
    pub fn embedded() -> Result<Self, LedgerError> {
        Self::parse(EMBEDDED)
    }

    /// Parse ledger text: `<id> <owner> <reason> <checks>` per line; `#` starts a comment line.
    ///
    /// # Errors
    /// [`LedgerError::Line`] for a malformed line, [`LedgerError::Duplicate`] for a repeated id.
    pub fn parse(text: &str) -> Result<Self, LedgerError> {
        let mut entries = BTreeMap::new();
        for (index, raw) in text.lines().enumerate() {
            let line = index.saturating_add(1);
            let raw = raw.trim();
            if raw.is_empty() || raw.starts_with('#') {
                continue;
            }
            let bad = |message: &str| LedgerError::Line {
                line,
                message: message.to_owned(),
            };
            let mut parts = raw.split_whitespace();
            let (Some(id), Some(owner), Some(reason), Some(checks), None) = (
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
            ) else {
                return Err(bad("expected four fields: id owner reason checks"));
            };
            let owner = match owner {
                "WP-05" => Owner::Wp05,
                "WP-06" => Owner::Wp06,
                "WP-08" => Owner::Wp08,
                _ => return Err(bad("owner is not WP-05, WP-06 or WP-08")),
            };
            let checks = if checks == "-" {
                Vec::new()
            } else {
                checks
                    .split(',')
                    .map(|c| match c {
                        "request-fields" => Ok(Check::RequestFields),
                        "binding" => Ok(Check::Binding),
                        _ => Err(bad("check is not request-fields or binding")),
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            let entry = LedgerEntry {
                id: id.to_owned(),
                owner,
                reason: reason.to_owned(),
                checks,
            };
            if entries.insert(id.to_owned(), entry).is_some() {
                return Err(LedgerError::Duplicate(id.to_owned()));
            }
        }
        Ok(Self { entries })
    }

    /// Entries sorted by id.
    pub fn entries(&self) -> impl Iterator<Item = &LedgerEntry> {
        self.entries.values()
    }

    /// The entry of a fixture.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&LedgerEntry> {
        self.entries.get(id)
    }

    /// The entries that one WP owns.
    pub fn owned_by(&self, owner: Owner) -> impl Iterator<Item = &LedgerEntry> {
        self.entries().filter(move |e| e.owner == owner)
    }

    /// Check that `ids` and the ledger list the same fixtures.
    ///
    /// # Errors
    /// [`LedgerError::MissingEntry`] for a fixture without an entry,
    /// [`LedgerError::UnknownEntry`] for an entry without a fixture.
    pub fn verify<'a>(&self, ids: impl IntoIterator<Item = &'a str>) -> Result<(), LedgerError> {
        let mut seen = BTreeSet::new();
        for id in ids {
            if !self.entries.contains_key(id) {
                return Err(LedgerError::MissingEntry(id.to_owned()));
            }
            seen.insert(id);
        }
        match self.entries.keys().find(|k| !seen.contains(k.as_str())) {
            Some(id) => Err(LedgerError::UnknownEntry(id.clone())),
            None => Ok(()),
        }
    }
}
