# Migrate from VasSonic

This guide is for teams that use Tencent VasSonic (last updated in 2019) and want to move to Rivqen. Rivqen has a **temporary legacy mode** that understands the old protocol, so you can migrate the client and the server step by step. The goal is the Rivqen protocol (RQP) before Rivqen 1.5.

::: warning Planned
Rivqen is not released yet. Use this guide to plan.
:::

## 1. Timeline

| Rivqen release | Legacy mode |
|---|---|
| 1.0 | Available as an optional module, **deprecated**, warnings on use |
| 1.1 – 1.3 | Security fixes only |
| 1.4.x | Last release with legacy modules |
| 1.5 | Removed — all pages must use RQP |

## 2. What changes

| Area | VasSonic | Rivqen (RQP) | Action for you |
|---|---|---|---|
| Block markup | `<!--sonicdiff-x-->…<!--sonicdiff-x-end-->` | `data-rq-block="x"` attribute | Run `rivqen-migrate` on your templates |
| Page title | Implicit block (rules differ per server) | Normal block: `<title data-rq-block="title">` | Mark the title if it changes |
| Versions | `etag`, `template-tag` headers | Manifest + `Rq-Revision`, `Rq-Template-Revision` | Nothing; the server SDK computes them |
| Data update | All blocks sent; client computes diff | Only changed blocks, validated patch | Nothing |
| Cache identity | Host + path + `sonic_*` parameters; other parameters ignored | Full URL + account partition | Remove `sonic_remain_params` workarounds |
| Accounts | Optional account name prefix | Mandatory partition per account | Pass a non-secret user ID to `open` |
| JS bridge | `addJavascriptInterface` object `sonic` | Origin-checked message bridge | Use `@rivqen/web-core`; the legacy bridge shim exists until 1.5 |
| `Cache-Control: no-store` | Ignored by default | Always respected | Check your headers |
| iOS | `UIWebView` + `NSURLProtocol` | `WKWebView`, public APIs | See [iOS getting started](/guide/getting-started/ios) |
| Subresource cache | External storage | App-private storage | Nothing |

## 3. Migration order

1. Inventory your pages: list block names, `sonic_*` parameters and `cache-offline` usage.
2. Install the Rivqen server SDK **with the legacy module**. Run the conformance runner against staging.
3. Install the Rivqen mobile SDK **with the legacy module**. Your pages keep working.
4. Run `rivqen-migrate` on your templates. Review the diff. Deploy RQP markup page by page.
5. Watch the `LegacyModeUsed` metric. It must go to zero.
6. Remove the legacy modules from the app and the server.

## 4. What `rivqen-migrate` does

| Legacy input | RQP output |
|---|---|
| `<!--sonicdiff-x--><p>…</p><!--sonicdiff-x-end-->` (block wraps one element) | `<p data-rq-block="x">…</p>` |
| Block wraps several elements | `<div data-rq-block="x">…</div>` wrapper, reported for review |
| Empty name, duplicate name, markers with spaces, nested markers | Reported as errors; no automatic change |
| Implicit title | `<title data-rq-block="title">` |

## 5. Checklist

- [ ] Every block has a unique, lowercase name.
- [ ] Personal pages send `Cache-Control: private` (or `no-store`).
- [ ] The app passes a partition for every authenticated session and clears it on logout.
- [ ] Page origins are in `allowedOrigins` and `bridgeOrigins`.
- [ ] Conformance runner passes against staging.
- [ ] `LegacyModeUsed` is zero before you upgrade to Rivqen 1.5.
