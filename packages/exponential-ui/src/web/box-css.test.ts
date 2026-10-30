// VAPP-152: the node sheet per node, the `<style>` batches a streamed
// update appends to, and the structural sharing between reduced trees.
import { describe, expect, it } from "bun:test"
import { CORE_CATALOG_ID, reduceNested, type UiNode } from "@exponential_app/eui"
import { AUTHOR_SEP, compileNodeSheet, nodeSheetBatches, type NodeSheetBatch } from "./box-css"
import { share } from "./share"

const node = (id: string, style?: Record<string, unknown>, children: UiNode[] = []): UiNode => ({ id, component: `Box`, props: {}, children, ...(style ? { style } : {}) }) as UiNode
const card = (i: number) => node(`m${i}`, { padding: 12, borderRadius: 8 }, [node(`m${i}-body`, { gap: 4 })])
const tree = (cards: number) => node(`root`, { padding: 16 }, [node(`plain`), ...Array.from({ length: cards }, (_, i) => card(i))])

describe(`compileNodeSheet`, () => {
  it(`keeps one chunk per styled node in walk order; css is their concatenation`, () => {
    const sheet = compileNodeSheet([tree(2)], `s`)
    expect([...sheet.chunks.keys()]).toEqual([`root`, `m0`, `m0-body`, `m1`, `m1-body`])
    expect(sheet.css).toBe(`@layer xui-node{${[...sheet.chunks.values()].join(``)}}`)
    expect(sheet.chunks.get(`m0`)).toContain(`.xui-s-s .xui-n-m0{`)
  })
  it(`reuses a node object's rules (same scope and theme)`, () => {
    const t = tree(1)
    const a = compileNodeSheet([t], `s`)
    const b = compileNodeSheet([t], `s`)
    expect(b.chunks.get(`m0`)).toBe(a.chunks.get(`m0`)!)
    expect(compileNodeSheet([t], `other`).chunks.get(`m0`)).toContain(`.xui-s-other .xui-n-m0{`)
  })
})

describe(`nodeSheetBatches`, () => {
  const step = (prev: readonly NodeSheetBatch[], cards: number) => nodeSheetBatches(prev, compileNodeSheet([tree(cards)], `s`).chunks)
  it(`a first sheet is one batch, equal to the whole sheet`, () => {
    const b = step([], 3)
    expect(b).toHaveLength(1)
    expect(b[0].css).toBe(compileNodeSheet([tree(3)], `s`).css)
  })
  it(`unchanged rules keep their batch OBJECT; new nodes land in one new batch`, () => {
    const first = step([], 8)
    const second = step(first, 9)
    expect(second).toHaveLength(2)
    expect(second[0]).toBe(first[0])
    expect([...second[1].chunks.keys()]).toEqual([`m8`, `m8-body`])
    expect(step(second, 9)[0]).toBe(second[0])
    expect(step(second, 9)[1]).toBe(second[1])
  })
  it(`merges a batch into the one before it once it is as large (log n batches)`, () => {
    let b = step([], 0)
    const counts: number[] = []
    for (let i = 1; i <= 64; i++) {
      b = step(b, i)
      counts.push(b.length)
      for (let k = 1; k < b.length; k++) expect(b[k].chunks.size).toBeLessThan(b[k - 1].chunks.size)
    }
    expect(Math.max(...counts)).toBeLessThanOrEqual(8)
    // Every node's rules are in exactly one batch.
    const all = b.flatMap((x) => [...x.chunks.keys()])
    expect(new Set(all).size).toBe(all.length)
    expect(all.length).toBe(1 + 64 * 2)
  })
  it(`a changed or removed node rewrites only its own batch`, () => {
    const first = step([], 8)
    const second = step(first, 9)
    const restyled = node(`root`, { padding: 16 }, [node(`plain`), ...Array.from({ length: 8 }, (_, i) => card(i)), node(`m8`, { padding: 2 }, [node(`m8-body`, { gap: 4 })])])
    const third = nodeSheetBatches(second, compileNodeSheet([restyled], `s`).chunks)
    expect(third[0]).toBe(second[0])
    expect(third[1].chunks.get(`m8`)).toContain(`padding:2px`)
    const gone = nodeSheetBatches(third, compileNodeSheet([tree(8)], `s`).chunks)
    expect(gone).toHaveLength(1)
    expect(gone[0]).toBe(second[0])
  })
})

describe(`share`, () => {
  it(`hands every unchanged node its old object, matched by id`, () => {
    const prev = tree(3)
    const next = share(prev, tree(4))
    expect(next).not.toBe(prev)
    expect(next.children[1]).toBe(prev.children[1])
    expect(next.children[3]).toBe(prev.children[3])
    expect(next.children[4]).toEqual(card(3))
    expect(share(prev, structuredClone(prev))).toBe(prev)
  })
})

describe(`VAPP-163: authored keys of a macro node`, () => {
  const heading = () =>
    reduceNested({ id: `h`, component: `Heading`, props: { text: `Hi`, level: `h1` }, style: { color: `$color.primaryForeground`, fontSize: 40, ":hover": { color: `#ff0000` } } }, { catalogId: CORE_CATALOG_ID }).root
  it(`emits them again in the xui-author layer, after xui-node`, () => {
    const sheet = compileNodeSheet([heading()], `s`)
    expect(sheet.chunks.get(`h`)).toContain(AUTHOR_SEP)
    const at = sheet.css.indexOf(`@layer xui-author{`)
    expect(at).toBeGreaterThan(sheet.css.indexOf(`@layer xui-node{`))
    const author = sheet.css.slice(at)
    expect(author).toContain(`.xui-s-s .xui-n-h{`)
    expect(author).toContain(`font-size:40px`)
    expect(author).toContain(`color:var(--xui-color-primaryForeground)`)
    expect(author).toContain(`:hover`)
    expect(sheet.css).not.toContain(AUTHOR_SEP)
  })
  it(`a dynamic authored value reads the same variable as the node layer`, () => {
    const root = reduceNested({ id: `b`, component: `Badge`, props: { text: `x` }, style: { padding: 2, width: { path: `/w` } } }, { catalogId: CORE_CATALOG_ID }).root
    const css = compileNodeSheet([root], `s`).css
    const author = css.slice(css.indexOf(`@layer xui-author{`))
    expect(author).toMatch(/width:var\(--xd-0\)/)
    expect(css.slice(0, css.indexOf(`@layer xui-author{`))).toMatch(/width:var\(--xd-0\)/)
  })
  it(`batches keep the two layers apart`, () => {
    const b = nodeSheetBatches([], compileNodeSheet([node(`root`, { padding: 1 }, [heading()])], `s`).chunks)
    expect(b[0].css.startsWith(`@layer xui-node{.xui-s-s .xui-n-root{padding:1px}`)).toBe(true)
    expect(b[0].css).toContain(`}@layer xui-author{`)
  })
})
