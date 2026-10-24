import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/app-sources.json" with { type: "json" }
import { sourceValue, type SourceStore } from "./app-sources"

describe(`app sources (VAPP-119)`, () => {
  const store = fixture.store as unknown as SourceStore
  test.each(fixture.cases)(`$source: $name`, (c) => {
    expect(sourceValue(c.source, c.params, store, { me: c.me })).toEqual(c.expected as never)
  })
  test(`the fixture names every source the reference derives`, () => {
    for (const name of fixture.sources) expect(sourceValue(name, {}, store, { me: null })).not.toBeUndefined()
    expect(sourceValue(`devices`, {}, store, { me: null })).toBeNull()
  })
})
