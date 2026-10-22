// FEED-128: the deterministic WEB PARITY check. Two web renderers that claim
// the same DOM + stylesheet contract (`@exponential-at/ui-react` is the
// REFERENCE; `@exponential-at/ui-solid` the first candidate) are loaded side
// by side in ONE headless Chromium — same viewport, device scale 1, the
// conformance font set, reduced motion, animations and transitions off — and
// every case is compared four ways, all EXACT:
//   dom    the `.xui-surface` subtrees serialised (tag, attributes by name,
//          text; adjacent text nodes MERGED, as the jsdom helper does: the
//          HTML parser of Playwright's pinned Chromium splits an innerHTML
//          text past 65 536 chars, React's `<style>`, where `textContent`
//          keeps one node), generated ids normalised (parity-compare.ts);
//   style  every `[data-xui-id]` element's (and every overlay part's) full
//          computed style;
//   frame  the same elements' getBoundingClientRect, 0 px tolerance;
//   pixels full-page screenshots through pixelmatch (threshold 0, AA
//          excluded): 0 differing pixels.
// Not a manifest suite (the natives' reports keep the manifest's list): a
// web-only gate with its own report (parity-report.schema.json), checked by
// `bun run --filter @exponential-at/ui parity:check <report.json>`.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import pixelmatch from "pixelmatch"
import sharp from "sharp"
import type { Browser, BrowserContext, Page } from "playwright"
import { allCases, caseInput, fontFaceCss, fontFiles, MANIFEST_PATH, type ConformanceManifest, type FontManifest } from "./dump"
import { launchChromium, pageUrl, type Query, type RunningHarness, type WebHarness } from "./browser-page"
import { caseSlug, firstDomDiff, firstFrameDiff, firstStyleDiff, normalizeIds, PARITY_FORMAT, type DomNode, type ParityCaseResult, type ParityReport, type StyleTable } from "./parity-compare"

export * from "./parity-compare"

const repoRoot = join(import.meta.dirname, `..`, `..`, `..`)
const fixturesDir = join(import.meta.dirname, `..`, `fixtures`)

/** The forced states every control specimen is re-dumped in (`data-xs` on
 *  every `[data-xui-part]`; the recipe sheet honours `[data-xs~=state]`). */
export const FORCED_STATES = [`hover`, `focus`, `pressed`, `checked`, `open`, `disabled`, `invalid`] as const

/** The specimens whose component is a CONTROL (control-geometry.json's
 *  interactive set): they get the forced-state cases. */
export const CONTROL_COMPONENTS = [`Button`, `Toggle`, `Segmented`, `Input`, `Textarea`, `Select`, `DatePicker`, `Checkbox`, `Radio`, `Switch`, `Slider`, `Tabs`, `NumberField`, `ChipInput`, `TimePicker`, `DateRangePicker`]

export interface ParityCase {
  /** `matrix:<conformance key>`, `specimen:<id>`, `state:<id>:<state>`, `overlay:<n>`. */
  id: string
  group: `matrix` | `specimen` | `state` | `overlay`
  query: Query
  viewport: { width: number; height: number }
  /** A forced state written onto every part before the dump. */
  state?: string
  /** An overlay case: wait for `window.__xuiOverlay()` to place it. */
  overlay?: boolean
}



export interface ParityOptions {
  reference: WebHarness
  candidate: WebHarness
  /** Report names (default `reference` / `candidate`). */
  names?: { reference?: string; candidate?: string }
  /** Default: `parityCases()`. */
  cases?: ParityCase[]
  /** Extra Chromium flags (always PARITY_CHROMIUM_ARGS) and an
   *  executable (default `EXPONENTIAL_UI_CHROME`, else Playwright's). */
  chrome?: { args?: string[]; executablePath?: string }
  /** Where differing screenshots land (default <repo>/.conformance/parity). */
  outDir?: string
  /** Cases compared at once (default 4). */
  concurrency?: number
  log?: (line: string) => void
}

/** Chromium for exact pixels: unhinted fractional glyph advances, and
 *  DETERMINISTIC raster (software, whole-tile, sRGB, grayscale AA): without
 *  `--disable-partial-raster` + `--disable-gpu` a composited layer's
 *  anti-aliased rounded corners (a carousel scroller, an open drawer) come
 *  out ±1 level between two loads of the SAME page now and then. */
export const PARITY_CHROMIUM_ARGS = [`--font-render-hinting=none`, `--disable-gpu`, `--disable-partial-raster`, `--force-color-profile=srgb`, `--disable-lcd-text`]

const fixture = <T>(name: string) => JSON.parse(readFileSync(join(fixturesDir, name), `utf8`)) as T
const repoJson = async (path: string) => JSON.parse(readFileSync(join(repoRoot, path), `utf8`)) as unknown

/** Every parity case: the conformance matrix (fixtures × themes × modes ×
 *  widths × directions, each through the harness's `?view=tree` with the
 *  case's tree + data exactly as the conformance page builds them), every
 *  specimen at `specimenWidth`, the control specimens in each forced state,
 *  and every overlay-geometry case with its overlay open. */
export async function parityCases(opts: { specimenWidth?: number; states?: readonly string[]; controls?: readonly string[] } = {}): Promise<ParityCase[]> {
  const out: ParityCase[] = []
  const manifest = (await repoJson(MANIFEST_PATH)) as ConformanceManifest
  const inputs = new Map<string, { tree: string; data: string }>()
  for (const c of allCases(manifest)) {
    // The tree + data depend on the fixture and the direction only.
    const k = `${c.fixture}/${c.direction}`
    if (!inputs.has(k)) {
      const { tree, data } = await caseInput(manifest, c, repoJson)
      inputs.set(k, { tree: JSON.stringify(tree), data: JSON.stringify(data) })
    }
    const input = inputs.get(k)!
    out.push({ id: `matrix:${c.key}`, group: `matrix`, query: { view: `tree`, tree: input.tree, data: input.data, theme: c.theme, mode: c.mode, width: c.width, dir: c.direction, locale: manifest.locale }, viewport: { width: c.width, height: 900 } })
  }
  const width = opts.specimenWidth ?? 900
  const specimens = fixture<{ specimens: { id: string; component?: string }[] }>(`specimens.json`).specimens
  const query = (id: string): Query => ({ view: `specimen`, id, theme: `exponential`, mode: `light`, width })
  for (const s of specimens) out.push({ id: `specimen:${s.id}`, group: `specimen`, query: query(s.id), viewport: { width, height: 900 } })
  const controls = new Set(opts.controls ?? CONTROL_COMPONENTS)
  for (const s of specimens.filter((s) => s.component && controls.has(s.component)))
    for (const state of opts.states ?? FORCED_STATES) out.push({ id: `state:${s.id}:${state}`, group: `state`, query: query(s.id), viewport: { width, height: 900 }, state })
  const overlays = fixture<{ cases: { name: string; viewport: { width: number; height: number } }[] }>(`overlay-geometry.json`).cases
  for (const [i, c] of overlays.entries()) out.push({ id: `overlay:${i}`, group: `overlay`, query: { view: `overlay`, case: i }, viewport: c.viewport, overlay: true })
  return out
}

/** The in-page probe (a string, so no transpiler helper leaks into it):
 *  `window.__xuiParity` = { dom, table, values, force }. */
const PROBE = `(() => {
  const surfaces = () => Array.from(document.querySelectorAll('.xui-surface')).filter((s) => !s.parentElement || !s.parentElement.closest('.xui-surface'));
  const ser = (n) => {
    if (n.nodeType === 3) return n.data.length ? { x: n.data } : null;
    if (n.nodeType !== 1) return null;
    const a = Array.from(n.attributes).map((x) => [x.name, x.value]).sort((p, q) => (p[0] < q[0] ? -1 : p[0] > q[0] ? 1 : 0));
    const c = [];
    for (const k of Array.from(n.childNodes)) {
      const s = ser(k);
      if (!s) continue;
      const last = c[c.length - 1];
      if (s.x !== undefined && last && last.x !== undefined) last.x += s.x;
      else c.push(s);
    }
    return { t: n.tagName.toLowerCase(), a, c };
  };
  const compared = () => {
    const out = [];
    for (const s of surfaces()) for (const el of [s, ...Array.from(s.querySelectorAll('*'))]) {
      const id = el.getAttribute('data-xui-id');
      if (id !== null) { out.push([id, el]); continue; }
      const part = el.getAttribute('data-xui-part');
      if (part !== null && el.closest('[data-xui-layer]')) {
        const owner = el.parentElement && el.parentElement.closest('[data-xui-id]');
        out.push([(owner ? owner.getAttribute('data-xui-id') : '') + '/' + part, el]);
      }
    }
    return out;
  };
  const properties = () => Array.from(getComputedStyle(document.documentElement));
  const hash = (str) => {
    let h1 = 0xdeadbeef, h2 = 0x41c6ce57;
    for (let i = 0; i < str.length; i++) { const ch = str.charCodeAt(i); h1 = Math.imul(h1 ^ ch, 2654435761); h2 = Math.imul(h2 ^ ch, 1597334677); }
    h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
    h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
    return (h2 >>> 0).toString(16) + (h1 >>> 0).toString(16);
  };
  const valuesOf = (el, props) => { const cs = getComputedStyle(el); return props.map((p) => cs.getPropertyValue(p)); };
  window.__xuiParity = {
    dom: () => surfaces().map(ser),
    table: () => {
      const props = properties();
      return { properties: ['(hash)'], count: props.length, elements: compared().map(([key, el]) => {
        const r = el.getBoundingClientRect();
        return { key, values: [hash(valuesOf(el, props).join('\\u0001'))], frame: { x: r.x, y: r.y, w: r.width, h: r.height } };
      }) };
    },
    values: (i) => { const props = properties(); const e = compared()[i]; return { properties: props, elements: e ? [{ key: e[0], values: valuesOf(e[1], props), frame: { x: 0, y: 0, w: 0, h: 0 } }] : [] }; },
    force: (state) => {
      for (const s of surfaces()) for (const el of Array.from(s.querySelectorAll('[data-xui-part]'))) {
        const prev = el.getAttribute('data-xs');
        el.setAttribute('data-xs', prev ? prev + ' ' + state : state);
      }
    },
  };
})()`

/** Motion off, applied identically to both pages AFTER the surface's own
 *  sheets (and `!important`, so no rule of theirs wins). */
const STILL_CSS = `*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}`

const frames = (page: Page, n = 2) => page.evaluate(`new Promise((r) => { const step = (k) => (k <= 0 ? r(null) : requestAnimationFrame(() => step(k - 1))); step(${n}) })`)

interface Side {
  page: Page
  context: BrowserContext
  errors: string[]
}

async function openSide(browser: Browser, baseUrl: string, c: ParityCase, fontCss: string, fontAllow: Set<string>): Promise<Side> {
  const context = await browser.newContext({ viewport: c.viewport, deviceScaleFactor: 1, locale: `en-US`, reducedMotion: `reduce`, timezoneId: `UTC` })
  // Offline: the harness answers, the font set is served off disk, the
  // fixtures' example.com media fail the same way on both sides.
  await context.route(`**/*`, async (route) => {
    const url = new URL(route.request().url())
    if (url.pathname.startsWith(`/__xui-parity/fonts/`)) {
      const rel = decodeURIComponent(url.pathname.slice(`/__xui-parity/fonts/`.length))
      if (!fontAllow.has(rel)) return route.abort()
      return route.fulfill({ status: 200, contentType: `font/ttf`, body: readFileSync(join(repoRoot, rel)) })
    }
    if (!route.request().url().startsWith(baseUrl)) return route.abort()
    // The page request goes out WITHOUT its query (a tree in the URL is far
    // longer than a server's request line): the harness serves one page for
    // every query and the page still reads its own `location.search`.
    if (route.request().isNavigationRequest() && url.search) return route.continue({ url: `${baseUrl}${url.pathname}` })
    return route.continue()
  })
  // The font set, before the bundle runs (documentElement may not exist yet).
  await context.addInitScript({
    content: `(() => { const add = () => { if (!document.documentElement) return false; const s = document.createElement('style'); s.setAttribute('data-xui-parity', 'fonts'); s.textContent = ${JSON.stringify(fontCss)}; document.documentElement.appendChild(s); return true; }; if (!add()) { const mo = new MutationObserver(() => { if (add()) mo.disconnect(); }); mo.observe(document, { childList: true }); } })()`,
  })
  const page = await context.newPage()
  const errors: string[] = []
  page.on(`pageerror`, (e) => errors.push(String(e)))
  page.on(`console`, (m) => {
    if (m.type() === `error` && !m.text().startsWith(`Failed to load resource`)) errors.push(m.text())
  })
  await page.goto(pageUrl(baseUrl, c.query))
  await page.waitForSelector(`.xui-surface [data-xui-id]`, { state: `attached`, timeout: 30_000 })
  await page.evaluate(`(() => { const s = document.createElement('style'); s.setAttribute('data-xui-parity', 'still'); s.textContent = ${JSON.stringify(STILL_CSS)}; document.body.appendChild(s); })()`)
  await page.evaluate(`Promise.all(Array.from(document.fonts).map((f) => f.load().catch(() => undefined))).then(() => document.fonts.ready)`)
  if (c.overlay) await page.waitForFunction(`typeof window.__xuiOverlay === 'function' && window.__xuiOverlay() !== null`, undefined, { timeout: 30_000 })
  await frames(page, 2)
  await page.evaluate(PROBE)
  if (c.state) {
    await page.evaluate(`window.__xuiParity.force(${JSON.stringify(c.state)})`)
    await frames(page, 2)
  }
  return { page, context, errors }
}

async function raster(png: Buffer): Promise<{ width: number; height: number; data: Uint8Array }> {
  const { data, info } = await sharp(png).ensureAlpha().raw().toBuffer({ resolveWithObject: true })
  return { width: info.width, height: info.height, data: new Uint8Array(data.buffer, data.byteOffset, data.length) }
}

async function compareCase(browser: Browser, ref: RunningHarness, cand: RunningHarness, c: ParityCase, fontCss: string, fontAllow: Set<string>, outDir: string): Promise<ParityCaseResult> {
  const started = Date.now()
  const result: ParityCaseResult = { case: c.id, ok: false, elapsedMs: 0 }
  const sides: Side[] = []
  try {
    const [a, b] = await Promise.all([openSide(browser, ref.baseUrl, c, fontCss, fontAllow), openSide(browser, cand.baseUrl, c, fontCss, fontAllow)])
    sides.push(a, b)
    const errs = [...a.errors.map((e) => `reference: ${e}`), ...b.errors.map((e) => `candidate: ${e}`)]
    if (errs.length) throw new Error(`page errors:\n${errs.join(`\n`)}`)
    const [domA, domB] = (await Promise.all([a.page.evaluate(`window.__xuiParity.dom()`), b.page.evaluate(`window.__xuiParity.dom()`)])) as [DomNode[], DomNode[]]
    result.dom = firstDomDiff(normalizeIds(domA), normalizeIds(domB))
    const [tA, tB] = (await Promise.all([a.page.evaluate(`window.__xuiParity.table()`), b.page.evaluate(`window.__xuiParity.table()`)])) as [StyleTable, StyleTable]
    const hashDiff = firstStyleDiff(tA, tB)
    if (hashDiff && hashDiff.property === `(hash)`) {
      // The first element whose computed style differs: its full values.
      const i = tA.elements.findIndex((e, k) => e.values[0] !== tB.elements[k]?.values[0])
      const [vA, vB] = (await Promise.all([a.page.evaluate(`window.__xuiParity.values(${i})`), b.page.evaluate(`window.__xuiParity.values(${i})`)])) as [StyleTable, StyleTable]
      result.style = firstStyleDiff(vA, vB) ?? hashDiff
    } else if (hashDiff) result.style = hashDiff
    result.frame = firstFrameDiff(tA, tB)
    // Native <video>/<audio> boxes are MASKED (same colour on both sides):
    // the browser's own media controls run a loading spinner in the UA
    // shadow root no author CSS stops; their DOM, style and frames are still
    // compared above.
    const shot = (p: Page) => p.screenshot({ fullPage: true, animations: `disabled`, caret: `hide`, mask: [p.locator(`.xui-surface video, .xui-surface audio`)], maskColor: `#ff00ff` })
    const [pA, pB] = await Promise.all([shot(a.page), shot(b.page)])
    const [rA, rB] = await Promise.all([raster(pA), raster(pB)])
    const size = `${rA.width}x${rA.height}`
    let differing: number
    let diff: Uint8Array | undefined
    if (rA.width !== rB.width || rA.height !== rB.height) differing = Math.max(rA.width * rA.height, rB.width * rB.height)
    else if (Buffer.from(rA.data).equals(Buffer.from(rB.data))) differing = 0
    else {
      diff = new Uint8Array(rA.data.length)
      differing = pixelmatch(rA.data, rB.data, diff, rA.width, rA.height, { threshold: 0, includeAA: false })
    }
    result.pixels = { differing, size: rA.width === rB.width && rA.height === rB.height ? size : `${size} vs ${rB.width}x${rB.height}` }
    if (differing > 0) {
      mkdirSync(outDir, { recursive: true })
      const base = join(outDir, caseSlug(c.id))
      writeFileSync(`${base}.ref.png`, pA)
      writeFileSync(`${base}.cand.png`, pB)
      result.pixels.referencePng = `${base}.ref.png`
      result.pixels.candidatePng = `${base}.cand.png`
      if (diff) {
        await sharp(Buffer.from(diff), { raw: { width: rA.width, height: rA.height, channels: 4 } }).png().toFile(`${base}.diff.png`)
        result.pixels.diffPng = `${base}.diff.png`
      }
    }
    result.ok = !result.dom && !result.style && !result.frame && differing === 0
  } catch (e) {
    result.error = e instanceof Error ? e.message : String(e)
  } finally {
    await Promise.all(sides.map((s) => s.context.close().catch(() => undefined)))
    result.elapsedMs = Date.now() - started
  }
  for (const k of [`dom`, `style`, `frame`] as const) if (result[k] === undefined) delete result[k]
  return result
}

export async function runParity(options: ParityOptions): Promise<ParityReport> {
  const started = Date.now()
  const cases = options.cases ?? (await parityCases())
  const manifest = (await repoJson(MANIFEST_PATH)) as ConformanceManifest
  const fonts = (await repoJson(manifest.fonts)) as FontManifest
  const fontAllow = new Set(fontFiles(fonts))
  const fontCss = fontFaceCss(fonts, (file) => `/__xui-parity/fonts/${file}`)
  const outDir = options.outDir ?? join(repoRoot, `.conformance`, `parity`)
  const [ref, cand] = await Promise.all([options.reference.start(), options.candidate.start()])
  let browser: Browser | undefined
  let chromium = `?`
  const results: ParityCaseResult[] = new Array(cases.length)
  try {
    const args = [...PARITY_CHROMIUM_ARGS, ...(options.chrome?.args ?? [])]
    browser = await launchChromium(args, options.chrome?.executablePath)
    chromium = browser.version()
    const b = browser
    let next = 0
    let done = 0
    const worker = async () => {
      for (let i = next++; i < cases.length; i = next++) {
        results[i] = await compareCase(b, ref, cand, cases[i]!, fontCss, fontAllow, outDir)
        // A harness that went away mid-case (respawned by bunHarness): once more.
        if (results[i]!.error?.includes(`ERR_CONNECTION_REFUSED`)) {
          await new Promise((r) => setTimeout(r, 3000))
          results[i] = await compareCase(b, ref, cand, cases[i]!, fontCss, fontAllow, outDir)
        }
        done += 1
        const r = results[i]!
        if (!r.ok || done % 25 === 0) options.log?.(`${done}/${cases.length} ${r.ok ? `ok  ` : `FAIL`} ${r.case} (${r.elapsedMs} ms)${r.ok ? `` : ` ${JSON.stringify({ error: r.error, dom: r.dom, style: r.style, frame: r.frame, pixels: r.pixels }).slice(0, 600)}`}`)
      }
    }
    await Promise.all(Array.from({ length: Math.max(1, options.concurrency ?? 4) }, worker))
  } finally {
    await browser?.close()
    await Promise.all([ref.stop(), cand.stop()])
  }
  const byGroup: Record<string, { cases: number; passed: number }> = {}
  for (const [i, r] of results.entries()) {
    const g = (byGroup[cases[i]!.group] ??= { cases: 0, passed: 0 })
    g.cases += 1
    if (r.ok) g.passed += 1
  }
  const passed = results.filter((r) => r.ok).length
  return {
    format: PARITY_FORMAT,
    reference: options.names?.reference ?? `reference`,
    candidate: options.names?.candidate ?? `candidate`,
    chromium: `Chromium ${chromium} headless, ${PARITY_CHROMIUM_ARGS.join(` `)}, deviceScaleFactor 1, reduced motion`,
    startedAt: new Date(started).toISOString(),
    elapsedMs: Date.now() - started,
    summary: { cases: results.length, passed, failed: results.length - passed, byGroup },
    cases: results,
  }
}

