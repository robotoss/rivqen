<script setup lang="ts">
// Interactive MODEL of page-load timelines: plain WebView vs Rivqen.
// It is a simple analytic model, not a measurement. Formulas are shown on the page.
import { computed, ref, watch } from 'vue'

const scenario = ref<'cold' | 'warm' | 'data'>('cold')
const rtt = ref(100) // ms
const mbps = ref(5) // Mbit/s
const webviewInit = ref(300) // ms, assumed
const serverMs = ref(50) // ms
const renderMs = ref(150) // ms
const cacheReadMs = ref(15) // ms
const fullKb = ref(2.2) // KB gzip, measured demo page
const patchKb = ref(0.5) // KB gzip, measured demo patch
const reuseConn = ref(false)
const playKey = ref(0)
const playing = ref(false)

const transfer = (kb: number) => (kb * 1024 * 8) / (mbps.value * 1_000_000) * 1000
const handshake = computed(() => (reuseConn.value ? 0 : 2 * rtt.value)) // TCP + TLS 1.3 ≈ 2 RTT
const fetchMs = (kb: number) => handshake.value + rtt.value + serverMs.value + transfer(kb)

type Seg = { label: string; start: number; dur: number; kind: string }
type Lane = { name: string; segs: Seg[]; firstContent: number; note: string }

const lanes = computed<Lane[]>(() => {
  const full = fetchMs(fullKb.value)
  const patch = fetchMs(patchKb.value)
  const w = webviewInit.value
  const r = renderMs.value
  if (scenario.value === 'cold') {
    const plainStart = w
    const rivqenDone = Math.max(w, full)
    return [
      { name: 'Plain WebView', note: 'Request starts after the WebView is ready.',
        segs: [
          { label: 'WebView init', start: 0, dur: w, kind: 'init' },
          { label: 'HTML request', start: plainStart, dur: full, kind: 'net' },
          { label: 'Render', start: plainStart + full, dur: r, kind: 'render' },
        ], firstContent: plainStart + full + r },
      { name: 'Rivqen', note: 'Request runs in parallel with WebView init.',
        segs: [
          { label: 'WebView init', start: 0, dur: w, kind: 'init' },
          { label: 'HTML request', start: 0, dur: full, kind: 'net' },
          { label: 'Render', start: rivqenDone, dur: r, kind: 'render' },
        ], firstContent: rivqenDone + r },
    ]
  }
  if (scenario.value === 'warm') {
    const cached = Math.max(w, cacheReadMs.value)
    return [
      { name: 'Plain WebView (HTTP cache, revalidate)', note: 'WebView waits for the 304 before it renders.',
        segs: [
          { label: 'WebView init', start: 0, dur: w, kind: 'init' },
          { label: 'Revalidate (304)', start: w, dur: fetchMs(0), kind: 'net' },
          { label: 'Render', start: w + fetchMs(0), dur: r, kind: 'render' },
        ], firstContent: w + fetchMs(0) + r },
      { name: 'Rivqen', note: 'Cached page shows at once; revalidation runs in the background.',
        segs: [
          { label: 'WebView init', start: 0, dur: w, kind: 'init' },
          { label: 'Cache read', start: 0, dur: cacheReadMs.value, kind: 'cache' },
          { label: 'Render cached', start: cached, dur: r, kind: 'render' },
          { label: 'Revalidate (304)', start: 0, dur: fetchMs(0), kind: 'bg' },
        ], firstContent: cached + r },
    ]
  }
  const cached = Math.max(w, cacheReadMs.value)
  return [
    { name: 'Plain WebView', note: 'Full page downloads again when any data changes.',
      segs: [
        { label: 'WebView init', start: 0, dur: w, kind: 'init' },
        { label: 'Full HTML', start: w, dur: full, kind: 'net' },
        { label: 'Render', start: w + full, dur: r, kind: 'render' },
      ], firstContent: w + full + r },
    { name: 'Rivqen', note: 'Cached page first; only changed blocks travel, then update in place.',
      segs: [
        { label: 'WebView init', start: 0, dur: w, kind: 'init' },
        { label: 'Cache read', start: 0, dur: cacheReadMs.value, kind: 'cache' },
        { label: 'Render cached', start: cached, dur: r, kind: 'render' },
        { label: 'Patch', start: 0, dur: patch, kind: 'bg' },
        { label: 'Apply', start: Math.max(cached + r, patch), dur: 20, kind: 'apply' },
      ], firstContent: cached + r },
  ]
})

// Each lane has one row per resource, so parallel steps do not hide each other.
const ROW_OF: Record<string, number> = { init: 0, render: 0, apply: 0, cache: 1, net: 2, bg: 2 }
const ROW_NAME = ['WebView', 'Cache', 'Network']
const rowsOf = (lane: Lane) => [...new Set(lane.segs.map((s) => ROW_OF[s.kind]))].sort()

const total = computed(() => Math.max(...lanes.value.flatMap((l) => l.segs.map((s) => s.start + s.dur))) * 1.05)
// Bars start after a fixed gutter that holds the row names.
const GUTTER = 62
const pos = (ms: number) => `calc(${GUTTER}px + (100% - ${GUTTER + 4}px) * ${ms / total.value})`
const len = (ms: number) => `calc((100% - ${GUTTER + 4}px) * ${ms / total.value})`
const gain = computed(() => {
  const [a, b] = lanes.value
  return { ms: Math.round(a.firstContent - b.firstContent), pct: Math.round((1 - b.firstContent / a.firstContent) * 100) }
})
const bytesNote = computed(() => scenario.value === 'data'
  ? `Bytes: ${fullKb.value.toFixed(1)} KB vs ${patchKb.value.toFixed(1)} KB (gzip)`
  : scenario.value === 'warm' ? 'Bytes: headers only for both' : `Bytes: ${fullKb.value.toFixed(1)} KB for both`)

function play() {
  playing.value = false
  playKey.value++
  requestAnimationFrame(() => requestAnimationFrame(() => (playing.value = true)))
}
watch([scenario, rtt, mbps, webviewInit, serverMs, renderMs, cacheReadMs, fullKb, patchKb, reuseConn], play)
play()
</script>

<template>
  <div class="rq-tl">
    <div class="rq-tl__badge">MODEL · illustrative, not a measurement</div>

    <div class="rq-tl__tabs" role="tablist">
      <button v-for="s in (['cold', 'warm', 'data'] as const)" :key="s" role="tab"
        :aria-selected="scenario === s" :class="{ on: scenario === s }" @click="scenario = s">
        {{ s === 'cold' ? 'First visit' : s === 'warm' ? 'Revisit, unchanged' : 'Revisit, data changed' }}
      </button>
      <button class="rq-tl__play" @click="play">▶ Replay</button>
    </div>

    <div v-for="lane in lanes" :key="lane.name + playKey" class="rq-tl__lane">
      <div class="rq-tl__lane-head">
        <strong>{{ lane.name }}</strong>
        <span>first content: <b>{{ Math.round(lane.firstContent) }} ms</b></span>
      </div>
      <div class="rq-tl__track" :style="{ height: rowsOf(lane).length * 26 + 6 + 'px' }">
        <div v-for="(r, ri) in rowsOf(lane)" :key="'r' + r" class="rq-tl__row" :style="{ top: ri * 26 + 3 + 'px' }">{{ ROW_NAME[r] }}</div>
        <div v-for="(s, i) in lane.segs" :key="i" class="rq-tl__seg" :class="['k-' + s.kind, { go: playing }]"
          :style="{ top: rowsOf(lane).indexOf(ROW_OF[s.kind]) * 26 + 3 + 'px', left: pos(s.start), width: len(s.dur), transitionDelay: (s.start / total) * 1.6 + 's', transitionDuration: Math.max(0.15, (s.dur / total) * 1.6) + 's' }"
          :title="`${s.label}: ${Math.round(s.dur)} ms`">
          <span>{{ s.label }}</span>
        </div>
        <div class="rq-tl__marker" :class="{ go: playing }" :style="{ left: pos(lane.firstContent), transitionDelay: (lane.firstContent / total) * 1.6 + 's' }" />
      </div>
      <div class="rq-tl__note">{{ lane.note }}</div>
    </div>

    <div class="rq-tl__legend">Striped bar: network work in the background, after the page is already visible. Red line: first content.</div>

    <div class="rq-tl__result">
      Rivqen first content is <b>{{ gain.ms }} ms</b> earlier (<b>{{ gain.pct }} %</b>) in this model. {{ bytesNote }}.
    </div>

    <details class="rq-tl__params" open>
      <summary>Parameters</summary>
      <label>RTT <input type="range" min="10" max="600" step="10" v-model.number="rtt"> <output>{{ rtt }} ms</output></label>
      <label>Bandwidth <input type="range" min="0.5" max="100" step="0.5" v-model.number="mbps"> <output>{{ mbps }} Mbit/s</output></label>
      <label>WebView init <input type="range" min="0" max="1500" step="25" v-model.number="webviewInit"> <output>{{ webviewInit }} ms</output></label>
      <label>Server time <input type="range" min="0" max="800" step="10" v-model.number="serverMs"> <output>{{ serverMs }} ms</output></label>
      <label>Render <input type="range" min="20" max="800" step="10" v-model.number="renderMs"> <output>{{ renderMs }} ms</output></label>
      <label>Full page <input type="range" min="0.5" max="200" step="0.5" v-model.number="fullKb"> <output>{{ fullKb }} KB</output></label>
      <label>Patch <input type="range" min="0.1" max="20" step="0.1" v-model.number="patchKb"> <output>{{ patchKb }} KB</output></label>
      <label class="chk"><input type="checkbox" v-model="reuseConn"> Reuse open connection (no TCP/TLS handshake)</label>
    </details>
  </div>
</template>

<style scoped>
.rq-tl { border: 1px solid var(--vp-c-divider); border-radius: 12px; padding: 16px; margin: 20px 0; background: var(--vp-c-bg-soft); }
.rq-tl__badge { display: inline-block; font-size: 12px; font-weight: 700; letter-spacing: .04em; padding: 2px 8px; border-radius: 999px; background: var(--vp-c-warning-soft); color: var(--vp-c-warning-1); margin-bottom: 12px; }
.rq-tl__tabs { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 16px; }
.rq-tl__tabs button { border: 1px solid var(--vp-c-divider); border-radius: 8px; padding: 4px 10px; font-size: 14px; background: var(--vp-c-bg); color: var(--vp-c-text-1); }
.rq-tl__tabs button.on { border-color: var(--vp-c-brand-1); color: var(--vp-c-brand-1); font-weight: 600; }
.rq-tl__play { margin-left: auto; }
.rq-tl__lane { margin-bottom: 14px; }
.rq-tl__lane-head { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px; font-size: 14px; margin-bottom: 4px; }
.rq-tl__track { position: relative; border-radius: 6px; background: var(--vp-c-bg); border: 1px solid var(--vp-c-divider); overflow: hidden; }
.rq-tl__row { position: absolute; left: 6px; width: 54px; border-right: 1px solid var(--vp-c-divider); height: 24px; line-height: 24px; font-size: 10px; text-transform: uppercase; letter-spacing: .05em; color: var(--vp-c-text-3); pointer-events: none; }
.rq-tl__seg { position: absolute; height: 24px; border-radius: 4px; transform: scaleX(0); transform-origin: left; transition-property: transform; transition-timing-function: linear; overflow: hidden; }
.rq-tl__seg.go { transform: scaleX(1); }
.rq-tl__seg span { display: block; padding: 3px 6px; font-size: 11px; line-height: 18px; white-space: nowrap; color: #fff; overflow: hidden; text-overflow: ellipsis; }
.k-init { background: #64748b; }
.k-net { background: #3b82f6; }
.k-render { background: #0e7c86; }
.k-cache { background: #8b5cf6; }
.k-apply { background: #f59e0b; }
.k-bg { background: repeating-linear-gradient(45deg, #3b82f6, #3b82f6 6px, #60a5fa 6px, #60a5fa 12px); opacity: .75; }
.rq-tl__marker { position: absolute; top: 0; bottom: 0; width: 2px; background: #ef4444; opacity: 0; transition: opacity .2s; }
.rq-tl__marker.go { opacity: 1; }
.rq-tl__note { font-size: 13px; color: var(--vp-c-text-2); margin-top: 4px; }
.rq-tl__legend { font-size: 12px; color: var(--vp-c-text-2); margin: -4px 0 8px; }
.rq-tl__result { margin: 8px 0 12px; padding: 10px 12px; border-radius: 8px; background: var(--vp-c-brand-soft); font-size: 14px; }
.rq-tl__params summary { cursor: pointer; font-weight: 600; margin-bottom: 8px; }
.rq-tl__params label { display: grid; grid-template-columns: 110px 1fr 90px; align-items: center; gap: 8px; font-size: 13px; margin: 4px 0; }
.rq-tl__params label.chk { display: flex; }
.rq-tl__params output { text-align: right; font-variant-numeric: tabular-nums; }
@media (max-width: 480px) { .rq-tl__params label { grid-template-columns: 90px 1fr 70px; } .rq-tl__play { margin-left: 0; } }
@media (prefers-reduced-motion: reduce) { .rq-tl__seg { transition: none; } }
</style>
