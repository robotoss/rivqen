# Cache policy matrix

These files are the policy matrix of the cache engine (WP-07 acceptance: "policy matrix
tests pass"; DL-182). One row is one case: the inputs of a policy decision and the expected
result. `rivqen-cache` must give the expected result for every row
(`crates/rivqen-cache/tests/policy_matrix.rs`).

## Rules

1. Each expected value comes from a source, not from the code: a section of RFC 9111,
   RFC 9110 or RFC 5861, a rule of `docs/engineering/cache/policy.md` §2 or §3, a rule of
   `docs/engineering/protocol/negotiation.md` §5 or §6.3, `manifest-patch.md` §1.4,
   `docs/engineering/cache/identity.md` §1, a rule of `docs/engineering/cache/index.md` §3
   (R-CACHE-02, R-CACHE-03), `legacy-wire.md` §7 or §10, a CL rule or §9.12 of
   `docs/engineering/protocol/legacy-client.md`, or an entry of the decision log (`DL-198`
   for the cases where the RFC texts leave a choice). The member `source` names it in one
   of these forms: `RFC 9111 §4.2.1`, `cache policy §2`, `negotiation §6.3`,
   `manifest-patch §1.4`, `cache identity §1`, `cache index §3`, `legacy-wire §7`,
   `legacy-client §9.12`, `CL-25`, `DL-198`.
2. The author of the rows does not read `crates/rivqen-cache/src/policy/`. A reviewer who
   did not write the policy code checks each row against its source.
3. One row tests one rule. A row for a boundary has a twin row on the other side of the
   boundary.
4. Times are whole seconds since 1970-01-01T00:00:00Z. HTTP dates in field values must
   agree with the numbers of the row.
5. A row that a source does not decide is not written. The open case goes to the sprint
   record as a question.

## Files

| File | `kind` | Decision |
|---|---|---|
| `lookup.json` | `lookup` | Freshness lifetime, current age and the use of a stored entry (policy §2, rules 4, 5, 8) |
| `failure.json` | `failure` | A stale entry after a network or TLS error (policy §2 rule 6, §3) |
| `store.json` | `store` | Store or not (policy §2 rule 1, R-CACHE-02, R-CACHE-03, `Vary: *`) |
| `legacy.json` | `legacy` | Legacy `cache-offline` (legacy wire §7; CL-15, CL-19, CL-25, CL-30, CL-39) |

Each file is a JSON array of rows, UTF-8, LF line ends, two spaces of indentation, sorted
by `id`.

## Row

| Member | Kinds | Value |
|---|---|---|
| `id` | all | `POL-<KIND>-<NNN>`, unique, for example `POL-LOOKUP-001` |
| `source` | all | Array of strings, at least one, for example `"RFC 9111 §4.2.1"` |
| `note` | all | One sentence: what the row shows |
| `kind` | all | `lookup`, `failure`, `store` or `legacy` |
| `response_fields` | all | Array of `[name, value]`: the field lines of the stored response |
| `request_fields` | `store`, `legacy` | Array of `[name, value]`: the field lines of the request that the policy reads (`cache-control`) |
| `request_time`, `response_time`, `now` | `lookup`, `failure` | Seconds |
| `manifest` | `lookup`, `failure` | `null`, or `{"ttl_seconds": n, "offline": bool}` with one or both members (RQP manifest policy) |
| `failure` | `failure` | `"network"` or `"tls"` |
| `sensitive` | `failure` | Boolean: the app marked the route sensitive |
| `status` | `store`, `legacy` | Status code |
| `credentials` | `store`, `legacy` | `"absent"`, `"present"` or `"unknown"` |
| `authenticated_cache` | `store`, `legacy` | Boolean: the app allows the cache of authenticated pages |
| `cache_offline` | `legacy` | The `cache-offline` value as text, or `null` for an absent field |
| `unavailable_time_ms` | `legacy` | The `http` period in milliseconds |
| `expect` | all | See below |

`expect` by kind:

| Kind | Members |
|---|---|
| `lookup` | `lifetime` (seconds), `age` (seconds, or `null` when `now` is before `response_time`), `use`: `"fresh"`, `"stale-while-revalidate"` or `"revalidate"` |
| `failure` | `serve`: boolean |
| `store` | `store`: boolean; with `false` also `reason`: `"no-store"`, `"vary-any"`, `"credentialed"` or `"status"` |
| `legacy` | `commit`: boolean, `remove_entry`: boolean, `disable_key_ms`: number or `null` |

A new member needs a row in these tables first.

## Meaning of the values

- `manifest: null` means no manifest policy: no `ttl_seconds` bound and `offline` false
  (DL-198 item i). A member that is absent in a `manifest` object also has no effect:
  no bound for `ttl_seconds`, false for `offline`.
- `lifetime` is never negative. A response without valid explicit freshness information
  has the lifetime 0 (DL-182, DL-198).
- A `failure` row asks one question: does the cache show the stored entry after the
  error? `serve: true` always means "with the stale state to the app" (policy §2 rule 6).
- A `legacy` row is one legacy response with the given `status`. All its other legacy
  fields are valid, the body is complete, the SHA-1 values match and the partition is not
  `null`, so only the cache rules decide. `commit: true` means that the rules allow the
  commit of the entry. `remove_entry: true` means that the rules remove the entry of the
  key when one exists. `disable_key_ms` is the period of CL-19, or `null`.
- A `store` row is the response to a request for a full document. A delta is not a row:
  its own `no-store` does not decide the store of the patched snapshot (DL-199).
