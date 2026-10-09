# RQP markup

This page specifies how a page marks its data blocks in the Rivqen protocol (RQP). The markup is valid HTML. It replaces the legacy comment markers.

**Status:** <Badge type="info" text="DESIGN" /> Decision: attribute markup + manifest ([ADR-006](/engineering/architecture/adr/#adr-006)).

[[toc]]

## 1. Example

```html
<!DOCTYPE html>
<html>
<head>
  <title data-rq-block="title">Shop</title>
  <script type="application/rivqen-manifest+json" id="rq-manifest">
    { "protocol": "rqp/1", "page_revision": "rev-042", "template_revision": "tpl-008",
      "blocks": [ { "id": "title", "format": "html", "sha256": "…" },
                  { "id": "price", "format": "html", "sha256": "…" },
                  { "id": "cart", "format": "json", "sha256": "…" } ] }
  </script>
</head>
<body>
  <header>Static header</header>
  <p class="price" data-rq-block="price">120.00</p>
  <script type="application/json" data-rq-block="cart">{"items":3}</script>
</body>
</html>
```

## 2. Grammar

| Rule | Specification |
|---|---|
| Block marker | The attribute `data-rq-block` on an HTML element |
| Block id | Value of the attribute: `[a-z0-9][a-z0-9_-]{0,63}` (lowercase ASCII) |
| Block content (`html`) | The **inner HTML** of the element. The element itself (tag and attributes) belongs to the template. |
| Block content (`json`) | The text content of a `<script type="application/json" data-rq-block="…">` element |
| Uniqueness | Each id appears once per document. A duplicate id makes the document invalid for RQP; the server sends the full document without a manifest. |
| Nesting | A block element MUST NOT contain another block element. |
| Allowed elements | Any element that can have content, except `<script>` (other than `application/json`), `<style>`, `<template>`, `<iframe>`, `<textarea>` |
| Title | The page title is a normal block: `<title data-rq-block="title">`. There is no implicit title rule. |
| Template | The document with the content of every block element replaced by an empty placeholder. The template revision is computed over this template. |

Parsing uses a real HTML tokenizer (for example `lol_html` in Rust, `parse5` in Node.js, `jsoup` in Java, `DOMDocument`/`Dom\HTMLDocument` in PHP), not regular expressions. Conformance fixtures make sure all parsers produce the same blocks.

## 3. Why attributes instead of comments

| Problem with legacy comment markers (found in the [audit](/engineering/protocol/legacy-divergences)) | RQP answer |
|---|---|
| Five implementations matched comments differently (title rules, empty names, case) | One grammar, one tokenizer behavior, conformance fixtures |
| Markers with spaces silently did not match | Attributes are parsed by the HTML tokenizer; spaces do not matter |
| Nested markers produced broken templates | Nesting is invalid and rejected |
| Duplicate names: last value won silently | Duplicates are invalid and reported |
| Implicit `<title>` handling corrupted pages with several titles (for example `<svg><title>`) | No implicit rules; the title is a normal block |
| Comments are removed by many HTML minifiers | `data-*` attributes survive minifiers |

## 4. Server helpers

Template authors do not write the manifest. Server SDKs compute it:

| SDK | Helper |
|---|---|
| Node.js | `renderWithRivqen()` reads `data-rq-block` from the rendered HTML; React `<RivqenBlock id="price">` emits the attribute |
| Java | `RivqenFilter` + optional Thymeleaf/JSP attribute helpers |
| PHP | `RivqenMiddleware` + optional Twig/Blade helpers |

## 5. Verification

- Fixtures `FX-RQ-MARKUP-*`: valid, duplicate, nested, forbidden element, uppercase id, minified HTML, Unicode content.
- Differential test: all four server parsers and the Rust parser give identical block lists and template revisions.

## Related

- [RQP negotiation](/engineering/protocol/negotiation)
- [RQP manifest and patch](/engineering/protocol/manifest-patch)
- [Legacy markers grammar](/engineering/protocol/legacy-markers) (deprecated)
