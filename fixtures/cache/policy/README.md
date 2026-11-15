# Cache policy matrix

These files are the policy matrix of the cache engine (WP-07 acceptance: "policy matrix
tests pass"; DL-182). One row is one case: the inputs of a policy decision and the expected
result. `rivqen-cache` must give the expected result for every row
(`crates/rivqen-cache/tests/policy_matrix.rs`).

## Rules

1. Each expected value comes from a source, not from the code: a section of RFC 9111,
   RFC 9110 or RFC 5861, a rule of `docs/engineering/cache/policy.md` §2 or §3, a rule of
   `docs/engineering/protocol/negotiation.md` §5, `manifest-patch.md` §1.4, or a CL rule of
   `docs/engineering/protocol/legacy-client.md` §9. The member `source` names it.
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
