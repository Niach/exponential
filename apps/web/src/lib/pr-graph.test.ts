import { describe, expect, it } from "vitest"
import { badgeChip, badgeKind, badgeShape, prGraph } from "./pr-graph"

// EXP-897 Part 4 — the badge model. Every `it` name here is mirrored by iOS
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
      issues: [lower, upper],
      sessions: [],
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
    const graph = prGraph({ issue: one, issues: [one, two], sessions: [] })
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
  })

  // EXP-876: the pill and its sheet are the surface built to name work that
  // spans several issues — and a batch RUN, which spans them, resolved
  // nothing at all before this (it links no issue and stamps no pr_url).
  // Mirrored ×4.
  it(`reports a batch badge for a batch run before its pr`, () => {
    const one = issue(`one`, { prUrl: null })
    const two = issue(`two`, { prUrl: null })
    const run = session(`run`, null, null, { batchIssueIds: [`one`, `two`] })
    const graph = prGraph({ session: run, issues: [one, two], sessions: [run] })
    expect(badgeKind(graph)).toBe(`batch`)
    // The composer's order, so the sheet reads like the row that named it.
    expect(graph.batch?.issues.map((row) => row.id)).toEqual([`one`, `two`])
    // No pull request yet, so no stack — a batch of two is not a stack of two.
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
      sessions: [run],
    })
    expect(badgeKind(graph)).toBe(`stack+batch`)
    expect(graph.entry?.key).toBe(url)
    expect(graph.batch?.issues).toHaveLength(2)
  })

  it(`reports nothing for a batch run whose issues are unknown`, () => {
    // No stored ids and no PR: the row reads "Batch run" and wears no pill,
    // rather than a pill that could say nothing.
    const run = session(`run`)
    const graph = prGraph({ session: run, issues: [], sessions: [run] })
    expect(badgeKind(graph)).toBeNull()
    expect(graph.entry).toBeNull()
    // And an ACTION run is never a batch, whatever else it carries.
    const action = session(`action`, null, null, {
      actionName: `Chat`,
      batchIssueIds: [`one`],
    })
    expect(
      prGraph({ session: action, issues: [issue(`one`)], sessions: [action] })
        .batch
    ).toBeNull()
  })

  it(`reports nothing for a lone pr`, () => {
    const lone = issue(`lone`)
    const graph = prGraph({ issue: lone, issues: [lone], sessions: [] })
    expect(badgeKind(graph)).toBeNull()
  })

  // EXP-1079: the desktop's `is_visible` + `badge_glyphs` — a run with a
  // family earns the pill on the Run face alone, and a PR relation always
  // wins over it.
  it(`shapes a runs badge for a run with a family, on the run face only`, () => {
    const sessions = [session(`child`, `root`), session(`root`)]
    const family = prGraph({ session: sessions[0], issues: [], sessions })
    expect(badgeShape(family, `run`)).toBe(`runs`)
    expect(badgeShape(family, `changes`)).toBeNull()
    expect(badgeShape(family, `issue`)).toBeNull()
    const alone = prGraph({
      session: session(`alone`),
      issues: [],
      sessions: [session(`alone`)],
    })
    expect(badgeShape(alone, `run`)).toBeNull()
    const batchA = issue(`bata`, { prUrl: `https://github.com/acme/app/pull/9` })
    const batchB = issue(`batb`, { prUrl: `https://github.com/acme/app/pull/9` })
    const batched = prGraph({
      session: sessions[0],
      issue: batchA,
      issues: [batchA, batchB],
      sessions,
    })
    expect(badgeShape(batched, `run`)).toBe(`batch`)
  })

  // EXP-1058: the header's stacked chip — front issue + how many behind.
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
    const both = prGraph({ issue: two, issues: [lower, one, two], sessions: [] })
    expect(badgeChip(both, `issue`)?.issue?.id).toBe(`one`)
    expect(badgeChip(both, `issue`)?.count).toBe(2)
    // A plain batch: the others of the batch.
    const batch = prGraph({ issue: one, issues: [one, { ...two, prBaseBranch: null }], sessions: [] })
    expect(badgeChip(batch, `changes`)?.count).toBe(1)
    // A run family with no issue: no front issue, the other runs behind.
    const sessions = [session(`child`, `root`), session(`root`)]
    const family = prGraph({ session: sessions[0], issues: [], sessions })
    expect(badgeChip(family, `run`)).toEqual({ issue: null, count: 1 })
    // No badge = no chip.
    expect(badgeChip(family, `issue`)).toBeNull()
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
