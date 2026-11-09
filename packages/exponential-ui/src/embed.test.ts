// #EDE-65: fixtures/embed.json against the TS reference (src/embed.ts); the
// painters replay `render` + `events` (React, Solid, gpui, SwiftUI, Compose).
import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/embed.json" with { type: "json" }
import { CORE_CATALOG_ID, coreCatalog } from "./catalog"
import { bindTree, resolveNodeProps } from "./dynamic"
import { EMBED_ERROR_ROOT, EMBED_EVENT, embedCard, embedMount, embedPayload, embedSurfaceId } from "./embed"
import { reduceNested } from "./reducer"
import { resolveNodeProps as resolveWebProps } from "./web/data"
import type { NestedNode } from "./types"

describe(`embed.json parse`, () => {
  for (const c of fixture.parse) {
    test(c.name, () => {
      const card = embedCard(c.surface)
      expect(card !== undefined).toBe(c.valid)
      if (card) {
        expect(card.data).toEqual((c as { data: Record<string, unknown> }).data)
        expect(card.root).toBe((c.surface as { root: NestedNode }).root as NestedNode)
      }
    })
  }
  test(`errorRoot is the reference's and reduces clean`, () => {
    expect(fixture.errorRoot).toEqual(EMBED_ERROR_ROOT as typeof fixture.errorRoot)
    expect(reduceNested(EMBED_ERROR_ROOT, { catalogId: CORE_CATALOG_ID }).issues).toEqual([])
    expect(embedMount(`nope`)).toEqual({ card: { root: EMBED_ERROR_ROOT, data: {} }, valid: false })
    expect(embedMount({ root: { id: `t`, component: `Text`, props: { text: `x` } } }).valid).toBe(true)
  })
})

describe(`embed.json hosts`, () => {
  const hosts = [...fixture.render, ...fixture.events]
  for (const c of hosts) {
    test(`${c.name}: the host reduces with no issues`, () => {
      expect(reduceNested(c.root as NestedNode, { catalogId: CORE_CATALOG_ID }).issues).toEqual([])
    })
  }
  test(`the catalog: one data prop, one event`, () => {
    const def = coreCatalog.components.Embed as { kind: string; props: Record<string, { type: string; shape?: string; bindable?: boolean }>; events: string[] }
    expect(def.kind).toBe(`native`)
    expect(Object.keys(def.props)).toEqual([`surface`])
    expect(def.props.surface).toMatchObject({ type: `object`, bindable: true })
    expect(def.props.surface.shape).toBeUndefined()
    expect(def.events).toEqual([EMBED_EVENT])
  })
})

describe(`the sandbox in the core`, () => {
  const inner = { root: { id: `leak`, component: `Text`, props: { text: { "@path": `/secret` } } }, data: {} }
  const data = { secret: `Host secret`, card: inner }
  test(`a literal surface is copied verbatim: no inner binding reads the host`, () => {
    expect(resolveNodeProps(coreCatalog.components.Embed as never, { surface: inner }, data)).toEqual({ surface: inner })
    expect(resolveWebProps(`Embed`, { surface: inner }, { data })).toEqual({ surface: inner })
  })
  test(`a top-level binding resolves against the host`, () => {
    expect(resolveWebProps(`Embed`, { surface: { "@path": `/card` } }, { data })).toEqual({ surface: inner })
  })
  test(`the bind pass leaves the nested tree alone`, () => {
    const root = reduceNested({ id: `e`, component: `Embed`, props: { surface: inner } }, { catalogId: CORE_CATALOG_ID }).root
    const bound = bindTree(root, data) as { props: Record<string, unknown> }
    expect(bound.props.surface).toEqual(inner)
  })
  test(`payload + surface id`, () => {
    expect(embedPayload({ name: `reply`, context: { text: `Yes` } })).toEqual({ name: `reply`, context: { text: `Yes` } })
    expect(embedPayload({ name: `go` })).toEqual({ name: `go`, context: {} })
    expect(embedSurfaceId(`chat`, `card.b`)).toBe(`chat/card.b`)
  })
})
