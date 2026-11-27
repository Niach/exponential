// #EXP-2204: fixtures/job-graph.json replays (eui-core, Swift and Kotlin
// replay the same file); the geometry equals catalog/style.json jobGraph.

import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/job-graph.json"
import styleJson from "../catalog/style.json" with { type: "json" }
import { JOB_GRAPH, jobGraphLayout, jobGraphUpstream } from "./job-graph"

const plain = (v: unknown) => JSON.parse(JSON.stringify(v))

describe(`fixtures/job-graph.json`, () => {
  test(`the geometry is style.json jobGraph`, () => {
    const { $comment: _, ...geometry } = styleJson.jobGraph
    expect(plain(JOB_GRAPH)).toEqual(geometry)
    expect(fixture.geometry).toEqual(geometry)
  })

  test(`every case: graph, list, upstream sets`, () => {
    expect(fixture.cases.length).toBeGreaterThan(5)
    for (const c of fixture.cases) {
      const graph = jobGraphLayout(c.jobs, false)
      expect(plain(graph), c.name).toEqual(c.graph)
      expect(plain(jobGraphLayout(c.jobs, true)), `${c.name} (list)`).toEqual(c.list)
      for (const u of c.upstream) expect(jobGraphUpstream(graph, u.id), `${c.name} ↑${u.id}`).toEqual(u.ids)
    }
  })
})

describe(`jobGraphLayout`, () => {
  test(`a matrix is one node: aggregate status, passed/total, needs through the collapse`, () => {
    const layout = jobGraphLayout([
      { id: `build`, status: `passed` },
      { id: `t1`, matrix: `test`, status: `passed`, needs: [`build`] },
      { id: `t2`, matrix: `test`, status: `failed`, needs: [`build`] },
      { id: `ship`, needs: [`t2`] },
    ], false)
    expect(layout.nodes.map((n) => [n.id, n.status, n.count, n.needs, n.column])).toEqual([
      [`build`, `passed`, null, [], 0],
      [`test`, `failed`, { passed: 1, total: 2 }, [`build`], 1],
      [`ship`, `queued`, null, [`test`], 2],
    ])
    expect(layout.edges[0]).toEqual({ from: `build`, to: `test`, start: [200, 20], c1: [224, 20], c2: [224, 20], end: [248, 20] })
  })

  test(`compact = full-width rows, no edges, no stages`, () => {
    const list = jobGraphLayout([{ id: `a`, stage: `x` }, { id: `b`, stage: `y` }], true)
    expect([list.mode, list.width, list.height, list.stages, list.edges]).toEqual([`list`, 0, 80, [], []])
    expect(list.nodes.map((n) => [n.x, n.y, n.width, n.height])).toEqual([[0, 0, 0, 40], [0, 40, 0, 40]])
  })
})
