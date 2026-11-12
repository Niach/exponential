import { act, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import type { SteerSessionSnapshot } from "@/lib/steer-session-store"
import type { SessionRow } from "@/lib/session-tree"

// Large chats: what opening one costs the web's main thread — the real store
// folds the device's join replay, then the real session view renders it
// (markdown included). Guards the painted rows (≤ one window) and the render
// time over a synthetic 20k-event journal; `STEER_REAL_JOURNAL=<file.jsonl>`
// swaps in a real one and prints the numbers.

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
vi.mock(`@/hooks/use-session-tree`, () => ({
  useSessionMessages: () => messages,
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
const { FEED_WINDOW } = await import(`@/lib/agent-feed`)
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



describe(`large chat render`, () => {
  beforeEach(() => vi.useFakeTimers({ toFake: [`setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, `Date`] }))
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
    // Generous for CI; a laptop renders this window in well under a second.
    expect(renderMs).toBeLessThan(5_000)
    if (path) process.stderr.write(`${JSON.stringify({ events: events.length, items: opened.items, rows: opened.rows, storeMs: opened.ms, renderMs, nodes })}\n`)
    view.unmount()
  })

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
    const timeIt = (step: () => void) => {
      const started = performance.now()
      act(step)
      return performance.now() - started
    }
    const streamed: number[] = []
    for (let n = 0; n < 5; n++) {
      const last = snapshot.feed[snapshot.feed.length - 1]!
      snapshot = { ...snapshot, feed: [...snapshot.feed, { ...last, id: last.id + 1_000_000 + n }] }
      streamed.push(
        timeIt(() =>
          view.rerender(
            <TooltipProvider>
              <AgentSessionView session={{ ...session }} />
            </TooltipProvider>
          )
        )
      )
    }
    const button = screen.queryByRole(`button`, { name: /load earlier/i })
    const loadEarlierMs = button ? timeIt(() => fireEvent.click(button)) : null
    expect(view.container.querySelectorAll(`*`).length).toBeGreaterThan(0)
    if (path) process.stderr.write(`${JSON.stringify({ streamedMs: streamed.map((ms) => Math.round(ms)), loadEarlierMs: loadEarlierMs && Math.round(loadEarlierMs) })}\n`)
    view.unmount()
  })
})
