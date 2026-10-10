// VAPP-87: the BROWSER HARNESS — one page, picked by query string, that the
// headless-Chromium suites (`browser/*.test.ts`) and the screenshot script
// drive, and `bun run --filter @exponential-at/ui-react dev:harness` serves
// for a human:
//   ?view=kitchen-sink&theme=<id>&mode=light|dark&width=<px>&rtl=1&echo=150
//   ?view=geometry&case=900|390|900-rtl|390-rtl        (fixed fake measure)
//   ?view=overlay&case=<n>                              (overlay-geometry.json)
//   ?view=catalog                                        (every fixture case)
//   ?view=conditions&case=<n>&ctx=<n>                    (style-conditions.json)
//   ?view=tree&tree=<json>&data=<json>&width=&height=&viewportHeight=&locale=&dir=  (any tree)
//     &gen=<key>:<n>  adds data[key] = n rows {id: "r<i>", title: "Item <i>"} (long lists without a long URL)
//   ?view=controls&theme=<id>                            (control-geometry.json, VAPP-91 conformance)
//   ?view=specimen&id=<id>&theme=<id>&mode=&width=&rtl=1 (one specimens.json entry, VAPP-93)
//   ?view=mount                                          (__xuiMount / __xuiVList: the round-2 conformance suites)
// No app code: the SDK, its fixtures and a stub host with a 150 ms echo.

import { StrictMode, useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import { flushSync } from "react-dom"
import { createRoot } from "react-dom/client"
import { CORE_CATALOG_ID, reduceNested, reduceSurface, BUILTIN_THEME_IDS, THEME_SCHEMA_ID, componentDef } from "@exponential-at/ui"
import type { FlatComponent, NestedNode, UiNode } from "@exponential-at/ui"
import benchFixture from "@exponential-at/ui/fixtures/bench-list.json"
import kitchenSink from "@exponential-at/ui/fixtures/kitchen-sink.json"
import geometry from "@exponential-at/ui/fixtures/layout-geometry.json"
import overlays from "@exponential-at/ui/fixtures/overlay-geometry.json"
import components from "@exponential-at/ui/fixtures/catalog-components.json"
import conditions from "@exponential-at/ui/fixtures/style-conditions.json"
import sinkData from "../fixtures/kitchen-sink.data.json"
import controlGeometry from "@exponential-at/ui/fixtures/control-geometry.json"
import specimens from "@exponential-at/ui/fixtures/specimens.json"
import { BASE_CSS, ExponentialSurface, WindowedList, useSurface } from "../src/index"
import type { HostPlugin, SurfaceHandle, SurfaceInputEvent, WindowedListHandle } from "../src/index"
import { harnessIcons } from "./icons"

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

function KitchenSink() {
  const { $comment: _c, ...data } = sinkData as Record<string, unknown>
  const surface = useSurface({ surfaceId: `ks`, initial: kitchenSink as unknown as NestedNode, data })
  window.__xuiData = () => surface.data
  const [host, setHost] = useState(``)
  const plugin = useMemo<HostPlugin>(
    () => ({
      icons: harnessIcons,
      onAction: (e) => {
        window.__xuiLog.push({ action: e.name, event: e.event, componentId: e.componentId, context: e.context, payload: e.payload })
        return new Promise((r) => setTimeout(r, echoMs))
      },
      onInput: (e: SurfaceInputEvent) => {
        window.__xuiLog.push({ input: e.name, value: e.value, revision: e.revision, kind: e.kind })
        // The simulated host: echoes the value into the data model after one
        // round trip, then acknowledges the revision.
        return new Promise<void>((resolve) =>
          setTimeout(() => {
            if (e.path) surface.setData(e.path, e.value)
            setHost(String(e.value ?? ``))
            resolve()
          }, echoMs)
        )
      },
    }),
    [surface]
  )
  return (
    <div style={{ display: `flex`, flexDirection: `column`, gap: 8, alignItems: `flex-start` }}>
      <ExponentialSurface id="ks" surface={surface} host={plugin} theme={themeId} mode={mode} direction={rtl ? `rtl` : `ltr`} width={width ?? `100%`} locale={params.get(`locale`) ?? undefined} />
      <div id="host-echo" data-testid="host-echo" style={{ font: `12px ui-monospace, monospace`, opacity: 0.7, padding: 8 }}>
        host: {host}
      </div>
    </div>
  )
}

function Geometry() {
  const c = (geometry.cases as Record<string, { width: number; direction: `ltr` | `rtl` }>)[params.get(`case`) ?? `900`]
  const { root } = reduceNested(geometry.surface as unknown as NestedNode, { catalogId: CORE_CATALOG_ID })
  // The case's direction overrides the root's `direction` (the core copies
  // the root's direction onto every node; the spike flipped it the same way).
  root.style = { ...(root.style ?? {}), direction: c.direction }
  const measures = geometry.measures as Record<string, { w: number; h: number }>
  return <ExponentialSurface id="geo" root={root} theme="neutral" mode="light" width={c.width} direction={c.direction} measure={(node: UiNode) => measures[node.id] ?? null} />
}

function Overlay() {
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
  return <ExponentialSurface id="ov" root={root} theme="neutral" mode="light" width={c.viewport.width} />
}

function Catalog() {
  const cases = (components as { cases: { name: string; node: NestedNode }[] }).cases
  return (
    <div style={{ display: `grid`, gap: 16, gridTemplateColumns: `repeat(auto-fill, minmax(280px, 1fr))`, padding: 16 }}>
      {cases.map((c) => {
        // A toast stays (duration 0) so a slow page still finds it painted.
        const node = c.node.component === `Toast` ? { ...c.node, props: { ...c.node.props, duration: 0 } } : c.node
        const { root, templates } = reduceNested(node as NestedNode, { catalogId: CORE_CATALOG_ID })
        return (
          <div key={c.name} style={{ display: `flex`, flexDirection: `column`, gap: 6 }}>
            <div style={{ font: `11px ui-monospace, monospace`, opacity: 0.6 }}>{c.name}</div>
            <ExponentialSurface id={c.name.replace(/[^a-z0-9]/gi, `-`)} root={root} templates={templates} theme={themeId} mode={mode} host={{ icons: harnessIcons }} />
          </div>
        )
      })}
    </div>
  )
}

/** One style-conditions.json case × context: a Box wearing the case's style
 *  on a surface of the context's size (the theme's breakpoints overridden
 *  when the context names its own), its states forced. */
function Conditions() {
  type Ctx = { width: number; height?: number; states?: string[]; breakpoints?: Record<string, number> }
  const c = (conditions.cases as { style: Record<string, unknown>; contexts: Ctx[] }[])[Number(params.get(`case`) ?? 0)]
  const ctx = c.contexts[Number(params.get(`ctx`) ?? 0)]
  const theme = ctx.breakpoints ? { $schema: THEME_SCHEMA_ID, id: `bp`, name: `Breakpoints`, extends: `neutral`, tokens: { breakpoint: ctx.breakpoints } } : `neutral`
  const { root } = reduceNested({ id: `n`, component: `Box`, style: { ...c.style, width: 40, height: 40 } } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID, validate: false })
  // No context height = no height at all (a content-sized root is not a
  // viewport), so height conditions do not match, as in the core.
  return <ExponentialSurface id="cond" root={root} theme={theme as never} mode="light" width={ctx.width} viewportHeight={ctx.height} style={ctx.height === undefined ? undefined : { height: ctx.height }} states={ctx.states?.map((s) => s)} />
}

/** Any nested tree from the query (keyboard + overlay suites). */
function Tree() {
  const nested = JSON.parse(params.get(`tree`) ?? `{}`) as NestedNode
  const data = JSON.parse(params.get(`data`) ?? `{}`) as Record<string, unknown>
  const gen = params.get(`gen`)?.split(`:`)
  if (gen?.length === 2) data[gen[0]] = Array.from({ length: Number(gen[1]) }, (_, i) => ({ id: `r${i}`, title: `Item ${i}` }))
  const surface = useSurface({ surfaceId: `tree`, initial: nested, data })
  window.__xuiData = () => surface.data
  const plugin = useMemo<HostPlugin>(() => ({ icons: harnessIcons, onAction: (e) => void window.__xuiLog.push({ action: e.name, event: e.event, componentId: e.componentId, context: e.context }) }), [])
  const h = params.get(`height`)
  const vh = params.get(`viewportHeight`)
  const dir = params.get(`dir`)
  return (
    <ExponentialSurface
      id="tree"
      surface={surface}
      host={plugin}
      theme={themeId}
      mode={mode}
      width={width ?? `100%`}
      locale={params.get(`locale`) ?? undefined}
      direction={dir === `rtl` || dir === `ltr` ? dir : undefined}
      viewportHeight={vh ? Number(vh) : undefined}
      style={h ? { height: Number(h) } : undefined}
    />
  )
}

/** VAPP-91 conformance: every control-geometry case of one theme, each its
 *  own surface; `__xuiControls()` reads the sizing part's box per case. */
function Controls() {
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
      const px = (v: string) => parseFloat(v) || 0
      out[c.key] = {
        width: r.width,
        height: r.height,
        minHeight: px(cs.minHeight),
        paddingHorizontal: px(cs.paddingLeft),
        paddingVertical: px(cs.paddingTop),
        gap: px(cs.columnGap),
        borderWidth: px(cs.borderLeftWidth),
        borderRadius: Math.min(px(cs.borderTopLeftRadius), 9999),
      }
    }
    return out
  }
  return (
    <div style={{ display: `flex`, flexWrap: `wrap`, gap: 16, padding: 16, alignItems: `flex-start` }}>
      {all.map((c) => {
        const example = (componentDef(c.component)?.example ?? {}) as Record<string, unknown>
        const { root, templates } = reduceNested({ id: `root`, component: c.component, props: { ...example, ...c.props } }, { catalogId: CORE_CATALOG_ID })
        return (
          <div key={c.key} data-case={c.key} style={{ width: 360 }}>
            <ExponentialSurface id={c.key.replace(/[^a-z0-9]/gi, `-`)} root={root} templates={templates} theme={themeId} mode="light" host={{ icons: harnessIcons }} />
          </div>
        )
      })}
    </div>
  )
}

/** VAPP-93: one specimens.json entry alone (the per-component docs shot). */
function Specimen() {
  const id = params.get(`id`) ?? ``
  const entry = (specimens as unknown as { specimens: { id: string; node: NestedNode }[] }).specimens.find((s) => s.id === id)
  const surface = useSurface({ surfaceId: id || `specimen`, catalogId: CORE_CATALOG_ID, initial: entry?.node ?? { id: `root`, component: `Box` } })
  const plugin = useMemo<HostPlugin>(
    () => ({
      icons: harnessIcons,
      onAction: (e) => {
        window.__xuiLog.push({ action: e.name, event: e.event, componentId: e.componentId, context: e.context, payload: e.payload })
        return new Promise((r) => setTimeout(r, echoMs))
      },
      onInput: (e: SurfaceInputEvent) => {
        window.__xuiLog.push({ input: e.name, value: e.value, revision: e.revision, kind: e.kind })
        return new Promise<void>((resolve) =>
          setTimeout(() => {
            if (e.path) surface.setData(e.path, e.value)
            resolve()
          }, echoMs)
        )
      },
    }),
    [surface]
  )
  if (!entry) return <div>unknown specimen {id}</div>
  return (
    <div data-testid="exponential-ui-specimen" data-specimen={id}>
      <ExponentialSurface id="specimen" surface={surface} host={plugin} theme={themeId} mode={mode} direction={rtl ? `rtl` : `ltr`} width={width ?? `100%`} />
    </div>
  )
}

/** Round 2: the 100,000-row list bench (fixtures/bench-list.json): the
 *  surface built from the fixture's components, rows generated; the first
 *  paint = reduce + bind + layout + paint of the first window (to the frame
 *  after the commit). scripts/bench-list.ts drives the scroll steps. */
function Bench() {
  const handle = useRef<SurfaceHandle>(null)
  const t0 = useRef(performance.now())
  const { root, templates, data } = useMemo(() => {
    const spec = benchFixture.rows
    const rows = Array.from({ length: spec.count }, (_, i) => ({ id: `r${i}`, title: `Row ${i}`, meta: String(i % 97) }))
    const r = reduceSurface(benchFixture.components as unknown as FlatComponent[], { catalogId: benchFixture.catalogId })
    return { root: r.root, templates: r.templates, data: { rows } }
  }, [])
  useEffect(() => {
    requestAnimationFrame(() => {
      window.__xuiBench = { firstPaintMs: performance.now() - t0.current, handle: () => handle.current, flush: (fn) => flushSync(fn) }
    })
  }, [])
  return <ExponentialSurface id="bench" root={root} templates={templates} data={data} theme={benchFixture.theme} mode="light" width={benchFixture.viewport.width} handleRef={handle} />
}

/** Round 2: the browser checks' surface (browser/round2.test.ts): a
 *  Resizable, a bounded sectioned List with sticky headers, a horizontal
 *  windowed List, animated / blurred / sticky boxes, overlays in a column. */
function Round2() {
  const handle = useRef<SurfaceHandle>(null)
  window.__xuiHandle = () => handle.current
  const days = [`Today`, `Yesterday`, `Earlier`]
  const data = useMemo(() => ({ sizes: [30, 70], items: Array.from({ length: 120 }, (_, i) => ({ id: `i${i}`, day: days[Math.min(2, Math.floor(i / 40))], title: `Item ${i}` })), chips: Array.from({ length: 300 }, (_, i) => ({ id: `c${i}`, label: `Chip ${i}` })) }), [])
  const r = useMemo(
    () =>
      reduceNested(
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
      ),
    []
  )
  return <ExponentialSurface id="r2" root={r.root} templates={r.templates} data={data} theme="neutral" mode="light" width={390} direction={rtl ? `rtl` : `ltr`} handleRef={handle} host={{ icons: harnessIcons, onAction: (e) => void window.__xuiLog.push({ action: e.name, context: e.context }) }} />
}

/** `?view=mount` (browser/suites.test.ts, the round-2 suites through THIS
 *  renderer): `__xuiMount(tree, options)` paints any nested tree in a fresh
 *  surface (`__xuiHandle` = its handle); `__xuiVList(case)` mounts a bare
 *  `WindowedList` in a scroller of `viewport` px (a trailing pad keeps any
 *  `scroll` reachable) and returns its probe. */
interface MountOptions {
  data?: Record<string, unknown>
  width?: number
  dir?: `ltr` | `rtl`
  /** A built-in theme id (default neutral). */
  theme?: string
  /** `$control.hairline` override (a Resizable handle's thickness). */
  hairline?: number
}
interface VListCase {
  extents: number[]
  gap: number
  viewport: number
  overscan?: number
  axis?: `vertical` | `horizontal`
  stickyRows?: number[]
}
interface VListProbe {
  /** Scroll the scroller to `at` and read the window back. */
  scroll: (at: number) => Promise<{ start: number; end: number; before: number; after: number; total: number; pinned: { row: number; offset: number } | null }>
  /** `scrollToIndex`, then the scroller's offset once the re-aims settle. */
  scrollToIndex: (index: number, align: `start` | `center` | `end` | `nearest`) => Promise<number>
}
const frames = (n = 2) => new Promise<void>((resolve) => {
  const step = (left: number) => (left <= 0 ? resolve() : requestAnimationFrame(() => step(left - 1)))
  step(n)
})
let mounted: ReturnType<typeof createRoot> | null = null
function remount(node: ReactNode): HTMLElement {
  mounted?.unmount()
  const host = document.getElementById(`mount`)!
  host.replaceChildren()
  const el = document.createElement(`div`)
  host.appendChild(el)
  mounted = createRoot(el)
  flushSync(() => mounted!.render(node))
  return el
}
function Mount() {
  useEffect(() => {
    window.__xuiMount = async (tree, options = {}) => {
      const { root, templates } = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
      const handle: { current: SurfaceHandle | null } = { current: null }
      window.__xuiHandle = () => handle.current
      const base = options.theme ?? `neutral`
      const theme = options.hairline === undefined ? base : ({ $schema: THEME_SCHEMA_ID, id: `hairline`, name: `Hairline`, extends: base, tokens: { control: { hairline: options.hairline } } } as never)
      remount(<ExponentialSurface id="m" root={root} templates={templates} data={options.data} theme={theme} mode="light" width={options.width ?? `100%`} direction={options.dir} handleRef={handle} host={{ icons: harnessIcons, onAction: (e) => void window.__xuiLog.push({ action: e.name, context: e.context }) }} />)
      await frames(3)
    }
    window.__xuiVList = async (c) => {
      const horizontal = c.axis === `horizontal`
      const main = horizontal ? `width` : `height`
      const cross = horizontal ? `height` : `width`
      const list: { current: WindowedListHandle | null } = { current: null }
      const estimate = (i: number) => c.extents[i]
      const el = remount(
        <div id="vl-scroller" style={{ [main]: c.viewport, [cross]: 200, overflow: `auto`, display: horizontal ? `flex` : `block` }}>
          <WindowedList ref={list} count={c.extents.length} itemKey={(i) => String(i)} estimatedItemHeight={estimate} overscan={c.overscan} axis={c.axis} gap={c.gap} stickyRows={c.stickyRows} renderItem={(i) => <div style={{ [main]: c.extents[i], [cross]: 20 }} />} />
          <div style={{ flex: `none`, [main]: c.viewport, [cross]: 1 }} />
        </div>
      )
      await frames(3)
      const scroller = el.querySelector<HTMLElement>(`#vl-scroller`)!
      const win = () => scroller.querySelector<HTMLElement>(`.xui-list-window`)!
      const pos = (item: HTMLElement) => (horizontal ? item.getBoundingClientRect().left - win().getBoundingClientRect().left : item.getBoundingClientRect().top - win().getBoundingClientRect().top)
      const offset = () => (horizontal ? scroller.scrollLeft : scroller.scrollTop)
      return {
        scroll: async (at) => {
          if (horizontal) scroller.scrollLeft = at
          else scroller.scrollTop = at
          await frames(3)
          const w = win()
          const start = Number(w.dataset.windowStart)
          const end = Number(w.dataset.windowEnd)
          const item = (i: number) => w.querySelector<HTMLElement>(`:scope > .xui-list-item[data-index="${i}"]`)!
          const painted = Number.parseFloat(w.style[main])
          const span = end > start ? { before: pos(item(start)), after: painted - (pos(item(end - 1)) + c.extents[end - 1]) } : { before: 0, after: painted }
          const pin = Array.from(w.querySelectorAll<HTMLElement>(`:scope > .xui-list-item`)).find((x) => x.style.zIndex === `1`)
          return { start, end, ...span, total: painted, pinned: pin ? { row: Number(pin.dataset.index), offset: pos(pin) } : null }
        },
        scrollToIndex: async (index, align) => {
          list.current!.scrollToIndex(index, align)
          await frames(8)
          return offset()
        },
      }
    }
  }, [])
  return (
    <>
      <style>{BASE_CSS}</style>
      <div id="mount" />
    </>
  )
}

function App() {
  if (view === `bench`) return <Bench />
  if (view === `round2`) return <Round2 />
  if (view === `mount`) return <Mount />
  if (view === `conditions`) return <Conditions />
  if (view === `tree`) return <Tree />
  if (view === `controls`) return <Controls />
  if (view === `geometry`) return <Geometry />
  if (view === `overlay`) return <Overlay />
  if (view === `catalog`) return <Catalog />
  if (!BUILTIN_THEME_IDS.includes(themeId)) return <div>unknown theme {themeId}</div>
  if (view === `specimen`) return <Specimen />
  return <KitchenSink />
}

document.body.style.margin = `0`
document.body.style.background = view === `geometry` || view === `overlay` ? `#fff` : mode === `dark` ? `#0f0f11` : `#f4f4f5`
createRoot(document.getElementById(`app`)!).render(
  <StrictMode>
    <App />
  </StrictMode>
)
