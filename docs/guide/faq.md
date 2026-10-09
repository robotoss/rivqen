# FAQ

## General

### Is Rivqen ready for production?

No. Rivqen is pre-alpha. This site is the design documentation. See [Project status](/guide/status).

### Is Rivqen a fork of VasSonic?

No. Rivqen is an independent implementation, written from scratch under a [clean-room policy](/legal/provenance). It has a temporary compatibility mode for the VasSonic legacy protocol, but it does not contain VasSonic code.

### Is Rivqen made by Tencent?

No. Rivqen is not affiliated with, sponsored by, or endorsed by Tencent.

### What does the name mean?

"Rivqen" is a coined name. "Riv" refers to a river — a continuous flow of data. "Qen" suggests "quick" and "engine". It has no other meaning.

## Integration

### Do I need to change my server?

For the full benefit (data-only updates), yes: add a Rivqen server SDK (an existing VasSonic server works in the temporary legacy mode until Rivqen 1.5). Without server changes, Rivqen can only start the download early and use normal HTTP caching.

### Will normal browser users see different HTML?

No. The server SDKs change responses only for requests from Rivqen clients.

### Does Rivqen work with any website?

Rivqen accelerates only origins in your allowlist, and full acceleration needs server support. Rivqen never modifies third-party content.

### Does Rivqen work with React Server Components or streaming SSR?

The React adapter targets current React SSR and hydration. Streaming SSR needs block-level buffering on the server. Support for specific frameworks is confirmed by the test matrix before release.

## Performance

### How much faster is it?

We do not publish numbers before measurements. Rivqen's targets are: no regression on first load compared with a plain WebView, and a measurable improvement on repeat visits. See [Benchmarks](/engineering/quality/benchmarks).

### Does it use HTTP/3?

Rivqen uses whatever the platform network stack negotiates. HTTP/3 is used when the platform and the server support it.

## Security and privacy

### Does Rivqen send data to a third party?

No. Rivqen has no backend service. Metrics stay in your app unless you export them.

### Can one user see another user's cached page?

No. Every cache entry belongs to a partition. Logout removes access to the partition at once. This is tested as a release gate.

### Does Rivqen make my app identifiable?

Rivqen does not add an SDK-identifying header by default. Protocol negotiation headers are sent only when needed for the protocol.

## Licensing

### What is the license?

Apache License 2.0. See [Licensing](/legal/licensing).

### Can I use Rivqen in a commercial app?

Yes, under the terms of the Apache License 2.0.
