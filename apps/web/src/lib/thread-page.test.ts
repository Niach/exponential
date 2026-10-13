import { describe, expect, it } from "vitest"
import {
  isFreshThreadChild,
  parseThreadChild,
  selectedThreadChild,
  threadChildSearch,
  threadGuideResults,
  orphanLiveSessions,
  sameThreadInputs,
  splitThreadList,
  threadPageState,
  threadPrCandidates,
  threadPrUrl,
  threadReplyTarget,
  threadRunStatus,
  toThreadViewItem,
  toThreadViewRun,
  THREAD_FRESH_CHILD_MS,
} from "@/lib/thread-page"
import type { ThreadChild } from "@/lib/thread-view"

// EXP-1284: the web thread page's own rules around the shared model.

const NOW = new Date(`2026-10-12T10:00:00.000Z`)
const at = (min: number) => new Date(NOW.getTime() + min * 60_000).toISOString()

const runRow = (over: Record<string, unknown> = {}) => ({
  id: `r1`,
  userId: `u1`,
  status: `running`,
  endedBy: null,
  needsInput: false,
  agentBusy: false,
  issueId: null,
  parentSessionId: null,
  resumedFromId: null,
  startedAt: at(-10),
  endedAt: null,
  updatedAt: at(-1),
  ...over,
})

describe(`threadRunStatus`, () => {
  it(`keeps a live row live and a heartbeat-dead one ended`, () => {
    expect(threadRunStatus(runRow(), NOW)).toBe(`running`)
    expect(threadRunStatus(runRow({ status: `in_review` }), NOW)).toBe(`in_review`)
    expect(threadRunStatus(runRow({ updatedAt: at(-60 * 24) }), NOW)).toBe(`ended`)
    expect(threadRunStatus(runRow({ status: `ended`, endedBy: `user` }), NOW)).toBe(`ended`)
  })

  it(`reads a sweep end (EXP-888) as still running`, () => {
    expect(threadRunStatus(runRow({ status: `ended`, endedBy: `stale` }), NOW)).toBe(`running`)
  })
})

describe(`toThreadViewRun / toThreadViewItem`, () => {
  it(`maps the synced rows onto the model's shapes`, () => {
    const run = toThreadViewRun(runRow({ agentBusy: true, startedAt: new Date(at(-10)) }), {
      name: `Fix the keyboard`,
      deviceLabel: `macbook`,
      now: NOW,
    })
    expect(run).toMatchObject({
      id: `r1`,
      name: `Fix the keyboard`,
      deviceLabel: `macbook`,
      status: `running`,
      agentBusy: true,
      startedAt: at(-10),
      endedAt: null,
    })
    expect(
      toThreadViewItem({
        id: `i1`,
        kind: `issue`,
        issueId: `a`,
        sessionId: null,
        source: `agent`,
        addedBySessionId: `r1`,
        sortOrder: 2,
        createdAt: new Date(at(1)),
      })
    ).toEqual({
      id: `i1`,
      kind: `issue`,
      issueId: `a`,
      sessionId: null,
      source: `agent`,
      addedBySessionId: `r1`,
      sortOrder: 2,
      createdAt: at(1),
    })
  })
})

describe(`the child on show`, () => {
  const strip: ThreadChild[] = [
    { kind: `conversation` },
    { kind: `issue`, issueId: `a`, identifier: `EXP-1276`, title: `ios keyboard` },
    { kind: `add` },
    { kind: `guide`, dot: true },
  ]

  it(`parses ?child= and falls back to the conversation`, () => {
    expect(parseThreadChild(undefined)).toEqual({ kind: `conversation` })
    expect(parseThreadChild(`guide`)).toEqual({ kind: `guide` })
    expect(parseThreadChild(`exp-1276`)).toEqual({ kind: `issue`, identifier: `EXP-1276` })
    expect(selectedThreadChild({ strip }, parseThreadChild(`EXP-1276`))).toEqual(strip[1])
    expect(selectedThreadChild({ strip }, parseThreadChild(`EXP-9`))).toEqual({
      kind: `conversation`,
    })
    expect(selectedThreadChild({ strip }, parseThreadChild(`guide`))).toEqual(strip[3])
  })

  it(`names each child in the search the other way round`, () => {
    expect(threadChildSearch(strip[0]!)).toBeUndefined()
    expect(threadChildSearch(strip[1]!)).toBe(`EXP-1276`)
    expect(threadChildSearch(strip[3]!)).toBe(`guide`)
  })
})

describe(`threadReplyTarget`, () => {
  const run = (id: string, userId: string, status: string, min: number) => ({
    id,
    userId,
    status,
    startedAt: at(min),
  })

  it(`sends to the latest live run the viewer owns`, () => {
    expect(
      threadReplyTarget({
        runs: [run(`r1`, `me`, `running`, -30), run(`r2`, `me`, `running`, -5), run(`r3`, `them`, `running`, -1)],
        viewerId: `me`,
        canSteer: true,
      })
    ).toEqual({ kind: `send`, runId: `r2` })
  })

  it(`sends to the orchestrating parent, not the parallel child it spawned`, () => {
    expect(
      threadReplyTarget({
        runs: [
          { ...run(`r1`, `me`, `running`, -30), parentSessionId: null, resumedFromId: null },
          { ...run(`r2`, `me`, `running`, -5), parentSessionId: `r1`, resumedFromId: null },
          { ...run(`r3`, `me`, `running`, -4), parentSessionId: `r1`, resumedFromId: null },
        ],
        viewerId: `me`,
        canSteer: true,
      })
    ).toEqual({ kind: `send`, runId: `r1` })
    // A resume of a child is a session of its own; a child whose parent is
    // not in the thread is one too.
    expect(
      threadReplyTarget({
        runs: [
          { ...run(`r1`, `me`, `ended`, -30), parentSessionId: null, resumedFromId: null },
          { ...run(`r2`, `me`, `running`, -5), parentSessionId: `elsewhere`, resumedFromId: null },
        ],
        viewerId: `me`,
        canSteer: true,
      })
    ).toEqual({ kind: `send`, runId: `r2` })
  })

  it(`resumes the viewer's ended run when it is the latest`, () => {
    expect(
      threadReplyTarget({
        runs: [run(`r1`, `me`, `ended`, -30), run(`r2`, `me`, `ended`, -5)],
        viewerId: `me`,
        canSteer: true,
      })
    ).toEqual({ kind: `resume`, runId: `r2` })
  })

  it(`starts a run when the latest is someone else's, or there is none`, () => {
    expect(
      threadReplyTarget({
        runs: [run(`r1`, `me`, `ended`, -30), run(`r2`, `them`, `ended`, -5)],
        viewerId: `me`,
        canSteer: true,
      })
    ).toEqual({ kind: `start` })
    expect(threadReplyTarget({ runs: [], viewerId: `me`, canSteer: true })).toEqual({
      kind: `start`,
    })
  })

  it(`hides for a viewer who cannot steer at all`, () => {
    expect(
      threadReplyTarget({ runs: [run(`r1`, `me`, `running`, -1)], viewerId: `me`, canSteer: false })
    ).toBeNull()
    expect(threadReplyTarget({ runs: [], viewerId: undefined, canSteer: true })).toBeNull()
  })
})

describe(`isFreshThreadChild`, () => {
  it(`rings a child created moments before it first rendered, once`, () => {
    const created = Date.parse(at(0))
    expect(isFreshThreadChild(at(0), created + 2_000)).toBe(true)
    expect(isFreshThreadChild(at(0), created + THREAD_FRESH_CHILD_MS + 1)).toBe(false)
    expect(isFreshThreadChild(null, created)).toBe(false)
  })
})

// Review T8: the newest OPEN PR, else the newest MERGED one, over the PRs on
// the thread's sessions AND issues (never a closed one).
describe(`threadPrUrl`, () => {
  it(`targets the newest open PR, whatever is newer and merged`, () => {
    expect(
      threadPrUrl(
        [
          { prUrl: `https://x/pull/1`, prState: `open`, updatedAt: at(-30) },
          { prUrl: `https://x/pull/2`, prState: `merged`, updatedAt: at(-5) },
          { prUrl: null, prState: null, updatedAt: at(-1) },
        ],
        []
      )
    ).toBe(`https://x/pull/1`)
  })

  it(`falls back to the newest merged PR, issues included`, () => {
    expect(
      threadPrUrl(
        [{ prUrl: `https://x/pull/2`, prState: `merged`, updatedAt: at(-50) }],
        [
          { prUrl: `https://x/pull/9`, prState: `merged`, updatedAt: at(-5) },
          { prUrl: `https://x/pull/7`, prState: `closed`, updatedAt: at(-1) },
        ]
      )
    ).toBe(`https://x/pull/9`)
    expect(threadPrUrl([], [{ prUrl: `https://x/pull/7`, prState: `closed`, updatedAt: at(0) }])).toBeNull()
    expect(threadPrUrl([], [{ prUrl: null, prState: null, updatedAt: at(0) }])).toBeNull()
  })

  it(`lists one candidate per PR-bearing row`, () => {
    expect(
      threadPrCandidates(
        [{ prUrl: `https://x/pull/1`, prState: `open`, updatedAt: new Date(at(-3)) }],
        [{ prUrl: null, prState: null, updatedAt: at(0) }]
      )
    ).toEqual([{ url: `https://x/pull/1`, state: `open`, at: at(-3) }])
  })
})

describe(`threadGuideResults (rule 8)`, () => {
  it(`reads the thread's Guide once it has one, else the run's`, () => {
    expect(threadGuideResults({ results: [`run`] }, { results: [`thread`] })).toEqual([`thread`])
    expect(threadGuideResults({ results: [`run`] }, { results: null })).toEqual([`run`])
    expect(threadGuideResults({ results: [`run`] }, null)).toEqual([`run`])
    expect(threadGuideResults({}, null)).toBeNull()
  })
})

// Review T6: the list scope = the ACTIVE team; Recent = MY threads.
describe(`splitThreadList`, () => {
  const bundle = (id: string, teamId: string, min: number, flags: { mineLive?: boolean; mine?: boolean; empty?: boolean } = {}) => ({
    thread: { id, teamId, updatedAt: at(min) },
    runs: flags.empty ? [] : [{ id: `r-${id}` }],
    issues: [],
    mineLive: flags.mineLive ?? false,
    mine: flags.mine ?? flags.mineLive ?? false,
  })

  it(`lists the active team only, Running first, Recent = mine`, () => {
    const all = [
      bundle(`a`, `t1`, -5, { mineLive: true }),
      bundle(`b`, `t1`, -1, { mine: true }),
      bundle(`c`, `t1`, -2),
      bundle(`d`, `t2`, 0, { mine: true }),
      bundle(`e`, `t1`, 0, { mine: true, empty: true }),
    ]
    const { running, recent } = splitThreadList(all, `t1`)
    expect(running.map((row) => row.thread.id)).toEqual([`a`])
    expect(recent.map((row) => row.thread.id)).toEqual([`b`])
    // Every team (the phone's banded lists).
    expect(splitThreadList(all, null).recent.map((row) => row.thread.id)).toEqual([`d`, `b`])
  })
})

// Review T4: a live session of mine that no listed thread holds.
describe(`orphanLiveSessions`, () => {
  it(`keeps only the live sessions no thread lists`, () => {
    const bundles = [{ thread: { id: `t`, teamId: `x`, updatedAt: at(0) }, runs: [{ id: `r1` }], issues: [], mineLive: true, mine: true }]
    expect(orphanLiveSessions(bundles, [{ id: `r1` }, { id: `r2` }])).toEqual([{ id: `r2` }])
    expect(orphanLiveSessions([], [{ id: `r1` }])).toEqual([{ id: `r1` }])
  })
})

// Review T16: a thread view is rebuilt only when ITS inputs change.
describe(`sameThreadInputs`, () => {
  it(`compares rows by identity and strings by value`, () => {
    const row = { id: `r1` }
    expect(sameThreadInputs([row, `macbook`, `running`], [row, `macbook`, `running`])).toBe(true)
    expect(sameThreadInputs([row], [{ id: `r1` }])).toBe(false)
    expect(sameThreadInputs([row, `running`], [row, `ended`])).toBe(false)
    expect(sameThreadInputs([row], [row, row])).toBe(false)
    expect(sameThreadInputs(undefined, [row])).toBe(false)
  })
})

// Review T14: never a flash of "Thread not found" while the thread syncs.
describe(`threadPageState`, () => {
  const base = { hasBundle: false, syncSettled: true, itemNamesIt: false, fetch: `idle` as const, graceOver: false }

  it(`renders the thread once its row synced`, () => {
    expect(threadPageState({ ...base, hasBundle: true })).toBe(`ready`)
  })

  it(`keeps the skeleton while the sync, an item or the fetch says wait`, () => {
    expect(threadPageState({ ...base, syncSettled: false })).toBe(`skeleton`)
    expect(threadPageState({ ...base, fetch: `pending` })).toBe(`skeleton`)
    expect(threadPageState({ ...base, fetch: `found` })).toBe(`skeleton`)
    expect(threadPageState({ ...base, itemNamesIt: true })).toBe(`skeleton`)
    // Not asked yet = not concluded yet.
    expect(threadPageState(base)).toBe(`skeleton`)
  })

  it(`concludes missing only on the server's word or after the grace`, () => {
    expect(threadPageState({ ...base, fetch: `missing` })).toBe(`missing`)
    expect(threadPageState({ ...base, fetch: `found`, graceOver: true })).toBe(`missing`)
    expect(threadPageState({ ...base, itemNamesIt: true, fetch: `missing` })).toBe(`missing`)
  })
})
