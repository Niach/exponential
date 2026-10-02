import { describe, expect, it } from "vitest"
import {
  badgeChip,
  badgeKind,
  badgeShape,
  batchPartners,
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
  // EXP-876: a batch run names itself off these.
  title: over.title ?? id.toUpperCase(),
  createdAt: over.createdAt ?? `2026-09-10T10:00:00Z`,
})

const session = (
  id: string,
  parentSessionId: string | null = null,
  issueId: string | null = null,
  over: Partial<{
    actionName: string | null
    batchIssueIds: string[] | null
    branch: string | null
  }> = {}
) => ({
  id,
  parentSessionId,
  startedAt: `2026-09-10T10:00:00Z`,
  issueId,
  actionName: over.actionName ?? null,
  batchIssueIds: over.batchIssueIds ?? null,
  branch: over.branch ?? null,
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

  // EXP-876: the pill and its sheet are the surface built to name work that
  // spans several issues: and a batch RUN, which spans them, resolved
  // nothing at all before this (it links no issue and stamps no pr_url).
  // Mirrored ×4.
  it(`reports a batch badge for a batch run before its pr`, () => {
    const one = issue(`one`, { prUrl: null })
    const two = issue(`two`, { prUrl: null })
    const run = session(`run`, null, null, { batchIssueIds: [`one`, `two`] })
    const graph = prGraph({ session: run, issues: [one, two] })
    expect(badgeKind(graph)).toBe(`batch`)
    // The composer's order, so the sheet reads like the row that named it.
    expect(graph.batch?.issues.map((row) => row.id)).toEqual([`one`, `two`])
    // No pull request yet, so no stack: a batch of two is not a stack of two.
    expect(graph.stack).toEqual([])
  })

  it(`reports a batch run's pull request once it opens`, () => {
    // The GROUPED entry wins the moment the PR exists: it carries the branch
    // and the base the stack chains on, so a stacked batch still reads
    // `stack+batch` from its run.
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
    const run = session(`run`, null, null, {
      batchIssueIds: [`one`, `two`],
      branch: `exp/batch-abcd1234`,
    })
    const graph = prGraph({
      session: run,
      issues: [lower, one, two],
    })
    expect(badgeKind(graph)).toBe(`stack+batch`)
    expect(graph.entry?.key).toBe(url)
    expect(graph.batch?.issues).toHaveLength(2)
  })

  it(`reports nothing for a batch run whose issues are unknown`, () => {
    // No stored ids and no PR: the row reads "Batch run" and wears no pill,
    // rather than a pill that could say nothing.
    const run = session(`run`)
    const graph = prGraph({ session: run, issues: [] })
    expect(badgeKind(graph)).toBeNull()
    expect(graph.entry).toBeNull()
    // And an ACTION run is never a batch, whatever else it carries.
    const action = session(`action`, null, null, {
      actionName: `Chat`,
      batchIssueIds: [`one`],
    })
    expect(
      prGraph({ session: action, issues: [issue(`one`)] })
        .batch
    ).toBeNull()
  })

  it(`reports nothing for a lone pr`, () => {
    const lone = issue(`lone`)
    const graph = prGraph({ issue: lone, issues: [lone] })
    expect(badgeKind(graph)).toBeNull()
  })

  // SLOP-16 r5: a run family alone earns NO badge on any face; a PR relation
  // still does from the run.
  it(`shapes no badge for a run family alone`, () => {
    const sessions = [session(`child`, `root`), session(`root`)]
    const family = prGraph({ session: sessions[0], issues: [] })
    expect(badgeShape(family)).toBeNull()
    expect(badgeChip(family)).toBeNull()
    const batchA = issue(`bata`, { prUrl: `https://github.com/acme/app/pull/9` })
    const batchB = issue(`batb`, { prUrl: `https://github.com/acme/app/pull/9` })
    const batched = prGraph({
      session: sessions[0],
      issue: batchA,
      issues: [batchA, batchB],
    })
    expect(badgeShape(batched)).toBe(`batch`)
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
    // A run subject reads its issue's blockers.
    const run = session(`run`, null, `me`)
    expect(
      badgeShape(prGraph({ session: run, issues: [me, b1], relations }))
    ).toBe(`blocked`)
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
    // A batch run with no issue lists every issue it covers.
    const one = issue(`one`, { prUrl: null })
    const two = issue(`two`, { prUrl: null })
    const run = session(`run`, null, null, { batchIssueIds: [`one`, `two`] })
    const byRun = prGraph({ session: run, issues: [one, two] })
    expect(batchPartners(byRun).map((row) => row.id)).toEqual([`one`, `two`])
    expect(overlaySections(byRun)).toEqual([`batch`])
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
