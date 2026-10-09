# Glossary

Use these terms with exactly these meanings in all Rivqen documents. Do not use synonyms.

| Term | Meaning |
|---|---|
| **ADR** | Architecture decision record. A document that records one important decision. |
| **Adapter** | The platform code (Kotlin, Swift) between the Rust core and the operating system. |
| **B0 / B1 / B2** | Benchmark baselines: plain WebView / upstream VasSonic / Rivqen. |
| **Base revision** | The revision that a patch expects the page to have before the patch is applied. |
| **Bridge (JS bridge)** | The message channel between the web page and the native SDK. |
| **Cache key** | The identifier of one cache entry. It includes the partition. |
| **`cache-offline`** | A legacy-mode response header that controls local use and storage of a page. |
| **Canonical URL** | The normalized form of a URL used in the cache key. |
| **Capability** | A feature that a client or server announces in protocol negotiation. |
| **Clean room** | A development method: one group describes behavior from the original; the implementation is written only from that description. |
| **Core (Rust core)** | The shared Rust library with Rivqen logic. |
| **Data block** | A named part of a page that changes often. |
| **Diff** | The difference between the cached data and the new data. |
| **`etag`** | In legacy mode, the header with the identifier of the full HTML version. Not a security value. |
| **Fallback** | A normal page load without Rivqen optimizations. |
| **FFI** | Foreign function interface. The boundary between Rust and Kotlin/Swift. |
| **Fixture (golden fixture)** | A set of input and expected output files that defines correct behavior. |
| **Gate** | A go/no-go decision at the end of a phase. |
| **Generation** | A number that increases when a session restarts. Old events are ignored. |
| **L0 / L1 / L2** | Cache layers: active session / memory / disk. |
| **Manifest (page manifest)** | In Rivqen protocol, a description of a page version: revisions, blocks and their hashes, policy. |
| **Marker** | An HTML comment or attribute that shows the start and end of a data block. |
| **Origin** | Scheme + host + port of a URL, as defined by the web platform. |
| **Outcome** | The result of a navigation: first load, not modified, data updated, template changed, served stale, fallback. |
| **Partition** | The account scope of cached data. |
| **Patch** | In Rivqen protocol, a validated set of operations that change data blocks. |
| **Quick mode / Standard mode** | Two client orchestration modes of upstream VasSonic. Not related to QUIC. |
| **Revalidation** | A request that asks the server if the cached page is still current. |
| **Revision** | A version label of a page or a template. |
| **Rivqen protocol (RQP)** | The new Rivqen protocol. |
| **Session** | One page load managed by Rivqen. |
| **Legacy mode** | Temporary compatibility with the wire protocol of Tencent VasSonic. Optional module; deprecated from 1.0, removed in 1.5. |
| **`data-rq-block`** | The HTML attribute that marks a data block in RQP. |
| **Manifest** | See "Manifest (page manifest)". |
| **RQP** | Rivqen protocol: the native protocol with `data-rq-block` markup, a manifest and `Rq-*` headers. |
| **Surface** | A public contract: API, ABI, wire format, message schema, configuration. |
| **Template** | The part of a page that rarely changes. It contains placeholders for data blocks. |
| **`template-tag`** | In legacy mode, the header with the identifier of the template version. |
| **Upstream** | The Tencent VasSonic repository at commit `59936bef`. |
| **VasSonic** | The original Tencent project. Used only to identify the upstream. |
| **WP** | Work package. A unit of the development plan. |
