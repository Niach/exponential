// EXP-1604: the styleguide page's Motion script (motion-islands.ts bundles
// it). Islands are static markup, so this mounts the SHARED web painter
// (`MotionView`, @exponential_app/eui/web) on every Motion an island
// painted: its props come from the same specimens the island rendered
// (bindings resolved against their data), its theme and mode from the
// surface. The WASM rides inline (gzip + base64): the page works over file://.
// EXP-1605: a Motion with `on.press` is pressable (Enter / Space); clicks and
// hovers reach its state machine through the painter.
// Round 8: the same for every Canvas (`CanvasView`, the canvas entry's
// genUI's fixtures/canvas-specimen.json); a press on the pointer pad writes its drawing
// coordinates into the pad's readout (the island cannot re-render).

import { DEFAULT_THEME_ID, builtinTheme, type NestedNode } from "@exponential_app/eui"
import { initMotionWasm } from "@exponential_app/eui/motion"
import { CanvasView, MotionView } from "@exponential_app/eui/web"
import { MOTION_WASM_GZ_BASE64 } from "@exponential_app/eui/wasm/motion/bytes.ts"
import specimens from "@exponential_app/eui/fixtures/motion.specimens.json"
import canvasSpecimens from "@exponential_app/eui/fixtures/canvas-specimen.json"

type Tree = { data: Record<string, unknown>; cases: NestedNode; illustration: NestedNode; interactive?: NestedNode }

const pointer = (data: Record<string, unknown>, path: string): unknown =>
  path
    .split(`/`)
    .filter(Boolean)
    .reduce<unknown>((at, key) => (at && typeof at === `object` ? (at as Record<string, unknown>)[key] : undefined), data)

const resolve = (value: unknown, data: Record<string, unknown>): unknown =>
  value && typeof value === `object` && !Array.isArray(value) && typeof (value as { path?: unknown }).path === `string` && Object.keys(value).length === 1 ? pointer(data, (value as { path: string }).path) : value

/** Motion id → its props (bindings resolved) and whether `on.press` makes it a button. */
function motionProps(tree: Tree): Map<string, { props: Record<string, unknown>; pressable: boolean }> {
  const out = new Map<string, { props: Record<string, unknown>; pressable: boolean }>()
  const walk = (node: NestedNode) => {
    if (node.component === `Motion`) out.set(node.id, { props: Object.fromEntries(Object.entries(node.props ?? {}).map(([k, v]) => [k, resolve(v, tree.data)])), pressable: Boolean(node.on?.press) })
    for (const child of node.children ?? []) walk(child as NestedNode)
  }
  walk(tree.cases)
  walk(tree.illustration)
  if (tree.interactive) walk(tree.interactive)
  return out
}

async function wasmBytes(): Promise<ArrayBuffer> {
  const bin = Uint8Array.from(atob(MOTION_WASM_GZ_BASE64), (c) => c.charCodeAt(0))
  return new Response(new Blob([bin]).stream().pipeThrough(new DecompressionStream(`gzip`))).arrayBuffer()
}

/** Canvas id → its props (bindings resolved). */
function canvasProps(tree: { data: Record<string, unknown>; cases: NestedNode }): Map<string, Record<string, unknown>> {
  const out = new Map<string, Record<string, unknown>>()
  const walk = (node: NestedNode) => {
    if (node.component === `Canvas`) out.set(node.id, Object.fromEntries(Object.entries(node.props ?? {}).map(([k, v]) => [k, resolve(v, tree.data)])))
    for (const child of node.children ?? []) walk(child as NestedNode)
  }
  walk(tree.cases)
  return out
}

function mountCanvases(canvases: HTMLElement[]): void {
  const props = canvasProps(canvasSpecimens as unknown as { data: Record<string, unknown>; cases: NestedNode })
  for (const el of canvases) {
    const id = el.dataset.xuiId ?? ``
    const spec = props.get(id)
    const canvas = el.querySelector(`canvas`)
    if (!spec || !canvas) continue
    const surface = el.closest<HTMLElement>(`[data-xui-surface]`)
    const mode = surface?.dataset.xuiMode === `light` ? `light` : `dark`
    const theme = builtinTheme(surface?.dataset.xuiTheme ?? DEFAULT_THEME_ID)
    const readout = surface?.querySelector<HTMLElement>(`[data-xui-id="${id}-readout"]`)
    new CanvasView(el, canvas).update(spec, {
      theme,
      mode,
      emit: (event, payload) => {
        if (event === `press` && readout && payload) readout.textContent = `Last press: x ${String(payload.x)} · y ${String(payload.y)}`
      },
    })
  }
}

async function mount(): Promise<void> {
  const roots = [...document.querySelectorAll(`[data-ui-island]`)].map((host) => host.shadowRoot).filter((r): r is ShadowRoot => r !== null)
  const motions = roots.flatMap((r) => [...r.querySelectorAll<HTMLElement>(`[data-xui-c="Motion"]`)])
  const canvases = roots.flatMap((r) => [...r.querySelectorAll<HTMLElement>(`[data-xui-c="Canvas"]`)])
  if (!motions.length && !canvases.length) return
  initMotionWasm(await wasmBytes())
  mountCanvases(canvases)
  const props = motionProps(specimens as unknown as Tree)
  const reduced = matchMedia(`(prefers-reduced-motion: reduce)`).matches
  for (const el of motions) {
    const spec = props.get(el.dataset.xuiId ?? ``)
    const canvas = el.querySelector(`canvas`)
    if (!spec || !canvas) continue
    const surface = el.closest<HTMLElement>(`[data-xui-surface]`)
    const mode = surface?.dataset.xuiMode === `light` ? `light` : `dark`
    const theme = builtinTheme(surface?.dataset.xuiTheme ?? DEFAULT_THEME_ID)
    new MotionView(el, canvas).update(spec.props, { theme, mode, reducedMotion: reduced, host: {}, pressable: spec.pressable }, undefined)
  }
}

void mount()
