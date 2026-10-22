// FEED-128: the BROWSER HARNESS on Solid — the SAME page protocol as the
// React renderer's harness/main.tsx (the web-parity suite drives both):
//   ?view=kitchen-sink&theme=<id>&mode=light|dark&width=<px>&rtl=1&echo=150
//   ?view=geometry&case=900|390|900-rtl|390-rtl        (fixed fake measure)
//   ?view=overlay&case=<n>                              (overlay-geometry.json)
//   ?view=catalog                                        (every fixture case)
//   ?view=conditions&case=<n>&ctx=<n>                    (style-conditions.json)
//   ?view=tree&tree=<json>&data=<json>&width=&height=&viewportHeight=&locale=&dir=&gen=<key>:<n>
//   ?view=controls&theme=<id>                            (control-geometry.json)
//   ?view=specimen&id=<id>&theme=<id>&mode=&width=&rtl=1 (one specimens.json entry)
//   ?view=primitives&id=<id>&theme=<id>&mode=&width=   (one primitive-specimens.json entry, FEED-131: harness/primitives/)
//   ?view=mount                                          (__xuiMount / __xuiVList)
//   ?view=bench / ?view=round2
//   ?view=bench-suite&scenario=<id>                      (VAPP-105: one scenario of @exponential-at/ui-bench; __xuiBenchSuite drives it)
// Natives not ported yet paint their `data-xui-pending` placeholder.

import { For, Show, createSignal, onMount, type JSX } from "solid-js"
import { render } from "solid-js/web"
import { CORE_CATALOG_ID, reduceNested, reduceSurface, BUILTIN_THEME_IDS, THEME_SCHEMA_ID, componentDef } from "@exponential-at/ui"
import type { FlatComponent, NestedNode, UiNode } from "@exponential-at/ui"
import benchFixture from "@exponential-at/ui/fixtures/bench-list.json" with { type: "json" }
import kitchenSink from "@exponential-at/ui/fixtures/kitchen-sink.json" with { type: "json" }
import geometry from "@exponential-at/ui/fixtures/layout-geometry.json" with { type: "json" }
import overlays from "@exponential-at/ui/fixtures/overlay-geometry.json" with { type: "json" }
import components from "@exponential-at/ui/fixtures/catalog-components.json" with { type: "json" }
import conditions from "@exponential-at/ui/fixtures/style-conditions.json" with { type: "json" }
import controlGeometry from "@exponential-at/ui/fixtures/control-geometry.json" with { type: "json" }
import specimens from "@exponential-at/ui/fixtures/specimens.json" with { type: "json" }
// The kitchen sink's data model is the React package's fixture (one file
// for both harnesses; a JSON file, not renderer code).
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
import { BASE_CSS, ExponentialSurface, WindowedList, createSurface } from "../src/index"
import type { HostPlugin, SurfaceHandle, SurfaceInputEvent, WindowedListHandle } from "../src/index"
import { harnessIcons } from "./icons"
import { mountPrimitives } from "./primitives/mount"

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

const px = (v: number | string | undefined) => (typeof v === `number` ? `${v}px` : v)

function KitchenSink(): JSX.Element {
  const { $comment: _c, ...data } = sinkData as Record<string, unknown>
  const surface = createSurface({ surfaceId: `ks`, initial: kitchenSink as unknown as NestedNode, data })
  window.__xuiData = () => surface.data
  const [host, setHost] = createSignal(``)
  const plugin: HostPlugin = {
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
  }
  return (
    <div style={{ display: `flex`, "flex-direction": `column`, gap: `8px`, "align-items": `flex-start` }}>
      <ExponentialSurface id="ks" surface={surface} host={plugin} theme={themeId} mode={mode} direction={rtl ? `rtl` : `ltr`} width={width ?? `100%`} locale={params.get(`locale`) ?? undefined} />
      <div id="host-echo" data-testid="host-echo" style={{ font: `12px ui-monospace, monospace`, opacity: 0.7, padding: `8px` }}>
        host: {host()}
      </div>
    </div>
  )
}

function Geometry(): JSX.Element {
  const c = (geometry.cases as Record<string, { width: number; direction: `ltr` | `rtl` }>)[params.get(`case`) ?? `900`]
  const { root } = reduceNested(geometry.surface as unknown as NestedNode, { catalogId: CORE_CATALOG_ID })
  // The case's direction overrides the root's `direction`.
  root.style = { ...(root.style ?? {}), direction: c.direction }
  const measures = geometry.measures as Record<string, { w: number; h: number }>
  return <ExponentialSurface id="geo" root={root} theme="neutral" mode="light" width={c.width} direction={c.direction} measure={(node: UiNode) => measures[node.id] ?? null} />
}

function Overlay(): JSX.Element {
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

function Catalog(): JSX.Element {
  const cases = (components as { cases: { name: string; node: NestedNode }[] }).cases
  return (
    <div style={{ display: `grid`, gap: `16px`, "grid-template-columns": `repeat(auto-fill, minmax(280px, 1fr))`, padding: `16px` }}>
      <For each={cases}>
        {(c) => {
          // A toast stays (duration 0) so a slow page still finds it painted.
          const node = c.node.component === `Toast` ? { ...c.node, props: { ...c.node.props, duration: 0 } } : c.node
          const { root, templates } = reduceNested(node as NestedNode, { catalogId: CORE_CATALOG_ID })
          return (
            <div style={{ display: `flex`, "flex-direction": `column`, gap: `6px` }}>
              <div style={{ font: `11px ui-monospace, monospace`, opacity: 0.6 }}>{c.name}</div>
              <ExponentialSurface id={c.name.replace(/[^a-z0-9]/gi, `-`)} root={root} templates={templates} theme={themeId} mode={mode} host={{ icons: harnessIcons }} />
            </div>
          )
        }}
      </For>
    </div>
  )
}

/** One style-conditions.json case × context. */
function Conditions(): JSX.Element {
  type Ctx = { width: number; height?: number; states?: string[]; breakpoints?: Record<string, number> }
  const c = (conditions.cases as { style: Record<string, unknown>; contexts: Ctx[] }[])[Number(params.get(`case`) ?? 0)]
  const ctx = c.contexts[Number(params.get(`ctx`) ?? 0)]
  const theme = ctx.breakpoints ? { $schema: THEME_SCHEMA_ID, id: `bp`, name: `Breakpoints`, extends: `neutral`, tokens: { breakpoint: ctx.breakpoints } } : `neutral`
  const { root } = reduceNested({ id: `n`, component: `Box`, style: { ...c.style, width: 40, height: 40 } } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID, validate: false })
  return <ExponentialSurface id="cond" root={root} theme={theme as never} mode="light" width={ctx.width} viewportHeight={ctx.height} style={ctx.height === undefined ? undefined : { height: px(ctx.height) }} states={ctx.states?.map((s) => s)} />
}

/** Any nested tree from the query (keyboard + overlay suites). */
function Tree(): JSX.Element {
  const nested = JSON.parse(params.get(`tree`) ?? `{}`) as NestedNode
  const data = JSON.parse(params.get(`data`) ?? `{}`) as Record<string, unknown>
  const gen = params.get(`gen`)?.split(`:`)
  if (gen?.length === 2) data[gen[0]] = Array.from({ length: Number(gen[1]) }, (_, i) => ({ id: `r${i}`, title: `Item ${i}` }))
  const surface = createSurface({ surfaceId: `tree`, initial: nested, data })
  window.__xuiData = () => surface.data
  const plugin: HostPlugin = { icons: harnessIcons, onAction: (e) => void window.__xuiLog.push({ action: e.name, event: e.event, componentId: e.componentId, context: e.context }) }
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
      style={h ? { height: px(Number(h)) } : undefined}
    />
  )
}

/** Every control-geometry case of one theme, each its own surface. */
function Controls(): JSX.Element {
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
  return (
    <div style={{ display: `flex`, "flex-wrap": `wrap`, gap: `16px`, padding: `16px`, "align-items": `flex-start` }}>
      <For each={all}>
        {(c) => {
          const example = (componentDef(c.component)?.example ?? {}) as Record<string, unknown>
          const { root, templates } = reduceNested({ id: `root`, component: c.component, props: { ...example, ...c.props } }, { catalogId: CORE_CATALOG_ID })
          return (
            <div data-case={c.key} style={{ width: `360px` }}>
              <ExponentialSurface id={c.key.replace(/[^a-z0-9]/gi, `-`)} root={root} templates={templates} theme={themeId} mode="light" host={{ icons: harnessIcons }} />
            </div>
          )
        }}
      </For>
    </div>
  )
}

/** One specimens.json entry alone (the per-component docs shot). */
function Specimen(): JSX.Element {
  const id = params.get(`id`) ?? ``
  const entry = (specimens as unknown as { specimens: { id: string; node: NestedNode }[] }).specimens.find((s) => s.id === id)
  const surface = createSurface({ surfaceId: id || `specimen`, catalogId: CORE_CATALOG_ID, initial: entry?.node ?? { id: `root`, component: `Box` } })
  const plugin: HostPlugin = {
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
  }
  return (
    <Show when={entry} fallback={<div>unknown specimen {id}</div>}>
      <div data-testid="exponential-ui-specimen" data-specimen={id}>
        <ExponentialSurface id="specimen" surface={surface} host={plugin} theme={themeId} mode={mode} direction={rtl ? `rtl` : `ltr`} width={width ?? `100%`} />
      </div>
    </Show>
  )
}

/** The 100,000-row list bench (fixtures/bench-list.json). */
function Bench(): JSX.Element {
  let handle: SurfaceHandle | null = null
  const t0 = performance.now()
  const spec = benchFixture.rows
  const rows = Array.from({ length: spec.count }, (_, i) => ({ id: `r${i}`, title: `Row ${i}`, meta: String(i % 97) }))
  const r = reduceSurface(benchFixture.components as unknown as FlatComponent[], { catalogId: benchFixture.catalogId })
  onMount(() => {
    requestAnimationFrame(() => {
      // Solid applies writes synchronously: `flush` just runs them.
      window.__xuiBench = { firstPaintMs: performance.now() - t0, handle: () => handle, flush: (fn) => fn() }
    })
  })
  return <ExponentialSurface id="bench" root={r.root} templates={r.templates} data={{ rows }} theme={benchFixture.theme} mode="light" width={benchFixture.viewport.width} handleRef={(h) => (handle = h)} />
}

/** The round-2 browser checks' surface (browser/round2.test.ts). */
function Round2(): JSX.Element {
  let handle: SurfaceHandle | null = null
  window.__xuiHandle = () => handle
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
  return <ExponentialSurface id="r2" root={r.root} templates={r.templates} data={data} theme="neutral" mode="light" width={390} direction={rtl ? `rtl` : `ltr`} handleRef={(h) => (handle = h)} host={{ icons: harnessIcons, onAction: (e) => void window.__xuiLog.push({ action: e.name, context: e.context }) }} />
}

/** `?view=mount` (browser/suites.test.ts): `__xuiMount(tree, options)` paints
 *  any nested tree in a fresh surface; `__xuiVList(case)` mounts a bare
 *  `WindowedList` in a scroller of `viewport` px. */
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
  scroll: (at: number) => Promise<{ start: number; end: number; before: number; after: number; total: number; pinned: { row: number; offset: number } | null }>
  scrollToIndex: (index: number, align: `start` | `center` | `end` | `nearest`) => Promise<number>
}
const frames = (n = 2) =>
  new Promise<void>((resolve) => {
    const step = (left: number) => (left <= 0 ? resolve() : requestAnimationFrame(() => step(left - 1)))
    step(n)
  })
let disposeMounted: (() => void) | null = null
function remount(node: () => JSX.Element): HTMLElement {
  disposeMounted?.()
  const host = document.getElementById(`mount`)!
  host.replaceChildren()
  const el = document.createElement(`div`)
  host.appendChild(el)
  disposeMounted = render(node, el)
  return el
}
function Mount(): JSX.Element {
  onMount(() => {
    window.__xuiMount = async (tree, options = {}) => {
      const { root, templates } = reduceNested(tree, { catalogId: CORE_CATALOG_ID })
      let handle: SurfaceHandle | null = null
      window.__xuiHandle = () => handle
      const base = options.theme ?? `neutral`
      const theme = options.hairline === undefined ? base : ({ $schema: THEME_SCHEMA_ID, id: `hairline`, name: `Hairline`, extends: base, tokens: { control: { hairline: options.hairline } } } as never)
      remount(() => <ExponentialSurface id="m" root={root} templates={templates} data={options.data} theme={theme} mode="light" width={options.width ?? `100%`} direction={options.dir} handleRef={(h) => (handle = h)} host={{ icons: harnessIcons, onAction: (e) => void window.__xuiLog.push({ action: e.name, context: e.context }) }} />)
      await frames(3)
    }
    window.__xuiVList = async (c) => {
      const horizontal = c.axis === `horizontal`
      const main = horizontal ? `width` : `height`
      const cross = horizontal ? `height` : `width`
      let list: WindowedListHandle | null = null
      const estimate = (i: number) => c.extents[i]
      // Built outside the JSX: the Solid compiler drops computed keys of an
      // inline `style={{…}}` literal.
      const box = (m: string, x: string, extra: Record<string, string> = {}): Record<string, string> => ({ ...extra, [main]: m, [cross]: x })
      const scrollerStyle = box(px(c.viewport)!, `200px`, { overflow: `auto`, display: horizontal ? `flex` : `block` })
      const padStyle = box(px(c.viewport)!, `1px`, { flex: `none` })
      const el = remount(() => (
        <div id="vl-scroller" style={scrollerStyle}>
          <WindowedList ref={(h) => (list = h)} count={c.extents.length} itemKey={(i) => String(i)} estimatedItemHeight={estimate} overscan={c.overscan} axis={c.axis} gap={c.gap} stickyRows={c.stickyRows} renderItem={(i) => <div style={box(px(c.extents[i()])!, `20px`)} />} />
          <div style={padStyle} />
        </div>
      ))
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
          ;(list as WindowedListHandle | null)!.scrollToIndex(index, align)
          await frames(8)
          return offset()
        },
      }
    }
  })
  return (
    <>
      <style>{BASE_CSS}</style>
      <div id="mount" />
    </>
  )
}

/** VAPP-105: ONE scenario of the benchmark suite
 *  (packages/exponential-ui-bench/scenarios/<id>.json) mounted through
 *  `createSurface`, driven from outside by `window.__xuiBenchSuite`
 *  (runners/solid.ts): the SAME driver surface as the React harness's
 *  `BenchSuiteApi`, so one page script measures both. Solid applies a
 *  signal write synchronously (no scheduler), so `flush` just runs the
 *  mutation; one "pass" = the writes applied + a layout read, as the React
 *  harness measures it (its flushSync). */
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
function BenchSuite(): JSX.Element {
  const id = params.get(`scenario`) ?? ``
  const scenario = BENCH_SCENARIOS[id]
  return (
    <Show when={scenario} fallback={<div>unknown scenario {id} (one of {benchManifest.scenarios.map((s) => s.id).join(`, `)})</div>}>
      <BenchSuiteSurface scenario={scenario!} />
    </Show>
  )
}
function BenchSuiteSurface(props: { scenario: BenchScenario }): JSX.Element {
  const scenario = props.scenario
  const t0 = performance.now()
  let handle: SurfaceHandle | null = null
  let rootEl: HTMLDivElement | undefined
  const [theme, setTheme] = createSignal(scenario.theme)
  const [modeState, setMode] = createSignal<`light` | `dark`>(scenario.mode)
  const [surfaceWidth, setWidth] = createSignal(scenario.viewport.width)
  const surface = createSurface({ surfaceId: `bench`, catalogId: scenario.catalogId, initial: `nested` in scenario.surface ? scenario.surface.nested : undefined, data: benchData(scenario) })
  // A flat surface (list-100k, streaming, template-10k, table-1k) arrives as
  // the updateComponents it would be streamed in, applied before the mount
  // (the React harness's layout effect re-renders before its first paint).
  if (`components` in scenario.surface) surface.apply({ updateComponents: { surfaceId: `bench`, components: scenario.surface.components } })
  const plugin: HostPlugin = { icons: harnessIcons, onAction: (e) => void window.__xuiLog.push({ action: e.name, componentId: e.componentId }) }
  onMount(() => {
    const surfaceEl = () => rootEl ?? null
    const nodeEl = (id: string) => surfaceEl()?.querySelector<HTMLElement>(`[data-xui-id="${CSS.escape(id)}"]`) ?? null
    const scrollerOf = (el: HTMLElement): HTMLElement => {
      if (el.scrollHeight > el.clientHeight + 1 && /auto|scroll/.test(getComputedStyle(el).overflowY)) return el
      for (const d of Array.from(el.querySelectorAll<HTMLElement>(`*`))) {
        if (d.scrollHeight > d.clientHeight + 1 && /auto|scroll/.test(getComputedStyle(d).overflowY)) return d
      }
      return el
    }
    const read = () => surfaceEl()?.getBoundingClientRect().height ?? 0
    const api: BenchSuiteApi = {
      ready: false,
      scenario: scenario.id,
      // Solid applies writes synchronously: `flush` just runs them.
      flush: (fn) => fn(),
      apply: (message) => surface.apply(message as Parameters<typeof surface.apply>[0]),
      setData: (path, value) => surface.setData(path, value),
      setTheme: (t) => void setTheme(t),
      setMode: (m) => void setMode(m),
      setWidth: (w) => void setWidth(w),
      scrollToIndex: (id, index, align) => handle?.scrollToIndex(id, index, align) ?? false,
      scroll: (id, by) => {
        const el = nodeEl(id)
        if (!el) return -1
        const scroller = scrollerOf(el)
        scroller.scrollTop += by
        scroller.dispatchEvent(new Event(`scroll`))
        void (scroller.querySelector(`.xui-list-window`) ?? scroller).getBoundingClientRect()
        return scroller.scrollTop
      },
      read,
      nodes: () => surfaceEl()?.querySelectorAll(`[data-xui-id]`).length ?? 0,
      depth: () => {
        let max = 0
        for (const el of Array.from(surfaceEl()?.querySelectorAll<HTMLElement>(`[data-xui-id]`) ?? [])) {
          let d = 0
          for (let p: HTMLElement | null = el; p && p !== surfaceEl(); p = p.parentElement) if (p.hasAttribute(`data-xui-id`)) d++
          max = Math.max(max, d)
        }
        return max
      },
      renderedItems: (selector) => surfaceEl()?.querySelectorAll(selector).length ?? 0,
      has: (selector) => document.querySelector(selector) !== null,
    }
    window.__xuiBenchSuite = api
    requestAnimationFrame(() => {
      const now = performance.now()
      api.firstPaintMs = now - t0
      const bundle = performance.getEntriesByType(`resource`).find((e) => e.name.endsWith(`/dist/main.js`)) as PerformanceResourceTiming | undefined
      api.coldStartMs = now - (bundle?.responseEnd ?? 0)
      api.ready = true
    })
  })
  return (
    <div ref={rootEl} data-testid="bench-suite" data-scenario={scenario.id}>
      <ExponentialSurface id="bench" surface={surface} host={plugin} theme={theme()} mode={modeState()} width={surfaceWidth()} viewportHeight={scenario.viewport.height} handleRef={(h) => (handle = h)} />
    </div>
  )
}

function App(): JSX.Element {
  if (view === `bench`) return <Bench />
  if (view === `bench-suite`) return <BenchSuite />
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

if (view === `primitives`) void mountPrimitives()
else {
  document.body.style.margin = `0`
  document.body.style.background = view === `geometry` || view === `overlay` ? `#fff` : mode === `dark` ? `#0f0f11` : `#f4f4f5`
  render(() => <App />, document.getElementById(`app`)!)
}
