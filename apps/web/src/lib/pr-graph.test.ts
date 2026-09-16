import { describe, expect, it } from "vitest"
import { badgeKind, badgeLabel, prGraph } from "./pr-graph"

// EXP-897 Part 4 — the badge model. Every `it` name here is mirrored by iOS
// PrGraphTests, Android PrGraphTest and the desktop `pr_graph` tests.

const issue = (
  id: string,
  over: Partial<{
    branch: string | null
    prBaseBranch: string | null
    prUrl: string | null
    status: string
  }> = {}
) => ({
  id,
  identifier: id.toUpperCase(),
  status: over.status ?? `in_progress`,
  branch: over.branch ?? `exp/${id.toUpperCase()}`,
  prBaseBranch: over.prBaseBranch ?? null,
  prUrl: over.prUrl ?? `https://github.com/acme/app/pull/${id}`,
})

const session = (
  id: string,
  parentSessionId: string | null = null,
  issueId: string | null = null
) => ({
  id,
  parentSessionId,
  startedAt: `2026-09-10T10:00:00Z`,
  issueId,
})

describe(`prGraph`, () => {
  it(`reports a stack badge for a stacked pr`, () => {
    const lower = issue(`lower`)
    const upper = issue(`upper`, { prBaseBranch: `exp/LOWER` })
    const graph = prGraph({
      issue: upper,
      issues: [lower, upper],
      sessions: [],
    })
    expect(badgeKind(graph)).toBe(`stack`)
    expect(graph.stack.map((row) => row.entry.issue.id)).toEqual([
      `lower`,
      `upper`,
    ])
    expect(graph.stack.map((row) => row.depth)).toEqual([0, 1])
    expect(badgeLabel(graph)).toBe(`2 of 2`)
    expect(graph.batch).toBeNull()
  })

  it(`reports a batch badge for a batch pr`, () => {
    const url = `https://github.com/acme/app/pull/9`
    const one = issue(`one`, { prUrl: url, branch: `exp/batch-abcd1234` })
    const two = issue(`two`, { prUrl: url, branch: `exp/batch-abcd1234` })
    const graph = prGraph({ issue: one, issues: [one, two], sessions: [] })
    expect(badgeKind(graph)).toBe(`batch`)
    expect(graph.batch?.issues.map((row) => row.id)).toEqual([`one`, `two`])
    expect(badgeLabel(graph)).toBe(`2 issues`)
    expect(graph.stack).toEqual([])
  })

  it(`reports both for a batch inside a stack`, () => {
    const url = `https://github.com/acme/app/pull/9`
    const lower = issue(`lower`)
    const one = issue(`one`, {
      prUrl: url,
      branch: `exp/batch-abcd1234`,
      prBaseBranch: `exp/LOWER`,
    })
    const two = issue(`two`, {
      prUrl: url,
      branch: `exp/batch-abcd1234`,
      prBaseBranch: `exp/LOWER`,
    })
    const graph = prGraph({
      issue: two,
      issues: [lower, one, two],
      sessions: [],
    })
    expect(badgeKind(graph)).toBe(`stack+batch`)
    // The batch is ONE stack member, never two rows.
    expect(graph.stack.map((row) => row.entry.key)).toEqual([
      lower.prUrl,
      url,
    ])
    expect(graph.batch?.issues).toHaveLength(2)
    expect(badgeLabel(graph)).toBe(`2 of 2`)
  })

  it(`reports nothing for a lone pr`, () => {
    const lone = issue(`lone`)
    const graph = prGraph({ issue: lone, issues: [lone], sessions: [] })
    expect(badgeKind(graph)).toBeNull()
    expect(badgeLabel(graph)).toBeNull()
  })

  it(`nests the subject run's whole tree, from its root`, () => {
    const sessions = [
      session(`child`, `root`),
      session(`root`),
      session(`stranger`),
    ]
    const graph = prGraph({
      session: sessions[0],
      issues: [],
      sessions,
    })
    expect(graph.tree.map((row) => `${row.session.id}@${row.depth}`)).toEqual([
      `root@0`,
      `child@1`,
    ])
  })

  it(`lists the subject issue's open blockers`, () => {
    const me = issue(`me`)
    const blocker = issue(`blocker`)
    const closed = issue(`closed`, { status: `done` })
    const graph = prGraph({
      issue: me,
      issues: [me, blocker, closed],
      sessions: [],
      relations: [
        { type: `blocks`, issueId: `blocker`, relatedIssueId: `me` },
        { type: `blocks`, issueId: `closed`, relatedIssueId: `me` },
      ],
    })
    expect(graph.blockedBy.map((row) => row.id)).toEqual([`blocker`])
  })
})
