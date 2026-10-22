// FEED-130: the BROWSER HARNESS on the custom element — the SAME page
// protocol as the React and Solid harnesses (the conformance runner and the
// web-parity suite drive all three), in vanilla TypeScript: every surface is
// an `<exponential-surface>` configured through its attributes, properties,
// methods and `xui-*` events only, which is the point (the element API
// suffices). The one exception is `__xuiVList` (harness/vlist.tsx), the bare
// Solid `WindowedList` primitive, not a surface.
//   ?view=kitchen-sink&theme=<id>&mode=light|dark&width=<px>&rtl=1&echo=150
//   ?view=geometry&case=900|390|900-rtl|390-rtl        (fixed fake measure)
//   ?view=overlay&case=<n>                              (overlay-geometry.json)
//   ?view=catalog                                        (every fixture case)
//   ?view=conditions&case=<n>&ctx=<n>                    (style-conditions.json)
//   ?view=tree&tree=<json>&data=<json>&width=&height=&viewportHeight=&locale=&dir=&gen=<key>:<n>
//   ?view=controls&theme=<id>                            (control-geometry.json)
//   ?view=specimen&id=<id>&theme=<id>&mode=&width=&rtl=1 (one specimens.json entry)
//   ?view=mount                                          (__xuiMount / __xuiVList)
//   ?view=bench / ?view=round2
//   ?view=bench-suite&scenario=<id>                      (VAPP-105: one scenario of @exponential-at/ui-bench; __xuiBenchSuite drives it)

import "@exponential-at/ui-element/define"
import { CORE_CATALOG_ID, reduceNested, reduceSurface, BUILTIN_THEME_IDS, THEME_SCHEMA_ID, componentDef } from "@exponential-at/ui"
import type { FlatComponent, NestedNode, UiNode } from "@exponential-at/ui"
import { BASE_CSS } from "@exponential-at/ui/web"
import benchFixture from "@exponential-at/ui/fixtures/bench-list.json" with { type: "json" }
import kitchenSink from "@exponential-at/ui/fixtures/kitchen-sink.json" with { type: "json" }
import geometry from "@exponential-at/ui/fixtures/layout-geometry.json" with { type: "json" }
import overlays from "@exponential-at/ui/fixtures/overlay-geometry.json" with { type: "json" }
import components from "@exponential-at/ui/fixtures/catalog-components.json" with { type: "json" }
import conditions from "@exponential-at/ui/fixtures/style-conditions.json" with { type: "json" }
import controlGeometry from "@exponential-at/ui/fixtures/control-geometry.json" with { type: "json" }
import specimens from "@exponential-at/ui/fixtures/specimens.json" with { type: "json" }
// The kitchen sink's data model is the React package's fixture (one file
// for every harness; a JSON file, not renderer code).
import sinkData from "../../exponential-ui-react/fixtures/kitchen-sink.data.json" with { type: "json" }
import benchManifest from "@exponential-at/ui-bench/scenarios/manifest.json" with { type: "json" }
import benchColdStart from "@exponential-at/ui-bench/scenarios/cold-start.json" with { type: "json" }
import benchCardGrid from "@exponential-at/ui-bench/scenarios/card-grid.json" with { type: "json" }
import benchList100k from "@exponential-at/ui-bench/scenarios/list-100k.json" with { type: "json" }
import benchStreaming from "@exponential-at/ui-bench/scenarios/streaming.json" with { type: "json" }
import benchThemeSwitch from "@exponential-at/ui-bench/scenarios/theme-switch.json" with { type: "json" }
import benchResize from "@exponential-at/ui-bench/scenarios/resize.json" with { type: "json" }
import benchDeepNesting from "@exponential-at/ui-bench/scenarios/deep-nesting.json" with { type: "json" }
import benchTemplate10k from "@exponential-at/ui-bench/scenarios/template-10k.json" with { type: "json" }
import benchOverlay from "@exponential-at/ui-bench/scenarios/overlay.json" with { type: "json" }
import benchTable1k from "@exponential-at/ui-bench/scenarios/table-1k.json" with { type: "json" }
import { TAG, createSurface } from "@exponential-at/ui-element"
import type { ExponentialSurfaceElement, HostPlugin, SurfaceHandle } from "@exponential-at/ui-element"
import { harnessIcons } from "./icons"
import { mountVList, type VListCase, type VListProbe } from "./vlist"

const params = new URLSearchParams(location.search)
const view = params.get(`view`) ?? `kitchen-sink`
const themeId = params.get(`theme`) ?? `exponential`
const mode = (params.get(`mode`) ?? `dark`) as `light` | `dark`
const rtl = params.get(`rtl`) === `1`
const width = params.get(`width`) ? Number(params.get(`width`)) : undefined
const echoMs = params.get(`echo`) ? Number(params.get(`echo`)) : 150

declare global {
  interface Window {
    __xuiFrames?: (rootId?: string) => { id: string; x: number; y: number; w: number; h: number }[]
    __xuiLog: unknown[]
    __xuiOverlay?: () => { x: number; y: number; w: number; h: number; side: string } | null
    __xuiData?: () => unknown
    __xuiControls?: () => Record<string, Record<string, number> | null>
    __xuiBench?: { firstPaintMs?: number; handle: () => SurfaceHandle | null; flush: (fn: () => void) => void }
    __xuiHandle?: () => SurfaceHandle | null
    __xuiMount?: (tree: NestedNode, options?: MountOptions) => Promise<void>
    __xuiVList?: (c: VListCase) => Promise<VListProbe>
    __xuiBenchSuite?: BenchSuiteApi
  }
}
window.__xuiLog = []

/** Every painted node's box relative to the root node's (the core's frames). */
window.__xuiFrames = (rootId = `root`) => {
  const origin = document.querySelector<HTMLElement>(`[data-xui-id="${rootId}"]`)
  if (!origin) return []
  const o = origin.getBoundingClientRect()
  return Array.from(document.querySelectorAll<HTMLElement>(`[data-xui-id]`)).map((el) => {
    const r = el.getBoundingClientRect()
    return { id: el.dataset.xuiId!, x: r.left - o.left, y: r.top - o.top, w: r.width, h: r.height }
  })
}

type ActionEvent = Parameters<NonNullable<HostPlugin[`onAction`]>>[0]
type InputEvent = Parameters<NonNullable<HostPlugin[`onInput`]>>[0]
type Waitable<T> = T & { waitUntil: (promise: Promise<unknown>) => void }

/** The element's settable surface (properties, camelCase). */
type SurfaceProps = Partial<Pick<ExponentialSurfaceElement, `surface` | `root` | `templates` | `data` | `theme` | `mode` | `direction` | `width` | `height` | `locale` | `surfaceId` | `plugin` | `measure` | `viewportHeight` | `states`>>

/** A new `<exponential-surface>` with `props` set as properties (before it
 *  is attached: the element paints on connect). Undefined values stay unset. */
function surfaceEl(props: SurfaceProps): ExponentialSurfaceElement {
  const el = document.createElement(TAG) as ExponentialSurfaceElement
  for (const [k, v] of Object.entries(props)) if (v !== undefined) (el as unknown as Record<string, unknown>)[k] = v
  return el
}

function onAction(el: HTMLElement, fn: (e: Waitable<ActionEvent>) => void): void {
  el.addEventListener(`xui-action`, (ev) => fn((ev as CustomEvent<Waitable<ActionEvent>>).detail))
}
function onInput(el: HTMLElement, fn: (e: Waitable<InputEvent>) => void): void {
  el.addEventListener(`xui-input`, (ev) => fn((ev as CustomEvent<Waitable<InputEvent>>).detail))
}

/** A `div` with inline `css` and `children`. */
function div(css: string, ...children: (Node | string)[]): HTMLDivElement {
  const el = document.createElement(`div`)
  if (css) el.style.cssText = css
  el.append(...children)
  return el
}

const px = (v: number | string | undefined) => (typeof v === `number` ? `${v}px` : v)

function KitchenSink(): Node {
  const { $comment: _c, ...data } = sinkData as Record<string, unknown>
  const surface = createSurface({ surfaceId: `ks`, initial: kitchenSink as unknown as NestedNode, data })
  window.__xuiData = () => surface.data
  const el = surfaceEl({ surfaceId: `ks`, surface, plugin: { icons: harnessIcons }, theme: themeId, mode, direction: rtl ? `rtl` : `ltr`, width: width ?? `100%`, locale: params.get(`locale`) ?? undefined })
  const hostText = document.createTextNode(``)
  onAction(el, (e) => {
    window.__xuiLog.push({ action: e.name, event: e.event, componentId: e.componentId, context: e.context, payload: e.payload })
    e.waitUntil(new Promise((r) => setTimeout(r, echoMs)))
  })
  onInput(el, (e) => {
    window.__xuiLog.push({ input: e.name, value: e.value, revision: e.revision, kind: e.kind })
    // The simulated host: echoes the value into the data model after one
    // round trip, then acknowledges the revision.
    e.waitUntil(
      new Promise<void>((resolve) =>
        setTimeout(() => {
          if (e.path) surface.setData(e.path, e.value)
          hostText.data = String(e.value ?? ``)
          resolve()
        }, echoMs)
      )
    )
  })
  const echo = div(`font: 12px ui-monospace, monospace; opacity: 0.7; padding: 8px`, `host: `, hostText)
  echo.id = `host-echo`
  echo.dataset.testid = `host-echo`
  return div(`display: flex; flex-direction: column; gap: 8px; align-items: flex-start`, el, echo)
}

function Geometry(): Node {
  const c = (geometry.cases as Record<string, { width: number; direction: `ltr` | `rtl` }>)[params.get(`case`) ?? `900`]
  const { root } = reduceNested(geometry.surface as unknown as NestedNode, { catalogId: CORE_CATALOG_ID })
  // The case's direction overrides the root's `direction`.
  root.style = { ...(root.style ?? {}), direction: c.direction }
  const measures = geometry.measures as Record<string, { w: number; h: number }>
  return surfaceEl({ surfaceId: `geo`, root, theme: `neutral`, mode: `light`, width: c.width, direction: c.direction, measure: (node: UiNode) => measures[node.id] ?? null })
}

function Overlay(): Node {
  const i = Number(params.get(`case`) ?? 0)
  const c = (overlays.cases as { anchor: { x: number; y: number; width: number; height: number }; size: { width: number; height: number }; viewport: { width: number; height: number }; side: string; align?: string }[])[i]
  const tree: NestedNode = {
    id: `root`,
    component: `Box`,
    style: { position: `relative`, width: c.viewport.width, height: c.viewport.height, overflow: `hidden` },
    children: [
      {
        id: `pop`,
        component: `Popover`,
        props: { open: true, side: c.side },
        style: { position: `absolute`, left: c.anchor.x, top: c.anchor.y, width: c.anchor.width, height: c.anchor.height },
        slots: { trigger: { id: `anchor`, component: `Box`, style: { width: c.anchor.width, height: c.anchor.height, backgroundColor: `$color.primary` } } },
        children: [{ id: `content`, component: `Box`, style: { width: c.size.width, height: c.size.height, backgroundColor: `$color.success` } }],
      },
    ],
  }
  const { root } = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
  window.__xuiOverlay = () => {
    const el = document.querySelector<HTMLElement>(`[data-xui-overlay="Popover"]`)
    if (!el) return null
    const r = el.getBoundingClientRect()
    return { x: r.left, y: r.top, w: r.width, h: r.height, side: el.dataset.side ?? `` }
  }
  return surfaceEl({ surfaceId: `ov`, root, theme: `neutral`, mode: `light`, width: c.viewport.width })
}

function Catalog(): Node {
  const cases = (components as { cases: { name: string; node: NestedNode }[] }).cases
  return div(
    `display: grid; gap: 16px; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); padding: 16px`,
    ...cases.map((c) => {
      // A toast stays (duration 0) so a slow page still finds it painted.
      const node = c.node.component === `Toast` ? { ...c.node, props: { ...c.node.props, duration: 0 } } : c.node
      const { root, templates } = reduceNested(node as NestedNode, { catalogId: CORE_CATALOG_ID })
      return div(`display: flex; flex-direction: column; gap: 6px`, div(`font: 11px ui-monospace, monospace; opacity: 0.6`, c.name), surfaceEl({ surfaceId: c.name.replace(/[^a-z0-9]/gi, `-`), root, templates, theme: themeId, mode, plugin: { icons: harnessIcons } }))
    })
  )
}

/** One style-conditions.json case × context. */
function Conditions(): Node {
  type Ctx = { width: number; height?: number; states?: string[]; breakpoints?: Record<string, number> }
  const c = (conditions.cases as { style: Record<string, unknown>; contexts: Ctx[] }[])[Number(params.get(`case`) ?? 0)]
  const ctx = c.contexts[Number(params.get(`ctx`) ?? 0)]
  const theme = ctx.breakpoints ? { $schema: THEME_SCHEMA_ID, id: `bp`, name: `Breakpoints`, extends: `neutral`, tokens: { breakpoint: ctx.breakpoints } } : `neutral`
  const { root } = reduceNested({ id: `n`, component: `Box`, style: { ...c.style, width: 40, height: 40 } } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID, validate: false })
  return surfaceEl({ surfaceId: `cond`, root, theme: theme as never, mode: `light`, width: ctx.width, viewportHeight: ctx.height, height: px(ctx.height), states: ctx.states?.map((s) => s) })
}

/** Any nested tree from the query (keyboard + overlay suites, the parity matrix). */
function Tree(): Node {
  const nested = JSON.parse(params.get(`tree`) ?? `{}`) as NestedNode
  const data = JSON.parse(params.get(`data`) ?? `{}`) as Record<string, unknown>
  const gen = params.get(`gen`)?.split(`:`)
  if (gen?.length === 2) data[gen[0]] = Array.from({ length: Number(gen[1]) }, (_, i) => ({ id: `r${i}`, title: `Item ${i}` }))
  const surface = createSurface({ surfaceId: `tree`, initial: nested, data })
  window.__xuiData = () => surface.data
  const h = params.get(`height`)
  const vh = params.get(`viewportHeight`)
  const dir = params.get(`dir`)
  const el = surfaceEl({
    surfaceId: `tree`,
    surface,
    plugin: { icons: harnessIcons },
    theme: themeId,
    mode,
    width: width ?? `100%`,
    locale: params.get(`locale`) ?? undefined,
    direction: dir === `rtl` || dir === `ltr` ? dir : undefined,
    viewportHeight: vh ? Number(vh) : undefined,
    height: h ? px(Number(h)) : undefined,
  })
  onAction(el, (e) => void window.__xuiLog.push({ action: e.name, event: e.event, componentId: e.componentId, context: e.context }))
  return el
}

/** Every control-geometry case of one theme, each its own surface. */
function Controls(): Node {
  type Entry = { part: string; cases: Record<string, { props: Record<string, unknown> }> }
  const byComponent = (controlGeometry.themes as unknown as Record<string, Record<string, Entry>>)[themeId] ?? {}
  const all = Object.entries(byComponent).flatMap(([component, entry]) => Object.entries(entry.cases).map(([name, c]) => ({ key: `${component} ${name}`, component, part: entry.part, props: c.props })))
  window.__xuiControls = () => {
    const out: Record<string, Record<string, number> | null> = {}
    for (const c of all) {
      const box = document.querySelector<HTMLElement>(`[data-case="${c.key}"]`)
      const el = box?.querySelector<HTMLElement>(`.xui-${c.component}-${c.part}`)
      if (!el) {
        out[c.key] = null
        continue
      }
      const r = el.getBoundingClientRect()
      const cs = getComputedStyle(el)
      const n = (v: string) => parseFloat(v) || 0
      out[c.key] = {
        width: r.width,
        height: r.height,
        minHeight: n(cs.minHeight),
        paddingHorizontal: n(cs.paddingLeft),
        paddingVertical: n(cs.paddingTop),
        gap: n(cs.columnGap),
        borderWidth: n(cs.borderLeftWidth),
        borderRadius: Math.min(n(cs.borderTopLeftRadius), 9999),
      }
    }
    return out
  }
  return div(
    `display: flex; flex-wrap: wrap; gap: 16px; padding: 16px; align-items: flex-start`,
    ...all.map((c) => {
      const example = (componentDef(c.component)?.example ?? {}) as Record<string, unknown>
      const { root, templates } = reduceNested({ id: `root`, component: c.component, props: { ...example, ...c.props } }, { catalogId: CORE_CATALOG_ID })
      const box = div(`width: 360px`, surfaceEl({ surfaceId: c.key.replace(/[^a-z0-9]/gi, `-`), root, templates, theme: themeId, mode: `light`, plugin: { icons: harnessIcons } }))
      box.dataset.case = c.key
      return box
    })
  )
}

/** One specimens.json entry alone (the per-component docs shot). */
function Specimen(): Node {
  const id = params.get(`id`) ?? ``
  const entry = (specimens as unknown as { specimens: { id: string; node: NestedNode }[] }).specimens.find((s) => s.id === id)
  if (!entry) return div(``, `unknown specimen ${id}`)
  const surface = createSurface({ surfaceId: id || `specimen`, catalogId: CORE_CATALOG_ID, initial: entry.node })
  const el = surfaceEl({ surfaceId: `specimen`, surface, plugin: { icons: harnessIcons }, theme: themeId, mode, direction: rtl ? `rtl` : `ltr`, width: width ?? `100%` })
  onAction(el, (e) => {
    window.__xuiLog.push({ action: e.name, event: e.event, componentId: e.componentId, context: e.context, payload: e.payload })
    e.waitUntil(new Promise((r) => setTimeout(r, echoMs)))
  })
  onInput(el, (e) => {
    window.__xuiLog.push({ input: e.name, value: e.value, revision: e.revision, kind: e.kind })
    e.waitUntil(
      new Promise<void>((resolve) =>
        setTimeout(() => {
          if (e.path) surface.setData(e.path, e.value)
          resolve()
        }, echoMs)
      )
    )
  })
  const box = div(``, el)
  box.dataset.testid = `exponential-ui-specimen`
  box.dataset.specimen = id
  return box
}

/** The 100,000-row list bench (fixtures/bench-list.json). */
function Bench(): Node {
  const t0 = performance.now()
  const spec = benchFixture.rows
  const rows = Array.from({ length: spec.count }, (_, i) => ({ id: `r${i}`, title: `Row ${i}`, meta: String(i % 97) }))
  const r = reduceSurface(benchFixture.components as unknown as FlatComponent[], { catalogId: benchFixture.catalogId })
  const el = surfaceEl({ surfaceId: `bench`, root: r.root, templates: r.templates, data: { rows }, theme: benchFixture.theme, mode: `light`, width: benchFixture.viewport.width })
  // After the element painted on attach (the caller appends it this task).
  queueMicrotask(() =>
    requestAnimationFrame(() => {
      // Solid applies writes synchronously: `flush` just runs them.
      window.__xuiBench = { firstPaintMs: performance.now() - t0, handle: () => el.handle, flush: (fn) => fn() }
    })
  )
  return el
}

/** The round-2 browser checks' surface (browser/round2.test.ts). */
function Round2(): Node {
  const days = [`Today`, `Yesterday`, `Earlier`]
  const data = { sizes: [30, 70], items: Array.from({ length: 120 }, (_, i) => ({ id: `i${i}`, day: days[Math.min(2, Math.floor(i / 40))], title: `Item ${i}` })), chips: Array.from({ length: 300 }, (_, i) => ({ id: `c${i}`, label: `Chip ${i}` })) }
  const r = reduceNested(
    {
      id: `root`,
      component: `Box`,
      style: { display: `flex`, flexDirection: `column`, gap: 16, padding: 16 },
      children: [
        { id: `rz`, component: `Resizable`, props: { sizes: { path: `/sizes` }, panels: [{ min: 20 }, {}], handle: true }, style: { height: 120 }, children: [{ id: `rz-a`, component: `Text`, props: { text: `A` } }, { id: `rz-b`, component: `Text`, props: { text: `B` } }] },
        { id: `sec`, component: `List`, props: { sectionBy: `day`, stickyHeaders: true, divided: true }, style: { height: 300 }, template: { component: `sec-row`, path: `/items`, key: `id` }, slots: { section: { id: `sec-head`, component: `Text`, props: { text: { path: `value` } } } }, children: [] },
        { id: `sec-row`, component: `Text`, props: { text: { path: `title` } }, style: { height: 40 } },
        { id: `hl`, component: `List`, props: { direction: `horizontal`, gap: `sm` }, style: { width: 358 }, template: { component: `hl-chip`, path: `/chips`, key: `id` }, children: [] },
        { id: `hl-chip`, component: `Text`, props: { text: { path: `label` } }, style: { width: 80 } },
        { id: `anim`, component: `Box`, style: { width: 40, height: 40, animation: `pulse`, backdropBlur: `$blur.md`, backgroundColor: `#ffffff80` } },
        { id: `col`, component: `Box`, style: { display: `flex`, flexDirection: `column` }, children: [{ id: `dr`, component: `Drawer`, props: { title: `Filters` }, slots: { trigger: { id: `dr-t`, component: `Button`, props: { label: `Filters` } } }, children: [] }] },
        { id: `vid`, component: `Video`, props: { src: `` } },
        { id: `ch`, component: `Chart`, props: { kind: `bar`, height: 180, title: `Runs`, categories: [`a`, `b`], series: [{ name: `x`, values: [1, 2] }, { name: `y`, values: [2, 1] }] } },
      ],
    } as unknown as NestedNode,
    { catalogId: CORE_CATALOG_ID }
  )
  const el = surfaceEl({ surfaceId: `r2`, root: r.root, templates: r.templates, data, theme: `neutral`, mode: `light`, width: 390, direction: rtl ? `rtl` : `ltr`, plugin: { icons: harnessIcons } })
  window.__xuiHandle = () => el.handle
  onAction(el, (e) => void window.__xuiLog.push({ action: e.name, context: e.context }))
  return el
}

/** `?view=mount` (the shared runner's round-2 suites): `__xuiMount(tree,
 *  options)` paints any nested tree in a fresh element; `__xuiVList(case)`
 *  mounts a bare `WindowedList` in a scroller of `viewport` px. */
interface MountOptions {
  data?: Record<string, unknown>
  width?: number
  dir?: `ltr` | `rtl`
  /** A built-in theme id (default neutral). */
  theme?: string
  /** `$control.hairline` override (a Resizable handle's thickness). */
  hairline?: number
}
const frames = (n = 2) =>
  new Promise<void>((resolve) => {
    const step = (left: number) => (left <= 0 ? resolve() : requestAnimationFrame(() => step(left - 1)))
    step(n)
  })
let disposeMounted: (() => void) | null = null
/** A fresh container in `#mount` (the previous mount disposed). */
function remount(): HTMLElement {
  disposeMounted?.()
  disposeMounted = null
  const host = document.getElementById(`mount`)!
  host.replaceChildren()
  const el = document.createElement(`div`)
  host.appendChild(el)
  return el
}
function Mount(): Node {
  window.__xuiMount = async (tree, options = {}) => {
    const { root, templates } = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
    const base = options.theme ?? `neutral`
    const theme = options.hairline === undefined ? base : ({ $schema: THEME_SCHEMA_ID, id: `hairline`, name: `Hairline`, extends: base, tokens: { control: { hairline: options.hairline } } } as never)
    const box = remount()
    const el = surfaceEl({ surfaceId: `m`, root, templates, data: options.data, theme, mode: `light`, width: options.width ?? `100%`, direction: options.dir, plugin: { icons: harnessIcons } })
    onAction(el, (e) => void window.__xuiLog.push({ action: e.name, context: e.context }))
    window.__xuiHandle = () => el.handle
    box.appendChild(el)
    disposeMounted = () => el.remove()
    await frames(3)
  }
  window.__xuiVList = async (c) => {
    const box = remount()
    const { probe, dispose } = await mountVList(box, c)
    disposeMounted = dispose
    return probe
  }
  const style = document.createElement(`style`)
  style.textContent = BASE_CSS
  const mount = document.createElement(`div`)
  mount.id = `mount`
  const frag = document.createDocumentFragment()
  frag.append(style, mount)
  return frag
}

/** VAPP-105: ONE scenario of the benchmark suite
 *  (packages/exponential-ui-bench/scenarios/<id>.json) on one
 *  `<exponential-surface>` driven through its properties and methods only,
 *  from outside by `window.__xuiBenchSuite` (runners/element.ts): the SAME
 *  driver surface as the React and Solid harnesses' `BenchSuiteApi`, so one
 *  page script measures all three. A property set is a signal write the
 *  Solid painter applies synchronously, so `flush` just runs the mutation;
 *  one "pass" = the writes applied + a layout read. */
interface BenchSuiteApi {
  ready: boolean
  scenario: string
  firstPaintMs?: number
  /** cold-start: from the harness bundle's responseEnd to the first painted frame. */
  coldStartMs?: number
  flush: (fn: () => void) => void
  apply: (message: unknown) => void
  setData: (path: string, value: unknown) => void
  setTheme: (id: string) => void
  setMode: (mode: `light` | `dark`) => void
  setWidth: (px: number) => void
  scrollToIndex: (id: string, index: number, align?: `start` | `center` | `end` | `nearest`) => boolean
  /** Scroll the node's scroller by `by` px (its own box when it scrolls, else its first scrolling descendant) and fire `scroll`. */
  scroll: (id: string, by: number) => number
  /** A synchronous layout read on the surface root (ends a pass). */
  read: () => number
  nodes: () => number
  depth: () => number
  renderedItems: (selector: string) => number
  has: (selector: string) => boolean
}
type BenchScenario = {
  id: string
  theme: string
  mode: `light` | `dark`
  catalogId: string
  viewport: { width: number; height: number }
  surface: { nested: NestedNode } | { components: FlatComponent[] }
  data?: Record<string, unknown> | { generate: { path: string; count: number; item: Record<string, string> } }
  steps: Record<string, unknown>
}
const BENCH_SCENARIOS: Record<string, BenchScenario> = Object.fromEntries(
  [benchColdStart, benchCardGrid, benchList100k, benchStreaming, benchThemeSwitch, benchResize, benchDeepNesting, benchTemplate10k, benchOverlay, benchTable1k].map((s) => [s.id, s as unknown as BenchScenario])
)
/** src/scenarios.ts generateItems, ported: every field a string. */
function benchItems(spec: { count: number; item: Record<string, string> }): Record<string, string>[] {
  const fill = (template: string, i: number) => template.replace(/\{i % (\d+)\}/g, (_, n: string) => String(i % Number(n))).replace(/\{i\}/g, String(i))
  return Array.from({ length: spec.count }, (_, i) => Object.fromEntries(Object.entries(spec.item).map(([k, t]) => [k, fill(t, i)])))
}
function benchData(scenario: BenchScenario): Record<string, unknown> {
  const data = scenario.data
  if (!data) return {}
  if (`generate` in data && typeof data.generate === `object`) {
    const spec = (data as { generate: { path: string; count: number; item: Record<string, string> } }).generate
    return { [spec.path.replace(/^\//, ``)]: benchItems(spec) }
  }
  return data as Record<string, unknown>
}
function BenchSuite(): Node {
  const id = params.get(`scenario`) ?? ``
  const scenario = BENCH_SCENARIOS[id]
  if (!scenario) return div(``, `unknown scenario ${id} (one of ${benchManifest.scenarios.map((s) => s.id).join(`, `)})`)
  const t0 = performance.now()
  const surface = createSurface({ surfaceId: `bench`, catalogId: scenario.catalogId, initial: `nested` in scenario.surface ? scenario.surface.nested : undefined, data: benchData(scenario) })
  // A flat surface (list-100k, streaming, template-10k, table-1k) arrives as
  // the updateComponents it would be streamed in, applied before the attach
  // (the React harness's layout effect re-renders before its first paint).
  if (`components` in scenario.surface) surface.apply({ updateComponents: { surfaceId: `bench`, components: scenario.surface.components } })
  const el = surfaceEl({ surfaceId: `bench`, surface, plugin: { icons: harnessIcons }, theme: scenario.theme, mode: scenario.mode, width: scenario.viewport.width, viewportHeight: scenario.viewport.height })
  onAction(el, (e) => void window.__xuiLog.push({ action: e.name, componentId: e.componentId }))
  const rootEl = div(``, el)
  rootEl.dataset.testid = `bench-suite`
  rootEl.dataset.scenario = scenario.id
  const nodeEl = (id: string) => rootEl.querySelector<HTMLElement>(`[data-xui-id="${CSS.escape(id)}"]`)
  const scrollerOf = (node: HTMLElement): HTMLElement => {
    if (node.scrollHeight > node.clientHeight + 1 && /auto|scroll/.test(getComputedStyle(node).overflowY)) return node
    for (const d of Array.from(node.querySelectorAll<HTMLElement>(`*`))) {
      if (d.scrollHeight > d.clientHeight + 1 && /auto|scroll/.test(getComputedStyle(d).overflowY)) return d
    }
    return node
  }
  const api: BenchSuiteApi = {
    ready: false,
    scenario: scenario.id,
    // A property set is applied synchronously: `flush` just runs the writes.
    flush: (fn) => fn(),
    apply: (message) => el.apply(message as Parameters<typeof el.apply>[0]),
    setData: (path, value) => el.setData(path, value),
    setTheme: (t) => void (el.theme = t),
    setMode: (m) => void (el.mode = m),
    setWidth: (w) => void (el.width = w),
    scrollToIndex: (id, index, align) => el.handle?.scrollToIndex(id, index, align) ?? false,
    scroll: (id, by) => {
      const node = nodeEl(id)
      if (!node) return -1
      const scroller = scrollerOf(node)
      scroller.scrollTop += by
      scroller.dispatchEvent(new Event(`scroll`))
      void (scroller.querySelector(`.xui-list-window`) ?? scroller).getBoundingClientRect()
      return scroller.scrollTop
    },
    read: () => rootEl.getBoundingClientRect().height,
    nodes: () => rootEl.querySelectorAll(`[data-xui-id]`).length,
    depth: () => {
      let max = 0
      for (const node of Array.from(rootEl.querySelectorAll<HTMLElement>(`[data-xui-id]`))) {
        let d = 0
        for (let p: HTMLElement | null = node; p && p !== rootEl; p = p.parentElement) if (p.hasAttribute(`data-xui-id`)) d++
        max = Math.max(max, d)
      }
      return max
    },
    renderedItems: (selector) => rootEl.querySelectorAll(selector).length,
    has: (selector) => document.querySelector(selector) !== null,
  }
  window.__xuiBenchSuite = api
  // After the element painted on attach (the caller appends it this task).
  queueMicrotask(() =>
    requestAnimationFrame(() => {
      const now = performance.now()
      api.firstPaintMs = now - t0
      const bundle = performance.getEntriesByType(`resource`).find((e) => e.name.endsWith(`/dist/main.js`)) as PerformanceResourceTiming | undefined
      api.coldStartMs = now - (bundle?.responseEnd ?? 0)
      api.ready = true
    })
  )
  return rootEl
}

function App(): Node {
  if (view === `bench`) return Bench()
  if (view === `bench-suite`) return BenchSuite()
  if (view === `round2`) return Round2()
  if (view === `mount`) return Mount()
  if (view === `conditions`) return Conditions()
  if (view === `tree`) return Tree()
  if (view === `controls`) return Controls()
  if (view === `geometry`) return Geometry()
  if (view === `overlay`) return Overlay()
  if (view === `catalog`) return Catalog()
  if (!BUILTIN_THEME_IDS.includes(themeId)) return div(``, `unknown theme ${themeId}`)
  if (view === `specimen`) return Specimen()
  return KitchenSink()
}

document.body.style.margin = `0`
document.body.style.background = view === `geometry` || view === `overlay` ? `#fff` : mode === `dark` ? `#0f0f11` : `#f4f4f5`
const app = document.getElementById(`app`)!
app.appendChild(App())
