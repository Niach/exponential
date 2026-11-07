// Round 9 (docs/round-9-contract.md): the web half of §5.1 (accessibility live
// + hidden), §9.1 (the Markdown inline profile) and §1.5 (the bare surface
// goes through the core's `SurfaceStore`).

import { act, render, renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { A2UI_VERSION, CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import { ExponentialSurface } from "./surface"
import { useSurface } from "./use-surface"

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root!

describe(`accessibility live + hidden`, () => {
  const attrs = (accessibility: Record<string, unknown>, data: Record<string, unknown> = {}, props: Record<string, unknown> = { text: `t` }, component = `Text`) => {
    const { container } = render(<ExponentialSurface root={tree({ id: `n`, component, props, accessibility } as NestedNode)} data={data} theme="neutral" />)
    return container.querySelector(`[data-xui-id="n"]`)!
  }
  it(`live maps to aria-live, on any component`, () => {
    expect(attrs({ live: `polite` }).getAttribute(`aria-live`)).toBe(`polite`)
    expect(attrs({ live: `assertive` }, {}, { direction: `column` }, `Stack`).getAttribute(`aria-live`)).toBe(`assertive`)
  })
  it(`accessibility.live wins over a Text's own live`, () => {
    expect(attrs({ live: `assertive` }, {}, { text: `t`, live: `polite` }).getAttribute(`aria-live`)).toBe(`assertive`)
    expect(attrs({ live: `off` }, {}, { text: `t`, live: `polite` }).getAttribute(`aria-live`)).toBe(`off`)
    expect(attrs({ label: `x` }, {}, { text: `t`, live: `polite` }).getAttribute(`aria-live`)).toBe(`polite`)
  })
  it(`hidden is a DynamicBoolean mapped to aria-hidden`, () => {
    expect(attrs({ hidden: true }).getAttribute(`aria-hidden`)).toBe(`true`)
    expect(attrs({ hidden: { "@path": `/h` } }, { h: `TRUE` }).getAttribute(`aria-hidden`)).toBe(`true`)
    expect(attrs({ hidden: { "@path": `/h` } }, { h: false }).hasAttribute(`aria-hidden`)).toBe(false)
  })
})

describe(`Markdown inline profile`, () => {
  const paint = (props: Record<string, unknown>, host = {}) => render(<ExponentialSurface root={tree({ id: `m`, component: `Markdown`, props } as NestedNode)} data={{}} theme="neutral" host={host} />).container
  it(`a paragraph answers to Text/root of its variant`, () => {
    const p = paint({ text: `plain`, profile: `inline`, variant: `caption` }).querySelector(`p`)!
    expect(p.className).toContain(`xui-Text-root`)
    expect(p.getAttribute(`data-r-variant`)).toBe(`caption`)
  })
  it(`a heading answers to Heading/root, h5 and h6 as h4`, () => {
    const h = paint({ text: `## Two`, profile: `inline` }).querySelector(`h2`)!
    expect(h.className).toContain(`xui-Heading-root`)
    expect(h.getAttribute(`data-m-level`)).toBe(`h2`)
    expect(paint({ text: `###### Six`, profile: `inline` }).querySelector(`h6`)!.getAttribute(`data-m-level`)).toBe(`h4`)
  })
  it(`the document profile stays`, () => {
    const c = paint({ text: `plain\n\n# One` })
    expect(c.querySelector(`p`)!.className).not.toContain(`xui-Text-root`)
    expect(c.querySelector(`h1`)!.className).not.toContain(`xui-Heading-root`)
  })
  it(`an inline Markdown skips the host's own painter`, () => {
    const Own = () => <i data-own="" />
    expect(paint({ text: `x` }, { Markdown: Own }).querySelector(`[data-own]`)).not.toBeNull()
    expect(paint({ text: `x`, profile: `inline` }, { Markdown: Own }).querySelector(`[data-own]`)).toBeNull()
  })
})

describe(`useSurface goes through the core`, () => {
  const create = (extra = {}) => ({ version: A2UI_VERSION, createSurface: { surfaceId: `main`, catalogId: CORE_CATALOG_ID, components: [{ id: `root`, component: `Text`, text: { "@path": `/t` } }], ...extra } })
  it(`refuses an unsupported version and keeps the options' data`, () => {
    const { result } = renderHook(() => useSurface({ data: { t: `seed` } }))
    act(() => result.current.apply({ ...create(), version: `2.0` }))
    expect(result.current.root).toBeNull()
    act(() => result.current.apply(create()))
    expect(result.current.root?.id).toBe(`root`)
    expect(result.current.data).toEqual({ t: `seed` })
  })
  it(`deletes, re-creates, and resets to the start`, () => {
    const { result } = renderHook(() => useSurface({ initial: { id: `root`, component: `Text`, props: { text: `start` } } }))
    expect(result.current.root?.props.text).toBe(`start`)
    act(() => result.current.apply(create({ dataModel: { t: `a` } })))
    expect(result.current.data).toEqual({ t: `a` })
    act(() => result.current.apply({ version: A2UI_VERSION, deleteSurface: { surfaceId: `main` } }))
    expect(result.current.deleted).toBe(true)
    expect(result.current.root).toBeNull()
    act(() => result.current.reset())
    expect(result.current.deleted).toBe(false)
    expect(result.current.root?.props.text).toBe(`start`)
  })
  it(`a data write keeps the tree`, () => {
    const { result } = renderHook(() => useSurface())
    act(() => result.current.apply(create()))
    const root = result.current.root
    act(() => result.current.setData(`/t`, `b`))
    expect(result.current.root).toBe(root)
    expect(result.current.data).toEqual({ t: `b` })
  })
})
