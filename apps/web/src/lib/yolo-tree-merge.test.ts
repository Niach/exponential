import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1146: yolo mode merges a follow-up PR tree once it is complete, root
// first. The planner and the executor are pure over injected deps; the fakes
// below model GitHub (a PR's base moves onto the default branch when the
// EXP-324 heal runs for its parent) and record every call in order.

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/integrations/github-app`, () => ({
  resolveRepoInstallationTokenInfo: vi.fn(),
}))
vi.mock(`@/lib/integrations/github-pr`, () => ({ getPullRequest: vi.fn() }))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  repoFromPrUrl: (prUrl: string) => {
    const match = prUrl.match(/github\.com\/([^/]+\/[^/]+)\/pull\/\d+/)
    return match ? match[1] : null
  },
  retargetChildrenOfMergedPr: vi.fn(),
}))
vi.mock(`@/lib/integrations/notifications`, () => ({ sendAgentMessage: vi.fn() }))
vi.mock(`@/lib/steer-child-messages`, () => ({
  loadSessionChain: vi.fn(),
  MAX_SESSION_CHAIN_DEPTH: 20,
}))
vi.mock(`@/lib/sessions/merged-own-pr`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  stampMergedOwnPr: vi.fn(),
  revertMergedOwnPr: vi.fn(),
}))
vi.mock(`@/lib/trpc/synthetic-context`, () => ({
  buildServerActorContext: vi.fn(),
}))

import {
  assembleYoloTree,
  executeYoloTree,
  maybeMergeYoloTree,
  planYoloTree,
  resetYoloTreeState,
  type YoloDeps,
  type YoloPull,
  type YoloRunRow,
  type YoloTree,
} from "@/lib/yolo-tree-merge"

const TEAM = `team-1`
const OWNER = `user-1`
const PR = (n: number) => `https://github.com/acme/app/pull/${n}`

let t0 = 1_000
function row(overrides: Partial<YoloRunRow> & { id: string }): YoloRunRow {
  t0 += 1
  return {
    parentSessionId: null,
    resumedFromId: null,
    userId: OWNER,
    status: `ended`,
    agentBusy: false,
    needsInput: false,
    issueId: null,
    batchIssueIds: null,
    actionName: null,
    branch: null,
    startedAt: new Date(t0),
    prUrl: null,
    prNumber: null,
    prState: null,
    workflowId: null,
    issueIdentifier: null,
    issuePrUrl: null,
    issuePrNumber: null,
    issuePrState: null,
    issueBranch: null,
    batchIssueId: null,
    batchIdentifier: null,
    batchPrUrl: null,
    batchPrNumber: null,
    batchPrState: null,
    ...overrides,
  }
}

/** An issue run whose PR (#n) lives on the issue row, like today's issue-scoped
 *  runs (SLOP-9 will stamp the session row too). */
function issueRun(
  id: string,
  n: number,
  overrides: Partial<YoloRunRow> = {}
): YoloRunRow {
  return row({
    id,
    issueId: `issue-${n}`,
    issueIdentifier: `EXP-${n}`,
    issuePrUrl: PR(n),
    issuePrNumber: n,
    issuePrState: `open`,
    issueBranch: `exp/EXP-${n}`,
    ...overrides,
  })
}

interface FakePull extends YoloPull {
  number: number
}

interface FakeState {
  pulls: Map<number, FakePull>
  events: string[]
  stamps: string[]
  reverts: string[]
  notifications: Array<{ title: string; body: string; issueId: string | null; ownerIds: string[] }>
  /** PR numbers whose merge the fake router refuses (`message`). */
  refuse: Map<number, Error>
  /** When false the heal does nothing (the EXP-324 bail-outs). */
  healWorks: boolean
  tree: YoloTree | null
  loads: number
  onMerge?: (n: number) => Promise<void>
}

function pull(n: number, headRef: string, baseRef: string, extra: Partial<FakePull> = {}): FakePull {
  return {
    number: n,
    state: `open`,
    merged: false,
    draft: false,
    headRef,
    baseRef,
    mergeable: true,
    mergeableState: `clean`,
    ...extra,
  }
}

function fakeDeps(state: FakeState): YoloDeps {
  const merge = async (n: number) => {
    state.events.push(`merge:#${n}`)
    const refusal = state.refuse.get(n)
    if (refusal) throw refusal
    state.pulls.get(n)!.merged = true
    state.pulls.get(n)!.state = `closed`
    await state.onMerge?.(n)
    return { merged: true }
  }
  return {
    loadTree: async () => {
      state.loads += 1
      return state.tree
    },
    getPull: async (_repo, n) => {
      state.events.push(`read:#${n}`)
      const found = state.pulls.get(n)
      if (!found) throw new Error(`GitHub returned 404 for #${n}`)
      return { ...found }
    },
    retarget: async ({ headBranch }) => {
      state.events.push(`retarget:${headBranch}`)
      if (!state.healWorks) return
      for (const p of state.pulls.values()) {
        if (p.baseRef === headBranch && p.state === `open`) p.baseRef = `master`
      }
    },
    mergeIssuePr: (run) => merge(run.prNumber!),
    mergeChorePr: (_run, _repo, n) => merge(n),
    stampOwnPr: async (id) => {
      state.stamps.push(id)
    },
    revertOwnPr: async (id) => {
      state.reverts.push(id)
    },
    notifyOwner: async (args) => {
      state.notifications.push(args)
    },
    sleep: async () => {},
    log: () => {},
  }
}

function freshState(): FakeState {
  return {
    pulls: new Map(),
    events: [],
    stamps: [],
    reverts: [],
    notifications: [],
    refuse: new Map(),
    healWorks: true,
    tree: null,
    loads: 0,
  }
}

/** root(#1) → c1(#2), c2(#3); c1 → gc(#4). Children based on their parent. */
function followUpTree(state: FakeState, overrides: Record<string, Partial<YoloRunRow>> = {}) {
  const rows = [
    issueRun(`root`, 1, { status: `in_review`, ...overrides.root }),
    issueRun(`c1`, 2, { parentSessionId: `root`, ...overrides.c1 }),
    issueRun(`c2`, 3, { parentSessionId: `root`, ...overrides.c2 }),
    issueRun(`gc`, 4, { parentSessionId: `c1`, ...overrides.gc }),
  ]
  state.pulls.set(1, pull(1, `exp/EXP-1`, `master`))
  state.pulls.set(2, pull(2, `exp/EXP-2`, `exp/EXP-1`))
  state.pulls.set(3, pull(3, `exp/EXP-3`, `exp/EXP-1`))
  state.pulls.set(4, pull(4, `exp/EXP-4`, `exp/EXP-2`))
  return assembleYoloTree(rows, [`root`], TEAM)!
}

beforeEach(() => {
  resetYoloTreeState()
})

describe(`assembleYoloTree`, () => {
  it(`drops superseded (resumed) rows and nests a child of the predecessor under the successor`, () => {
    const rows = [
      issueRun(`root-old`, 1, { status: `ended` }),
      issueRun(`root-new`, 1, { status: `in_review`, resumedFromId: `root-old` }),
      // The re-stamp missed this child: it still points at the old root.
      issueRun(`c1`, 2, { parentSessionId: `root-old` }),
      issueRun(`c2`, 3, { parentSessionId: `root-new` }),
    ]
    const tree = assembleYoloTree(rows, [`root-old`, `root-new`], TEAM)!
    expect(tree.rootSessionId).toBe(`root-new`)
    expect(tree.runs.map((run) => [run.sessionId, run.depth])).toEqual([
      [`root-new`, 0],
      [`c1`, 1],
      [`c2`, 1],
    ])
  })

  it(`resolves a run's PR: its own stamp, else its issue, else the batch issue on its branch`, () => {
    const rows = [
      row({
        id: `root`,
        status: `in_review`,
        prUrl: PR(7),
        prNumber: 7,
        prState: `open`,
        branch: `exp/chat-abc`,
        issueId: `issue-1`,
        issueIdentifier: `EXP-1`,
        issuePrUrl: PR(1),
        issuePrNumber: 1,
        issuePrState: `open`,
      }),
      issueRun(`c1`, 2, { parentSessionId: `root` }),
      row({
        id: `batch`,
        parentSessionId: `root`,
        batchIssueIds: [`issue-8`, `issue-9`],
        branch: `exp/batch-1a2b3c4d`,
        batchIssueId: `issue-8`,
        batchIdentifier: `EXP-8`,
        batchPrUrl: PR(8),
        batchPrNumber: 8,
        batchPrState: `open`,
      }),
      row({ id: `chat`, parentSessionId: `root`, status: `running` }),
    ]
    const tree = assembleYoloTree(rows, [`root`], TEAM)!
    const by = Object.fromEntries(tree.runs.map((run) => [run.sessionId, run]))
    expect(by.root).toMatchObject({ prNumber: 7, kind: `issue`, mergeIssueId: `issue-1` })
    expect(by.c1).toMatchObject({ prNumber: 2, headBranch: `exp/EXP-2` })
    expect(by.batch).toMatchObject({
      prNumber: 8,
      kind: `batch`,
      mergeIssueId: `issue-8`,
      name: `EXP-8 +1`,
      headBranch: `exp/batch-1a2b3c4d`,
    })
    expect(by.chat).toMatchObject({ kind: `chore`, prless: true, name: `Chat` })
  })
})

describe(`planYoloTree`, () => {
  it(`waits while the last run is still working, even with its PR open`, () => {
    const state = freshState()
    const tree = followUpTree(state, { gc: { status: `running`, agentBusy: true } })
    const plan = planYoloTree(tree)
    expect(plan.complete).toBe(false)
    expect(plan.blocking.map((run) => run.name)).toEqual([`EXP-4`])
  })

  it(`a live run without a PR blocks; an ended one without a PR is skipped`, () => {
    const state = freshState()
    const tree = followUpTree(state, {
      c1: { status: `running`, issuePrUrl: null, issuePrNumber: null, issuePrState: null },
      c2: { issuePrUrl: null, issuePrNumber: null, issuePrState: null },
    })
    const plan = planYoloTree(tree)
    expect(plan.blocking.map((run) => run.name)).toEqual([`EXP-2`])
    expect(plan.skippedNoPr.map((run) => run.name)).toEqual([`EXP-3`])
  })

  it(`a person-started root counts as done once idle with its PR open; a PR-less chat root never blocks`, () => {
    const state = freshState()
    const tree = followUpTree(state, { root: { status: `in_review`, agentBusy: false } })
    expect(planYoloTree(tree).complete).toBe(true)
    expect(planYoloTree(tree).order.map((run) => run.name)).toEqual([
      `EXP-1`,
      `EXP-2`,
      `EXP-3`,
      `EXP-4`,
    ])

    const chat = assembleYoloTree(
      [
        row({ id: `chat`, status: `running`, agentBusy: false }),
        issueRun(`c1`, 2, { parentSessionId: `chat` }),
      ],
      [`chat`],
      TEAM
    )!
    const plan = planYoloTree(chat)
    expect(plan.complete).toBe(true)
    expect(plan.skippedNoPr).toEqual([])
    expect(plan.order.map((run) => run.name)).toEqual([`EXP-2`])
  })
})

describe(`executeYoloTree`, () => {
  it(`merges root first, healing the children between merges, one commit per PR`, async () => {
    const state = freshState()
    const tree = followUpTree(state)
    const plan = planYoloTree(tree)
    expect(plan.complete).toBe(true)

    const result = await executeYoloTree(tree, plan, fakeDeps(state))

    expect(result.mergedNow).toBe(4)
    const merges = state.events.filter((e) => e.startsWith(`merge:`))
    expect(merges).toEqual([`merge:#1`, `merge:#2`, `merge:#3`, `merge:#4`])
    // The EXP-324 heal runs right after the root and before any child lands,
    // and after each child before its own children.
    const order = state.events.filter((e) => !e.startsWith(`read:`))
    expect(order.indexOf(`retarget:exp/EXP-1`)).toBeGreaterThan(order.indexOf(`merge:#1`))
    expect(order.indexOf(`retarget:exp/EXP-1`)).toBeLessThan(order.indexOf(`merge:#2`))
    expect(order.indexOf(`retarget:exp/EXP-2`)).toBeLessThan(order.indexOf(`merge:#4`))
    // Nobody is messaged about a clean tree.
    expect(state.notifications).toEqual([])
    // Ended children get no own-PR stamp; the live root does.
    expect(state.stamps).toEqual([`root`])
    expect(state.reverts).toEqual([])
  })

  it(`leaves a dirty grandchild open and names it to the owner`, async () => {
    const state = freshState()
    const tree = followUpTree(state)
    const plan = planYoloTree(tree)
    state.onMerge = async (n) => {
      // After its parent lands, the grandchild conflicts on the default branch.
      if (n === 2) state.pulls.get(4)!.mergeableState = `dirty`
    }

    const result = await executeYoloTree(tree, plan, fakeDeps(state))

    expect(result.mergedNow).toBe(3)
    expect(state.events.filter((e) => e.startsWith(`merge:`))).toEqual([
      `merge:#1`,
      `merge:#2`,
      `merge:#3`,
    ])
    expect(result.outcomes.get(`gc`)).toEqual({ kind: `dirty` })
    expect(state.pulls.get(4)!.state).toBe(`open`)
  })

  it(`a child refused as a conflict by the router is dirty; its own children wait; the message names both`, async () => {
    const state = freshState()
    const tree = followUpTree(state)
    const plan = planYoloTree(tree)
    state.refuse.set(2, Object.assign(new Error(`Pull Request has merge conflicts`), {}))

    const result = await executeYoloTree(tree, plan, fakeDeps(state))

    expect(result.outcomes.get(`c1`)).toEqual({ kind: `dirty` })
    expect(result.outcomes.get(`gc`)).toMatchObject({ kind: `waiting` })
    expect(result.outcomes.get(`c2`)).toEqual({ kind: `merged` })
    // The grandchild was never merged into the unmerged child's branch.
    expect(state.events).not.toContain(`merge:#4`)
    expect(state.pulls.get(4)!.baseRef).toBe(`exp/EXP-2`)
  })

  it(`never merges a child still based on a merged branch when the heal bailed`, async () => {
    const state = freshState()
    state.healWorks = false
    const tree = followUpTree(state)
    const plan = planYoloTree(tree)

    const result = await executeYoloTree(tree, plan, fakeDeps(state))

    expect(result.mergedNow).toBe(1)
    expect(result.outcomes.get(`c1`)).toMatchObject({ kind: `failed` })
    expect(result.outcomes.get(`c1`)!.detail).toContain(`still based on the merged branch exp/EXP-1`)
    expect(result.outcomes.get(`gc`)).toMatchObject({ kind: `waiting` })
  })

  it(`heals the children of a root that merged before they existed, then merges them`, async () => {
    const state = freshState()
    const tree = followUpTree(state, { root: { issuePrState: `merged` } })
    state.pulls.get(1)!.merged = true
    state.pulls.get(1)!.state = `closed`
    const plan = planYoloTree(tree)

    const result = await executeYoloTree(tree, plan, fakeDeps(state))

    expect(result.outcomes.get(`root`)).toEqual({ kind: `merged_before` })
    expect(state.events[0]).toBe(`retarget:exp/EXP-1`)
    expect(state.events.filter((e) => e.startsWith(`merge:`))).toEqual([
      `merge:#2`,
      `merge:#3`,
      `merge:#4`,
    ])
  })

  it(`stamps a live run before its merge and reverts when the merge fails`, async () => {
    const state = freshState()
    const tree = followUpTree(state, { root: { status: `in_review` } })
    const plan = planYoloTree(tree)
    state.refuse.set(1, new Error(`Required status check "ci" is expected.`))

    const result = await executeYoloTree(tree, plan, fakeDeps(state))

    expect(state.stamps).toEqual([`root`])
    expect(state.reverts).toEqual([`root`])
    expect(result.outcomes.get(`root`)).toEqual({
      kind: `failed`,
      detail: `Required status check "ci" is expected.`,
    })
    // Nothing below an unmerged root lands.
    expect(result.mergedNow).toBe(0)
    for (const id of [`c1`, `c2`, `gc`]) {
      expect(result.outcomes.get(id)).toMatchObject({ kind: `waiting` })
    }
  })

  it(`polls GitHub's lazy mergeability before trusting the state`, async () => {
    const state = freshState()
    const tree = followUpTree(state)
    const plan = planYoloTree(tree)
    let reads = 0
    const deps = fakeDeps(state)
    const getPull = deps.getPull
    deps.getPull = async (repo, n) => {
      const found = await getPull(repo, n)
      if (n === 2) {
        reads += 1
        // Two null reads after the base moved, then a verdict.
        if (reads <= 2) return { ...found, mergeable: null, mergeableState: `unknown` }
      }
      return found
    }

    const result = await executeYoloTree(tree, plan, deps)

    expect(result.outcomes.get(`c1`)).toEqual({ kind: `merged` })
    expect(reads).toBeGreaterThanOrEqual(3)
  })
})

describe(`maybeMergeYoloTree`, () => {
  it(`is a no-op outside yolo mode`, async () => {
    const state = freshState()
    const result = await maybeMergeYoloTree(`root`, fakeDeps(state))
    expect(result).toEqual({ status: `not_yolo` })
    expect(state.events).toEqual([])
  })

  it(`reports an incomplete tree and merges nothing`, async () => {
    const state = freshState()
    state.tree = followUpTree(state, { gc: { status: `running`, agentBusy: true } })
    const result = await maybeMergeYoloTree(`gc`, fakeDeps(state))
    expect(result).toEqual({ status: `incomplete`, blocking: [`EXP-4`] })
    expect(state.events).toEqual([])
  })

  it(`merges the whole tree once the last run ended, and reports the caller's own outcome`, async () => {
    const state = freshState()
    state.tree = followUpTree(state)
    const result = await maybeMergeYoloTree(`gc`, fakeDeps(state))
    expect(result).toMatchObject({ status: `merged`, mergedNow: 4 })
    if (result.status !== `merged`) throw new Error(`unreachable`)
    expect(result.outcomes.root).toEqual({ kind: `merged` })
    expect(result.outcomes.gc).toEqual({ kind: `merged` })
  })

  it(`messages the owner once per set of leftovers: a dirty child, a run that ended without a PR`, async () => {
    const state = freshState()
    state.tree = followUpTree(state, {
      c2: { issuePrUrl: null, issuePrNumber: null, issuePrState: null },
    })
    state.onMerge = async (n) => {
      if (n === 2) state.pulls.get(4)!.mergeableState = `dirty`
    }
    const deps = fakeDeps(state)

    await maybeMergeYoloTree(`gc`, deps)

    expect(state.notifications).toHaveLength(1)
    const note = state.notifications[0]!
    expect(note.ownerIds).toEqual([OWNER])
    expect(note.title).toBe(`Yolo merge of EXP-1: 2 merged, 2 need you`)
    expect(note.body).toContain(`#4 EXP-4: conflicts on the default branch after the retarget`)
    expect(note.body).toContain(`Fix merge conflicts is one click away in Reviews`)
    expect(note.body).toContain(`EXP-3: ended without a PR; skipped.`)
    // The inbox row opens the dirty PR's issue.
    expect(note.issueId).toBe(`issue-4`)

    // The root's next idle edge finds the same leftovers: no second message.
    state.tree = followUpTree(state, {
      root: { issuePrState: `merged` },
      c1: { issuePrState: `merged` },
      c2: { issuePrUrl: null, issuePrNumber: null, issuePrState: null },
    })
    state.pulls.get(4)!.mergeableState = `dirty`
    await maybeMergeYoloTree(`root`, deps)
    expect(state.notifications).toHaveLength(1)
  })

  it(`dedupes a concurrent trigger and reruns once on a fresh snapshot`, async () => {
    const state = freshState()
    state.tree = followUpTree(state)
    const deps = fakeDeps(state)
    let concurrent: Awaited<ReturnType<typeof maybeMergeYoloTree>> | null = null
    state.onMerge = async (n) => {
      if (n === 1) concurrent = await maybeMergeYoloTree(`c2`, deps)
    }

    const result = await maybeMergeYoloTree(`gc`, deps)

    expect(concurrent).toEqual({ status: `in_flight` })
    expect(result).toMatchObject({ status: `merged` })
    // The flagged rerun reloaded the tree once more after the first pass.
    expect(state.loads).toBe(3)
  })

  it(`never throws: a failing load is logged and reported`, async () => {
    const state = freshState()
    const deps = fakeDeps(state)
    deps.loadTree = async () => {
      throw new Error(`boom`)
    }
    const result = await maybeMergeYoloTree(`root`, deps)
    expect(result).toEqual({ status: `error`, error: `boom` })
  })
})
