import { describe, expect, it } from "vitest"
import {
  badgeChip,
  badgeKind,
  badgeShape,
  batchPartners,
  issuePrRows,
  overlaySections,
  PR_GRAPH_OVERLAY_COPY,
  prGraph,
  RELATED_WORK_TITLE,
  stackOthers,
} from "./pr-graph"

// EXP-897 Part 4: the badge model. Every `it` name here is mirrored by iOS
// PrGraphTests, Android PrGraphTest and the desktop `pr_graph` tests.

const issue = (
  id: string,
  over: Partial<{
    branch: string | null
    prBaseBranch: string | null
    prUrl: string | null
    status: string
    title: string
    createdAt: string
  }> = {}
) => ({
  id,
  identifier: id.toUpperCase(),
  status: over.status ?? `in_progress`,
  branch: over.branch ?? `exp/${id.toUpperCase()}`,
  prBaseBranch: over.prBaseBranch ?? null,
  prUrl: over.prUrl ?? `https://github.com/acme/app/pull/${id}`,
  title: over.title ?? id.toUpperCase(),
  createdAt: over.createdAt ?? `2026-09-10T10:00:00Z`,
})

describe(`prGraph`, () => {
  it(`reports a stack badge for a stacked pr`, () => {
    const lower = issue(`lower`)
    const upper = issue(`upper`, { prBaseBranch: `exp/LOWER` })
    const graph = prGraph({
      issue: upper,
      issues: [lower, upper]
    })
    expect(badgeKind(graph)).toBe(`stack`)
    expect(graph.stack.map((row) => row.entry.issue.id)).toEqual([
      `lower`,
      `upper`,
    ])
    expect(graph.stack.map((row) => row.depth)).toEqual([0, 1])
    expect(graph.batch).toBeNull()
  })

  it(`reports a batch badge for a batch pr`, () => {
    const url = `https://github.com/acme/app/pull/9`
    const one = issue(`one`, { prUrl: url, branch: `exp/batch-abcd1234` })
    const two = issue(`two`, { prUrl: url, branch: `exp/batch-abcd1234` })
    const graph = prGraph({ issue: one, issues: [one, two] })
    expect(badgeKind(graph)).toBe(`batch`)
    expect(graph.batch?.issues.map((row) => row.id)).toEqual([`one`, `two`])
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
      issues: [lower, one, two]
    })
    expect(badgeKind(graph)).toBe(`stack+batch`)
    // The batch is ONE stack member, never two rows.
    expect(graph.stack.map((row) => row.entry.key)).toEqual([
      lower.prUrl,
      url,
    ])
    expect(graph.batch?.issues).toHaveLength(2)
  })

  it(`reports nothing for a lone pr`, () => {
    const lone = issue(`lone`)
    const graph = prGraph({ issue: lone, issues: [lone] })
    expect(badgeKind(graph)).toBeNull()
  })

  // EXP-1097: open blockers alone earn the badge, behind the PR relations;
  // the front issue is the first open blocker.
  it(`shapes a blocked badge for an issue with open blockers`, () => {
    const me = issue(`me`)
    const b1 = issue(`b1`)
    const b2 = issue(`b2`)
    const closed = issue(`closed`, { status: `done` })
    const relations = [
      { type: `blocks`, issueId: `b2`, relatedIssueId: `me` },
      { type: `blocks`, issueId: `b1`, relatedIssueId: `me` },
      { type: `blocks`, issueId: `closed`, relatedIssueId: `me` },
    ]
    const graph = prGraph({ issue: me, issues: [me, b1, b2, closed], relations })
    expect(badgeShape(graph)).toBe(`blocked`)
    expect(badgeChip(graph)?.issue?.id).toBe(`b1`)
    expect(badgeChip(graph)?.count).toBe(1)
    // Only closed blockers: no badge.
    const done = prGraph({ issue: me, issues: [me, closed], relations })
    expect(badgeShape(done)).toBeNull()
    // A batch wins over the blockers.
    const batchA = issue(`me`, { prUrl: `https://github.com/acme/app/pull/9` })
    const batchB = issue(`batb`, { prUrl: `https://github.com/acme/app/pull/9` })
    expect(
      badgeShape(
        prGraph({ issue: batchA, issues: [batchA, batchB, b1], relations })
      )
    ).toBe(`batch`)
  })

  // EXP-1058: the badge's count: front issue + how many behind.
  // Mirrored ×4 (`badge_chip_*` desktop, PrGraphTests, PrGraphTest).
  it(`names the representative issue and the count on the stacked chip`, () => {
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
    // A batch inside a stack: the subject PR's representative, every other
    // issue on the stack behind it.
    const both = prGraph({ issue: two, issues: [lower, one, two] })
    expect(badgeChip(both)?.issue?.id).toBe(`one`)
    expect(badgeChip(both)?.count).toBe(2)
    // A plain batch: the others of the batch.
    const batch = prGraph({ issue: one, issues: [one, { ...two, prBaseBranch: null }] })
    expect(badgeChip(batch)?.count).toBe(1)
    // No badge = no chip.
    const lone = issue(`lone`)
    expect(badgeChip(prGraph({ issue: lone, issues: [lone] }))).toBeNull()
  })

  // SLOP-16 r5: ONE body on every face: Blocked by · Same pull request ·
  // Pull request stack, each only when it has rows.
  it(`lists the related work bands in one order`, () => {
    const url = `https://github.com/acme/app/pull/9`
    const lower = issue(`lower`)
    const me = issue(`me`, { prUrl: url, branch: `exp/batch-x`, prBaseBranch: `exp/LOWER` })
    const partner = issue(`partner`, { prUrl: url, branch: `exp/batch-x`, prBaseBranch: `exp/LOWER` })
    const blocker = issue(`blocker`)
    const graph = prGraph({
      issue: me,
      issues: [lower, me, partner, blocker],
      relations: [{ type: `blocks`, issueId: `blocker`, relatedIssueId: `me` }],
    })
    expect(overlaySections(graph)).toEqual([`blocked`, `batch`, `stack`])
    expect(batchPartners(graph).map((row) => row.id)).toEqual([`partner`])
    // The OTHER pull requests of the stack, bottom-up, the subject's own out.
    expect(stackOthers(graph).map((entry) => entry.issue.id)).toEqual([`lower`])
    // A lone pull request: nothing.
    const lone = issue(`lone`)
    expect(overlaySections(prGraph({ issue: lone, issues: [lone] }))).toEqual([])
  })

  it(`lists the subject issue's open blockers`, () => {
    const me = issue(`me`)
    const blocker = issue(`blocker`)
    const closed = issue(`closed`, { status: `done` })
    const graph = prGraph({
      issue: me,
      issues: [me, blocker, closed],
      relations: [
        { type: `blocks`, issueId: `blocker`, relatedIssueId: `me` },
        { type: `blocks`, issueId: `closed`, relatedIssueId: `me` },
      ],
    })
    expect(graph.blockedBy.map((row) => row.id)).toEqual([`blocker`])
  })
})

// EXP-1293: PR state comes off `pull_requests` + `pull_request_issues`.
describe(`issuePrRows`, () => {
  const pr = (
    id: string,
    number: number,
    over: Partial<{ state: string; head: string; base: string; basePullRequestId: string | null }> = {}
  ) => ({
    id,
    number,
    url: `https://github.com/acme/app/pull/${number}`,
    head: over.head ?? `exp/${id}`,
    base: over.base ?? `master`,
    basePullRequestId: over.basePullRequestId ?? null,
    state: over.state ?? `open`,
  })
  const plain = (id: string) => ({ id, identifier: id.toUpperCase(), status: `in_progress` })

  it(`joins each issue to its open pull request, else the newest`, () => {
    const rows = issuePrRows(
      [plain(`a`), plain(`b`), plain(`c`)],
      [pr(`p1`, 1, { state: `closed` }), pr(`p2`, 2), pr(`p3`, 3, { state: `merged` })],
      [
        { pullRequestId: `p1`, issueId: `a` },
        { pullRequestId: `p2`, issueId: `a` },
        { pullRequestId: `p1`, issueId: `b` },
        { pullRequestId: `p3`, issueId: `b` },
      ]
    )
    expect(rows.map((row) => row.pullRequestId)).toEqual([`p2`, `p3`, null])
    expect(rows[0]).toMatchObject({ prNumber: 2, prState: `open`, branch: `exp/p2`, prBaseBranch: `master` })
    expect(rows[2]).toMatchObject({ prUrl: null, prState: null, branch: null })
  })

  it(`stacks on the resolved base pull request and groups a batch by pull request`, () => {
    const rows = issuePrRows(
      [plain(`lower`), plain(`one`), plain(`two`)],
      [
        pr(`p1`, 1, { head: `exp/LOWER` }),
        // The base branch says nothing (renamed); the resolved id decides.
        pr(`p2`, 2, { head: `exp/batch`, base: `gone`, basePullRequestId: `p1` }),
      ],
      [
        { pullRequestId: `p1`, issueId: `lower` },
        { pullRequestId: `p2`, issueId: `one` },
        { pullRequestId: `p2`, issueId: `two` },
      ]
    )
    const graph = prGraph({ issue: rows[2], issues: rows })
    expect(badgeKind(graph)).toBe(`stack+batch`)
    expect(graph.stack.map((row) => row.entry.key)).toEqual([`p1`, `p2`])
    expect(graph.batch?.issues.map((row) => row.id)).toEqual([`one`, `two`])
  })
})

// SLOP-16 r5: THE "Related work" view's strings, byte-identical ×4.
describe(`related work copy`, () => {
  it(`pins the related work strings`, () => {
    expect(RELATED_WORK_TITLE).toBe(`Related work`)
    expect(PR_GRAPH_OVERLAY_COPY).toEqual({
      title: `Related work`,
      blocked: `Blocked by`,
      batch: `Same pull request`,
      stack: `Pull request stack`,
      empty: `Nothing else is linked to this issue.`,
    })
  })
})
