// #EXP-2204 (React's natives/ci.test.tsx): LogLines (fold, search `n of m`
// + Enter/Shift+Enter + reveal, line anchors + the shift range, Follow off
// on a person's scroll-up, the window, two-way writes and events), JobGraph
// (hover = the upstream edges + highlight, select, the list below md) and
// the CodeBlock additions (anchors, blame, the window), on Solid.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import type { NestedNode } from "@exponential_app/eui"
import type { SurfaceActionEvent } from "@exponential_app/eui/web"
import { createSurface, type SurfaceState } from "../create-surface"
import { ExponentialSurface } from "../surface"

afterEach(cleanup)

const act = (f: () => void) => f()

function mount(tree: NestedNode, data: Record<string, unknown> = {}, width?: number) {
  const events: { event: string; payload?: Record<string, unknown> }[] = []
  let state!: SurfaceState
  const r = render(() => {
    state = createSurface({ surfaceId: `ci`, initial: tree, data })
    return <ExponentialSurface surface={state} host={{ onAction: (e: SurfaceActionEvent) => void events.push({ event: e.event, payload: e.payload }) }} theme="neutral" id="ci" width={width} />
  })
  return { ...r, events, data: () => state.data as Record<string, unknown>, q: (sel: string) => r.container.querySelector(sel) as HTMLElement, qa: (sel: string) => [...r.container.querySelectorAll(sel)] as HTMLElement[] }
}

const ON = { select: { event: { name: `anchor` } }, toggle: { event: { name: `fold` } }, search: { event: { name: `find` } }, follow: { event: { name: `follow` } }, timestamps: { event: { name: `times` } } }
const LOG = [`##[group]Install`, `$ bun install`, `Resolved 1204 packages`, `##[endgroup]`, `##[group]Test`, `\u001b[31m(fail)\u001b[0m store > get`, `##[error]1 test failed`, `##[endgroup]`, `2026-11-27T09:14:01.1Z done store`]
const logTree = (props: Record<string, unknown> = {}) => ({ id: `log`, component: `LogView`, props: { lines: LOG, sections: [{ status: `passed`, duration: `12s` }, { status: `failed`, duration: `41s` }], ...props }, on: ON }) as unknown as NestedNode
const lineNumbers = (m: ReturnType<typeof mount>) => m.qa(`.xui-LogLines-line`).map((l) => Number(l.dataset.xuiId!.split(`.`).pop()))

describe(`LogLines`, () => {
  it(`opens the failed step by default; a press folds a step, fires toggle, writes a bound open`, () => {
    const m = mount(logTree({ open: { "@path": `/open` } }), { open: [1] })
    expect(lineNumbers(m)).toEqual([3, 4, 5])
    const install = m.qa(`.xui-LogLines-section`)[0]!
    expect(install.getAttribute(`aria-expanded`)).toBe(`false`)
    expect(install.dataset.rStatus).toBe(`passed`)
    fireEvent.click(install)
    expect(lineNumbers(m)).toEqual([1, 2, 3, 4, 5])
    expect(m.data().open).toEqual([0, 1])
    expect(m.events.at(-1)).toEqual({ event: `toggle`, payload: { section: 0, open: true } })
    const unbound = mount(logTree())
    expect(lineNumbers(unbound)).toEqual([3, 4, 5])
  })
  it(`the status table: glyph, spin only while running, the recipe status`, () => {
    const m = mount({ id: `log`, component: `LogView`, props: { lines: [`##[group]A`, `x`, `##[endgroup]`, `##[group]B`, `y`] } } as unknown as NestedNode)
    const icons = m.qa(`.xui-LogLines-sectionIcon`)
    expect(icons.map((i) => i.dataset.rStatus)).toEqual([`passed`, `running`])
    expect(icons.map((i) => i.querySelector(`svg`)!.getAttribute(`data-icon`))).toEqual([`ci-passed`, `ci-running`])
    expect(icons.map((i) => i.hasAttribute(`data-xui-ci-spin`))).toEqual([false, true])
  })
  it(`marks every match, counts n of m, steps with Enter / Shift+Enter (wrapping) and reveals the match's step`, () => {
    const m = mount(logTree({ query: { "@path": `/q` } }), { q: `` })
    expect(m.q(`.xui-LogLines-count`)).toBeNull()
    expect(m.q(`.xui-LogLines-next`).hasAttribute(`disabled`)).toBe(true)
    const input = m.q(`.xui-LogLines-search`) as HTMLInputElement
    fireEvent.input(input, { target: { value: `PACKAGES` } })
    expect(m.data().q).toBe(`PACKAGES`)
    expect(m.events.at(-1)).toEqual({ event: `search`, payload: { query: `PACKAGES` } })
    // The match sits in the folded Install step: typing revealed it.
    expect(lineNumbers(m)).toContain(2)
    expect(m.q(`.xui-LogLines-count`).textContent).toBe(`1 of 1`)
    fireEvent.input(input, { target: { value: `store` } })
    expect(m.q(`.xui-LogLines-count`).textContent).toBe(`1 of 2`)
    const current = () => m.q(`.xui-LogLines-match[data-xs~="selected"]`).closest(`.xui-LogLines-line`)!.getAttribute(`data-xui-id`)
    expect(m.qa(`.xui-LogLines-match`).map((e) => e.textContent)).toEqual([`store`, `store`])
    expect(current()).toBe(`log.log.line.3`)
    fireEvent.keyDown(input, { key: `Enter` })
    expect(m.q(`.xui-LogLines-count`).textContent).toBe(`2 of 2`)
    expect(current()).toBe(`log.log.line.5`)
    fireEvent.keyDown(input, { key: `Enter` })
    expect(current()).toBe(`log.log.line.3`)
    fireEvent.keyDown(input, { key: `Enter`, shiftKey: true })
    expect(current()).toBe(`log.log.line.5`)
  })
  it(`a match across ansi tokens keeps the tokens' kinds; a ##[error] line colours every token`, () => {
    const m = mount(logTree({ query: `l) st` }))
    const match = m.q(`.xui-LogLines-match`)
    expect([...match.querySelectorAll(`.xui-LogLines-token`)].map((t) => [t.textContent, t.getAttribute(`data-r-kind`)])).toEqual([[`l)`, `error`], [` st`, `plain`]])
    const err = m.q(`[data-xui-id="log.log.line.4"]`)
    expect(err.dataset.rKind).toBe(`error`)
    expect([...err.querySelectorAll(`.xui-LogLines-token`)].every((t) => t.getAttribute(`data-r-kind`) === `error`)).toBe(true)
  })
  it(`a number press selects its line, shift extends the range, writes back + fires select, the lines take selected`, () => {
    const m = mount(logTree({ open: [0, 1], selection: { "@path": `/sel` }, follow: { "@path": `/follow` } }), { sel: ``, follow: true })
    const gutter = (n: number) => m.q(`[data-xui-id="log.log.gutter.${n}"]`)
    expect(gutter(2).getAttribute(`aria-label`)).toBe(`Line 2`)
    expect(gutter(2).getAttribute(`href`)).toBe(`#L2`)
    fireEvent.click(gutter(2))
    expect(m.data().sel).toBe(`L2`)
    expect(m.events).toContainEqual({ event: `select`, payload: { anchor: `L2`, start: 2, end: 2 } })
    // Selecting stops the follow (written back, `follow` fired).
    expect(m.data().follow).toBe(false)
    expect(m.events).toContainEqual({ event: `follow`, payload: { follow: false } })
    fireEvent.click(gutter(4), { shiftKey: true })
    expect(m.data().sel).toBe(`L2-L4`)
    expect(m.qa(`.xui-LogLines-line[data-xs~="selected"]`).map((l) => l.dataset.xuiId)).toEqual([`log.log.line.2`, `log.log.line.3`, `log.log.line.4`])
  })
  it(`a selection from outside reveals its step and turns Follow off`, () => {
    const m = mount(logTree({ open: { "@path": `/open` }, selection: { "@path": `/sel` } }), { open: [], sel: `L1` })
    expect(m.data().open).toEqual([0])
    expect(lineNumbers(m)).toContain(1)
    expect(m.q(`.xui-LogLines-follow`).getAttribute(`aria-pressed`)).toBe(`false`)
  })
  it(`Timestamps shows the time column (written back + event); Follow toggles`, () => {
    const m = mount(logTree({ timestamps: { "@path": `/t` }, open: [0, 1] }), { t: false })
    expect(m.q(`.xui-LogLines-time`)).toBeNull()
    fireEvent.click(m.q(`.xui-LogLines-timestamps`))
    expect(m.data().t).toBe(true)
    expect(m.events.at(-1)).toEqual({ event: `timestamps`, payload: { timestamps: true } })
    expect(m.q(`[data-xui-id="log.log.time.5"]`).textContent).toBe(`09:14:01`)
    expect(m.q(`.xui-LogLines-timestamps`).dataset.xs).toContain(`selected`)
    expect(m.q(`.xui-LogLines-follow`).getAttribute(`aria-pressed`)).toBe(`true`)
    fireEvent.click(m.q(`.xui-LogLines-follow`))
    expect(m.q(`.xui-LogLines-follow`).getAttribute(`aria-pressed`)).toBe(`false`)
  })
  it(`windows a long log (only the rows in view ± overscan) and a person's scroll-up stops Follow`, () => {
    const lines = Array.from({ length: 2000 }, (_, i) => `line ${i + 1}`)
    const m = mount({ id: `log`, component: `LogView`, props: { lines, maxLines: 10, follow: { "@path": `/f` } }, on: ON } as unknown as NestedNode, { f: true })
    const built = lineNumbers(m)
    expect(built.length).toBeLessThan(40)
    // Follow on: the window sits at the end.
    expect(built.at(-1)).toBe(2000)
    const body = m.q(`.xui-LogLines-body`)
    let top = body.scrollTop
    Object.defineProperty(body, `scrollHeight`, { configurable: true, get: () => 2000 * 22.75 + 16 })
    Object.defineProperty(body, `clientHeight`, { configurable: true, get: () => 10 * 22.75 + 16 })
    Object.defineProperty(body, `scrollTop`, { configurable: true, get: () => top, set: (v: number) => void (top = v) })
    act(() => {
      top = 2000 * 22.75 - 10 * 22.75
      fireEvent.scroll(body)
    })
    expect(m.data().f).toBe(true)
    act(() => {
      top = 1000
      fireEvent.scroll(body)
    })
    expect(m.data().f).toBe(false)
    expect(m.events).toContainEqual({ event: `follow`, payload: { follow: false } })
    const now = lineNumbers(m)
    expect(now[0]).toBeLessThan(1000 / 22.75)
    expect(now.at(-1)).toBeGreaterThan(1000 / 22.75)
  })
  it(`a11y: the log region, the polite log body, named toolbar controls`, () => {
    const m = mount(logTree())
    expect(m.q(`[data-xui-id="log"]`).getAttribute(`role`)).toBe(`region`)
    expect(m.q(`[data-xui-id="log"]`).getAttribute(`aria-label`)).toBe(`Log`)
    const body = m.q(`.xui-LogLines-body`)
    expect([body.getAttribute(`role`), body.getAttribute(`aria-live`)]).toEqual([`log`, `polite`])
    expect(m.q(`.xui-LogLines-search`).getAttribute(`aria-label`)).toBe(`Search log`)
    expect(m.q(`.xui-LogLines-prev`).getAttribute(`aria-label`)).toBe(`Previous match`)
  })
})

const JOBS = [
  { id: `checkout`, name: `checkout`, status: `passed`, duration: `4s` },
  { id: `build`, name: `build`, status: `passed`, needs: [`checkout`] },
  { id: `lint`, name: `lint`, status: `passed`, needs: [`checkout`] },
  { id: `t1`, name: `test 1`, matrix: `test`, status: `passed`, needs: [`build`] },
  { id: `t2`, name: `test 2`, matrix: `test`, status: `running`, needs: [`build`] },
  { id: `publish`, name: `publish`, status: `cancelled`, needs: [`test`, `lint`] },
]
const graph = { id: `g`, component: `JobGraph`, props: { jobs: JOBS, value: { "@path": `/job` } }, on: { select: { event: { name: `open` } } } } as unknown as NestedNode

describe(`JobGraph`, () => {
  it(`lays the nodes out absolutely; no connector until a node is hovered, then its upstream only`, () => {
    const m = mount(graph, { job: `` }, 1280)
    const nodes = m.qa(`.xui-JobGraph-node`)
    expect(nodes.map((n) => n.getAttribute(`aria-label`))).toEqual([`checkout, Passed`, `build, Passed`, `lint, Passed`, `test, Running, 1/2`, `publish, Cancelled`])
    expect(nodes[3]!.style.left).toBe(`496px`)
    expect(m.qa(`.xui-JobGraph-edge`)).toHaveLength(0)
    fireEvent.mouseEnter(nodes[3]!)
    expect(m.qa(`.xui-JobGraph-edge`)).toHaveLength(2)
    expect(m.qa(`.xui-JobGraph-node[data-xs~="highlight"]`).map((n) => n.getAttribute(`aria-label`)!.split(`,`)[0])).toEqual([`checkout`, `build`, `test`])
    fireEvent.mouseLeave(nodes[3]!)
    expect(m.qa(`.xui-JobGraph-edge`)).toHaveLength(0)
    fireEvent.focus(nodes[4]!)
    expect(m.qa(`.xui-JobGraph-edge`)).toHaveLength(5)
    expect(m.q(`.xui-JobGraph-edge`).getAttribute(`d`)).toMatch(/^M\d+ \d+C/)
  })
  it(`a press selects (bound value + select {id}); the matrix shows passed/total; the running glyph spins`, () => {
    const m = mount(graph, { job: `` }, 1280)
    const test = m.qa(`.xui-JobGraph-node`)[3]!
    expect(test.querySelector(`.xui-JobGraph-meta`)!.textContent).toBe(`1/2`)
    expect(test.querySelector(`[data-xui-ci-spin]`)).not.toBeNull()
    fireEvent.click(test)
    expect(m.data().job).toBe(`test`)
    expect(m.events.at(-1)).toEqual({ event: `select`, payload: { id: `test` } })
    expect(m.qa(`.xui-JobGraph-node`)[3]!.dataset.xs).toContain(`selected`)
    expect(m.q(`[data-xui-id="g"]`).getAttribute(`role`)).toBe(`group`)
  })
  it(`below md: a status list of rows in column order, pressable`, () => {
    const m = mount(graph, { job: `build` }, 390)
    expect(m.qa(`.xui-JobGraph-node`)).toHaveLength(0)
    const rows = m.qa(`.xui-JobGraph-row`)
    expect(rows.map((r) => r.querySelector(`.xui-JobGraph-name`)!.textContent)).toEqual([`checkout`, `build`, `lint`, `test`, `publish`])
    expect(rows[1]!.dataset.xs).toContain(`selected`)
    expect(rows.every((r) => r.querySelector(`.xui-JobGraph-chevron svg`))).toBe(true)
    fireEvent.click(rows[4]!)
    expect(m.data().job).toBe(`publish`)
  })
})

const CODE = Array.from({ length: 12 }, (_, i) => `const v${i + 1} = ${i + 1}`).join(`\n`)

describe(`CodeBlock anchors, blame, window`, () => {
  it(`anchors: numbers are links; a press selects, shift extends, writes back + select; selected lines`, () => {
    const m = mount({ id: `c`, component: `CodeBlock`, props: { code: CODE, language: `ts`, anchors: true, selection: { "@path": `/sel` } }, on: { select: { event: { name: `anchor` } } } } as unknown as NestedNode, { sel: `L3` })
    const n = (i: number) => m.q(`[data-xui-id="c.lineNumber.${i - 1}"]`)
    expect(n(3).tagName).toBe(`A`)
    expect(n(3).getAttribute(`aria-label`)).toBe(`Line 3`)
    expect(m.qa(`.xui-CodeBlock-line[data-xs~="selected"]`).map((l) => l.dataset.line)).toEqual([`3`])
    fireEvent.click(n(6), { shiftKey: true })
    expect(m.data().sel).toBe(`L3-L6`)
    expect(m.events.at(-1)).toEqual({ event: `select`, payload: { anchor: `L3-L6`, start: 3, end: 6 } })
    expect(m.qa(`.xui-CodeBlock-line[data-xs~="selected"]`).map((l) => l.dataset.line)).toEqual([`3`, `4`, `5`, `6`])
    fireEvent.click(n(2))
    expect(m.data().sel).toBe(`L2`)
  })
  it(`gutter blame: author + age on each block's first line, the hairline from the second block`, () => {
    const m = mount({ id: `c`, component: `CodeBlock`, props: { code: CODE, language: `ts`, gutter: `blame`, blame: [{ start: 5, author: `Grace`, age: `2d` }, { start: 1, author: `Ada`, age: `3w` }] } } as unknown as NestedNode)
    const blame = m.qa(`.xui-CodeBlock-blame`)
    expect(blame).toHaveLength(12)
    expect(blame.map((b) => b.textContent)).toEqual([`Ada3w`, ``, ``, ``, `Grace2d`, ``, ``, ``, ``, ``, ``, ``])
    expect(blame.map((b) => b.hasAttribute(`data-border`))).toEqual([false, false, false, false, true, false, false, false, false, false, false, false])
    // `gutter` alone shows no numbers (lineNumbers decides).
    expect(m.q(`.xui-CodeBlock-lineNumber`)).toBeNull()
  })
  it(`windows a self-scrolling block past the threshold (wrap off), not a growing one`, () => {
    const long = Array.from({ length: 400 }, (_, i) => `x${i}`).join(`\n`)
    const w = mount({ id: `c`, component: `CodeBlock`, props: { code: long, maxLines: 10 } } as unknown as NestedNode)
    expect(w.qa(`[data-line]`).length).toBeLessThan(30)
    expect(w.qa(`.xui-codeblock-gap`)).toHaveLength(1)
    cleanup()
    expect(mount({ id: `c`, component: `CodeBlock`, props: { code: long } } as unknown as NestedNode).qa(`[data-line]`)).toHaveLength(400)
    cleanup()
    expect(mount({ id: `c`, component: `CodeBlock`, props: { code: long, maxLines: 10, wrap: true } } as unknown as NestedNode).qa(`[data-line]`)).toHaveLength(400)
  })
})
