// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/upstream.json" with { type: "json" }
import {
  missingUpstreams,
  parseUpstreamSource,
  UPSTREAM_BINDING,
  upstreamBanners,
  upstreamBindingValue,
  upstreamCacheKey,
  upstreamCallable,
  upstreamDeclareIssues,
  upstreamReadError,
  upstreamStep,
  type UpstreamEvent,
  type UpstreamState,
} from "./upstream"

test(`every state has a label`, () => {
  expect(Object.keys(fixture.labels).sort()).toEqual([...fixture.states].sort())
})

describe(`upstreamStep`, () => {
  for (const entry of fixture.flowCases) {
    test(entry.name, () => {
      const state = (entry.events as UpstreamEvent[]).reduce(upstreamStep, `disconnected` as UpstreamState)
      expect(state).toBe(entry.expected as UpstreamState)
    })
  }
})

describe(`upstreamCallable`, () => {
  for (const entry of fixture.callableCases) {
    test(entry.state, () => expect(upstreamCallable(entry.state as UpstreamState)).toBe(entry.expected))
  }
})

describe(`missingUpstreams`, () => {
  for (const entry of fixture.missingCases) {
    test(entry.name, () => {
      expect(missingUpstreams(entry.declared, entry.states as Record<string, UpstreamState>, entry.servers)).toEqual(entry.expected)
    })
  }
})

describe(`VAPP-230: upstream read bindings`, () => {
  const b = fixture.binding
  test(`the constants`, () => {
    expect(UPSTREAM_BINDING.cacheSeconds).toBe(60)
    expect(UPSTREAM_BINDING.scheme).toBe(`upstream`)
  })
  for (const c of b.parseCases) test(`parse ${c.source}`, () => expect(parseUpstreamSource(c.source)).toEqual(c.expected as never))
  for (const c of b.declareCases) test(`declare: ${c.name}`, () => expect(upstreamDeclareIssues(c.sources, c.upstreams)).toEqual(c.expected))
  for (const c of b.readCases)
    test(`read: ${c.name}`, () => expect(upstreamReadError({ server: `crm`, tool: c.tool }, c.annotations as never, c.sources)).toBe(c.expected as string | null))
  for (const c of b.cacheKeyCases) test(`cache key ${c.expected.split(`\n`)[3]}`, () => expect(upstreamCacheKey(c.member, c.serverId, c.tool, c.args)).toBe(c.expected))
  for (const c of b.valueCases) test(`value: ${c.name}`, () => expect(upstreamBindingValue(c.result as never)).toEqual(c.expected as never))
  for (const c of b.bannerCases) test(`banner: ${c.name}`, () => expect(upstreamBanners(c.answers as never)).toEqual(c.expected))
})
