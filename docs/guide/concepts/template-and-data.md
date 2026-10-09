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

## 4. Rules for good blocks

1. Make a block for each part that changes **independently**.
2. Keep blocks small. A block that contains the whole page gives no benefit.
3. Use lowercase names: letters, digits, `-` and `_`. Each name once per page.
4. Do not nest blocks.
5. Do not put scripts inside HTML blocks. For data, use a `json` block (`<script type="application/json" data-rq-block="…">`).
6. Put user-specific content in blocks only on pages that are cached per account.

## 5. Without server support

If your server does not mark blocks, Rivqen can still start the download early and use normal HTTP caching. It cannot send data-only updates. See [Protocol modes](/guide/concepts/protocol-modes).
