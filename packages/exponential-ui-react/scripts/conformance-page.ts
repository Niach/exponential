// The CONFORMANCE page (browser entry, bundled by scripts/conformance-serve.ts):
// one case of packages/exponential-ui/fixtures/conformance-cases.json —
// `?case=<fixture>/<theme>/<mode>/<width>/<direction>` — rendered by the
// React renderer with the conformance font set (@font-face rules over the
// committed font files), then `window.__xuiConformance.dump()` returns the
// frame dump (packages/exponential-ui/conformance/dom.ts). No JSX: the
// package's scripts are plain .ts.

import { createElement, StrictMode, useMemo } from "react"
import { createRoot } from "react-dom/client"
import type { NestedNode } from "@exponential-at/ui"
import { allCases, caseInput, fontFaceCss, parseCaseKey, MANIFEST_PATH, type CaseDump, type ConformanceManifest, type FontManifest } from "../../exponential-ui/conformance/dump"
import { domDump } from "../../exponential-ui/conformance/dom"
import { ExponentialSurface, useSurface } from "../src/index"
import type { HostPlugin } from "../src/index"
import { harnessIcons } from "../harness/icons"

declare global {
  interface Window {
    __xuiConformance?: { ready: Promise<void>; dump: () => CaseDump; error?: string }
  }
}

const read = async (path: string) => {
  const r = await fetch(`/repo/${path}`)
  if (!r.ok) throw new Error(`${path}: ${r.status}`)
  return r.json() as Promise<unknown>
}

function Case(props: { tree: NestedNode; data: Record<string, unknown>; theme: string; mode: `light` | `dark`; width: number; direction: `ltr` | `rtl`; locale: string }) {
  const surface = useSurface({ surfaceId: `conformance`, initial: props.tree, data: props.data })
  const host = useMemo<HostPlugin>(() => ({ icons: harnessIcons }), [])
  return createElement(ExponentialSurface, { id: `conformance`, surface, host, theme: props.theme, mode: props.mode, direction: props.direction, width: props.width, locale: props.locale })
}

let textComponents: string[] = []

async function main() {
  const key = new URLSearchParams(location.search).get(`case`) ?? ``
  const manifest = (await read(MANIFEST_PATH)) as ConformanceManifest
  if (!allCases(manifest).some((c) => c.key === key)) throw new Error(`unknown conformance case "${key}"`)
  const c = parseCaseKey(key)
  textComponents = manifest.textComponents
  const fonts = (await read(manifest.fonts)) as FontManifest
  const style = document.createElement(`style`)
  style.textContent = fontFaceCss(fonts, (file) => `/repo/${file}`)
  document.head.appendChild(style)
  // Load every face up front (font-display: block + an explicit load), so
  // the first layout already uses them.
  await Promise.all(Array.from(document.fonts).map((f) => f.load().catch(() => undefined)))
  const { tree, data } = await caseInput(manifest, c, read)
  document.body.style.margin = `0`
  document.body.style.background = c.mode === `dark` ? `#0f0f11` : `#f4f4f5`
  const app = document.getElementById(`app`)!
  createRoot(app).render(createElement(StrictMode, null, createElement(Case, { tree: tree as unknown as NestedNode, data, theme: c.theme, mode: c.mode, width: c.width, direction: c.direction, locale: manifest.locale })))
  // Two frames after the surface mounted: React committed, layout settled.
  for (let i = 0; i < 200 && !app.querySelector(`.xui-surface [data-xui-id]`); i++) await new Promise((r) => setTimeout(r, 25))
  await document.fonts.ready
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
  const failed = Array.from(document.fonts).filter((f) => f.status === `error`)
  if (failed.length) throw new Error(`fonts failed: ${failed.map((f) => `${f.family} ${f.weight}`).join(`, `)}`)
}

const ready = main()
window.__xuiConformance = { ready, dump: () => domDump(document.querySelector(`.xui-surface`)!, textComponents) }
ready.catch((e: unknown) => {
  window.__xuiConformance!.error = String(e)
})
