// Review #27: the Agent page's HOME column against composer-home.json: the
// greeting is `homeGreeting` of the local wall clock (a fixture case
// replayed through the page), the composer asks the fixture's placeholder,
// and RECENT is a band of at most `recent.count` rows, no band at all
// without sessions. EXP-1284: lists show THREADS — a Recent row is a thread
// (a resume succession sits in ONE thread, so it is one row). Lives under a
// `-` prefix so the route generator ignores it.
import { render, screen, within } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/composer-home.json"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import type { RemoteStart } from "@/hooks/use-remote-start"
import type { Team } from "@/db/schema"
import { buildThreadView } from "@/lib/thread-view"

const mocks = vi.hoisted(() => ({
  rows: {} as Record<string, unknown[]>,
  recent: [] as unknown[],
  boards: [] as unknown[],
  model: null as unknown,
}))

vi.mock(`@tanstack/react-router`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  createFileRoute: () => (options: Record<string, unknown>) => options,
  redirect: vi.fn(),
  useNavigate: () => vi.fn(),
  useParams: () => ({ teamSlug: `acme` }),
}))
// The synced queries by ALIAS: the Recent band reads `s` (sessions).
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      let alias = ``
      const chain = { where: () => chain }
      const query = {
        from: (source: Record<string, unknown>) => {
          alias = Object.keys(source)[0] ?? ``
          return chain
        },
      }
      const result = build(query)
      if (result === undefined) return { data: undefined }
      return { data: mocks.rows[alias] ?? [] }
    },
  }
})
vi.mock(`@/lib/collections`, () => ({
  actionCollection: {},
  boardCollection: {},
  codingSessionCollection: {},
  deviceCollection: {},
  deviceWorktreeCollection: {},
  issueCollection: {},
  issueRelationCollection: {},
  teamMemberCollection: {},
  userCollection: {},
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBoards: () => mocks.boards,
  useTeamBoardsWithReady: () => ({ boards: mocks.boards, boardsReady: true }),
  useTeamBySlug: () => ({ id: `t1`, slug: `acme` }),
  useTeamById: () => ({ id: `t1`, slug: `acme` }),
  useTeamUsers: () => ({ users: [] }),
}))
// The row's own chrome has its own tests; here a row is its session.
vi.mock(`@/hooks/use-agents-data`, () => ({
  useAgentsData: () => ({}),
  useSessionListRows: (_teamId: string, sessions: { id: string }[]) =>
    sessions.map((session) => ({ session })),
}))
vi.mock(`@/components/session-tree`, () => ({
  sessionDeviceRow: () => undefined,
  useDeviceRows: () => [],
  sessionRowFacts: (row: { session: { id: string } }) => ({
    agent: `claude`,
    markState: `idle`,
    identifier: null,
    title: row.session.id,
    caption: ``,
    captionTone: undefined,
    deviceName: null,
    paused: false,
  }),
}))
vi.mock(`@/hooks/use-open-session`, () => ({ useOpenSession: () => vi.fn() }))
// EXP-1284: the Recent band reads the THREAD list.
vi.mock(`@/hooks/use-threads`, () => ({
  useThreadList: () => ({ running: [], recent: mocks.recent, isReady: true, now: new Date() }),
  useThreadBundle: () => ({ bundle: null, isReady: true, now: new Date() }),
  threadIdOfRun: () => null,
  waitForThreadOfRun: async () => null,
}))
vi.mock(`@/components/getting-started/getting-started-section`, () => ({
  GettingStartedSection: () => null,
}))
// The composer's MODEL has its own suite; the card renders for real.
vi.mock(`@/hooks/use-launch-composer`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLaunchComposer: () => mocks.model,
}))
vi.mock(`@/hooks/use-team-issue-graph`, () => ({
  useTeamIssueGraph: () => ({ relations: [], issues: [], counts: new Map() }),
}))
vi.mock(`@/hooks/use-issue-search-results`, () => ({
  useIssueSearchResults: ({ rows }: { rows: unknown[] }) => ({ results: rows }),
}))
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => false,
}))

import { HomeColumn } from "@/routes/t/$teamSlug/agent"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never

const device = {
  deviceId: `dev-1`,
  deviceLabel: `buildbox`,
  agents: [`claude`],
  online: true,
}

function fakeModel(): LaunchComposerModel {
  const launch = {
    device,
    deviceId: device.deviceId,
    setDeviceId: vi.fn(),
    requestDevice: vi.fn(),
    unavailableRequestId: null,
    availableAgents: [`claude`],
    agent: `claude`,
    agentNotReady: false,
    switchAgent: vi.fn(),
    model: `opus`,
    setModel: vi.fn(),
    subagentModel: ``,
    setSubagentModel: vi.fn(),
    effortValue: `cli-default`,
    setEffortValue: vi.fn(),
    ultracode: false,
    setUltracode: vi.fn(),
    planMode: false,
    setPlanMode: vi.fn(),
    mcpServerIds: [],
    setMcpServerIds: vi.fn(),
    toggleMcpServer: vi.fn(),
    computerUseAvailable: false,
    computerUse: false,
    setComputerUse: vi.fn(),
    accountOptions: [],
    accountKey: undefined,
    setAccountKey: vi.fn(),
    buildOptions: () => ({ agent: `claude`, model: `opus`, effort: ``, ultracode: false, planMode: false }),
  }
  return {
    teamId: `t1`,
    subject: null,
    selectedAction: null,
    actions: [],
    eligibleIssues: [],
    checkedIssues: [],
    toggleIssue: vi.fn(),
    pickAction: vi.fn(),
    clearAction: vi.fn(),
    setInput: vi.fn(),
    seedPrIssueId: undefined,
    fixConflicts: null,
    text: ``,
    setText: vi.fn(),
    images: [],
    addFiles: vi.fn(() => 0),
    removeImage: vi.fn(),
    repos: [],
    repoId: ``,
    setRepoId: vi.fn(),
    repoOptions: [],
    resumeCandidate: null,
    resume: true,
    setResume: vi.fn(),
    resumeActive: false,
    blockedStart: [],
    blockedOpen: false,
    closeBlockedStart: vi.fn(),
    startAnyway: vi.fn(),
    blockedStack: { plan: null, reason: null, ident: null, first: null },
    startStacked: vi.fn(),
    launch,
    candidateDevices: [device],
    deviceRequestNote: null,
    mcpServers: null,
    mcpConnectHref: () => null,
    submitLabel: `Start chat`,
    blocked: true,
    overCap: false,
    spansRepos: false,
    spansBranches: false,
    costHint: false,
    busy: false,
    sentTo: null,
    starting: null,
    submit: vi.fn(),
  } as unknown as LaunchComposerModel
}

const team = { id: `t1`, slug: `acme`, name: `Acme` } as Team

function renderHome() {
  return render(
    <HomeColumn
      team={team}
      currentUserId="me"
      userName={fixture.cases[0]!.user}
      isMember
      steerEnabled
      remote={{} as RemoteStart}
      users={[]}
      seed={null}
      onSeedConsumed={() => {}}
      onActionChange={() => {}}
    />
  )
}

/** A Recent thread holding `runs` (oldest first), named by its title. */
function thread(id: string, title: string, runs: { id: string; resumedFromId?: string }[]) {
  const at = `2026-10-12T01:00:00.000Z`
  const view = buildThreadView({
    thread: { id, title, results: null, updatedAt: at },
    items: runs.map((run, index) => ({
      id: `i-${run.id}`,
      kind: `run` as const,
      issueId: null,
      sessionId: run.id,
      source: `user` as const,
      addedBySessionId: null,
      sortOrder: index + 1,
      createdAt: at,
    })),
    issues: [],
    runs: runs.map((run, index) => ({
      id: run.id,
      name: `Chat`,
      userId: `me`,
      deviceLabel: null,
      status: `ended`,
      needsInput: false,
      agentBusy: false,
      issueId: null,
      parentSessionId: null,
      resumedFromId: run.resumedFromId ?? null,
      startedAt: `2026-10-12T0${index + 1}:00:00.000Z`,
      endedAt: null,
    })),
  })
  return { thread: { id, teamId: `t1` }, runs, issues: [], view, liveRun: null }
}

beforeEach(() => {
  mocks.rows = {}
  mocks.recent = []
  mocks.boards = [{ id: `b1` }]
  mocks.model = fakeModel()
  // The greeting reads the LOCAL wall clock: pin it to the first case.
  vi.useFakeTimers({ toFake: [`Date`] })
  vi.setSystemTime(new Date(fixture.cases[0]!.local))
})

afterEach(() => {
  vi.useRealTimers()
})

describe(`the Agent home column (composer-home.json)`, () => {
  it(`greets with homeGreeting of the local clock and asks the fixture's question`, () => {
    renderHome()
    expect(screen.getByTestId(fixture.testIds.greeting).textContent).toBe(
      fixture.cases[0]!.expected
    )
    expect(screen.getByTestId(fixture.testIds.mark)).toBeTruthy()
    expect(screen.getByPlaceholderText(fixture.placeholder)).toBeTruthy()
  })

  it(`draws no Recent band without sessions`, () => {
    renderHome()
    expect(screen.queryByTestId(fixture.testIds.recent)).toBeNull()
  })

  it(`draws at most recent.count rows under the band label`, () => {
    // The thread list already sorts by activity (`partitionThreadList`).
    mocks.recent = [5, 4, 3, 2, 1].map((n) => thread(`t-${n}`, `thread-${n}`, [{ id: `run-${n}` }]))
    renderHome()
    const band = screen.getByTestId(fixture.testIds.recent)
    expect(within(band).getByText(fixture.recent.label)).toBeTruthy()
    const rows = within(band).getAllByTestId(fixture.testIds.recentRow)
    expect(rows.length).toBe(fixture.recent.count)
    expect(rows[0]!.textContent).toContain(`thread-5`)
  })

  it(`folds a resume succession into one row (its thread)`, () => {
    mocks.recent = [
      thread(`t-1`, `Offline sync`, [{ id: `first` }, { id: `resumed`, resumedFromId: `first` }]),
    ]
    renderHome()
    const rows = within(screen.getByTestId(fixture.testIds.recent)).getAllByTestId(
      fixture.testIds.recentRow
    )
    expect(rows.length).toBe(1)
    expect(rows[0]!.textContent).toContain(`Offline sync`)
    expect(rows[0]!.textContent).toContain(`2 sessions`)
  })
})
