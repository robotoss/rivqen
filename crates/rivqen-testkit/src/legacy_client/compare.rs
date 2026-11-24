// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The comparison of two projections (DL-110; `fixtures/legacy/README.md`
//! "Client `expected.json`", the order column of `client.phases`).
//!
//! - `sessions`: in step order, entry by entry.
//! - `requests` (exchange order), `actions` (exact order), `snapshots` (step
//!   order, then the records in their sorted order): entry by entry.
//! - `cookies`, `intercepts`, `results`: as multisets. Equal entries match
//!   first; the rest of each side is paired in canonical text order, and a
//!   pair gives one difference for each member that differs.
//! - A record compares `data_sha256` by presence only (DL-118).
//! - Each problem of the runner is one difference (member `runner`).
//!
//! A difference names the phase, the member and the index, and holds only the
//! two differing values (JSON text), never the rest of the entry.

use std::fmt;

use super::projection::{
    Action, CookieCall, InterceptView, Member, Phase, Projection, RecordView, RequestView,
    ResultView, Snapshot,
};

/// One difference between the expected and the actual projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    /// The 1-based phase number; `None` outside the phases.
    pub phase: Option<usize>,
    /// The member: `sessions`, `phases`, `steps`, `requests`, `cookies`,
    /// `actions`, `intercepts`, `results`, `snapshots` or `runner`.
    pub member: String,
    /// The 0-based index in the list of the member (for a multiset: in the
    /// expected list, or in the actual list for an extra entry).
    pub index: Option<usize>,
    /// The path inside the entry (for example `body_sha256` or
    /// `entries[0].etag`); empty for the whole entry.
    pub path: String,
    /// The expected value as JSON text; `None` when the entry is extra.
    pub expected: Option<String>,
    /// The actual value as JSON text; `None` when the entry is missing.
    pub actual: Option<String>,
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(phase) = self.phase {
            write!(f, "phase {phase} ")?;
        }
        f.write_str(&self.member)?;
        if let Some(index) = self.index {
            write!(f, "[{index}]")?;
        }
        if !self.path.is_empty() {
            write!(f, ".{}", self.path)?;
        }
        match (&self.expected, &self.actual) {
            (Some(e), Some(a)) => write!(f, ": expected {e}, actual {a}"),
            (Some(e), None) => write!(f, ": missing, expected {e}"),
            (None, Some(a)) => write!(f, ": extra {a}"),
            (None, None) => f.write_str(": differs"),
        }
    }
}

/// The differences of `actual` from `expected`; empty when they are equal
/// under the rules of the README.
#[must_use]
pub fn compare(expected: &Projection, actual: &Projection) -> Vec<Difference> {
    let mut out = Vec::new();
    for (i, problem) in actual.problems.iter().enumerate() {
        out.push(Difference {
            phase: None,
            member: "runner".to_owned(),
            index: Some(i),
            path: String::new(),
            expected: None,
            actual: Some(problem.clone()),
        });
    }
    let at = Place {
        phase: None,
        member: "sessions",
    };
    ordered(
        &at,
        &expected
            .sessions
            .iter()
            .map(|s| (s.text(), s.members()))
            .collect::<Vec<_>>(),
        &actual
            .sessions
            .iter()
            .map(|s| (s.text(), s.members()))
            .collect::<Vec<_>>(),
        &mut out,
    );
    if expected.phases.len() != actual.phases.len() {
        out.push(Difference {
            phase: None,
            member: "phases".to_owned(),
            index: None,
            path: "length".to_owned(),
            expected: Some(expected.phases.len().to_string()),
            actual: Some(actual.phases.len().to_string()),
        });
    }
    for (i, (e, a)) in expected.phases.iter().zip(&actual.phases).enumerate() {
        phase(i.saturating_add(1), e, a, &mut out);
    }
    out
}

/// Where a list is in the projection.
struct Place {
    phase: Option<usize>,
    member: &'static str,
}

impl Place {
    fn difference(
        &self,
        index: Option<usize>,
        path: String,
        expected: Option<String>,
        actual: Option<String>,
    ) -> Difference {
        Difference {
            phase: self.phase,
            member: self.member.to_owned(),
            index,
            path,
            expected,
            actual,
        }
    }
}

/// An entry: its canonical text and its members.
type Entry = (String, Vec<Member>);

fn entries<T>(
    list: &[T],
    text: impl Fn(&T) -> String,
    members: impl Fn(&T) -> Vec<Member>,
) -> Vec<Entry> {
    list.iter().map(|x| (text(x), members(x))).collect()
}

fn phase(number: usize, e: &Phase, a: &Phase, out: &mut Vec<Difference>) {
    let place = |member| Place {
        phase: Some(number),
        member,
    };
    if e.steps != a.steps {
        let steps = |s: Option<(u32, u32)>| {
            s.map_or_else(|| "[]".to_owned(), |(x, y)| format!("[{x},{y}]"))
        };
        out.push(place("steps").difference(
            None,
            String::new(),
            Some(steps(e.steps)),
            Some(steps(a.steps)),
        ));
    }
    ordered(
        &place("requests"),
        &entries(&e.requests, RequestView::text, RequestView::members),
        &entries(&a.requests, RequestView::text, RequestView::members),
        out,
    );
    multiset(
        &place("cookies"),
        &entries(&e.cookies, CookieCall::text, CookieCall::members),
        &entries(&a.cookies, CookieCall::text, CookieCall::members),
        out,
    );
    ordered(
        &place("actions"),
        &entries(&e.actions, Action::text, Action::members),
        &entries(&a.actions, Action::text, Action::members),
        out,
    );
    multiset(
        &place("intercepts"),
        &entries(&e.intercepts, InterceptView::text, InterceptView::members),
        &entries(&a.intercepts, InterceptView::text, InterceptView::members),
        out,
    );
    multiset(
        &place("results"),
        &entries(&e.results, ResultView::text, ResultView::members),
        &entries(&a.results, ResultView::text, ResultView::members),
        out,
    );
    ordered(
        &place("snapshots"),
        &entries(&e.snapshots, snapshot_text, snapshot_members),
        &entries(&a.snapshots, snapshot_text, snapshot_members),
        out,
    );
}

/// The comparison text of a snapshot: its members (so `data_sha256` counts
/// by presence only).
fn snapshot_text(s: &Snapshot) -> String {
    snapshot_members(s)
        .into_iter()
        .map(|(p, v)| format!("{p}={v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The members of a snapshot: `entries.length`, then each member of each
/// record as `entries[i].member`.
fn snapshot_members(s: &Snapshot) -> Vec<Member> {
    let mut out = vec![("entries.length".to_owned(), s.entries.len().to_string())];
    for (i, record) in s.entries.iter().enumerate() {
        for (name, value) in RecordView::members(record) {
            out.push((format!("entries[{i}].{name}"), value));
        }
    }
    out
}

/// The member differences of two entries (members of both, in the order of
/// the expected entry, then the members that only the actual entry has).
fn members(place: &Place, index: usize, e: &[Member], a: &[Member], out: &mut Vec<Difference>) {
    for (name, value) in e {
        let other = a.iter().find(|(n, _)| n == name).map(|(_, v)| v);
        if other != Some(value) {
            out.push(place.difference(
                Some(index),
                name.clone(),
                Some(value.clone()),
                other.cloned(),
            ));
        }
    }
    for (name, value) in a {
        if !e.iter().any(|(n, _)| n == name) {
            out.push(place.difference(Some(index), name.clone(), None, Some(value.clone())));
        }
    }
}

fn ordered(place: &Place, e: &[Entry], a: &[Entry], out: &mut Vec<Difference>) {
    let longest = e.len().max(a.len());
    for i in 0..longest {
        match (e.get(i), a.get(i)) {
            (Some(x), Some(y)) => members(place, i, &x.1, &y.1, out),
            (Some(x), None) => {
                out.push(place.difference(Some(i), String::new(), Some(x.0.clone()), None))
            }
            (None, Some(y)) => {
                out.push(place.difference(Some(i), String::new(), None, Some(y.0.clone())))
            }
            (None, None) => {}
        }
    }
}

fn multiset(place: &Place, expected: &[Entry], actual: &[Entry], out: &mut Vec<Difference>) {
    // The comparison text: the members (equal members give equal texts).
    let key = |entry: &Entry| {
        entry
            .1
            .iter()
            .map(|(path, value)| format!("{path}={value}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut left: Vec<(usize, String)> = expected
        .iter()
        .enumerate()
        .map(|(index, entry)| (index, key(entry)))
        .collect();
    let mut right: Vec<(usize, String)> = actual
        .iter()
        .enumerate()
        .map(|(index, entry)| (index, key(entry)))
        .collect();
    left.sort_by(|one, two| one.1.cmp(&two.1));
    right.sort_by(|one, two| one.1.cmp(&two.1));
    // Remove the equal entries (both lists are sorted by text).
    let mut unmatched = Vec::new();
    let mut cursor = 0_usize;
    for (index, text) in left {
        while right.get(cursor).is_some_and(|(_, other)| *other < text) {
            cursor = cursor.saturating_add(1);
        }
        if right.get(cursor).is_some_and(|(_, other)| *other == text) {
            right.remove(cursor);
        } else {
            unmatched.push(index);
        }
    }
    // Pair the rest in text order: one difference for each member that
    // differs; the rest of the longer side is missing or extra.
    let mut extra = right.into_iter().map(|(index, _)| index);
    for index in unmatched {
        let Some(want) = expected.get(index) else {
            continue;
        };
        match extra.next().and_then(|other| actual.get(other)) {
            Some(got) => members(place, index, &want.1, &got.1, out),
            None => {
                out.push(place.difference(Some(index), String::new(), Some(want.0.clone()), None))
            }
        }
    }
    for index in extra {
        if let Some(got) = actual.get(index) {
            out.push(place.difference(Some(index), String::new(), None, Some(got.0.clone())));
        }
    }
}
