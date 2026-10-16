import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SteerSessionSnapshot } from "@/lib/steer-session-store"
import type { SessionRow } from "@/lib/session-tree"

// EXP-1324 (+ EXP-1321): the raw session view paints its transcript as Trace
// lines (tools, groups, reasoning, subagents, failed tests) and its bottom
// strip as THE Queue. The store is stubbed: this pins the RENDER of a feed.

let snapshot: SteerSessionSnapshot

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
      kick: () => {},
      reconnect: () => {},
      loadEarlier: () => {},
      sendMessage: () => true,
      answerQuestion: () => {},
      unqueue: () => {},
    }),
  }
})
vi.mock(`@/hooks/use-team-data`, () => ({ useTeamUsers: () => ({ users: [] }) }))
vi.mock(`@/hooks/use-session-device`, () => ({
  useSessionDevice: () => ({ online: true, label: `Studio Mac` }),
}))
vi.mock(`@/components/session-account-switch`, () => ({
  SessionAccountSwitchButton: () => null,
}))

const { AgentSessionView } = await import(`./agent-session`)

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
    backgroundTasks: [],
    taskList: [],
    queue: [],
    workflows: new Map(),
    runningWorkflow: null,
    answerStates: {},
    connected: true,
    canLoadEarlier: false,
    ...over,
  }
}

const GRADLE_FAIL = `StartupBenchmark > coldStart FAILED\n\n12 tests completed, 1 failed`

beforeEach(() => {
  snapshot = snap({})
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
  })
})
