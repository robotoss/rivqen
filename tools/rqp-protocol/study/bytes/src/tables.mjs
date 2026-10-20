// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Markdown tables of the report (evidence/rqp-bytes/report.md) from a results
// file, so that no number of the report is copied by hand.

const n = (x) => x.toLocaleString('en-US');
const gb = (s) => `${n(s.gzip)} / ${n(s.br)}`;
const rgb = (s) => `${n(s.raw)} / ${n(s.gzip)} / ${n(s.br)}`;
const row = (cells) => `| ${cells.join(' | ')} |`;
const table = (head, rows, align) => [row(head), row(align ?? head.map((_, i) => (i === 0 ? '---' : '---:'))), ...rows.map(row)].join('\n');
const cell = (s) => (s.scenario === 'demo' ? 'demo' : `${s.blocks} ${s.size}${s.changed_blocks === undefined ? '' : ` ${s.changed_blocks}`}`);

/** Tables keyed by name. */
export function tables(r) {
  const out = {};
  const demo = r.bodies.find((b) => b.scenario === 'demo');
  const lab = r.comparison ?? [];
  const cmp = (scenario) => lab.find((c) => c.scenario === scenario);

  out.demo_visits = table(
    ['Visit', 'RQP raw / gzip / br', 'Legacy raw / gzip / br'],
    [
      ['Cold', rgb(demo.full), rgb(cmp('demo').cold.legacy)],
      ['Unchanged', `${demo.revisit_unchanged.status}, 0`, '304, 0'],
      ['Data change', rgb(demo.changed), rgb(cmp('demo').changed.legacy)],
    ],
  );

  const mr = demo.manifest_raw.rows;
  const mm = demo.manifest_marginal;
  out.demo_manifest = table(
    ['Component', 'Raw', 'Marginal gzip / br'],
    [
      ['Whole manifest element', n(demo.manifest_raw.total), gb(mm.manifest_element)],
      ['Script element wrapper', n(mr.wrapper), gb(mm.wrapper)],
      ['JSON structure', n(mr.json_structure), '—'],
      ['Member names', n(mr.member_names), `${gb(mm.member_names_vs_short)} (vs short names)`],
      ['`protocol` value', n(mr.protocol), `${gb(mm.protocol)} (member)`],
      ['`origin` and `path` values', n(mr.origin + mr.path), `${gb(mm.origin_path)} (members)`],
      ['`template_revision` (prefix, quotes / digest)', `${mr.template_revision_prefix_quotes} / ${mr.template_revision_digest}`, `${gb(mm.template_revision)} (member)`],
      ['`page_revision` (prefix, quotes / digest)', `${mr.page_revision_prefix_quotes} / ${mr.page_revision_digest}`, `${gb(mm.page_revision)} (member)`],
      ['Block ids and formats', n(mr.block_ids + mr.block_formats), `${gb(mm.block_ids_formats)} (members)`],
      ['Block digests', n(mr.block_digests), `${gb(mm.block_digests)} (members)`],
      ['`policy` and `limits` values', n(mr.policy_limits_values), `${gb(mm.policy_limits)} (members)`],
      ['Whole `blocks` member', '—', gb(mm.blocks_member)],
    ],
  );

  const dr = demo.delta_raw.rows;
  const dm = demo.delta_marginal;
  out.demo_delta = table(
    ['Component', 'Raw', 'Marginal gzip / br'],
    [
      ['Whole envelope', n(demo.delta_raw.total), gb(demo.changed)],
      ['JSON structure', n(dr.json_structure), '—'],
      ['Member names', n(dr.member_names), `${gb(dm.member_names_vs_short)} (vs short names)`],
      ['`type` and `protocol` values', n(dr.type + dr.protocol), `${gb(dm.type_protocol)} (members)`],
      ['`base_revision`', n(dr.base_revision_prefix_quotes + dr.base_revision_digest), `${gb(dm.base_revision)} (member)`],
      ['`next_revision`', n(dr.next_revision_prefix_quotes + dr.next_revision_digest), `${gb(dm.next_revision)} (member)`],
      ['`template_revision`', n(dr.template_revision_prefix_quotes + dr.template_revision_digest), `${gb(dm.template_revision)} (member)`],
      ['`op` and `format` values', n(dr.op_values + dr.format_values), `${gb(dm.op_and_format)} (members)`],
      ['`block_id` values', n(dr.block_id_values), '—'],
      ['Content: quotes / bytes / escapes', `${dr.content_quotes} / ${dr.content_bytes} / ${dr.content_escapes}`, '—'],
      [`Operation hashes (${demo.delta_raw.operations} operations)`, n(dr.op_hashes), `${gb(dm.op_hashes)} (members)`],
    ],
  );

  out.markup = table(
    ['Page', 'Blocks', '`data-rq-block` raw / gzip / br', 'Legacy markers raw / gzip / br'],
    r.bodies
      .filter((b) => b.scenario === 'demo' || b.changed_blocks === 'one')
      .map((b) => [cell({ ...b, changed_blocks: undefined }), b.scenario === 'demo' ? '4' : String(b.blocks), rgb(b.markup_cost_data_rq_block), rgb(b.markup_cost_legacy_markers)]),
  );

  out.grid_cold = table(
    ['Page', 'F gzip / br', 'Manifest gzip / br', 'Digests gzip / br', 'Legacy gzip / br'],
    r.bodies
      .filter((b) => b.scenario === 'demo' || b.changed_blocks === 'one')
      .map((b) => [
        cell({ ...b, changed_blocks: undefined }),
        gb(b.full),
        gb(b.manifest_marginal.manifest_element),
        gb(b.manifest_marginal.block_digests),
        cmp(b.scenario) ? gb(cmp(b.scenario).cold.legacy) : '—',
      ]),
  );

  out.grid_change = table(
    ['Page', 'RQP response', 'RQP gzip / br', 'Op hashes gzip / br', 'Legacy gzip / br'],
    r.bodies.map((b) => [
      cell(b),
      b.changed_kind === 'delta' ? `D, ${b.delta_raw.operations} op` : 'F (D not smaller)',
      gb(b.changed),
      b.delta_marginal ? gb(b.delta_marginal.op_hashes) : '—',
      cmp(b.scenario) ? gb(cmp(b.scenario).changed.legacy) : '—',
    ]),
  );

  const pickCells = (results) => ['demo', 'grid-4-small', 'grid-16-large', 'grid-64-small'].map((k) => results.find((x) => x.scenario === k || x.scenario.startsWith(`${k}-`)));
  const gains = (list, label) =>
    table(
      ['Option', ...['demo', '4 small', '16 large', '64 small'].map((c) => `${c} ${label}`)],
      list.map((o) => [`\`${o.id}\``, ...pickCells(o.results).map((x) => (x ? gb(x.gain) : '—'))]),
    );
  out.options_manifest = gains(r.options.manifest, 'gzip / br');
  const deltaAll = r.options.delta.map((o) => ({ ...o, results: o.results.filter((x) => x.scenario === 'demo' || x.changed_blocks === 'all') }));
  const deltaOne = r.options.delta.map((o) => ({ ...o, results: o.results.filter((x) => x.scenario === 'demo' || x.changed_blocks === 'one') }));
  out.options_delta_all = gains(deltaAll, 'gzip / br');
  out.options_delta_one = gains(deltaOne, 'gzip / br');

  if (r.headers !== null) {
    out.headers = r.headers
      .map((h) =>
        [`**${h.scenario}** (\`Rq-Have\` value: ${h.rq_have_value_bytes} bytes)`, '', table(
          ['Set', 'Request HTTP/1.1 / HPACK', 'Response HTTP/1.1 / HPACK'],
          Object.entries(h.sets).map(([k, v]) => [k, `${v.request_http1} / ${v.request_hpack}`, `${v.response_http1} / ${v.response_hpack}`]),
        )].join('\n'),
      )
      .join('\n\n');
  }

  // A variant D is sent only when it is smaller than F of the new page (negotiation section 4, step 7).
  const sentOrF = (v, b) => (v.raw < b.full_after_change.raw ? gb(v) : `F ${gb(b.full_after_change)}`);
  const variant = (list, id, scenario) => list.find((o) => o.id === id).results.find((x) => x.scenario === scenario)?.variant ?? null;
  out.proposals = table(
    ['Page', 'Visit', 'Baseline', 'P1', 'P2', 'Legacy'],
    r.bodies
      .filter((b) => b.scenario === 'demo' || b.changed_blocks === 'all')
      .flatMap((b) => {
        const c = cmp(b.scenario);
        const one = b.scenario === 'demo' ? 'demo' : `grid-${b.blocks}-${b.size}-one`;
        return [
          [cell({ ...b, changed_blocks: undefined }), 'cold', gb(b.full), gb(variant(r.options.manifest, 'MC1_defaults_and_minimal', one)), gb(variant(r.options.manifest, 'MC2_compact', one)), c ? gb(c.cold.legacy) : '—'],
          [
            '',
            'change',
            gb(b.changed),
            sentOrF(variant(r.options.delta, 'DC1_minimal_object', b.scenario), b),
            sentOrF(variant(r.options.delta, 'DC3_minimal_array', b.scenario), b),
            c ? gb(c.changed.legacy) : '—',
          ],
        ];
      }),
  );
  if (r.headers !== null && lab.length > 0) {
    // Whole visit = request header block + response header block (HPACK, new connection) + gzip body.
    const visitRows = (bodyKey, headerKey, label) => {
      const b = r.bodies.find((x) => x.scenario === bodyKey);
      const h = r.headers.find((x) => x.scenario === headerKey).sets;
      const c = cmp(bodyKey);
      const coldKey = bodyKey === 'demo' ? 'demo' : bodyKey.replace(/-(one|all)$/, '-one');
      const p1Cold = variant(r.options.manifest, 'MC1_defaults_and_minimal', coldKey).gzip;
      const p1Change = variant(r.options.delta, 'DC1_minimal_object', bodyKey);
      const p1ChangeBody = p1Change.raw < b.full_after_change.raw ? p1Change.gzip : b.full_after_change.gzip;
      const hp = (k) => h[k].request_hpack + h[k].response_hpack;
      const line = (visit, rqpSet, legacySet, rqpBody, p1Body, legacyBody) => [
        visit === 'cold' ? label : '',
        visit,
        n(hp(rqpSet) + rqpBody),
        n(hp(rqpSet) + p1Body),
        n(hp(legacySet) + legacyBody),
      ];
      return [
        line('cold', 'rqp_first', 'legacy_first', b.full.gzip, p1Cold, c.cold.legacy.gzip),
        line('unchanged', 'rqp_revisit_304', 'legacy_revisit_304', 0, 0, 0),
        line('change', 'rqp_revisit_delta', 'legacy_revisit_data', b.changed.gzip, p1ChangeBody, c.changed.legacy.gzip),
      ];
    };
    out.visit_totals = table(
      ['Page', 'Visit', 'RQP baseline', 'RQP P1', 'Legacy'],
      [
        ...visitRows('demo', 'demo', 'demo'),
        ...visitRows('grid-4-small-all', 'grid-4', '4 small all'),
        ...visitRows('grid-64-small-one', 'grid-64', '64 small one'),
        ...visitRows('grid-64-small-all', 'grid-64', '64 small all'),
      ],
    );
  }
  return out;
}
