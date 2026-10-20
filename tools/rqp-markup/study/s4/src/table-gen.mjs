// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Seeded generator of table shapes for the H-85 study (WP-17 S4 T-03): tables with
// omitted end tags of cells, rows, row groups, captions and column groups, with
// cell content that the implied closure crosses (formatting, special and guarded
// elements, forms, nested tables, blocks, noscript, template, svg, select), and
// with context around the table (open formatting elements, a block that holds the
// table, blocks after the table). Synthetic documents only. Document i of seed s is
// the same on every run.

import { docSeed, makeRng } from '../../../crosscheck/src/generator.mjs';

const pick = (rng, list) => list[Math.floor(rng() * list.length)];
const chance = (rng, p) => rng() < p;

/** Closed content: valid in a cell by the shipped rules. */
const CELL_CLOSED = [
  'x', ' ', 'a < b', '&amp;', '<b>x</b>', '<a href="#">y</a>', '<div>x</div>', '<p>x</p>', '<span>x</span>', '<br>',
  '<img src="x">', '<!--c-->', '<form></form>', '<button>x</button>', '<ul><li>x</li></ul>', '<svg><path d="M0"></svg>',
  '<math><mi>x</mi></math>', '<noscript><img src="x"></noscript>', '<select><option>x</select>', '<h1>h</h1>',
];
/** Open content: elements that an implied closure of the cell crosses. */
const CELL_OPEN = ['<b>x', '<em>e', '<div>x', '<p>x', '<span>x', '<span><div></span>', '<form>', '<h2>h', '<li>', '<i><b>x</i>', '<u><s>x'];
/** Rare content: guarded elements, markers, stray end tags, noscript, template, foreign table parts. */
const CELL_RARE = [
  '<a href="#">y', '<object>', '<button>', '<nobr>n', '<marquee>m', '<applet>', '</p>', '</b>', '</td>', '</tr>', '</div>',
  '</span>', '</th>', '<noscript><b>x</b></noscript>', '<noscript><td>x</td></noscript>', '<svg><td></svg>',
  '<template><td>x</template>', '<hr>', '<input>', '<table>', '<b><object>', '<i><marquee>', '<a href="#">y<object>',
  '<a href="#"><marquee>', '<b><applet>',
];
/** Content in table, row group or row context (the tree builder foster-parents it). */
const ROW_CONTEXT = ['x', '<div>x</div>', '<b>', '<object>', '<marquee>', '<span>x</span>', '<form>', '<a href="#">a</a>', '<input type="hidden">'];
function cellPiece(rng) {
  const r = rng();
  return pick(rng, r < 0.55 ? CELL_CLOSED : r < 0.85 ? CELL_OPEN : CELL_RARE);
}
const BLOCKS = [
  (k) => `<div data-rq-block="b${k}">x</div>`,
  (k) => `<span data-rq-block="b${k}">s<b>t</b></span>`,
  (k) => `<p data-rq-block="b${k}">p</p>`,
  (k) => `<div data-rq-block="b${k}"><table><tr><td>x<td>y</table></div>`,
  (k) => `<div data-rq-block="b${k}"><table><tr><td>x</td><td>y</td></tr></table></div>`,
  (k) => `<div data-rq-block="b${k}"><a href="#">l</a></div>`,
];
const BEFORE = ['', '', '', '<b>', '<a href="#">', '<div>', '<p>', '<form>', '<span>', '<i>', '<object>', '<nobr>'];
const AFTER = ['', 'z', 'z', '</b>', '</a>', '</div>', '<a href="#">q</a>', '<b>w</b>', '<p>t', '</p>', '</td>'];
/** The end of the document: end tags of formatting elements that a reconstruction could still have. */
const END = ['', '', '</b>', '</a>', '</i>', 'x</b>', '</em>', '<a href="#">e</a>'];

class Builder {
  constructor(rng) {
    this.rng = rng;
    this.parts = [];
    this.blocks = 0;
  }

  add(s) {
    this.parts.push(s);
  }

  block() {
    this.add(pick(this.rng, BLOCKS)(this.blocks++));
  }

  cell(depth) {
    const n = Math.floor(this.rng() * 4);
    for (let i = 0; i < n; i++) {
      const r = this.rng();
      if (r < 0.12) this.block();
      else if (r < 0.2 && depth < 2) this.table(depth + 1);
      else this.add(cellPiece(this.rng));
    }
  }

  rows(depth) {
    const nRows = 1 + Math.floor(this.rng() * 3);
    for (let r = 0; r < nRows; r++) {
      if (chance(this.rng, 0.85)) this.add(pick(this.rng, ['<tr>', '<tr>', '<TR class="r">']));
      const nCells = Math.floor(this.rng() * 4);
      for (let c = 0; c < nCells; c++) {
        this.add(pick(this.rng, ['<td>', '<td>', '<th>', '<td colspan="2">']));
        this.cell(depth);
        if (chance(this.rng, 0.45)) this.add(pick(this.rng, ['</td>', '</td>', '</th>']));
      }
      if (chance(this.rng, 0.45)) this.add('</tr>');
      if (chance(this.rng, 0.1)) this.add(pick(this.rng, ROW_CONTEXT)); // content in row context (foster parenting)
    }
  }

  table(depth) {
    this.add(chance(this.rng, 0.9) ? '<table>' : '<TABLE border="1">');
    if (chance(this.rng, 0.25)) {
      this.add('<caption>');
      this.cell(depth);
      if (chance(this.rng, 0.5)) this.add('</caption>');
    }
    if (chance(this.rng, 0.25)) {
      if (chance(this.rng, 0.7)) {
        this.add('<colgroup>');
        const n = Math.floor(this.rng() * 3);
        for (let i = 0; i < n; i++) this.add('<col>');
        if (chance(this.rng, 0.5)) this.add('</colgroup>');
      } else this.add('<col>');
    }
    const nGroups = 1 + Math.floor(this.rng() * 3);
    for (let g = 0; g < nGroups; g++) {
      const group = pick(this.rng, ['', '', 'tbody', 'thead', 'tfoot', 'tbody']);
      if (group !== '') this.add(`<${group}>`);
      this.rows(depth);
      if (group !== '' && chance(this.rng, 0.5)) this.add(`</${group}>`);
      if (chance(this.rng, 0.1)) this.add(pick(this.rng, ['<caption>c', '<colgroup><col>', '<col>', '<tbody>', '<thead>']));
    }
    if (chance(this.rng, 0.9)) this.add('</table>');
  }
}

/**
 * One synthetic document with tables.
 * @param {number} seed sweep seed
 * @param {number} index document index
 * @returns {string}
 */
export function generateTableDocument(seed, index) {
  const rng = makeRng(docSeed(seed, index));
  const b = new Builder(rng);
  if (chance(rng, 0.3)) b.add('<!DOCTYPE html>');
  const wrap = chance(rng, 0.1);
  const before = pick(rng, BEFORE);
  b.add(before);
  if (wrap) b.add(`<div data-rq-block="w">`);
  const nTables = 1 + Math.floor(rng() * 2);
  for (let i = 0; i < nTables; i++) {
    b.table(0);
    if (chance(rng, 0.3)) b.add(pick(rng, AFTER));
  }
  if (wrap) b.add('</div>');
  b.add(pick(rng, AFTER));
  const nAfter = Math.floor(rng() * 3);
  for (let i = 0; i < nAfter; i++) {
    if (chance(rng, 0.5)) b.add(pick(rng, ['z', 'z<a href="#">a</a>', '<b>b</b>']));
    b.block();
  }
  if (chance(rng, 0.3)) b.add('<span data-rq-block="tail">a<a href="#">b</a>c</span>');
  b.add(pick(rng, END));
  return b.parts.join('');
}
