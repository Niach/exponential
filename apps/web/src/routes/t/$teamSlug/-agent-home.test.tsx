// Review #27 / EXP-1293: the Agent page's HOME column against
// composer-home.json: the DAWN (FEED-100: the brand mark's top half) over the
// composer, no greeting, the fixture's placeholder, nothing beneath the
// options line, and a board-less team's Getting started block
// `composerToGettingStarted` px below it. Lives under a `-` prefix so the
// route generator ignores it.
import { render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/composer-home.json"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
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
  useParams: () => ({ teamSlug: `acme` }),
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
  promptCollection: {},
  boardCollection: {},
  deviceCollection: {},
  issueCollection: {},
  issueRelationCollection: {},
  pullRequestCollection: {},
  pullRequestIssueCollection: {},
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
    selectedPrompt: null,
    prompts: [],
    eligibleIssues: [],
    checkedIssues: [],
    toggleIssue: vi.fn(),
    pickPrompt: vi.fn(),
    clearPrompt: vi.fn(),
    text: ``,
    setText: vi.fn(),
    images: [],
    addFiles: vi.fn(() => 0),
    removeImage: vi.fn(),
    repos: [],
    repoId: ``,
    setRepoId: vi.fn(),
    repoOptions: [],
    launch,
    candidateDevices: [device],
    deviceRequestNote: null,
    mcpServers: null,
    mcpConnectHref: () => null,
    submitLabel: `Start session`,
    blocked: true,
    overCap: false,
    spansRepos: false,
    spansBranches: false,
    costHint: false,
    busy: false,
    submit: vi.fn(),
  } as unknown as LaunchComposerModel
}

const team = { id: `t1`, slug: `acme`, name: `Acme` } as Team

function renderHome() {
  return render(
    <HomeColumn
      team={team}
      isMember
      steerEnabled
      devices={[]}
      users={[]}
      seed={null}
      onSeedConsumed={() => {}}
    />
  )
}

beforeEach(() => {
  mocks.boards = [{ id: `b1` }]
  mocks.model = fakeModel()
})

describe(`the Agent home column (composer-home.json)`, () => {
  it(`draws the dawn over the composer, no greeting, and asks the fixture's question`, () => {
    renderHome()
    const dawn = screen.getByTestId(fixture.testIds.mark)
    expect(dawn.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(screen.queryByTestId(fixture.testIds.greeting)).toBeNull()
    const field = screen.getByPlaceholderText(fixture.placeholder)
    // The dawn leads the column; the composer follows it.
    expect(dawn.compareDocumentPosition(field) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(screen.queryByTestId(`recent-runs-toggle`)).toBeNull()
  })

  it(`renders nothing beneath the composer's options line`, () => {
    const { container } = renderHome()
    const column = container.firstElementChild!
    expect(
      column.lastElementChild?.contains(screen.getByPlaceholderText(fixture.placeholder))
    ).toBe(true)
    expect(screen.queryByTestId(`getting-started`)).toBeNull()
  })

  it(`puts Getting started composerToGettingStarted px below a board-less team's composer`, () => {
    mocks.boards = []
    const { container } = renderHome()
    const block = screen.getByTestId(`getting-started`)
    expect(container.firstElementChild!.lastElementChild).toBe(block.parentElement)
    expect(block.parentElement?.style.marginTop).toBe(
      `${fixture.geometry.composerToGettingStarted}px`
    )
  })
})
