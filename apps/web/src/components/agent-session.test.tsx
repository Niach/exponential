import { fireEvent, render, screen, within } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SteerSessionSnapshot } from "@/lib/steer-session-store"
import type { SessionRow } from "@/lib/session-tree"
import enetFixture from "@exp/domain-contract/fixtures/steer-enet.json"

// EXP-1324 (+ EXP-1321): the raw session view paints its transcript as Trace
// lines (tools, groups, reasoning, subagents, failed tests) and its bottom
// strip as THE Queue. The store is stubbed: this pins the RENDER of a feed.

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
vi.mock(`@/hooks/use-session-tree`, () => ({ useSessionMessages: () => messages, useSessionCard: () => null }))
vi.mock(`@/hooks/use-session-nav`, () => ({ useXrefAgent: () => () => null }))
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
vi.mock(`@/components/chat/chat-markdown`, () => ({
  renderChatText: (text: string) => <p>{text}</p>,
}))

const { AgentSessionView } = await import(`./agent-session`)
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

function snap(over: Partial<SteerSessionSnapshot>): SteerSessionSnapshot {
  return {
    phase: { kind: `live` },
    feed: [],
    latestDiff: null,
    compacting: null,
    config: null,
    usage: null,
    contextLayout: null,
    rateLimit: null,
    turnState: `ended`,
    turnStartedAt: null,
    turnTokens: null,
    turnWriting: null,
    backgroundTasks: [],
    taskList: [],
    queue: [],
    workflows: new Map(),
    runningWorkflow: null,
    answerStates: {},
    connected: true,
    canLoadEarlier: false,
    deviceOffline: false,
    replayed: true,
    ...over,
  }
}

const GRADLE_FAIL = `StartupBenchmark > coldStart FAILED\n\n12 tests completed, 1 failed`

beforeEach(() => {
  snapshot = snap({})
  messages = []
  answerMutate.mockClear()
  relayAnswer.mockClear()
})

describe(`AgentSessionView Trace transcript`, () => {
  it(`paints reasoning, a command with its Output, a subagent and failed tests`, () => {
    snapshot = snap({
      feed: [
        { id: 1, seq: 1, kind: `thought`, thoughtId: `t1`, settled: true, durationMs: 5200 },
        {
          id: 2,
          seq: 2,
          kind: `tool`,
          name: `Bash`,
          detail: `./gradlew :app:startupProfile`,
          toolKind: `execute`,
          settled: true,
          output: `BUILD SUCCESSFUL in 41s`,
        },
        { id: 3, seq: 3, kind: `narration`, text: `Measuring.` },
        { id: 4, seq: 4, kind: `subagent`, subagentId: `a1`, agentType: `Explore`, status: `started`, at: 1_000 },
        { id: 5, seq: 5, kind: `subagent`, subagentId: `a1`, agentType: `Explore`, status: `completed`, at: 73_000 },
        { id: 6, seq: 6, kind: `narration`, text: `Now the benchmark.` },
        {
          id: 7,
          seq: 7,
          kind: `tool`,
          name: `Bash`,
          detail: `./gradlew :benchmark:connectedBenchmarkAndroidTest`,
          toolKind: `execute`,
          settled: true,
          failed: true,
          output: GRADLE_FAIL,
        },
      ],
    })
    render(<AgentSessionView session={session} />)
    expect(screen.getByTestId(`session-thought-line`).textContent).toBe(`Thought for 5s`)
    const titles = [...document.querySelectorAll(`[data-slot="trace-title"]`)].map(
      (node) => node.textContent
    )
    expect(titles).toContain(`Ran ./gradlew :app:startupProfile`)
    expect(titles).toContain(`Failed: 1 of 12 tests in StartupBenchmark`)
    const subagent = screen.getByTestId(`session-subagent-line`)
    expect(subagent.textContent).toContain(`Ran the Explore agent`)
    expect(subagent.textContent).toContain(`1m 12s`)
    // The failed-tests line is an error line; its output is the destructive block.
    const failedLine = [...document.querySelectorAll(`[data-slot="trace-line"]`)].find((node) =>
      node.textContent?.startsWith(`Failed:`)
    )
    expect(failedLine?.getAttribute(`data-status`)).toBe(`error`)
    // Output is folded until the reader opens it, then it is THE CodeBlock.
    expect(screen.queryByTestId(`session-tool-output`)).toBeNull()
    fireEvent.click(screen.getAllByRole(`button`, { name: `Show the output` })[0]!)
    const output = screen.getByTestId(`session-tool-output`)
    expect(output.getAttribute(`data-size`)).toBe(`sm`)
    expect(output.textContent).toContain(`Output`)
    expect(output.querySelector(`[data-language="ansi"]`)).not.toBeNull()
    expect(output.querySelector(`[data-token="success"]`)?.textContent).toBe(
      `BUILD SUCCESSFUL in 41s`
    )
  })

  it(`an ended row no device fed names the machine holding its transcript (VAPP-171)`, () => {
    const detail = `buildbox is offline. The transcript lives on that machine.`
    snapshot = snap({ phase: { kind: `ended`, detail }, deviceOffline: true, connected: false })
    render(<AgentSessionView session={{ ...session, status: `ended` } as SessionRow} />)
    expect(screen.getByTestId(`transcript-device-offline`).textContent).toBe(detail)
  })

  // EXP-1487 (steer-enet.json NO FEED, EMPTY TRANSCRIPT): replays `noFeed`.
  for (const c of enetFixture.noFeed) {
    it(`NO FEED, empty transcript: ${c.name} = ${c.state}`, () => {
      const offline = `buildbox is offline. The transcript lives on that machine.`
      hostOnline = !(c as { deviceOffline?: boolean }).deviceOffline
      snapshot = c.ended
        ? snap({ phase: { kind: `ended`, detail: offline }, deviceOffline: true, connected: false })
        : snap({ phase: { kind: `starting` }, connected: false })
      const row = {
        ...session,
        status: c.ended ? `ended` : c.busy ? `running` : `idle`,
        busy: c.busy,
        needsInput: c.needsInput,
      } as unknown as SessionRow
      render(<AgentSessionView session={row} />)
      const text = document.body.textContent ?? ``
      const startingCopy = `The agent is starting. Waiting for the live stream…`
      hostOnline = true
      if (c.state === `offline`) {
        expect(screen.getByTestId(`transcript-device-offline`).textContent).toBe(offline)
      } else if (c.state === `away`) {
        // #EXP-1703: the device is named, no "starting" placeholder.
        expect(screen.queryByTestId(`transcript-placeholder`)).toBeNull()
        expect(screen.getByTestId(`transcript-device-away`).textContent).toContain(`Studio Mac is offline`)
        expect(text).toContain(enetFixture.noFeedAway.text)
      } else if (c.state === `starting`) {
        expect(screen.getByTestId(`transcript-placeholder`).textContent).toContain(startingCopy)
      } else {
        expect(screen.queryByTestId(`transcript-placeholder`)).toBeNull()
        expect(text).not.toContain(startingCopy)
        expect(text).toContain(enetFixture.noFeedIdle.title)
        expect(text).toContain(enetFixture.noFeedIdle.text)
      }
    })
  }

  it(`the running call is its shimmering line alone`, () => {
    snapshot = snap({
      feed: [
        {
          id: 1,
          seq: 1,
          kind: `tool`,
          name: `Bash`,
          detail: `bun test`,
          toolKind: `execute`,
        },
      ],
    })
    render(<AgentSessionView session={session} />)
    const line = document.querySelector(`[data-slot="trace-line"]`)!
    expect(line.getAttribute(`data-status`)).toBe(`running`)
    expect(line.textContent).toBe(`Running bun test`)
    expect(screen.queryByRole(`button`, { name: `Show the output` })).toBeNull()
  })

  it(`parallel unsettled calls all read as running while the turn streams`, () => {
    // Claude runs tool calls side by side: only the LAST is the live row,
    // yet the earlier one is no less running until its own settle lands.
    snapshot = snap({
      turnState: `started`,
      feed: [
        { id: 1, seq: 1, kind: `tool`, name: `Bash`, detail: `bun test`, toolKind: `execute` },
        { id: 2, seq: 2, kind: `narration`, text: `Meanwhile.` },
        { id: 3, seq: 3, kind: `tool`, name: `Bash`, detail: `bun run typecheck`, toolKind: `execute` },
      ],
    })
    render(<AgentSessionView session={session} />)
    const lines = [...document.querySelectorAll(`[data-slot="trace-line"]`)]
    expect(lines.map((line) => line.textContent)).toEqual([
      `Running bun test`,
      `Running bun run typecheck`,
    ])
    expect(lines.map((line) => line.getAttribute(`data-status`))).toEqual([`running`, `running`])
  })

  it(`EXP-1501: a streaming tool call names itself on the working row with its own clock`, () => {
    snapshot = snap({
      turnState: `started`,
      turnStartedAt: Date.now() - 120_000,
      turnWriting: { tool: `ExitPlanMode`, startedAt: Date.now() - 54_000 },
      feed: [{ id: 1, seq: 1, kind: `narration`, text: `Planning.` }],
    })
    render(<AgentSessionView session={session} />)
    expect(screen.getByText(/^Writing ExitPlanMode… \(5[45]s\)$/)).toBeTruthy()
  })

  it(`a call that never settled reads settled once the stream is gone`, () => {
    // The turn ended (or the publisher died) with the settle never sent: the
    // past tense, not a shimmer that never stops.
    snapshot = snap({
      turnState: `ended`,
      feed: [
        { id: 1, seq: 1, kind: `tool`, name: `Bash`, detail: `bun test`, toolKind: `execute` },
        { id: 2, seq: 2, kind: `narration`, text: `Done.` },
      ],
    })
    render(<AgentSessionView session={session} />)
    const line = document.querySelector(`[data-slot="trace-line"]`)!
    expect(line.textContent).toBe(`Ran bun test`)
    expect(line.getAttribute(`data-status`)).toBeNull()
  })

  it(`a tool run folds under its summary caption`, () => {
    snapshot = snap({
      feed: [
        { id: 1, seq: 1, kind: `tool`, name: `Read`, detail: `a.ts`, toolKind: `read`, settled: true },
        { id: 2, seq: 2, kind: `tool`, name: `Read`, detail: `b.ts`, toolKind: `read`, settled: true },
      ],
    })
    render(<AgentSessionView session={session} />)
    const group = screen.getByTestId(`session-tool-group`)
    const trigger = group.querySelector(`[data-slot="trace-collapsible-trigger"]`) as HTMLElement
    expect(trigger.getAttribute(`aria-expanded`)).toBe(`false`)
    expect(group.textContent).not.toContain(`Read a.ts`)
    fireEvent.click(trigger)
    expect(group.textContent).toContain(`Read a.ts`)
    expect(group.textContent).toContain(`Read b.ts`)
  })
})

describe(`AgentSessionView Queue`, () => {
  it(`lists the work to do open and the completed folded`, () => {
    snapshot = snap({
      taskList: [
        { content: `Profile the cold start`, status: `completed` },
        { content: `Defer the warm-up`, status: `completed` },
        { content: `Measuring on the Galaxy A54`, status: `in_progress` },
        { content: `Open the PR with the numbers`, status: `pending` },
      ],
      backgroundTasks: [
        {
          id: `b1`,
          kind: `shell`,
          description: `Rerun the web and web-phone captures`,
          startedAt: Date.now() - 130_000,
        },
      ],
    })
    render(<AgentSessionView session={session} />)
    // N-12: THE Queue is a DockCard (its collapsed line names the running
    // work); its sheet holds the Trace.
    const card = screen.getByTestId(`dock-card`)
    expect(card.dataset.kind).toBe(`tasks`)
    expect(card.textContent).toContain(`3 to do`)
    expect(card.textContent).toContain(`2 completed`)
    expect(card.textContent).toContain(`Measuring on the Galaxy A54`)
    fireEvent.click(card)
    const queue = screen.getByTestId(`session-task-queue`)
    const todo = screen.getByTestId(`session-task-queue-todo`)
    expect(todo.textContent).toContain(`3 to do`)
    expect(todo.textContent).toContain(`Measuring on the Galaxy A54`)
    expect(todo.textContent).toContain(`Rerun the web and web-phone captures`)
    expect(todo.textContent).toMatch(/2m 1\ds/)
    const running = [...todo.querySelectorAll(`[data-slot="trace-line"]`)].find(
      (node) => node.textContent === `Measuring on the Galaxy A54`
    )
    expect(running?.getAttribute(`data-status`)).toBe(`running`)
    const completed = screen.getByTestId(`session-task-queue-completed`)
    expect(completed.textContent).toBe(`2 completed`)
    expect(queue.textContent).not.toContain(`Profile the cold start`)
    fireEvent.click(completed.querySelector(`[data-slot="trace-collapsible-trigger"]`)!)
    expect(completed.textContent).toContain(`Profile the cold start`)
  })

  it(`is absent with nothing to do`, () => {
    snapshot = snap({ taskList: [{ content: `Ship it`, status: `completed` }] })
    render(<AgentSessionView session={session} />)
    expect(screen.queryByTestId(`session-task-queue`)).toBeNull()
    expect(screen.queryByTestId(`dock-card`)).toBeNull()
  })
})

// #EXP-1538 (dock.json LOCAL FIRST): the store's last-known slots draw in
// the first frame while the stream connects; an ended session shows none.
describe(`AgentSessionView Dock local first`, () => {
  const tasks = [{ content: `Measuring on the Galaxy A54`, status: `in_progress` as const }]

  it(`draws THE Queue in the first render while the stream connects, and measures it`, () => {
    performance.clearMeasures()
    snapshot = snap({ phase: { kind: `connecting` }, connected: false, taskList: tasks })
    render(<AgentSessionView session={session} />)
    expect(screen.getByTestId(`dock-card`).dataset.kind).toBe(`tasks`)
    const [measure] = performance.getEntriesByName(`dock:open-to-visible`)
    expect(measure).toBeTruthy()
  })

  it(`an ended session shows none of the stream's slots`, () => {
    snapshot = snap({ phase: { kind: `ended` }, taskList: tasks, rateLimit: { status: `limited`, resetsAt: Date.now() + 60_000 } as SteerSessionSnapshot[`rateLimit`] })
    render(<AgentSessionView session={{ ...session, status: `ended` } as SessionRow} />)
    expect(screen.queryByTestId(`dock-card`)).toBeNull()
  })
})

describe(`AgentSessionView tool-posted cards (EXP-1280)`, () => {
  const planMessage = (over: Record<string, unknown> = {}) => ({
    id: `m-plan`,
    sessionId: `s1`,
    teamId: `t1`,
    authorKind: `agent`,
    authorId: null,
    authorSessionId: null,
    body: ``,
    card: {
      kind: `plan`,
      plan: `1. Lift the partition`,
      questions: [
        { id: `q`, prompt: `Which layout?`, multi: false, custom: false, options: [{ label: `Two` }, { label: `One` }] },
      ],
    },
    answeredAt: null,
    consumedAt: null,
    createdAt: new Date().toISOString(),
    ...over,
  })
  const proposeCall = {
    id: 3,
    seq: 3,
    kind: `tool` as const,
    name: `mcp__exponential__exponential_session_self_plan`,
    settled: true,
  }

  it(`#EXP-1662: a waiting plan never docks; it is ONE row that opens the plan tab`, () => {
    messages = [planMessage()]
    snapshot = snap({ feed: [{ id: 1, seq: 1, kind: `narration`, text: `Planned.` }, proposeCall] })
    render(<AgentSessionView session={session} composer={<div data-testid="composer" />} />)
    // dock.json: the plan is not a DockCard; the composer stays.
    expect(screen.queryByTestId(`dock-card`)).toBeNull()
    expect(screen.getByTestId(`composer`)).toBeTruthy()
    const card = screen.getByTestId(`session-tool-card`)
    expect(card.textContent).toContain(`Plan: Lift the partition`)
    expect(card.textContent).toContain(`Needs you`)
    expect(card.textContent).not.toContain(`Which layout?`)
    fireEvent.click(within(card).getByTestId(`plan-row`))
    expect(openPlan).toHaveBeenCalledWith(session.id, `m-plan`)
  })

  it(`an answered card sits after the call that posted it; an engine card is the relay's`, () => {
    messages = [
      planMessage({
        answeredAt: new Date().toISOString(),
        card: { ...planMessage().card, decision: `approved`, fresh: false, answers: { q: [`One`] } },
      }),
      planMessage({ id: `m-engine`, card: { kind: `plan`, plan: `Engine plan`, questionId: `x` } }),
    ]
    snapshot = snap({
      feed: [proposeCall, { id: 4, seq: 4, kind: `narration`, text: `Implementing.` }],
    })
    render(<AgentSessionView session={session} composer={<div data-testid="composer" />} />)
    expect(screen.queryByTestId(`dock-card`)).toBeNull()
    expect(screen.getByTestId(`composer`)).toBeTruthy()
    const cards = screen.getAllByTestId(`session-tool-card`)
    expect(cards).toHaveLength(1)
    expect(cards[0]!.textContent).not.toContain(`Engine plan`)
    // After the call, before the next narration.
    const narration = screen.getByText(`Implementing.`)
    expect(cards[0]!.compareDocumentPosition(narration) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })
})

// #EXP-1732: the Chat tab IS the root's session view; what the retired
// Conversation body drew off the synced messages lives here now.
describe(`AgentSessionView as the Chat tab (#EXP-1732)`, () => {
  const base = {
    sessionId: `s1`,
    teamId: `t1`,
    authorSessionId: null,
    answeredAt: null,
    createdAt: new Date().toISOString(),
  }
  const suggestions = (items: Record<string, unknown>[]) => ({
    ...base,
    id: `m-sugg`,
    authorKind: `agent`,
    authorId: null,
    body: ``,
    consumedAt: new Date().toISOString(),
    card: { kind: `suggestions`, items },
  })

  it(`the viewer's queued message sits inline with its X (EXP-1317); a consumed one is the feed's`, () => {
    messages = [
      { ...base, id: `m-q`, authorKind: `user`, authorId: `u1`, body: `Also bump the CLI`, consumedAt: null, card: null },
      { ...base, id: `m-done`, authorKind: `user`, authorId: `u1`, body: `Done already`, consumedAt: new Date().toISOString(), card: null },
    ]
    snapshot = snap({ feed: [{ id: 1, seq: 1, kind: `narration`, text: `On it.` }] })
    render(
      <TooltipProvider>
        <AgentSessionView session={session} composer={<div data-testid="composer" />} />
      </TooltipProvider>
    )
    const extras = screen.getByTestId(`session-chat-extras`)
    expect(extras.textContent).toContain(`Also bump the CLI`)
    expect(extras.textContent).not.toContain(`Done already`)
    expect(within(extras).getByTestId(`chat-unqueue`)).toBeTruthy()
  })

  it(`open suggestions are THE Dock's card with the actions; settled ones stay inline`, () => {
    const actions = { start: vi.fn(), startAll: vi.fn(), here: vi.fn(), file: vi.fn(), dismiss: vi.fn() }
    messages = [suggestions([{ id: `a`, title: `Split the store`, text: `` }, { id: `b`, title: `Cache it`, text: `` }])]
    snapshot = snap({ feed: [{ id: 1, seq: 1, kind: `narration`, text: `On it.` }] })
    const { unmount } = render(
      <AgentSessionView session={session} composer={<div />} suggestions={actions as never} />
    )
    expect(screen.getByTestId(`dock-card`)).toBeTruthy()
    expect(screen.queryByTestId(`session-chat-extras`)?.textContent ?? ``).not.toContain(`Split the store`)
    unmount()
    // Read-only (no actions): nothing docks, the card reads inline.
    snapshot = snap({ feed: [{ id: 1, seq: 1, kind: `narration`, text: `On it.` }] })
    render(<AgentSessionView session={session} composer={<div />} />)
    expect(screen.getByTestId(`session-chat-extras`).textContent).toContain(`Split the store`)
  })

  it(`an empty feed (connecting) still shows the first queued message and a settled card, under the placeholder`, () => {
    messages = [
      { ...base, id: `m-first`, authorKind: `user`, authorId: `u1`, body: `Fix the login`, consumedAt: null, card: null },
      suggestions([{ id: `a`, title: `Split the store`, text: `` }]),
    ]
    snapshot = snap({ phase: { kind: `connecting` }, feed: [] })
    render(
      <TooltipProvider>
        <AgentSessionView session={session} composer={<div />} />
      </TooltipProvider>
    )
    const placeholder = screen.getByTestId(`transcript-placeholder`)
    const extras = screen.getByTestId(`session-placeholder-extras`)
    expect(extras.textContent).toContain(`Fix the login`)
    expect(extras.textContent).toContain(`Split the store`)
    expect(placeholder.compareDocumentPosition(extras) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    // Same column: one scroller holds both.
    expect(placeholder.parentElement).toBe(extras.parentElement)
  })

  it(`no extras = nothing under the placeholder`, () => {
    snapshot = snap({ phase: { kind: `connecting` }, feed: [] })
    render(<AgentSessionView session={session} composer={<div />} />)
    expect(screen.getByTestId(`transcript-placeholder`)).toBeTruthy()
    expect(screen.queryByTestId(`session-placeholder-extras`)).toBeNull()
  })

  it(`a tool-posted card carries its message id for the ?card= deep link`, () => {
    messages = [
      {
        ...base,
        id: `m-plan`,
        authorKind: `agent`,
        authorId: null,
        body: ``,
        consumedAt: null,
        card: { kind: `plan`, plan: `1. Lift the partition`, questions: [] },
      },
    ]
    snapshot = snap({ feed: [{ id: 1, seq: 1, kind: `narration`, text: `On it.` }] })
    render(<AgentSessionView session={session} composer={<div />} />)
    expect(screen.getByTestId(`session-tool-card`).getAttribute(`data-card-id`)).toBe(`m-plan`)
  })
})

describe(`AgentSessionView relay ask (S-3)`, () => {
  it(`the engine's waiting question stays inline, undocked, and its hotkeys answer it through the relay`, () => {
    snapshot = snap({
      turnState: `running`,
      feed: [
        {
          id: 1,
          seq: 1,
          kind: `question`,
          text: `Which env?`,
          questionId: `q1`,
          options: [
            { label: `prod`, key: `1` },
            { label: `staging`, key: `2` },
          ],
        },
      ] as unknown as SteerSessionSnapshot[`feed`],
    })
    render(<AgentSessionView session={session} composer={<div data-testid="composer" />} />)
    expect(screen.queryByTestId(`dock-card`)).toBeNull()
    expect(screen.getByText(`Which env?`)).toBeTruthy()
    fireEvent.keyDown(window, { key: `2` })
    expect(relayAnswer).toHaveBeenCalledTimes(1)
    expect(relayAnswer.mock.calls[0]![2]).toEqual([`staging`])
    expect(answerMutate).not.toHaveBeenCalled()
  })
})
