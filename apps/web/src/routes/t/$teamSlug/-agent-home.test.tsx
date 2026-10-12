// Review #27: the Agent page's HOME column against composer-home.json: the
// greeting is `homeGreeting` of the local wall clock (a fixture case
// replayed through the page), the composer asks the fixture's placeholder,
// nothing renders beneath the options line, and a board-less team's Getting
// started block sits `composerToGettingStarted` px below it. Lives under a
// `-` prefix so the route generator ignores it.
import { render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/composer-home.json"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import type { RemoteStart } from "@/hooks/use-remote-start"
import type { Team } from "@/db/schema"

const mocks = vi.hoisted(() => ({
  boards: [] as unknown[],
  model: null as unknown,
}))

vi.mock(`@tanstack/react-router`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  createFileRoute: () => (options: Record<string, unknown>) => options,
  redirect: vi.fn(),
  useNavigate: () => vi.fn(),
}))
// The composer's own synced queries (membership, readiness) read nothing.
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      const chain = { where: () => chain }
      const result = build({ from: () => chain })
      return { data: result === undefined ? undefined : [] }
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
vi.mock(`@/components/getting-started/getting-started-section`, () => ({
  GettingStartedSection: () => <div data-testid="getting-started" />,
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

beforeEach(() => {
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

  it(`renders nothing beneath the composer's options line`, () => {
    const { container } = renderHome()
    const last = container.lastElementChild
    expect(last?.contains(screen.getByPlaceholderText(fixture.placeholder))).toBe(true)
    expect(screen.queryByTestId(`getting-started`)).toBeNull()
  })

  it(`puts Getting started composerToGettingStarted px below a board-less team's composer`, () => {
    mocks.boards = []
    const { container } = renderHome()
    const block = screen.getByTestId(`getting-started`)
    expect(container.lastElementChild).toBe(block.parentElement)
    expect(block.parentElement?.style.marginTop).toBe(
      `${fixture.geometry.composerToGettingStarted}px`
    )
  })
})
