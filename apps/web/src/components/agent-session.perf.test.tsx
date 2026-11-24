import { act, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { SteerSessionSnapshot } from "@/lib/steer-session-store"
import type { SessionRow } from "@/lib/session-tree"

// Large chats: what opening one costs the web's main thread — the real store
// folds the device's join replay, then the real session view renders it
// (markdown included). Guards the WORK over a synthetic 20k-event journal:
// the painted rows (≤ one window) and how many message bubbles a stream or a
// page re-renders. #EXP-1886 (6): no wall-clock bound, since a loaded runner
// timed the old 5 s one out; `STEER_REAL_JOURNAL=<file.jsonl>` swaps in a
// real journal and prints the timings.

/** A hang guard only (a laptop runs each case in ~2 s). */
const HANG_GUARD_MS = 120_000

// The transcript's row renders: a row that MOUNTS (new to the window) and a
// row that RE-renders (already painted; the work #PERF-9 keeps away).
const rows = vi.hoisted(() => ({ renders: 0, mounts: 0 }))
vi.mock(`@exp/ui`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@exp/ui")>()
  const { useEffect } = await import(`react`)
  const TranscriptRow = (props: Parameters<typeof actual.TranscriptRow>[0]) => {
    rows.renders++
    useEffect(() => {
      rows.mounts++
    }, [])
    return <actual.TranscriptRow {...props} />
  }
  return { ...actual, TranscriptRow }
})

let snapshot: SteerSessionSnapshot
const relayAnswer = vi.fn()

vi.mock(`@/lib/steer-session-store`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/steer-session-store")>()
  return {
    ...actual,
    acquireSteerSession: () => ({
      subscribe: () => () => {},
      getSnapshot: () => snapshot,
      connect: () => () => {},
      noteSessionStatus: () => {},
      noteDeviceLabel: () => {},
      noteHostOffline: () => {},
      kick: () => {},
      reconnect: () => {},
      loadEarlier: () => {},
      sendMessage: () => true,
      answerQuestion: (...args: unknown[]) => relayAnswer(...args),
      unqueue: () => {},
    }),
  }
})
vi.mock(`@/hooks/use-team-data`, () => ({ useTeamUsers: () => ({ users: [] }) }))
let hostOnline = true
vi.mock(`@/hooks/use-session-device`, () => ({
  useSessionDevice: () => ({ online: hostOnline, label: `Studio Mac` }),
}))
vi.mock(`@/components/session-account-switch`, () => ({
  SessionAccountSwitchButton: () => null,
}))

// EXP-1280: the session's synced messages (tool-posted cards) and the
// `sessions.answer` call they answer through.
let messages: unknown[] = []
const answerMutate = vi.fn(async (_input: unknown, _opts?: unknown) => ({ txid: 1 }))
// #EXP-1998: the page hook, stable while `messages` is (like the real memo).
let messagePage: { rows: unknown[]; more: boolean } | undefined
vi.mock(`@/hooks/use-session-tree`, () => ({
  useSessionMessages: () => messages,
  useSessionMessagePage: () => {
    if (!messagePage || messagePage.rows !== messages) messagePage = { rows: messages, more: false }
    return messagePage
  },
  useSessionCard: () => null,
  loadEarlierMessages: () => {},
}))
// Stable like the real hook (memoized over the agent rows).
vi.mock(`@/hooks/use-session-nav`, () => {
  const xref = () => null
  return { useXrefAgent: () => xref }
})
const openPlan = vi.hoisted(() => vi.fn())
vi.mock(`@/hooks/use-open-plan`, () => ({ useOpenPlan: () => openPlan }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: { sessions: { answer: { mutate: answerMutate } } } }))
vi.mock(`@/lib/collections`, () => ({
  messageCollection: { utils: { awaitTxId: async () => undefined }, get: () => undefined },
  // EXP-1459: the sealing path's lookups (no key holder here: nothing seals).
  sessionCollection: { get: () => undefined, values: () => [] },
  deviceCollection: { values: () => [] },
  userCollection: { values: () => [] },
}))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: { user: { id: `u1` } } }) }))

// Entity chips resolve routes and synced rows; a perf run paints their text.
vi.mock(`@/components/entity-preview/entity-ref-chip`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/components/entity-preview/entity-ref-chip")>()),
  EntityRefChip: ({ label }: { label?: string }) => <span>{label}</span>,
}))
vi.mock(`@/components/entity-preview/entity-ref-chips`, () => ({ EntityRefChips: () => null }))
vi.mock(`@/components/entity-preview/use-entity-ref-target`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/components/entity-preview/use-entity-ref-target")>()),
  useEntityRefTargets: () => [],
  useEntityRefTarget: () => ({}),
  EntityRefLink: ({ children }: { children?: React.ReactNode }) => <span>{children}</span>,
}))

const { AgentSessionView } = await import(`./agent-session`)
const { FEED_MOUNT_SLICE, FEED_WINDOW } = await import(`@/lib/agent-feed`)
const { openChat, realJournal, replay, REPLAY_TAIL, syntheticJournal } = await import(`@/lib/perf/large-chat`)
const { TooltipProvider } = await import(`@exp/ui`)

const session = {
  id: `s1`,
  teamId: `t1`,
  parentId: null,
  rootId: `s1`,
  kind: `thread`,
  userId: `u1`,
  deviceId: `d1`,
  status: `running`,
  busy: false,
  needsInput: false,
  blocked: null,
  shared: false,
  title: `Profile the launch path`,
  guide: null,
  cli: `claude`,
  startedAt: new Date().toISOString(),
  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString(),
} as unknown as SessionRow



/** The agent message bubbles painted (each one's `FeedText`). */
const agentBubbles = (root: HTMLElement) => root.querySelectorAll(`[aria-label="Agent message"]`).length

describe(`large chat render`, () => {
  beforeEach(() => {
    rows.renders = 0
    rows.mounts = 0
    vi.useFakeTimers({ toFake: [`setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, `Date`] })
  })
  afterEach(() => vi.useRealTimers())

  it(`renders one window of the newest tail`, async () => {
    const path = process.env.STEER_REAL_JOURNAL
    const events = path ? realJournal(path) : syntheticJournal(20_000)
    const opened = await openChat(replay(events, process.env.STEER_FULL_REPLAY ? null : REPLAY_TAIL).frames)
    snapshot = { ...opened.snapshot, phase: { kind: `ended` } }
    const started = performance.now()
    const view = render(
      <TooltipProvider>
        <AgentSessionView session={{ ...session, status: `ended` } as SessionRow} />
      </TooltipProvider>
    )
    const renderMs = performance.now() - started
    const nodes = view.container.querySelectorAll(`*`).length
    expect(opened.rows).toBeLessThanOrEqual(FEED_WINDOW)
    // The window's rows render once each, never the journal's (~4k).
    expect(agentBubbles(view.container)).toBeGreaterThan(0)
    expect(rows.mounts).toBeGreaterThan(0)
    expect(rows.mounts).toBeLessThanOrEqual(FEED_WINDOW)
    expect(rows.renders).toBe(rows.mounts)
    if (path) process.stderr.write(`${JSON.stringify({ events: events.length, items: opened.items, rows: opened.rows, storeMs: opened.ms, renderMs, nodes })}\n`)
    view.unmount()
  }, HANG_GUARD_MS)

  // #PERF-9: the two re-renders a reader feels: a `Load earlier` page (one
  // more window mounts over the rows already there) and a streamed event
  // (one row moves). Unchanged rows keep their element (no re-render).
  it(`a Load earlier page and a streamed event re-render only what moved`, async () => {
    const path = process.env.STEER_REAL_JOURNAL
    const events = path ? realJournal(path) : syntheticJournal(20_000)
    const opened = await openChat(replay(events, REPLAY_TAIL).frames)
    snapshot = { ...opened.snapshot, phase: { kind: `live` } }
    const view = render(
      <TooltipProvider>
        <AgentSessionView session={session} />
      </TooltipProvider>
    )
    // The work a step does: its wall time (printed only), the rows it
    // mounted and the painted rows it re-rendered (asserted).
    const measure = (step: () => void) => {
      const { renders, mounts } = rows
      const started = performance.now()
      act(step)
      const mounted = rows.mounts - mounts
      return { ms: performance.now() - started, mounted, rerendered: rows.renders - renders - mounted }
    }
    const streamed: { ms: number; mounted: number; rerendered: number }[] = []
    // A streamed bubble: the newest narration again, as a new event.
    const narration = [...snapshot.feed].reverse().find((item) => item.kind === `narration`)!
    for (let n = 0; n < 5; n++) {
      snapshot = { ...snapshot, feed: [...snapshot.feed, { ...narration, id: snapshot.feed.at(-1)!.id + 1_000_000 + n }] }
      const step = measure(() =>
        view.rerender(
          <TooltipProvider>
            <AgentSessionView session={{ ...session }} />
          </TooltipProvider>
        )
      )
      streamed.push(step)
    }
    // The new row mounts; of the painted ones only the few at its seam and
    // at the window's sliding top re-render (all of them = ~200).
    for (const step of streamed) {
      expect(step.mounted).toBe(1)
      expect(step.rerendered).toBeLessThanOrEqual(4)
    }

    const button = screen.getByRole(`button`, { name: /load earlier/i })
    const press = measure(() => fireEvent.click(button))
    // #EXP-1999: the press mounts nothing itself; the page lands in slices
    // of at most FEED_MOUNT_SLICE items, one task each, the rows already
    // there keeping their element but the seam's (every row = ~200).
    expect(press.mounted).toBe(0)
    const slices: { ms: number; mounted: number; rerendered: number }[] = []
    for (let n = 0; n < FEED_WINDOW; n++) {
      const slice = measure(() => vi.advanceTimersToNextTimer())
      if (slice.mounted === 0 && slice.rerendered === 0) break
      slices.push(slice)
    }
    expect(slices.length).toBeGreaterThan(1)
    for (const slice of slices) {
      expect(slice.mounted).toBeLessThanOrEqual(FEED_MOUNT_SLICE)
      expect(slice.rerendered).toBeLessThanOrEqual(4)
    }
    // One more window mounted in all.
    const mounted = slices.reduce((sum, slice) => sum + slice.mounted, 0)
    expect(mounted).toBeGreaterThan(FEED_MOUNT_SLICE)
    expect(mounted).toBeLessThanOrEqual(FEED_WINDOW)
    if (path) process.stderr.write(`${JSON.stringify({ streamedMs: streamed.map((step) => Math.round(step.ms)), sliceMs: slices.map((slice) => Math.round(slice.ms)) })}\n`)
    view.unmount()
  }, HANG_GUARD_MS)
})
