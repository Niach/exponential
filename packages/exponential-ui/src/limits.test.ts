// VAPP-103: the TS reference against hostile or huge agent input — the
// same refusals as the Rust core (apps/desktop/crates/exponential-ui/tests/
// robustness.rs), the same issue text. The shared cases are the `limit-*`
// reduce cases of fixtures/template-items.json (every runner replays them).

import { describe, expect, test } from "bun:test"
import { CORE_CATALOG_ID } from "./catalog"
import { POINTER_ISSUES, runAction, writePointer } from "./dynamic"
import { LIMIT_ISSUES, LIMITS, MAX_COMPONENTS, MAX_DEPTH, MAX_MESSAGE_BYTES, MAX_POINTER_BYTES, MAX_POINTER_SEGMENTS, MAX_TEMPLATE_ITEMS } from "./limits"
import { templateBudget, templateInstances, templateSiteKey } from "./list"
import { reduceNested, reduceSurface } from "./reducer"
import { HostRouter } from "./host/router"
import { SurfaceStore } from "./host/runtime"
import limitsJson from "../catalog/limits.json" with { type: "json" }
import type { FlatComponent, NestedNode, UiNode } from "./types"

const issues = (r: { issues: { id: string; message: string }[] }) => r.issues.map((i) => `${i.id}: ${i.message}`)
const count = (n: UiNode): number => 1 + n.children.reduce((s, c) => s + count(c), 0) + Object.values(n.slots ?? {}).reduce((s, c) => s + count(c), 0)
const chain = (levels: number): FlatComponent[] => [
  ...Array.from({ length: levels }, (_, i) => ({ id: i === 0 ? `root` : `n${i}`, component: `Box`, children: [`n${i + 1}`] })),
  { id: `n${levels}`, component: `Text`, text: `leaf` },
]
const timed = <T>(budgetMs: number, f: () => T): T => {
  const t = performance.now()
  const out = f()
  expect(performance.now() - t).toBeLessThan(budgetMs)
  return out
}

describe(`limits (catalog/limits.json)`, () => {
  test(`the constants are the contract`, () => {
    expect(LIMITS).toEqual(Object.fromEntries(Object.entries(limitsJson).filter(([k]) => !k.startsWith(`$`))) as Record<string, number>)
    expect([MAX_COMPONENTS, MAX_DEPTH, MAX_MESSAGE_BYTES, MAX_TEMPLATE_ITEMS, MAX_POINTER_BYTES, MAX_POINTER_SEGMENTS]).toEqual([20000, 48, 4194304, 10000, 1024, 64])
  })

  test(`an id listed twice renders once (21 components naming the next twice)`, () => {
    const flat: FlatComponent[] = [
      ...Array.from({ length: 21 }, (_, i) => ({ id: i === 0 ? `root` : `n${i}`, component: `Stack`, children: [`n${i + 1}`, `n${i + 1}`] })),
      { id: `n21`, component: `Text`, text: `x` },
    ]
    const r = timed(1000, () => reduceSurface(flat, { catalogId: CORE_CATALOG_ID }))
    expect(issues(r)).toEqual(Array.from({ length: 21 }, (_, i) => `n${21 - i}: ${LIMIT_ISSUES.usedTwice}`))
    expect(count(r.root)).toBeLessThan(200)
  })

  test(`a deep chain is an issue and an Unknown, never a stack overflow`, () => {
    const r = reduceSurface(chain(20_000), { catalogId: CORE_CATALOG_ID })
    expect(issues(r)).toEqual([`n${MAX_DEPTH}: ${LIMIT_ISSUES.depth}`])
    let n = r.root
    for (let i = 1; i <= MAX_DEPTH; i++) n = n.children[0]!
    expect([n.id, n.component, n.children.length]).toEqual([`n${MAX_DEPTH}`, `Unknown`, 0])
    let nested: NestedNode = { id: `leaf`, component: `Text`, props: { text: `x` } }
    for (let i = 0; i < 100; i++) nested = { id: `b${i}`, component: `Box`, children: [nested] }
    expect(issues(reduceNested(nested, { catalogId: CORE_CATALOG_ID }))).toEqual([`b${99 - MAX_DEPTH}: ${LIMIT_ISSUES.depth}`])
  })

  test(`past maxComponents the rest is dropped`, () => {
    const n = MAX_COMPONENTS + 50
    const flat: FlatComponent[] = [{ id: `root`, component: `Stack`, children: Array.from({ length: n }, (_, i) => `c${i}`) }, ...Array.from({ length: n }, (_, i) => ({ id: `c${i}`, component: `Text`, text: `x` }))]
    const r = reduceSurface(flat, { catalogId: CORE_CATALOG_ID })
    expect(issues(r)).toEqual([`c${MAX_COMPONENTS - 1}: ${LIMIT_ISSUES.components}`])
    expect(r.root.children.length).toBe(MAX_COMPONENTS - 1)
  })

  test(`a template renders at most maxTemplateItems items`, () => {
    const items = Array.from({ length: MAX_TEMPLATE_ITEMS + 5 }, (_, i) => i)
    expect(templateInstances({ items }, { component: `row`, path: `/items` }).length).toBe(MAX_TEMPLATE_ITEMS)
  })

  test(`maxTemplateItems counts per SURFACE, depth-first, a windowed List aside`, () => {
    const node = (id: string, component: string, children: UiNode[] = [], template?: UiNode[`template`]): UiNode => ({ id, component, props: {}, children, ...(template ? { template } : {}) })
    const cell = node(`cell`, `Box`)
    const row = node(`row`, `Box`, [], { component: `cell`, path: `cells` })
    const templates: Record<string, UiNode> = { row, cell }
    const half = MAX_TEMPLATE_ITEMS / 2
    const data = { rows: [{ cells: Array(half - 1).fill(0) }, { cells: Array(half).fill(0) }], big: Array(MAX_TEMPLATE_ITEMS * 2).fill(0) }
    const root = node(`root`, `Box`, [node(`list`, `List`, [], { component: `cell`, path: `/big` }), node(`rows`, `Box`, [], { component: `row`, path: `/rows` })])
    const b = templateBudget(root, data, (id) => templates[id])
    expect(b.allowed.has(templateSiteKey(`list`, ``))).toBe(false)
    expect(b.allowed.get(templateSiteKey(`rows`, ``))).toBe(2)
    expect(b.allowed.get(templateSiteKey(`row`, `/rows/0`))).toBe(half - 1)
    // 2 rows + (half - 1) cells spent: the second row gets what is left.
    expect(b.allowed.get(templateSiteKey(`row`, `/rows/1`))).toBe(half - 1)
    expect(b.exceeded).toBe(`cell`)
    expect(templateBudget(root, { rows: [] }, (id) => templates[id]).exceeded).toBeNull()
  })

  test(`template instances count against maxComponents NODES: past it the rest is not built, ONE issue`, () => {
    const node = (id: string, component: string, children: UiNode[] = [], template?: UiNode[`template`]): UiNode => ({ id, component, props: {}, children, ...(template ? { template } : {}) })
    // 3 nodes per item; 2 static nodes + the later site's holder.
    const row = node(`row`, `Box`, [node(`row-a`, `Text`), node(`row-b`, `Text`)])
    const templates: Record<string, UiNode> = { row }
    const root = node(`root`, `Box`, [node(`rows`, `Box`, [], { component: `row`, path: `/rows` }), node(`more`, `Box`, [], { component: `row`, path: `/more` })])
    const b = templateBudget(root, { rows: Array(7000).fill(0), more: [0] }, (id) => templates[id])
    const fits = Math.floor((MAX_COMPONENTS - 3) / 3)
    expect(b.allowed.get(templateSiteKey(`rows`, ``))).toBe(fits)
    expect(b.allowed.get(templateSiteKey(`more`, ``))).toBe(0)
    expect(b.componentsExceeded).toBe(`row`)
    expect(b.exceeded).toBeNull()
    expect(3 + fits * 3).toBeLessThanOrEqual(MAX_COMPONENTS)
    const ok = templateBudget(root, { rows: Array(10).fill(0), more: [0] }, (id) => templates[id])
    expect([ok.componentsExceeded, ok.allowed.get(templateSiteKey(`more`, ``))]).toEqual([null, 1])
  })

  test(`data pointers refuse gaps, huge indices, non-indices and long paths (iteratively)`, () => {
    const data = { a: [1, 2, 3] }
    expect(writePointer(data, `/a/4000000000`, 0)).toEqual({ data, error: `data: index 4000000000 is past the end of the array (3 items)` })
    expect(writePointer(data, `/a/4`, 0).error).toBe(POINTER_ISSUES.pastEnd(`4`, 3))
    expect(writePointer(data, `/a/x`, 0).error).toBe(`data: "x" is not an array index`)
    let d: unknown = data
    for (const [p, v] of [[`/a/3`, 4], [`/a/-`, 5], [`/a/5/b`, 6]] as const) d = writePointer(d, p, v).data
    expect(d).toEqual({ a: [1, 2, 3, 4, 5, { b: 6 }] })
    expect(data).toEqual({ a: [1, 2, 3] })
    expect(writePointer({}, `/x`.repeat(MAX_POINTER_SEGMENTS + 1), 1).error).toBe(`data: pointer has more than ${MAX_POINTER_SEGMENTS} segments`)
    expect(writePointer({}, `/${`x`.repeat(MAX_POINTER_BYTES)}`, 1).error).toBe(`data: pointer longer than ${MAX_POINTER_BYTES} bytes`)
    expect(writePointer({}, `/a`.repeat(20_000), 1).error).toBeDefined()
    const deepest = writePointer({}, `/k`.repeat(MAX_POINTER_SEGMENTS), 1)
    expect(deepest.error).toBeUndefined()
    // Own keys only: `__proto__` is data, never the prototype.
    const proto = writePointer({}, `/__proto__/polluted`, true).data as Record<string, unknown>
    expect(Object.getPrototypeOf(proto)).toBe(Object.prototype)
    expect(({} as Record<string, unknown>).polluted).toBeUndefined()
    expect(Object.keys(proto)).toEqual([`__proto__`])
    // `set` reports the refusal and writes nothing.
    const out = runAction({ functionCall: { call: `set`, args: { path: `/a/9`, value: 1 } } }, data)
    expect(out).toEqual({ data, error: POINTER_ISSUES.pastEnd(`9`, 3) })
  })

  test(`the store refuses a bad write; the router refuses an oversized message`, () => {
    const store = new SurfaceStore(`s`, CORE_CATALOG_ID, () => [])
    expect(store.setData(``, { a: [] })).toBeUndefined()
    expect(store.setData(`/a/7`, 1)).toBe(POINTER_ISSUES.pastEnd(`7`, 0))
    expect(store.data).toEqual({ a: [] })
    const router = new HostRouter()
    router.route({ version: `v0.9`, createSurface: { surfaceId: `s`, catalogId: CORE_CATALOG_ID } })
    const big = { version: `v0.9`, updateComponents: { surfaceId: `s`, components: [{ id: `root`, component: `Text`, text: `x`.repeat(MAX_MESSAGE_BYTES) }] } }
    expect(router.route(big)).toEqual([{ op: `send`, message: { version: `v0.9`, error: { code: `INVALID_MESSAGE`, surfaceId: `s`, message: LIMIT_ISSUES.messageBytes } } }])
    // Multi-byte text counts in UTF-8 bytes.
    const wide = { version: `v0.9`, updateComponents: { surfaceId: `s`, components: [{ id: `root`, component: `Text`, text: `é`.repeat(MAX_MESSAGE_BYTES / 2) }] } }
    expect(router.route(wide)[0]!.op).toBe(`send`)
    expect(router.route({ version: `v0.9`, updateComponents: { surfaceId: `s`, components: [{ id: `root`, component: `Text`, text: `ok` }] } })[0]!.op).toBe(`components`)
  })

  test(`2,000 streamed updateComponents cost linear`, () => {
    const store = new SurfaceStore(`s`, CORE_CATALOG_ID, () => [])
    const ids = Array.from({ length: 2000 }, (_, i) => `c${i}`)
    timed(1500, () => {
      store.setComponents([{ id: `root`, component: `List`, children: ids }])
      for (let i = 0; i < 2000; i++) store.setComponents([{ id: `c${i}`, component: `Text`, text: `Row ${i}` }])
      expect(store.issues).toEqual([])
    })
    store.setComponents([{ id: `c5`, component: `Text`, text: `five` }])
    expect(store.components.length).toBe(2001)
    expect(store.root!.children.length).toBe(2000)
  })
})
