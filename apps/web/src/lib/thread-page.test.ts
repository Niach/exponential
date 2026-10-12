import { describe, expect, it } from "vitest"
import {
  isFreshThreadChild,
  parseThreadChild,
  selectedThreadChild,
  threadChildSearch,
  threadGuideResults,
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

describe(`threadPrUrl`, () => {
  it(`takes the newest run's PR, else the URL the issues share`, () => {
    expect(
      threadPrUrl(
        [
          { prUrl: `https://x/pull/1`, startedAt: at(-30) },
          { prUrl: `https://x/pull/2`, startedAt: at(-5) },
          { prUrl: null, startedAt: at(-1) },
        ],
        []
      )
    ).toBe(`https://x/pull/2`)
    expect(
      threadPrUrl(
        [{ prUrl: null, startedAt: at(-1) }],
        [{ prUrl: `https://x/pull/7` }, { prUrl: `https://x/pull/9` }, { prUrl: `https://x/pull/9` }]
      )
    ).toBe(`https://x/pull/9`)
    expect(threadPrUrl([], [{ prUrl: null }])).toBeNull()
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
