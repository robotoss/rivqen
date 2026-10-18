# Template and data

Rivqen splits a page into a **template** and **data blocks**. This split is what lets Rivqen send only the changed parts of a page.

## 1. The idea

Most pages have a stable frame (layout, styles, scripts, navigation) and a few parts that change often (prices, counters, lists, user names). If the client keeps the frame, the server needs to send only the changed parts.

| Part | Example | Changes |
|---|---|---|
| Template | Header, layout, CSS and JS links, footer | Rarely (on a new release) |
| Data block | Price, stock level, news list, banner | Often (every visit) |

## 2. How to mark data blocks

Add the attribute `data-rq-block` with a block name to the element whose **content** changes.

```html
<title data-rq-block="title">Shop</title>
...
<p class="price" data-rq-block="price">120.00</p>
<ul class="news" data-rq-block="news">
  <li>…</li>
</ul>
```

The element (tag and attributes) belongs to the template. Its content is the block. The server SDK finds the blocks, computes revisions and writes the page manifest for you. The full rules are in [RQP markup](/engineering/protocol/markup).

### React and SSR

```tsx
// Planned API — not released
<RivqenBlock id="price" as="p" className="price">
  {price}
</RivqenBlock>
```

### Legacy pages (temporary)

Pages that still use VasSonic comment markers work in the optional legacy mode until Rivqen 1.5. Convert them with the `rivqen-migrate` tool. See [Migrate from VasSonic](/guide/migrate-from-vassonic).

## 3. What the server sends

| Visit | Server response |
|---|---|
| First visit | The full HTML with a manifest. The client stores the template and the data. |
| Nothing changed | `304 Not Modified`. No body. |
| Data changed, template the same | A patch with only the changed blocks. |
| Template changed | The full new HTML. |
| A changed block has a `script`, `style`, `iframe` or other patch-safety element, an `on…` attribute, or a `javascript:`, `vbscript:` or `data:` URL in an attribute value (section 4, rules 5, 7 to 9) | The full new HTML. |
| The server changed the page cache settings (`policy`, `limits` in the manifest) | The full new HTML. |

## 4. Rules for good blocks

1. Make a block for each part that changes **independently**.
2. Keep blocks small. A block that contains the whole page gives no benefit.
3. Use lowercase names: letters, digits, `-` and `_`. Each name once per page.
4. Do not nest blocks.
5. Inside a block that changes, do not use `script`, `style`, `link`, `meta`, `iframe`, `object`, `embed`, `base`, `basefont` or `bgsound` elements, or SVG elements such as `a`, `foreignObject`, `script`, `style` and animation elements. A block can have them, but every change of the block then sends the full page. For data, use a `json` block (`<script type="application/json" data-rq-block="…">` or `<script type="application/ld+json" data-rq-block="…">`).
6. Put user-specific content in blocks only on pages that are cached per account.
7. Inside a block, do not use event-handler attributes (`onclick`, `onload` and all other `on…` names). Attach handlers from the page script instead.
8. Inside a block, do not start an attribute value with a character reference that hides a URL scheme (for example `&#106;avascript:` or `java&colon;`). Other character references, such as `&amp;`, `&quot;` and `&#169;`, are allowed.
9. Inside a block, do not use `javascript:`, `vbscript:` or `data:` URLs, and do not start any attribute value with these words. Exception: a raster image (`data:image/png`, `data:image/jpeg`, …, not `data:image/svg+xml`) in `src` or `srcset` of `img` and `source` and in `poster` of `video`.

Rules 5 and 7 to 9 apply to the content of HTML blocks, not to the block element itself (its tag and attributes belong to the template). They are not a full HTML sanitizer: use a Content Security Policy for the rest ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)). A page that breaks them still works. But the client accepts no patch with such content, so it gets the full page for every change of that block. The server SDK sends the full page at once ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)).

```html
<!-- Patched as a block update -->
<p data-rq-block="price"><a href="/cart?item=7&amp;qty=1">120.00</a></p>

<!-- Full page on each change: onclick and a script in the block -->
<p data-rq-block="price"><a onclick="buy()" title="&quot;Sale&quot;">120.00</a><script>track()</script></p>
```

## 5. Without server support

If your server does not mark blocks, Rivqen can still start the download early and use normal HTTP caching. It cannot send data-only updates. See [Protocol modes](/guide/concepts/protocol-modes).
