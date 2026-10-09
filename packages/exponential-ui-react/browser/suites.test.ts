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
  THEME_SCHEMA_ID,
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
  DEFAULT_STRINGS,
  animationFrame,
  displayString,
  intlFormatter,
  scrollOffsetForIndex,
  tableRowKeys,
  tokenizeCode,
  keyframesCss,
  resolveStyleValues,
  styleToCss,
} from "@exponential-at/ui"
import type { ChildTemplate, ConformanceReport, Decoded, ExtensionDef, FlatComponent, ModeName, NestedNode, UiNode, VappPackage } from "@exponential-at/ui"
import { CLIENT_FUNCTIONS, LITERAL_ROWS_ROOT, absolutePath, bindTree as reactBindTree, isBinding, memoFormatter, resolveNodeProps, resolveValue, setPointer, type DataModel, type ResolveContext } from "../src/data"
import { itemInstance, runNodeAction, templateItems } from "../src/node-view"
import type { SurfaceContextValue } from "../src/context"
import { BASE_CSS } from "../src/base-css"
import { openPage, startHarness, type Harness } from "./support"

const fixture = (name: string) => JSON.parse(readFileSync(join(import.meta.dirname, `..`, `..`, `exponential-ui`, `fixtures`, name), `utf8`))
const canon = (v: unknown) => JSON.stringify(v)
const pkg = JSON.parse(readFileSync(join(import.meta.dirname, `..`, `package.json`), `utf8`)) as { name: string; version: string }

type Suite = { cases: number; passed: number; failed: string[] }
const suites: Record<string, Suite> = {}
function record(suite: string, name: string, ok: boolean | string | (() => boolean | string), detail = ``) {
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
    // Every case is painted once its last surface (a toast in its layer) is.
    await page.waitForFunction(() => document.querySelector(`[data-xui-surface="Toast-example"] [data-xui-id]`) !== null, undefined, { timeout: 60_000 })
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

  // Round 2 (manifest v2) through THIS renderer: the bind pass and the
  // action runner are React's (`bindTree`, `runNodeAction`), format calls
  // run React's bind table on the surface Formatter, template instances
  // are React's `templateItems` + `itemInstance`; style conditions, text
  // direction, Resizable and the windowed list are PAINTED in Chromium and
  // read back. Where React calls the core directly (tokenizeCode, the
  // reducer, tableRowKeys, an arbitrary scrollTo inset, the animation
  // frames) the core replay stands in, said per line.
  it(`bind, code-tokens (React's bind pass and action runner)`, () => {
    const strings = DEFAULT_STRINGS as Readonly<Record<string, string>>
    const formatter = memoFormatter(intlFormatter(`en-US`, `UTC`))
    const rctx = (data: unknown, scope = ``): ResolveContext => ({ data, scope, strings, locale: `en-US`, formatter, functions: CLIENT_FUNCTIONS })
    const find = (n: UiNode, id: string): UiNode | undefined => {
      let hit: UiNode | undefined
      walk(n, (x) => (hit ??= x.id === id ? x : undefined))
      return hit
    }
    /** A press through `runNodeAction` on a stub surface: `set` writes a copy
     *  of the data, any other function is the host's `call`. */
    const press = (node: UiNode, data: unknown) => {
      let out = data as DataModel
      let call: { call: string; args: Record<string, unknown> } | undefined
      const fn = (node.on?.press as { functionCall?: { call: string }; function?: { call: string } } | undefined)
      const name = (fn?.functionCall ?? fn?.function)?.call
      const functions = Object.fromEntries(Object.entries(CLIENT_FUNCTIONS).map(([k, f]) => [k, (args: Record<string, unknown>, c: ResolveContext) => {
        if (k === name) call = { call: k, args }
        return f(args, { ...c, openUrl: () => {} })
      }]))
      const ctx = { surfaceId: `s`, data, functions, openUrl: () => {}, locale: `en-US`, strings, formatter, setData: (p: string, v: unknown) => (out = setPointer(out, p, v)), host: { onAction: () => undefined, onFunctionCall: (e: { name: string; args: Record<string, unknown> }) => void (call = { call: e.name, args: e.args }) } } as unknown as SurfaceContextValue
      runNodeAction(ctx, node, node.id, ``, `press`)
      return call ? { data: out, call } : { data: out }
    }
    type BindCase = { name: string; input?: NestedNode; expanded: UiNode; issues?: unknown[]; datasets: { data: unknown; bound: UiNode | null; presses: { id: string; outcome: unknown }[]; rowSlots?: { id: string; rows: { index: number; slots: Record<string, UiNode | null> }[] }[] }[] }
    const bind = fixture(`bind-time.json`) as { cases: BindCase[]; extra: BindCase[] }
    for (const c of [...bind.cases, ...bind.extra])
      record(`bind`, c.name, () => {
        if (c.input && c.issues) {
          const r = reduceNested(c.input, { catalogId: CORE_CATALOG_ID })
          if (canon(r.root) !== canon(c.expanded) || canon(r.issues) !== canon(c.issues)) return `expand`
        }
        for (const d of c.datasets) {
          if (canon(reactBindTree(c.expanded, rctx(d.data))) !== canon(d.bound)) return `bound`
          for (const p of d.presses) if (canon(press(find(c.expanded, p.id)!, d.data)) !== canon(p.outcome)) return `press ${p.id}`
          // A slot cell = the Table's RowScope: bound rows scope the row
          // pointer, literal rows mount the row at a synthetic pointer.
          for (const t of d.rowSlots ?? []) {
            const table = find(c.expanded, t.id)!
            const rowsProp = table.props.rows
            const rows = resolveNodeProps(`Table`, { rows: rowsProp }, rctx(d.data)).rows as Record<string, unknown>[]
            for (const r of t.rows)
              for (const [name, slot] of Object.entries(r.slots)) {
                const literal = `${LITERAL_ROWS_ROOT}/${t.id}/${r.index}`
                const got = isBinding(rowsProp) ? reactBindTree(table.slots![name], rctx(d.data, `${absolutePath(rowsProp.path)}/${r.index}`)) : reactBindTree(table.slots![name], rctx(setPointer(d.data as DataModel, literal, rows[r.index]), literal))
                if (canon(got) !== canon(slot)) return `row ${r.index}`
              }
          }
        }
        return true
      })
    // The CodeBlock painter calls the core tokenizer.
    for (const c of fixture(`code-tokens.json`).cases as { name: string; language: string; code: string; expected: unknown }[]) record(`code-tokens`, c.name, () => canon(tokenizeCode(c.code, c.language)) === canon(c.expected))
  })

  it(`style-conditions: the compiled CSS in Chromium (hover: none = a touch context)`, async () => {
    type Ctx = { width: number; height?: number; hover?: boolean; reducedMotion?: boolean; states?: string[]; breakpoints?: Record<string, number> }
    const conditions = fixture(`style-conditions.json`) as { cases: { name: string; style: Record<string, unknown>; contexts: Ctx[]; expected: Record<string, unknown>[] }[] }
    for (const [ci, c] of conditions.cases.entries()) {
      let verdict: true | string = true
      for (const [xi, ctx] of c.contexts.entries()) {
        // A touch context (hasTouch) is Chromium's `(hover: none)`.
        const context = await h.browser.newContext({ viewport: { width: Math.max(ctx.width + 40, 400), height: Math.max((ctx.height ?? 120) + 40, 400) }, reducedMotion: ctx.reducedMotion ? `reduce` : `no-preference`, hasTouch: ctx.hover === false })
        const page = await context.newPage()
        await page.goto(h.url({ view: `conditions`, case: ci, ctx: xi }))
        await page.waitForSelector(`[data-xui-id="n"]`, { state: `attached` })
        await page.waitForTimeout(100)
        const theme = ctx.breakpoints ? loadTheme({ $schema: THEME_SCHEMA_ID, id: `bp`, name: `Breakpoints`, extends: `neutral`, tokens: { breakpoint: ctx.breakpoints } } as never, { themes: BUILTIN_THEMES }) : builtinTheme(`neutral`)
        const want = styleToCss(resolveStyleValues(theme, c.expected[xi], `light`), theme.fonts)
        if (ctx.reducedMotion) delete want.transition // reduced motion zeroes every duration (contract §2)
        const got = await page.evaluate((css) => {
          const node = document.querySelector(`[data-xui-id="n"]`) as HTMLElement
          const probe = document.createElement(`div`)
          probe.className = `xui-el`
          node.parentElement!.appendChild(probe)
          for (const [k, v] of Object.entries(css as Record<string, string>)) probe.style.setProperty(k, v)
          const a = getComputedStyle(node)
          const b = getComputedStyle(probe)
          const bad = Object.keys(css).map((k) => (k === `transition` ? `transition-duration` : k)).filter((k) => a.getPropertyValue(k) !== b.getPropertyValue(k))
          probe.remove()
          return bad
        }, want)
        await context.close()
        if (got.length) {
          verdict = `context ${xi}: ${got.join(`, `)}`
          break
        }
      }
      record(`style-conditions`, c.name, verdict)
    }
  }, 120_000)

  it(`format, template-items (React's bind table + template keying)`, () => {
    const formatter = memoFormatter(intlFormatter(`en-US`, `UTC`))
    const f = fixture(`format.json`) as { calls: { name: string; call: unknown; expected: unknown }[]; zoned: { name: string; timeZone: string; call: unknown; expected: string }[]; display: { name: string; value: unknown; expected: string }[] }
    for (const c of f.calls) record(`format`, c.name, () => canon(resolveValue(c.call, { data: {}, locale: `en-US`, formatter })) === canon(c.expected))
    // `zoned`: the surface Formatter in the case's IANA zone (React's is Intl,
    // a platform Formatter: U+202F / U+00A0 compare as a space).
    const spaces = (v: unknown) => String(v).replace(/[  ]/g, ` `)
    for (const c of f.zoned) record(`format`, `zoned: ${c.name}`, () => spaces(resolveValue(c.call, { data: {}, locale: `en-US`, formatter: memoFormatter(intlFormatter(`en-US`, c.timeZone)) })) === spaces(c.expected))
    for (const d of f.display) record(`format`, `display: ${d.name}`, () => displayString(d.value) === d.expected)
    const items = fixture(`template-items.json`)
    const keyed = (data: unknown, template: ChildTemplate, scope: string) => templateItems({ data, templateNode: () => ({}) } as unknown as SurfaceContextValue, { template } as unknown as UiNode, scope)?.items ?? []
    for (const c of items.keys) record(`template-items`, `keys: ${c.name}`, () => canon(keyed({ items: c.items }, { component: `x`, path: `/items`, ...(c.key === undefined ? {} : { key: c.key }) } as ChildTemplate, ``).map((i) => i.key)) === canon(c.expected))
    // The Table painter keys its rows with the core's `tableRowKeys`.
    for (const c of items.rowKeys) record(`template-items`, `rowKeys: ${c.name}`, () => canon(tableRowKeys(c.rows, c.rowKey)) === canon(c.expected))
    for (const c of items.instances as { name: string; data: unknown; template: ChildTemplate; inner?: ChildTemplate; scope: string; instance: string; expected: unknown[] }[])
      record(`template-items`, `instances: ${c.name}`, () => {
        const outer = keyed(c.data, c.template, c.scope).map((i) => ({ key: i.key, path: i.path, index: i.index, instance: itemInstance(c.instance, i.key) }))
        if (!c.inner) return canon(outer) === canon(c.expected)
        const nested = outer.flatMap((o) => keyed(c.data, c.inner!, o.path).map((i) => {
          const instance = itemInstance(o.instance, i.key)
          return { outer: o.key, key: i.key, path: i.path, index: i.index, instance, ids: { issue: `issue${instance}`, title: `issue.title${instance}` } }
        }))
        return canon(nested) === canon(c.expected)
      })
    // React paints the core reducer's output.
    for (const c of items.reduce as { name: string; catalogId: string; nested?: NestedNode; components?: FlatComponent[]; expected: unknown }[])
      record(`template-items`, `reduce: ${c.name}`, () => canon(c.nested ? reduceNested(c.nested, { catalogId: c.catalogId }) : reduceSurface(c.components!, { catalogId: c.catalogId })) === canon(c.expected))
  })

  it(`text-direction, resizable, virtual-list, animations: painted in Chromium`, async () => {
    const context = await h.browser.newContext({ viewport: { width: 1200, height: 900 }, deviceScaleFactor: 1 })
    const page = await context.newPage()
    await page.goto(h.url({ view: `mount` }))
    await page.waitForFunction(() => typeof window.__xuiMount === `function` && typeof window.__xuiVList === `function`)
    const mount = (tree: unknown, options: Record<string, unknown> = {}) => page.evaluate(([t, o]) => window.__xuiMount!(t as never, o as never), [tree, options] as const)
    const frames = () => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(r)))))

    // text-direction: every node's computed `direction` and its computed
    // `text-align` resolved to the physical side.
    for (const c of fixture(`text-direction.json`).cases as { name: string; tree: NestedNode; surface: `ltr` | `rtl`; expected: Record<string, { direction: string; textAlign: string }> }[]) {
      await mount(c.tree, { dir: c.surface, width: 600 })
      const got = await page.evaluate((ids) => {
        const out: Record<string, { direction: string; textAlign: string }> = {}
        for (const id of ids) {
          const el = document.querySelector<HTMLElement>(`[data-xui-id="${id}"]`)
          if (!el) continue
          const cs = getComputedStyle(el)
          const a = cs.textAlign
          const physical = a === `start` || a === `-webkit-auto` ? (cs.direction === `rtl` ? `right` : `left`) : a === `end` ? (cs.direction === `rtl` ? `left` : `right`) : a === `-webkit-left` ? `left` : a === `-webkit-right` ? `right` : a === `-webkit-center` ? `center` : a
          out[id] = { direction: cs.direction, textAlign: physical }
        }
        return out
      }, Object.keys(c.expected))
      const bad = Object.keys(c.expected).filter((id) => canon(got[id]) !== canon(c.expected[id]))
      record(`text-direction`, c.name, bad.length === 0, bad.map((id) => `${id} ${canon(got[id])}`).join(`; `))
    }

    // resizable: a Resizable painted per case; sizes read off the panels.
    const near = (a: number[], b: number[], tol = 1e-6) => a.length === b.length && a.every((v, i) => Math.abs(v - b[i]!) <= tol)
    const rz = fixture(`resizable.json`)
    const panelsOf = (count: number) => Array.from({ length: count }, (_, i) => ({ id: `p${i}`, component: `Box` }))
    const sizes = (count: number) => page.evaluate((n) => Array.from({ length: n }, (_, i) => Number(document.querySelector<HTMLElement>(`[data-xui-id="rz.panel.${i}"]`)!.dataset.size)), count)
    /** A pointer drag of handle `i` by (dx, dy) px: synthetic pointer events (fractional coordinates). */
    const drag = (i: number, dx: number, dy: number) =>
      page.evaluate(
        ([i, dx, dy]) => {
          // No such handle (`bad-handle`): nothing to drag, sizes unchanged.
          const el = document.querySelector<HTMLElement>(`[data-xui-id="rz.handle.${i}"]`)
          if (!el) return
          const r = el.getBoundingClientRect()
          const x = r.left + r.width / 2
          const y = r.top + r.height / 2
          const fire = (type: string, mx: number, my: number) => el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, composed: true, pointerId: 7, isPrimary: true, button: 0, buttons: type === `pointerup` ? 0 : 1, clientX: x + mx, clientY: y + my }))
          fire(`pointerdown`, 0, 0)
          fire(`pointermove`, dx, dy)
          fire(`pointerup`, dx, dy)
        },
        [i, dx, dy] as const
      )
    for (const c of rz.normalize as { name: string; sizes: unknown; count: number; panels?: unknown[]; expected: number[] }[]) {
      await mount({ id: `rz`, component: `Resizable`, props: { sizes: c.sizes, ...(c.panels ? { panels: c.panels } : {}) }, style: { width: 401, height: 60 }, children: panelsOf(c.count) })
      const got = await sizes(c.count)
      record(`resizable`, `normalize: ${c.name}`, near(got, c.expected), canon(got))
    }
    for (const c of rz.resize as { name: string; sizes: number[]; handle: number; delta: number; panels?: unknown[]; expected: number[] }[]) {
      // 400 px of panels: 1 % = 4 px.
      const n = c.sizes.length
      await mount({ id: `rz`, component: `Resizable`, props: { sizes: c.sizes, ...(c.panels ? { panels: c.panels } : {}) }, style: { width: 400 + (n - 1), height: 60 }, children: panelsOf(n) }, { hairline: 1 })
      await drag(c.handle, c.delta * 4, 0)
      await frames()
      const got = await sizes(n)
      record(`resizable`, `resize: ${c.name}`, near(got, c.expected), canon(got))
    }
    for (const c of rz.keys as { name: string; sizes: number[]; handle: number; key: string; orientation: `horizontal` | `vertical`; direction: `ltr` | `rtl`; panels?: unknown[]; expected: number[] }[]) {
      const n = c.sizes.length
      await mount({ id: `rz`, component: `Resizable`, props: { sizes: c.sizes, direction: c.orientation, ...(c.panels ? { panels: c.panels } : {}) }, style: { width: 401, height: 401 }, children: panelsOf(n) }, { dir: c.direction })
      await page.focus(`[data-xui-id="rz.handle.${c.handle}"]`)
      await page.keyboard.press(c.key)
      await frames()
      const got = await sizes(n)
      record(`resizable`, `keys: ${c.name}`, near(got, c.expected), canon(got))
    }
    for (const c of rz.extents as { name: string; sizes: number[]; container: number; handle: number; expected: number[] }[]) {
      const n = c.sizes.length
      await mount({ id: `rz`, component: `Resizable`, props: { sizes: c.sizes }, style: { width: c.container, height: 60 }, children: panelsOf(n) }, { hairline: c.handle })
      const got = await page.evaluate((n) => Array.from({ length: n }, (_, i) => document.querySelector(`[data-xui-id="rz.panel.${i}"]`)!.getBoundingClientRect().width), n)
      // CSS lays out in 1/64 px.
      record(`resizable`, `extents: ${c.name}`, near(got, c.expected, 1 / 64), canon(got))
    }
    for (const c of rz.drag as { name: string; px: number; container: number; panels: number; orientation: `horizontal` | `vertical`; direction: `ltr` | `rtl`; expected: number }[]) {
      const even = Array.from({ length: c.panels }, () => 100 / c.panels)
      const vertical = c.orientation === `vertical`
      await mount({ id: `rz`, component: `Resizable`, props: { sizes: even, direction: c.orientation }, style: vertical ? { width: 200, height: c.container } : { width: c.container, height: 60 }, children: panelsOf(c.panels) }, { dir: c.direction, hairline: 1 })
      const before = await sizes(c.panels)
      await drag(0, vertical ? 0 : c.px, vertical ? c.px : 0)
      await frames()
      const got = (await sizes(c.panels))[0]! - before[0]!
      record(`resizable`, `drag: ${c.name}`, Math.abs(got - c.expected) <= 1e-6, String(got))
    }

    // virtual-list: a bare WindowedList in a scroller of `viewport` px.
    const vl = fixture(`virtual-list.json`)
    const expand = (e: number[] | { count: number; extent: number }) => (Array.isArray(e) ? e : Array.from({ length: e.count }, () => e.extent))
    const vlist = (c: { extents: number[]; gap: number; viewport: number; overscan?: number; axis?: string; stickyRows?: number[] }, act: string, a: unknown, b?: unknown) =>
      page.evaluate(
        async ([c, act, a, b]) => {
          const probe = await window.__xuiVList!(c as never)
          if (act === `window`) return probe.scroll(a as number)
          await probe.scroll(a as number)
          return probe.scrollToIndex((b as { index: number }).index, (b as { align: `start` }).align)
        },
        [c, act, a, b] as const
      )
    for (const c of vl.windows as { name: string; extents: number[] | { count: number; extent: number }; gap: number; scroll: number; viewport: number; overscan?: number; expected: Record<string, number> }[]) {
      const got = (await vlist({ extents: expand(c.extents), gap: c.gap, viewport: c.viewport, overscan: c.overscan, axis: c.name.includes(`horizontal`) ? `horizontal` : `vertical` }, `window`, c.scroll)) as unknown as Record<string, number>
      const keys = [`start`, `end`, `before`, `after`, `total`]
      record(`virtual-list`, `window: ${c.name}`, keys.every((k) => Math.abs(got[k]! - c.expected[k]!) <= 0.5), canon(got))
    }
    for (const c of vl.scrollTo as { name: string; extents: number[] | { count: number; extent: number }; gap: number; index: number; viewport: number; scroll: number; align: `start`; inset?: number; expected: number }[]) {
      if (c.inset) {
        // An arbitrary inset is not paintable (WindowedList takes it from a
        // pinned header row); the painter's jump calls this function.
        record(`virtual-list`, `scrollTo: ${c.name}`, scrollOffsetForIndex(expand(c.extents), c.gap, c.index, c.viewport, c.scroll, c.align, c.inset) === c.expected)
        continue
      }
      const got = (await vlist({ extents: expand(c.extents), gap: c.gap, viewport: c.viewport }, `scrollTo`, c.scroll, { index: c.index, align: c.align })) as number
      record(`virtual-list`, `scrollTo: ${c.name}`, Math.abs(got - c.expected) <= 0.5, String(got))
    }
    for (const c of vl.sticky as { name: string; rowExtents: number[]; headerRows: number[]; scrolls: number[]; expected: unknown[] }[]) {
      const got: unknown[] = []
      for (const s of c.scrolls) got.push(((await vlist({ extents: c.rowExtents, gap: 0, viewport: 60, stickyRows: c.headerRows }, `window`, s)) as { pinned: unknown }).pinned)
      record(`virtual-list`, `sticky: ${c.name}`, canon(got) === canon(c.expected), canon(got))
    }
    // Sections: a sectioned List painted flat, its rows read back.
    for (const c of vl.sections as { name: string; items: Record<string, unknown>[]; sectionBy: string; expected: unknown }[]) {
      await mount({ id: `L`, component: `List`, props: { sectionBy: c.sectionBy }, template: { component: `L-row`, path: `/items`, key: `id` }, children: [{ id: `L-row`, component: `Text`, props: { text: { path: `id` } } }] }, { data: { items: c.items } })
      const got = await page.evaluate((ids) => {
        const rows: ({ header: number } | { item: number })[] = []
        const sections: { value: string; start: number; count: number }[] = []
        for (const row of Array.from(document.querySelectorAll<HTMLElement>(`[data-xui-id="L"] > .xui-list-row`))) {
          const head = row.querySelector<HTMLElement>(`[data-xui-id^="L.section."]`)
          if (head) {
            rows.push({ header: sections.length })
            sections.push({ value: head.textContent ?? ``, start: 0, count: 0 })
            continue
          }
          const item = ids.indexOf(row.querySelector<HTMLElement>(`[data-xui-c="Text"]`)!.textContent ?? ``)
          rows.push({ item })
          const s = sections[sections.length - 1]
          if (s.count === 0) s.start = item
          s.count += 1
        }
        return { sections, rows }
      }, c.items.map((i) => String(i.id)))
      record(`virtual-list`, `sections: ${c.name}`, canon(got) === canon(c.expected), canon(got))
    }
    for (const c of vl.sectionedScrollTo as { name: string; index: number; viewport: number; scroll: number; align: `start`; stickyHeaders: boolean; items: Record<string, unknown>[]; sectionBy: string; headerExtent: number; itemExtent: number; expected: number }[]) {
      await mount(
        { id: `L`, component: `List`, props: { sectionBy: c.sectionBy, stickyHeaders: c.stickyHeaders }, style: { height: c.viewport }, template: { component: `L-row`, path: `/items`, key: `id` }, slots: { section: { id: `L-head`, component: `Text`, props: { text: { path: `value` } }, style: { height: c.headerExtent } } }, children: [{ id: `L-row`, component: `Text`, props: { text: { path: `id` } }, style: { height: c.itemExtent } }] },
        { data: { items: c.items }, width: 300 }
      )
      const got = await page.evaluate(
        async ([scroll, index, align]) => {
          const list = document.querySelector<HTMLElement>(`[data-xui-id="L"]`)!
          list.scrollTop = scroll as number
          await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))
          window.__xuiHandle!()!.scrollToIndex(`L`, index as number, align as `start`)
          for (let i = 0; i < 8; i++) await new Promise((r) => requestAnimationFrame(r))
          return list.scrollTop
        },
        [c.scroll, c.index, c.align] as const
      )
      record(`virtual-list`, `sectionedScrollTo: ${c.name}`, Math.abs(got - c.expected) <= 0.5, String(got))
    }

    // animations: the CSS this renderer paints per theme (duration, easing,
    // iterations off the computed style); the frames are the keyframes the
    // base sheet ships (each case checks them against the fixture), sampled by
    // the core.
    const anim = fixture(`animations.json`)
    for (const [id, entries] of Object.entries(anim.themes as Record<string, Record<string, { timing: { durationMs: number; easing: number[] | string; iterations: number | string }; frames: { t: number; frame: Record<string, number | null> }[]; reduced: unknown }>>))
      for (const [name, e] of Object.entries(entries)) {
        await mount({ id: `a`, component: `Box`, style: { width: 40, height: 40, animation: name } }, { theme: id })
        const css = await page.evaluate(() => {
          const cs = getComputedStyle(document.querySelector(`[data-xui-id="a"]`)!)
          return { duration: cs.animationDuration, easing: cs.animationTimingFunction, iterations: cs.animationIterationCount }
        })
        const theme = builtinTheme(id)
        const ms = css.duration.endsWith(`ms`) ? Number.parseFloat(css.duration) : Number.parseFloat(css.duration) * 1000
        const easing = Array.isArray(e.timing.easing) ? `cubic-bezier(${e.timing.easing.join(`, `)})` : e.timing.easing
        const verdict = (() => {
          // The keyframes + @property rules this renderer ships (base-css.ts) are the fixture's.
          if (keyframesCss(name) !== (anim.css as Record<string, string>)[name] || !BASE_CSS.includes(keyframesCss(name)) || !BASE_CSS.includes(anim.properties as string)) return `css`
          if (Math.abs(ms - e.timing.durationMs) > 0.5) return `duration ${css.duration}`
          if (css.easing !== easing) return `easing ${css.easing}`
          if (css.iterations !== String(e.timing.iterations)) return `iterations ${css.iterations}`
          for (const fr of e.frames) {
            const got = animationFrame(name, fr.t, theme) as unknown as Record<string, number | null>
            for (const [k, v] of Object.entries(fr.frame)) if (v === null ? got[k] !== null : Math.abs((got[k] as number) - v) > 1e-3) return `${fr.t}.${k}`
          }
          return canon(animationFrame(name, 5, theme, { reducedMotion: true })) === canon(e.reduced) ? true : `reduced`
        })()
        record(`animations`, `${id}/${name}`, verdict)
      }
    /** The painted opacity of node `a` with its animation paused at `t` ms. */
    const opacityAt = (t: number) =>
      page.evaluate((t) => {
        const el = document.querySelector<HTMLElement>(`[data-xui-id="a"]`)!
        for (const a of el.getAnimations()) {
          a.pause()
          a.currentTime = t
        }
        return Number(getComputedStyle(el).opacity)
      }, t)
    // `override`: animationDuration keeps the set's factor; the frame painted.
    {
      const o = anim.override as { theme: string; name: string; durationToken: string; timing: { durationMs: number }; frame: { opacity: number } }
      await mount({ id: `a`, component: `Box`, style: { width: 40, height: 40, animation: o.name, animationDuration: o.durationToken } }, { theme: o.theme })
      const duration = await page.evaluate(() => getComputedStyle(document.querySelector(`[data-xui-id="a"]`)!).animationDuration)
      const ms = duration.endsWith(`ms`) ? Number.parseFloat(duration) : Number.parseFloat(duration) * 1000
      const painted = await opacityAt(o.timing.durationMs / 2)
      record(`animations`, `override`, Math.abs(ms - o.timing.durationMs) <= 0.5 && Math.abs(painted - o.frame.opacity) <= 1e-3, `${duration} ${painted}`)
    }
    // `opacity`: the painted opacity = the node's own × the frame's.
    for (const c of anim.opacity as { name: string; own: number; animation: string; theme: string; t: number; expected: number }[]) {
      await mount({ id: `a`, component: `Box`, style: { width: 40, height: 40, opacity: c.own, animation: c.animation } }, { theme: c.theme })
      const painted = await opacityAt(c.t)
      record(`animations`, `opacity: ${c.name}`, Math.abs(painted - c.expected) <= 1e-3, String(painted))
    }
    await context.close()
  }, 300_000)

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
