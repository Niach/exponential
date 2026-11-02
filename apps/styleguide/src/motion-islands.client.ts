// EXP-1604: the styleguide page's Motion script (motion-islands.ts bundles
// it). Islands are static markup, so this mounts the SHARED web painter
// (`MotionView`, @exponential_app/eui/web) on every Motion an island
// painted: its props come from the same specimens the island rendered
// (bindings resolved against their data), its theme and mode from the
// surface. The WASM rides inline (gzip + base64): the page works over file://.

import { DEFAULT_THEME_ID, builtinTheme, type NestedNode } from "@exponential_app/eui"
import { initMotionWasm } from "@exponential_app/eui/motion"
import { MotionView } from "@exponential_app/eui/web"
import { MOTION_WASM_GZ_BASE64 } from "@exponential_app/eui/wasm/motion/bytes.ts"
import specimens from "./entries/motion.specimens.json"

type Tree = { data: Record<string, unknown>; cases: NestedNode; illustration: NestedNode }

const pointer = (data: Record<string, unknown>, path: string): unknown =>
  path
    .split(`/`)
    .filter(Boolean)
    .reduce<unknown>((at, key) => (at && typeof at === `object` ? (at as Record<string, unknown>)[key] : undefined), data)

const resolve = (value: unknown, data: Record<string, unknown>): unknown =>
  value && typeof value === `object` && !Array.isArray(value) && typeof (value as { path?: unknown }).path === `string` && Object.keys(value).length === 1 ? pointer(data, (value as { path: string }).path) : value

/** Motion id → its props, bindings resolved. */
function motionProps(tree: Tree): Map<string, Record<string, unknown>> {
  const out = new Map<string, Record<string, unknown>>()
  const walk = (node: NestedNode) => {
    if (node.component === `Motion`) out.set(node.id, Object.fromEntries(Object.entries(node.props ?? {}).map(([k, v]) => [k, resolve(v, tree.data)])))
    for (const child of node.children ?? []) walk(child as NestedNode)
  }
  walk(tree.cases)
  walk(tree.illustration)
  return out
}

async function wasmBytes(): Promise<ArrayBuffer> {
  const bin = Uint8Array.from(atob(MOTION_WASM_GZ_BASE64), (c) => c.charCodeAt(0))
  return new Response(new Blob([bin]).stream().pipeThrough(new DecompressionStream(`gzip`))).arrayBuffer()
}

async function mount(): Promise<void> {
  const roots = [...document.querySelectorAll(`[data-ui-island]`)].map((host) => host.shadowRoot).filter((r): r is ShadowRoot => r !== null)
  const motions = roots.flatMap((r) => [...r.querySelectorAll<HTMLElement>(`[data-xui-c="Motion"]`)])
  if (!motions.length) return
  initMotionWasm(await wasmBytes())
  const props = motionProps(specimens as unknown as Tree)
  const reduced = matchMedia(`(prefers-reduced-motion: reduce)`).matches
  for (const el of motions) {
    const p = props.get(el.dataset.xuiId ?? ``)
    const canvas = el.querySelector(`canvas`)
    if (!p || !canvas) continue
    const surface = el.closest<HTMLElement>(`[data-xui-surface]`)
    const mode = surface?.dataset.xuiMode === `light` ? `light` : `dark`
    const theme = builtinTheme(surface?.dataset.xuiTheme ?? DEFAULT_THEME_ID)
    new MotionView(el, canvas).update(p, { theme, mode, reducedMotion: reduced, host: {} }, undefined)
  }
}

void mount()
