# Java server SDK

**Packages:** `dev.rivqen:rivqen-server` (core), `dev.rivqen:rivqen-servlet`, `dev.rivqen:rivqen-spring-boot-starter`, plus a BOM. **Work package:** WP-14.

**Status:** <Badge type="info" text="DESIGN" />

## 1. Upstream for comparison

<Badge type="tip" text="FACT" /> Upstream `sonic-java` is a `javax.servlet` `Filter` (`SonicFilter`) with a response copier and Gson, shipped as `VasSonic-1.1.jar` (`upstream:sonic-java/src/main/java/com/github/tencent/`, `sonic-java/lib/`). It stores extracted data blocks in **static** fields shared across requests (U-01 in [Divergences](/engineering/protocol/legacy-divergences#_3-upstream-behaviors-that-rivqen-will-not-reproduce)), and it adds legacy headers to every text response.

## 2. Design

| Item | Design |
|---|---|
| Baseline | Java 17+ bytecode; tested on Java 17, 21 and 25 (LTS) |
| Servlet | Jakarta Servlet 6.x filter (`jakarta.servlet`); Spring Boot 4 requires Servlet 6.1 |
| Spring Boot | Auto-configuration starter for Spring Boot 4.x; `rivqen.*` properties |
| Reactive | Spring WebFlux `WebFilter` later, on demand; Netty/Vert.x later |
| JSON | Jackson (already present in most Spring apps); configured to **not** HTML-escape output |
| Streaming | Response wrapper with a bounded buffer; hash after completion; correct `Content-Length` handling |
| Conditional requests | Works together with Spring's `ShallowEtagHeaderFilter` only if Rivqen runs first; document the order |

## 3. Rules

1. Per-request state only. No static mutable fields.
2. Correct handling of `Content-Encoding`, streaming responses and conditional requests.
3. Test on an embedded Tomcat 11 / Jetty 12.1 and behind a production gateway.
4. Document proxy and CDN configuration.

## Related

- [Server SDKs overview](/engineering/server/)
- [Getting started: Java](/guide/getting-started/server-java)
