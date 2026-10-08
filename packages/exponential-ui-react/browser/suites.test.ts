// VAPP-91: the React renderer's Exponential UI conformance runner. Every
// suite of `@exponential-at/ui/conformance/manifest.json`: the pure ones
// through the TS reference this renderer is built on, the painted ones in
// headless Chromium through the harness. Writes the report
// (EXPONENTIAL_UI_CONFORMANCE_REPORT, default
// ../../.conformance/exponential-ui-react.json) and fails unless
// `checkReport` says conformant.

import { afterAll, beforeAll, describe, expect, it } from "vitest"
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import {
  A2UI_BASIC_CATALOG_ID,
  BUILTIN_THEMES,
  BUILTIN_THEME_IDS,
  CORE_CATALOG_ID,
  HostRouter,
  JsonlDecoder,
  MODES,
  SseDecoder,
  UNKNOWN_COMPONENT,
  builtinTheme,
  checkGeometry,
  checkReport,
  clientCapabilities,
  combineDecisions,
  conformanceManifest,
  decideFunction,
  decideUrl,
  defineExtension,
  loadTheme,
  mcpActionCall,
  mediaRequest,
  messagesFromMcpResult,
  parseSource,
  placeOverlay,
  preorder,
  reduceNested,
  reduceSurface,
  resolveRecipe,
  supportedCatalogIds,
  tryLoadTheme,
  validatePackage,
} from "@exponential-at/ui"
import type { ConformanceReport, Decoded, ExtensionDef, FlatComponent, ModeName, NestedNode, UiNode, VappPackage } from "@exponential-at/ui"
import { openPage, startHarness, type Harness } from "./support"

const fixture = (name: string) => JSON.parse(readFileSync(join(import.meta.dirname, `..`, `..`, `exponential-ui`, `fixtures`, name), `utf8`))
const canon = (v: unknown) => JSON.stringify(v)
const pkg = JSON.parse(readFileSync(join(import.meta.dirname, `..`, `package.json`), `utf8`)) as { name: string; version: string }

type Suite = { cases: number; passed: number; failed: string[] }
const suites: Record<string, Suite> = {}
function record(suite: string, name: string, ok: boolean | (() => boolean | string), detail = ``) {
  const s = (suites[suite] ??= { cases: 0, passed: 0, failed: [] })
  s.cases += 1
  let verdict: boolean | string
  try {
    verdict = typeof ok === `function` ? ok() : ok
  } catch (e) {
    verdict = e instanceof Error ? e.message : String(e)
  }
  if (verdict === true) s.passed += 1
  else s.failed.push(`${name}${typeof verdict === `string` ? `: ${verdict}` : detail ? `: ${detail}` : ``}`)
}

let h: Harness
beforeAll(async () => {
  h = await startHarness()
})
afterAll(async () => h?.close())

function walk(node: UiNode, visit: (n: UiNode) => void): void {
  visit(node)
  for (const slot of Object.values(node.slots ?? {})) walk(slot, visit)
  node.children.forEach((c) => walk(c, visit))
}

describe(`Exponential UI conformance: @exponential-at/ui-react`, () => {
  it(`catalog: every case reduces cleanly and paints without Unknown`, async () => {
    const cases = fixture(`catalog-components.json`).cases as { name: string; node: NestedNode }[]
    const page = await openPage(h, { view: `catalog`, theme: `exponential`, mode: `light` }, { width: 1400, height: 900 })
    const painted = (await page.evaluate(() => Array.from(document.querySelectorAll(`.xui-surface`)).map((s) => ({ id: s.getAttribute(`data-xui-surface`), nodes: s.querySelectorAll(`[data-xui-id]`).length, unknown: s.querySelectorAll(`.xui-Unknown-root, [data-xui-component="Unknown"]`).length })))) as { id: string; nodes: number; unknown: number }[]
    await page.context().close()
    const byId = new Map(painted.map((p) => [p.id, p]))
    for (const c of cases)
      record(`catalog`, c.name, () => {
        const { issues } = reduceNested(c.node, { catalogId: CORE_CATALOG_ID })
        if (issues.length) return `issues ${canon(issues)}`
        const p = byId.get(c.name.replace(/[^a-z0-9]/gi, `-`))
        if (!p) return `not painted`
        if (p.unknown) return `Unknown painted`
        // A closed overlay (round 1: `Toast/open=false`) paints nothing at all.
        return p.nodes > 0 || (c.node.props as { open?: unknown } | undefined)?.open === false || `no nodes`
      })
  })

  it(`macros, basic-map, extension`, () => {
    for (const c of fixture(`catalog-macros.json`).cases as { name: string; input: NestedNode; expected: UiNode }[])
      record(`macros`, c.name, () => canon(reduceNested(c.input, { catalogId: CORE_CATALOG_ID }).root) === canon(c.expected))
    for (const c of fixture(`catalog-basic-map.json`).cases as { name: string; components: FlatComponent[]; expected: unknown }[])
      record(`basic-map`, c.name, () => canon(reduceSurface(c.components, { catalogId: A2UI_BASIC_CATALOG_ID })) === canon(c.expected))
    const ext = fixture(`catalog-extension.json`)
    const extension = defineExtension(ext.extension as ExtensionDef)
    for (const c of ext.cases as { name: string; catalogId: string; components: FlatComponent[]; expected: unknown }[])
      record(`extension`, c.name, () => canon(reduceSurface(c.components, { catalogId: c.catalogId, extensions: [extension] })) === canon(c.expected))
  })

  it(`themes`, () => {
    const resolved = fixture(`theme-resolved.json`).themes as Record<string, unknown>
    for (const [id, expected] of Object.entries(resolved)) record(`theme-resolved`, id, () => canon(builtinTheme(id)) === canon(expected))
    for (const c of fixture(`theme-recipes.json`).cases as { component: string; part: string; props: Record<string, unknown>; visuals: Record<string, Record<string, Record<string, unknown>>> }[])
      record(`theme-recipes`, `${c.component}/${c.part} ${canon(c.props)}`, () => {
        for (const [themeId, byMode] of Object.entries(c.visuals))
          for (const [mode, byState] of Object.entries(byMode))
            for (const [state, style] of Object.entries(byState)) {
              const states = state === `default` ? [] : [state]
              if (canon(resolveRecipe(builtinTheme(themeId), { component: c.component, part: c.part, props: c.props, states }, mode as ModeName)) !== canon(style)) return `${themeId}/${mode}/${state}`
            }
        return true
      })
    for (const c of fixture(`theme-extends.json`).cases as { name: string; theme: never; expected: { chain: string[]; probes: { component: string; part: string; props: Record<string, unknown>; states: string[]; mode: ModeName; style: unknown }[] } }[])
      record(`theme-extends`, c.name, () => {
        const theme = loadTheme(c.theme, { themes: BUILTIN_THEMES })
        if (canon(theme.chain) !== canon(c.expected.chain)) return `chain`
        return c.expected.probes.every((p) => canon(resolveRecipe(theme, { component: p.component, part: p.part, props: p.props, states: p.states }, p.mode)) === canon(p.style))
      })
    for (const c of fixture(`theme-invalid.json`).cases as { name: string; theme: unknown; issues: unknown }[])
      record(`theme-invalid`, c.name, () => {
        const { theme, issues } = tryLoadTheme(c.theme, { themes: BUILTIN_THEMES })
        return theme === null && canon(issues) === canon(c.issues)
      })
  })

  it(`control-geometry: the painted controls keep the theme's box`, async () => {
    const themes = fixture(`control-geometry.json`).themes as Record<string, Record<string, { part: string; cases: Record<string, { geometry: Record<string, number> }> }>>
    for (const [themeId, byComponent] of Object.entries(themes)) {
      const page = await openPage(h, { view: `controls`, theme: themeId }, { width: 1400, height: 900 })
      const boxes = (await page.evaluate(() => window.__xuiControls!())) as Record<string, Record<string, number> | null>
      await page.context().close()
      for (const [component, entry] of Object.entries(byComponent))
        for (const [name, c] of Object.entries(entry.cases))
          record(`control-geometry`, `${themeId} ${component} ${name}`, () => {
            const box = boxes[`${component} ${name}`]
            if (!box) return `part ${entry.part} not painted`
            const issues = checkGeometry(c.geometry, box, 0.5)
            return issues.length === 0 || issues.map((i) => `${i.key} ${i.expected}≠${i.actual}`).join(`, `)
          })
    }
  })

  it(`layout: frames within 1 px of taffy`, async () => {
    const geometry = fixture(`layout-geometry.json`)
    const tolerance = geometry.measure.tolerancePx as number
    for (const [name, c] of Object.entries(geometry.cases as Record<string, { frames: { id: string; x: number; y: number; w: number; h: number }[] }>)) {
      const page = await openPage(h, { view: `geometry`, case: name })
      const frames = (await page.evaluate(() => window.__xuiFrames!())) as { id: string; x: number; y: number; w: number; h: number }[]
      await page.context().close()
      const byId = new Map(frames.map((f) => [f.id, f]))
      record(`layout`, name, () => {
        if (frames.length !== c.frames.length) return `${frames.length} frames, want ${c.frames.length}`
        for (const t of c.frames) {
          const b = byId.get(t.id)
          if (!b) return `${t.id} missing`
          for (const axis of [`x`, `y`, `w`, `h`] as const) if (Math.abs(b[axis] - t[axis]) > tolerance) return `${t.id}.${axis}`
        }
        return true
      })
    }
  })

  it(`overlay: placement, side and flip`, async () => {
    const overlays = fixture(`overlay-geometry.json`)
    const cases = overlays.cases as { name: string; viewport: { width: number; height: number }; anchor: { x: number; y: number; width: number; height: number }; side: `top`; expected: { side: string } }[]
    for (const [i, c] of cases.entries()) {
      const page = await openPage(h, { view: `overlay`, case: i }, { width: c.viewport.width, height: c.viewport.height })
      await page.waitForSelector(`[data-xui-overlay="Popover"]`)
      await page.waitForTimeout(150)
      const got = (await page.evaluate(() => window.__xuiOverlay!())) as { x: number; y: number; w: number; h: number; side: string } | null
      await page.context().close()
      record(`overlay`, c.name, () => {
        if (!got) return `not painted`
        const want = placeOverlay(c.anchor, { width: got.w, height: got.h }, c.viewport, { side: c.side })
        return got.side === c.expected.side && want.side === c.expected.side && Math.abs(got.x - want.x) <= overlays.tolerancePx && Math.abs(got.y - want.y) <= overlays.tolerancePx
      })
    }
  })

  it(`replay: the kitchen sink in every built-in theme and mode`, async () => {
    const expanded = fixture(`kitchen-sink.expanded.json`)
    const sink = fixture(`kitchen-sink.json`) as NestedNode
    const order = preorder(expanded.root as UiNode)
    for (const themeId of BUILTIN_THEME_IDS)
      for (const mode of MODES) {
        const page = await openPage(h, { view: `kitchen-sink`, theme: themeId, mode })
        const painted = (await page.evaluate(() => Array.from(document.querySelectorAll(`[data-xui-surface="ks"] [data-xui-id]`)).map((e) => (e as HTMLElement).dataset.xuiId!))) as string[]
        await page.context().close()
        record(`replay`, `${themeId}/${mode}`, () => {
          const reduced = reduceNested(sink, { catalogId: CORE_CATALOG_ID })
          if (canon(reduced.root) !== canon(expanded.root) || canon(reduced.issues) !== canon(expanded.issues)) return `reduced tree differs`
          let unknown = false
          walk(reduced.root, (n) => (unknown ||= n.component === UNKNOWN_COMPONENT))
          if (unknown) return `Unknown`
          // DOM order = pre-order (the a11y order) for every painted node.
          const idx = painted.filter((id) => order.includes(id)).map((id) => order.indexOf(id))
          if (idx.length < 50) return `only ${idx.length} nodes painted`
          return idx.every((v, i) => i === 0 || v > idx[i - 1]!) || `DOM order is not the pre-order`
        })
      }
  })

  it(`host-transport, host-policy, host-router`, () => {
    const feed = (dec: { push(c: string): Decoded; end(): Decoded }, chunks: string[]) => {
      const out: Decoded = { messages: [], issues: [] }
      for (const d of [...chunks.map((c) => dec.push(c)), dec.end()]) {
        out.messages.push(...d.messages)
        out.issues.push(...d.issues)
      }
      return out
    }
    const t = fixture(`host-transport.json`)
    for (const c of t.jsonl) record(`host-transport`, `jsonl: ${c.name}`, () => canon(feed(new JsonlDecoder(), c.chunks)) === canon(c.expected))
    for (const c of t.sse) record(`host-transport`, `sse: ${c.name}`, () => canon(feed(new SseDecoder(), c.chunks)) === canon(c.expected))
    for (const c of t.mcp) record(`host-transport`, `mcp: ${c.name}`, () => canon(messagesFromMcpResult(c.result)) === canon(c.expected))
    for (const c of t.mcpAction) record(`host-transport`, `mcpAction: ${c.name}`, () => canon(mcpActionCall(c.message, c.tool)) === canon(c.expected))
    const p = fixture(`host-policy.json`)
    for (const c of p.functions) record(`host-policy`, `function: ${c.name}`, () => decideFunction(c.policy, c.fn, c.registered) === c.expected)
    for (const c of p.combine) record(`host-policy`, `combine: ${c.a}+${c.b}`, () => combineDecisions(c.a, c.b) === c.expected)
    for (const c of p.urls) record(`host-policy`, `url: ${c.name}`, () => canon(decideUrl(c.policy, c.url)) === canon(c.expected))
    for (const c of p.media) record(`host-policy`, `media: ${c.name}`, () => canon(mediaRequest(c.url, c.options)) === canon(c.expected))
    for (const c of p.sources) record(`host-policy`, `source: ${c.uri}`, () => canon(parseSource(c.uri)) === canon(c.expected))
    for (const c of p.negotiation)
      record(`host-policy`, `negotiation: ${c.extensionIds.length}`, () => canon(supportedCatalogIds(c.extensionIds)) === canon(c.expected.supportedCatalogIds) && canon(clientCapabilities(c.extensionIds)) === canon(c.expected.clientCapabilities))
    const r = fixture(`host-router.json`)
    const packages = r.packages as Record<string, VappPackage>
    for (const v of r.validation) record(`host-router`, `validation: ${v.package}`, () => canon(validatePackage(packages[v.package])) === canon(v.expected))
    for (const flow of r.flows as { name: string; extensionIds?: string[]; packages?: string[]; installIssues?: Record<string, unknown>; steps: { message: unknown; expected: unknown }[] }[])
      record(`host-router`, `flow: ${flow.name}`, () => {
        const router = new HostRouter({ extensionIds: flow.extensionIds ?? [] })
        for (const id of flow.packages ?? []) if (canon(router.installPackage(packages[id]!)) !== canon(flow.installIssues![id])) return `install ${id}`
        for (const [i, s] of flow.steps.entries()) if (canon(router.route(s.message)) !== canon(s.expected)) return `step ${i}`
        return true
      })
  })

  it(`writes the report and is conformant`, () => {
    const report: ConformanceReport = { renderer: pkg.name, platform: `web`, version: pkg.version, conformanceVersion: conformanceManifest().version, suites }
    const out = process.env.EXPONENTIAL_UI_CONFORMANCE_REPORT ?? join(import.meta.dirname, `..`, `..`, `..`, `.conformance`, `exponential-ui-react.json`)
    mkdirSync(dirname(out), { recursive: true })
    writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`)
    const verdict = checkReport(report)
    expect(verdict.problems).toEqual([])
    expect(verdict.conformant).toBe(true)
  })
})

void BUILTIN_THEMES
void CORE_CATALOG_ID
