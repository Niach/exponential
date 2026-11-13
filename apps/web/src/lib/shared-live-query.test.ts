// #PERF-7: identical on-demand reads share ONE live query (one subset
// request), and a cleaned-up one is rebuilt.
import { describe, expect, it } from "vitest"
import { createCollection, localOnlyCollectionOptions } from "@tanstack/react-db"
import { sharedLiveQuery } from "./shared-live-query"

const items = createCollection(
  localOnlyCollectionOptions<{ id: string; n: number }>({
    id: `perf7-items`,
    getKey: (item) => item.id,
    initialData: [{ id: `a`, n: 1 }, { id: `b`, n: 2 }],
  })
)

describe(`sharedLiveQuery`, () => {
  it(`hands every caller of a key the same live query`, () => {
    const first = sharedLiveQuery(`k`, (q) => q.from({ items }))
    const second = sharedLiveQuery(`k`, (q) => q.from({ items }))
    expect(second).toBe(first)
    expect(sharedLiveQuery(`other`, (q) => q.from({ items }))).not.toBe(first)
  })

  it(`rebuilds a key whose live query was cleaned up`, async () => {
    const first = sharedLiveQuery(`gone`, (q) => q.from({ items }))
    await first.cleanup()
    const again = sharedLiveQuery(`gone`, (q) => q.from({ items }))
    expect(again).not.toBe(first)
    await again.toArrayWhenReady()
    expect(again.size).toBe(2)
  })
})
