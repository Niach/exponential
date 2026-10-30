import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/text-clusters.json" with { type: "json" }
import { textClusters, trackedClusters } from "./text-clusters"

describe(`YO-30 text clusters`, () => {
  test(`fixtures/text-clusters.json replays`, () => {
    for (const c of fixture.cases) {
      expect(textClusters(c.text), c.name).toEqual(c.clusters)
      expect(trackedClusters(c.text), c.name).toBe(c.tracked)
      for (const k of c.attach) expect([...c.clusters[k]!].length, `${c.name}: cluster ${k} carries a mark`).toBeGreaterThan(1)
    }
  })
})
