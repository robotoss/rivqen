// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Demo pages. The catalog page is about 20 KB of HTML (2.2 KB gzip):
// a large stable template and a few small blocks that change often.

const state = { data: 0, template: 0 };

/** Advance demo data (price and stock change). */
export function nextData() {
  state.data += 1;
  return { ...state };
}

/** Change the template (for example a new menu item). */
export function nextTemplate() {
  state.template += 1;
  return { ...state };
}

export function demoState() {
  return { ...state };
}

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

function products() {
  let html = '';
  for (let i = 1; i <= 60; i++) {
    html += `<article class="card"><img src="/img/p${i}.webp" alt="Product ${i}" width="160" height="160">` +
      `<h3>Product ${i}</h3><p class="desc">Durable everyday item number ${i}. Free returns within 30 days. ` +
      `Ships from the central warehouse. Rated 4.${i % 10} by customers.</p></article>\n`;
  }
  return html;
}

const STYLE = Array.from({ length: 80 }, (_, i) =>
  `.c${i}{margin:${i % 7}px;padding:${i % 5}px;color:#${(100000 + i * 7919).toString(16).slice(0, 6)}}`).join('\n');

/** Render the catalog page for the current demo state. */
export function renderCatalog() {
  const price = (120 + state.data * 5).toFixed(2);
  const stock = Math.max(0, 25 - state.data);
  const extraMenu = state.template > 0 ? `<li><a href="/new">New arrivals v${state.template}</a></li>` : '';
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title data-rq-block="title">Shop — Catalog (${esc(price)})</title>
<style>
${STYLE}
</style>
</head>
<body>
<header><nav><ul><li><a href="/">Home</a></li><li><a href="/catalog">Catalog</a></li>${extraMenu}</ul></nav></header>
<main>
<section class="hero"><h1>Catalog</h1>
<p class="price">Today: <span data-rq-block="price">${esc(price)}</span> EUR</p>
<p class="stock" data-rq-block="stock">In stock: ${stock}</p>
</section>
<section class="grid">
${products()}
</section>
<script type="application/json" data-rq-block="cart">{"items":${state.data % 4},"currency":"EUR"}</script>
</main>
<footer><p>Static footer. Demo page for the Rivqen protocol.</p></footer>
</body>
</html>
`;
}
