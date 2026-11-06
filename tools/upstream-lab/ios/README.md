# iOS function runner (WP-01 S1, H-119)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

A Rivqen command-line program for macOS. It calls the pure functions of the **unmodified** iOS SDK of VasSonic (commit `59936bef`) and writes the results as JSON Lines. Interface: [client lab contract](../client/CONTRACT.md) section 7. Decision: H-119 (pure functions only; no session, no `NSURLProtocol`, no `UIWebView`, no simulator).

> **Clean-room.** The upstream files stay in the read-only clone (`UPSTREAM`). They are compiled in place, with flags only (`-fno-objc-arc -fobjc-weak`), into `build/` (git-ignored). Nothing of upstream is copied here. The runner is written from the Rivqen pages and from the public names of the SDK (`docs/legal/provenance.md`, CR-03). It does not use method swizzling, runtime patching or key-value access to private state. The sample app of upstream is not read.

## Use

```sh
UPSTREAM=/path/to/VasSonic sh tools/upstream-lab/ios/build.sh
sh tools/upstream-lab/ios/run.sh [cases.json [out.jsonl]]
sh tools/upstream-lab/ios/check.sh
```

| Step | Behavior |
|---|---|
| `build.sh` | Fails unless `UPSTREAM` is at `59936beff656d4b5718ff6444d6c5e001a2c5231` and `sonic-iOS/Sonic` has no local change. Compiles all 14 upstream `.m` files, then the runner `runner/main.m` and its JSON writer `runner/json.m` (ARC, `-Wall -Wextra -Werror`), and links `build/rivqen-ios-fn`. Upstream warnings are suppressed (`-w`); the runner has none |
| `run.sh` | Reads a case file (default `../client/functions.json`) and writes `build/functions.jsonl` and `build/functions.jsonl.meta.json`. Sets `HOME` and `CFFIXED_USER_HOME` to `build/home`, so the cache directory that the SDK makes stays in `build/` |
| `check.sh` | Builds `runner/json.m` with `check/json_check.m` (Foundation only, the runner flags) and runs it. Needs no `UPSTREAM`. Output in `build/check` |
| Network | None. The runner reads only the case file |

`samples/cases.json` is a small input file for checks. The evidence copy is `evidence/traces/client/ios/functions.jsonl` (written by the conductor lane, CONTRACT section 8).

## Why 14 files build on macOS

11 of the 14 files compile for macOS with Foundation only. The other three import UIKit or the cache file that does. The only UIKit symbol that they use is one notification name. `shim/UIKit/UIKit.h` is a Rivqen stand-in header that declares that name, and `runner/main.m` defines it. No UIKit class exists in the build.

## Functions

| `fn` | iOS call | Notes |
|---|---|---|
| `session_id` | `sonicSessionID(url)` | Public C function. The account is not an argument on iOS: it is the engine property set by `-[SonicEngine setCurrentUserAccount:]`. The runner sets it only when `account_related` is `true` and `account` is not empty. The engine ignores an empty account and keeps the last one. So the runner runs all cases without account first, then the cases with account |
| `split` | `+[SonicUtil splitTemplateAndDataFromHtmlData:]` | Empty `html`: the SDK returns an empty dictionary, and the result has `template: null`, `data: null` |
| `diff` | `+[SonicUtil mergeDynamicData:withOriginData:]` | `local: null` is passed as `nil`. A value that is not a string makes the SDK send a message to a wrong class; the runner catches the exception and writes `error` |
| `is_legacy_response` | `-[SonicServer isSonicResponse]` | The fields go through the public delegate entry `connection:didReceiveResponse:`, as a network response does. The SDK lowercases the names there. `fields` is an object or a list of `[name, value]` pairs (CONTRACT §7; pairs added in T-05); a name that repeats exactly is an `error`, because `NSHTTPURLResponse` takes one value per name |
| `etag` | `-[SonicServer responseHeaderForKey:]` after the same entry | Gives the weak-tag normalized value, or `null` when absent |
| `charset` | `-[SonicServer encodingFromHeaders]` | The method is defined in `SonicServer.m` but not declared in the header. The runner declares it in a category (a declaration only). `value` is the IANA name of the encoding; `ns_string_encoding` (extra member) is the number. A missing charset gives UTF-8 |
| `message` | none | `unsupported`: the page message is built by a session method from private session state (status codes, server object, read-only diff data) with no public setter. Stays code reading |
| `template_change` | none | `unsupported`: private session method that reads a private property. Stays code reading |
| any other | none | `unsupported`: unknown function |

Calls that are not in the public header are listed in `functions.jsonl.meta.json` under `non_public_calls`. The conductor lane copies that list into `run.json` (CONTRACT section 7).

## Order of `split` keys

`data` of `split` and `diff` of `diff` are written in the order that the SDK dictionary reports its keys (`allKeys`). That order is **not defined**: the SDK stores the blocks in an `NSDictionary`, which has no order contract. On this Foundation it is stable between runs (checked: two runs are byte-identical) but it is not the document order. Do not treat it as protocol. A consumer must compare the objects as unordered maps.

## Output

One line per case, in input order. Keys sorted, UTF-8, LF. Shape: `{"fn","id","platform":"ios","result"}`, or `error`, or `unsupported` with a reason.

Numbers: a finite double is written with `%.17g` (round trip); an integer as an integer. JSON has no form for a number that is not finite (NaN, infinity). When a result holds one, the case is an `error` with the JSON Pointer of the value, for example `the result has a number that is not finite (-inf) at /result/diff/{a}`, and has no `result` (S2 fix of the S1 review finding: `%.17g` wrote a bare `-inf`, and the line was not JSON). The writer itself writes such a number as the string `"nan"`, `"inf"` or `"-inf"`, so a line is valid JSON also on a path that the runner does not check (no such path exists now). <Badge type="tip" text="FACT" /> Foundation on the development Mac (macOS 26.6.2, SDK 27.2, 2026-11-06) reads `-1e400` in a case file as minus infinity and refuses `1e400` and `NaN` (the whole file then does not parse). Sample case `S-NF-01` (`diff` with the server value `-1e400`) reaches this rule if the SDK passes the value through; if the SDK throws for a value that is not a string, the case is an `error` as `S-DF-03`. Which of the two occurs is NOT FOUND until the next run with `UPSTREAM` (WP-01 S2, T-05). Metadata (`*.meta.json`): macOS version and build, SDK version, compiler, upstream SHA, case count, `non_public_calls`, `limit`.

## Foundation probe

`probe/run.sh` builds `probe/boolvalue.m` (Foundation only, no upstream code) and writes `-[NSString boolValue]` of each `template_change` case value. The iOS SDK reads `template-change` with that call (D-29), but the SDK method is private, so `template_change` stays `unsupported` in the runner. Evidence copy: `evidence/traces/client/ios/boolvalue.jsonl` (T-05).

## Error text

An exception reason can name an object address (`0x…`), which changes from run to run. The runner writes it as `0x(address)`, so two runs are byte-equal (T-05 fix).

## Limits

- <Badge type="warning" text="RESEARCH" /> The runner uses the Foundation of the current macOS, not the Foundation of the iOS of the SDK era (2017 to 2019). `NSURL` parsing (host case, odd characters), `NSDictionary` order, `NSString boolValue` and `NSJSONSerialization` output can differ. A result is evidence for this Foundation only. The meta file records the versions.
- `message` and `template_change` have no run. Their iOS behavior is code reading.
- The SDK engine singleton keeps the account once set. One process is one run.

## Mutation check

Not applicable: this is lab code. No mutation tool is set up for Objective-C in this repository. The checks are the build (`-Werror` for the runner), `check.sh`, the sample run and the byte-equal rerun.

`check.sh` (S2) checks the writer: a number that is not finite is a JSON string and the line parses; finite numbers keep their `%.17g` text (`0.10000000000000001`, `-0`, `1.7976931348623157e+308`); the finder gives the JSON Pointer (RFC 6901 escapes `~0`, `~1`; list index; ordered object; the first value in write order); Foundation reads `-1e400` as minus infinity. Eight hand-made faults of `json.m` (among them the S1 rule: `%.17g` for every double) each fail the check.
