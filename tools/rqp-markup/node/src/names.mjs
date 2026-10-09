// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Name sets of docs/engineering/protocol/markup.md section 2.1 (rqp/1).
// Names are lower case, as the tokenizer gives them.

import { TextMode } from './tokenizer.mjs';

const set = (...names) => Object.freeze(new Set(names));

export const HEADINGS = set('h1', 'h2', 'h3', 'h4', 'h5', 'h6');

export const VOID = set(
  'area', 'base', 'basefont', 'bgsound', 'br', 'col', 'embed', 'frame', 'hr', 'image',
  'img', 'input', 'keygen', 'link', 'meta', 'param', 'source', 'track', 'wbr',
);

export const TEXT = set(
  'title', 'textarea', 'iframe', 'noembed', 'noframes', 'noscript', 'style', 'xmp',
  'script', 'plaintext',
);

/** M-04: the tokenizer state after a start tag in HTML context. */
export const TEXT_MODE = Object.freeze(new Map([
  ['title', TextMode.RCDATA],
  ['textarea', TextMode.RCDATA],
  ['iframe', TextMode.RAWTEXT],
  ['noembed', TextMode.RAWTEXT],
  ['noframes', TextMode.RAWTEXT],
  ['noscript', TextMode.RAWTEXT],
  ['style', TextMode.RAWTEXT],
  ['xmp', TextMode.RAWTEXT],
  ['script', TextMode.SCRIPT_DATA],
  ['plaintext', TextMode.PLAINTEXT],
]));

export const PCLOSE = set(
  'address', 'article', 'aside', 'blockquote', 'center', 'dd', 'details', 'dialog', 'dir',
  'div', 'dl', 'dt', 'fieldset', 'figcaption', 'figure', 'footer', 'form', ...HEADINGS,
  'header', 'hgroup', 'hr', 'li', 'listing', 'main', 'menu', 'nav', 'ol', 'p', 'plaintext',
  'pre', 'search', 'section', 'summary', 'table', 'ul', 'xmp',
);

export const GUARDED = set(
  'a', 'applet', 'button', 'caption', 'colgroup', 'marquee', 'math', 'nobr', 'object',
  'select', 'svg', 'table', 'tbody', 'td', 'template', 'tfoot', 'th', 'thead', 'tr',
);

export const TABLE_PART = set('caption', 'colgroup', 'tbody', 'td', 'tfoot', 'th', 'thead', 'tr');

export const TABLE_FAMILY = set('table', ...TABLE_PART);

export const BREAKOUT = set(
  'b', 'big', 'blockquote', 'body', 'br', 'center', 'code', 'dd', 'div', 'dl', 'dt', 'em',
  'embed', 'font', ...HEADINGS, 'head', 'hr', 'i', 'img', 'li', 'listing', 'menu', 'meta',
  'nobr', 'ol', 'p', 'pre', 'ruby', 's', 'small', 'span', 'strike', 'strong', 'sub', 'sup',
  'table', 'tt', 'u', 'ul', 'var',
);

export const INTEGRATION = set(
  'annotation-xml', 'desc', 'foreignobject', 'mi', 'mn', 'mo', 'ms', 'mtext', 'title',
);

export const BLOCK_HTML = set(
  'article', 'aside', 'div', 'footer', ...HEADINGS, 'header', 'main', 'nav', 'p', 'section',
  'span', 'title',
);

export const FORBIDDEN_CONTENT = set(
  'base', 'basefont', 'bgsound', 'body', 'embed', 'frame', 'frameset', 'head', 'html',
  'iframe', 'link', 'math', 'meta', 'noembed', 'noframes', 'noscript', 'object', 'optgroup',
  'option', 'plaintext', 'script', 'select', 'style', 'svg', 'template', 'textarea', 'title',
  'xmp',
);

/** M-08: tokens allowed inside a select element. */
export const SELECT_START = set('option', 'optgroup', 'hr');
export const SELECT_END = set('option', 'optgroup', 'select');

/** M-09: allowed nearest table-family element F for each table-part start tag. */
export const TABLE_PARENTS = Object.freeze(new Map([
  ['caption', set('table')],
  ['colgroup', set('table')],
  ['tbody', set('table')],
  ['thead', set('table')],
  ['tfoot', set('table')],
  ['col', set('table', 'colgroup')],
  ['tr', set('table', 'tbody', 'thead', 'tfoot')],
  ['td', set('table', 'tbody', 'thead', 'tfoot', 'tr')],
  ['th', set('table', 'tbody', 'thead', 'tfoot', 'tr')],
]));

/** M-19: allowed F for a block start tag (none is also allowed). */
export const BLOCK_FAMILY = set('td', 'th', 'caption');

/** M-24 c and j: list containers. */
export const LISTS = set('ul', 'ol', 'menu', 'dl');

/** M-24 c and j: list items. */
export const LIST_ITEMS = set('li', 'dd', 'dt');

/** M-24 k: values of F (M-09) where the WHATWG parser foster-parents content. */
export const TABLE_CONTEXT = set('table', 'tbody', 'thead', 'tfoot', 'tr', 'colgroup');

/** M-11 rule 3: start tags not allowed in noscript content. */
export const NOSCRIPT_FORBIDDEN = set(
  'frameset', 'math', 'noscript', 'plaintext', 'select', 'svg', 'template',
);
