// EXP-1293: the agent settings page's create mode — validation is the
// contract's `validateAgentForm` (inline FieldError, nothing sent), and a
// valid form is ONE `agents.create` (NEW-2: agents are definition rows).
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SyncedAgent } from "@/components/team/agents-section"
import type { Team } from "@/db/schema"

const create = vi.hoisted(() => vi.fn())
const update = vi.hoisted(() => vi.fn())
const mcpList = vi.hoisted(() => vi.fn())

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    agents: {
      create: { mutate: create },
      get: { query: vi.fn(() => Promise.resolve({ instructions: `` })) },
      update: { mutate: update },
      delete: { mutate: vi.fn() },
    },
    mcpServers: { list: { query: mcpList } },
  },
}))
vi.mock(`@/lib/collections`, () => ({
  deviceCollection: {},
  userCollection: {},
  agentCollection: { utils: { awaitTxId: () => Promise.resolve() } },
}))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: { user: { id: `me` } } }),
}))
vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => vi.fn() }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})

const { AgentSettingsPage } = await import(`@/components/team/agents-section`)

const team = { id: `11111111-1111-4111-8111-111111111111`, slug: `acme` } as Team

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

beforeEach(() => {
  create.mockReset()
  update.mockReset()
  mcpList.mockReset()
  mcpList.mockResolvedValue([])
})

describe(`AgentSettingsPage (create)`, () => {
  it(`shows the contract's errors inline and sends nothing`, async () => {
    render(<AgentSettingsPage team={team} agent={null} isOwner />)
    expect(screen.getByTestId(`agent-settings-page`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`agent-create`))
    await waitFor(() => expect(screen.getByRole(`alert`).textContent).toBe(`Name the agent`))
    expect(create).not.toHaveBeenCalled()
  })

  it(`hides the create button from a member`, () => {
    render(<AgentSettingsPage team={team} agent={null} isOwner={false} />)
    expect(screen.queryByTestId(`agent-create`)).toBeNull()
    expect(screen.queryByTestId(`agent-add-trigger`)).toBeNull()
  })
})

// #EXP-1793 (agent-settings.json `mcpRow`): the MCP servers row under
// Skills: the picked names (team list order), None when empty, shown while
// the team has a server or the agent names one; an owner's pick saves at once.
describe(`AgentSettingsPage MCP servers row (#EXP-1793)`, () => {
  const connection = { status: `not_needed`, expiresAt: null, error: null }
  const SERVERS = [
    { id: `m1`, name: `GlitchTip`, url: `https://glitchtip.example/mcp`, command: null, connection },
    { id: `m2`, name: `Linear`, url: `https://mcp.linear.app/mcp`, command: null, connection },
    { id: `m3`, name: `Sentry`, url: `https://mcp.sentry.dev/mcp`, command: null, connection },
  ]
  const agent = (mcpServerIds: string[]) =>
    ({
      id: `ag-1`,
      teamId: team.id,
      name: `Triage`,
      role: `agent`,
      icon: null,
      color: null,
      defaults: {},
      triggers: [],
      skillIds: [],
      mcpServerIds,
    }) as unknown as SyncedAgent

  // The row = the Picker's trigger (its test id rides the open rows).
  const findRow = async () => (await screen.findByText(`MCP servers`)).closest(`button`)!
  const queryRow = () => screen.queryByText(`MCP servers`)

  it(`reads the picked names in the team's order`, async () => {
    mcpList.mockResolvedValue(SERVERS)
    render(<AgentSettingsPage team={team} agent={agent([`m3`, `m1`, `m2`])} isOwner={false} />)
    const row = await findRow()
    expect(row.textContent).toContain(`GlitchTip, Linear +1`)
  })

  it(`reads None with nothing picked; no servers and no pick = no row`, async () => {
    mcpList.mockResolvedValue(SERVERS)
    const { unmount } = render(<AgentSettingsPage team={team} agent={agent([])} isOwner />)
    expect((await findRow()).textContent).toContain(`None`)
    unmount()
    mcpList.mockResolvedValue([])
    render(<AgentSettingsPage team={team} agent={agent([])} isOwner />)
    await waitFor(() => expect(mcpList).toHaveBeenCalled())
    expect(queryRow()).toBeNull()
  })

  it(`an owner's pick saves with agents.update {mcpServerIds}`, async () => {
    mcpList.mockResolvedValue(SERVERS)
    update.mockResolvedValue({ txid: 1 })
    render(<AgentSettingsPage team={team} agent={agent([`m1`])} isOwner />)
    fireEvent.click(await findRow())
    fireEvent.click(await within(await screen.findByTestId(`agent-mcp-servers`)).findByText(`Linear`))
    await waitFor(() =>
      expect(update).toHaveBeenCalledWith({ id: `ag-1`, mcpServerIds: [`m1`, `m2`] }, expect.anything())
    )
  })
})
