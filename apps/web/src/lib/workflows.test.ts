import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  foldCompoundNodes,
  nodeEdges,
  workflowIntegrationBranch,
} from "@/lib/workflows"

// EXP-981 — the pure half of the workflow graph: how picked issues fold into
// nodes and how `blocks` relations become edges between them.
const parents = (...pairs: Array<[child: string, parent: string]>) => new Map(pairs)
const open = (...ids: string[]) => new Set(ids)

describe(`foldCompoundNodes`, () => {
  it(`keeps unrelated picks as plain nodes`, () => {
    expect(foldCompoundNodes([`a`, `b`], parents(), open())).toEqual([
      { issueId: `a`, memberIssueIds: [] },
      { issueId: `b`, memberIssueIds: [] },
    ])
  })

  it(`folds a picked sub-issue into its picked parent`, () => {
    expect(
      foldCompoundNodes([`k`, `p`], parents([`k`, `p`]), open(`k`, `p`))
    ).toEqual([{ issueId: `p`, memberIssueIds: [`k`] }])
  })

  it(`folds into the TOP picked ancestor through an unpicked middle one`, () => {
    expect(
      foldCompoundNodes(
        [`g`, `k`],
        parents([`k`, `m`], [`m`, `g`]),
        open(`g`, `m`, `k`)
      )
    ).toEqual([{ issueId: `g`, memberIssueIds: [`k`, `m`] }])
  })

  it(`adopts open sub-issues nobody picked, never finished ones`, () => {
    expect(
      foldCompoundNodes(
        [`p`],
        parents([`new`, `p`], [`done`, `p`]),
        open(`p`, `new`)
      )
    ).toEqual([{ issueId: `p`, memberIssueIds: [`new`] }])
  })

  it(`leaves a sub-issue whose parent is not in the workflow a plain node`, () => {
    expect(foldCompoundNodes([`k`], parents([`k`, `elsewhere`]), open(`k`))).toEqual([
      { issueId: `k`, memberIssueIds: [] },
    ])
  })

  it(`survives a parent cycle`, () => {
    const folded = foldCompoundNodes(
      [`a`, `b`],
      parents([`a`, `b`], [`b`, `a`]),
      open(`a`, `b`)
    )
    expect(folded.flatMap((n) => [n.issueId, ...n.memberIssueIds]).sort()).toEqual([
      `a`,
      `b`,
    ])
  })
})

describe(`nodeEdges`, () => {
  const nodes = [
    { id: `n1`, issueId: `p`, memberIssueIds: [`k1`, `k2`] },
    { id: `n2`, issueId: `x`, memberIssueIds: [] },
  ]

  it(`lifts a relation between members to an edge between their nodes`, () => {
    expect(nodeEdges(nodes, [{ issueId: `k2`, relatedIssueId: `x` }])).toEqual([
      [`n1`, `n2`],
    ])
  })

  it(`drops relations inside one node and ones leaving the workflow`, () => {
    expect(
      nodeEdges(nodes, [
        { issueId: `k1`, relatedIssueId: `k2` },
        { issueId: `x`, relatedIssueId: `outside` },
      ])
    ).toEqual([])
  })
})

describe(`workflowIntegrationBranch`, () => {
  it(`is exp/wf-<id8>`, () => {
    expect(workflowIntegrationBranch(`8cef8d22-dafc-4fb7-8e4f-01483ab0b5d0`)).toBe(
      `exp/wf-8cef8d22`
    )
  })
})
