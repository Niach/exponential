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
// No app code: the SDK, its fixtures and a stub host with a 150 ms echo.

import { StrictMode, useMemo, useState } from "react"
import { createRoot } from "react-dom/client"
import { CORE_CATALOG_ID, reduceNested, BUILTIN_THEME_IDS, componentDef } from "@exponential-at/ui"
import type { NestedNode, UiNode } from "@exponential-at/ui"
import kitchenSink from "@exponential-at/ui/fixtures/kitchen-sink.json"
import geometry from "@exponential-at/ui/fixtures/layout-geometry.json"
import overlays from "@exponential-at/ui/fixtures/overlay-geometry.json"
import components from "@exponential-at/ui/fixtures/catalog-components.json"
import conditions from "@exponential-at/ui/fixtures/style-conditions.json"
import sinkData from "../fixtures/kitchen-sink.data.json"
import controlGeometry from "@exponential-at/ui/fixtures/control-geometry.json"
import specimens from "@exponential-at/ui/fixtures/specimens.json"
import { ExponentialSurface, useSurface } from "../src/index"
import type { HostPlugin, SurfaceInputEvent } from "../src/index"
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
        const { root } = reduceNested(c.node, { catalogId: CORE_CATALOG_ID })
        return (
          <div key={c.name} style={{ display: `flex`, flexDirection: `column`, gap: 6 }}>
            <div style={{ font: `11px ui-monospace, monospace`, opacity: 0.6 }}>{c.name}</div>
            <ExponentialSurface id={c.name.replace(/[^a-z0-9]/gi, `-`)} root={root} theme={themeId} mode={mode} host={{ icons: harnessIcons }} />
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
  const theme = ctx.breakpoints ? { id: `bp`, name: `Breakpoints`, extends: `neutral`, tokens: { breakpoint: ctx.breakpoints } } : `neutral`
  const { root } = reduceNested({ id: `n`, component: `Box`, style: { ...c.style, width: 40, height: 40 } } as unknown as NestedNode, { catalogId: CORE_CATALOG_ID, validate: false })
  return <ExponentialSurface id="cond" root={root} theme={theme as never} mode="light" width={ctx.width} viewportHeight={ctx.height} style={{ height: ctx.height ?? 120 }} states={ctx.states?.map((s) => s)} />
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
        const { root } = reduceNested({ id: `root`, component: c.component, props: { ...example, ...c.props } }, { catalogId: CORE_CATALOG_ID })
        return (
          <div key={c.key} data-case={c.key} style={{ width: 360 }}>
            <ExponentialSurface id={c.key.replace(/[^a-z0-9]/gi, `-`)} root={root} theme={themeId} mode="light" host={{ icons: harnessIcons }} />
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

function App() {
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
