// #EXP-1988 (agent-settings.json `assistant.teamPicks`): Settings › Assistant
// draws one section per team of its member (header = the team's name) with
// the skills and MCP servers rows over that team's rows; a section edits
// `assistantPickAgent`'s agent and writes only its team's picks.
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SyncedAgent } from "@/components/team/agents-section"

const update = vi.hoisted(() => vi.fn())
const state = vi.hoisted(() => ({ agents: [] as unknown[] }))

vi.mock(`@/lib/trpc-client`, () => ({ trpc: { agents: { update: { mutate: update } } } }))
vi.mock(`@/lib/collections`, () => ({ teamCollection: {} }))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: { user: { id: `me` } } }) }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [{ id: `t-a` }, { id: `t-b` }] }) }
})
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamMemberships: () => ({
    myTeams: [
      { id: `t-a`, name: `Alpha`, slug: `alpha` },
      { id: `t-b`, name: `Beta`, slug: `beta` },
    ],
  }),
}))
vi.mock(`@/hooks/use-agent-row`, () => ({ useAllAgentRows: () => state.agents }))
vi.mock(`@/hooks/use-skills`, () => ({
  useSkills: (teamId: string) => ({
    skills:
      teamId === `t-a`
        ? [{ id: `s-a1`, name: `alpha-skill`, description: ``, teamId }]
        : [
            { id: `s-b1`, name: `beta-skill`, description: ``, teamId },
            { id: `s-b2`, name: `gamma-skill`, description: ``, teamId },
          ],
    isReady: true,
  }),
}))
const connection = { status: `not_needed`, expiresAt: null, error: null }
vi.mock(`@/hooks/use-mcp-servers`, () => ({
  useMcpServers: (teamId: string) => ({
    servers:
      teamId === `t-a`
        ? [{ id: `m-a`, name: `GlitchTip`, url: `https://a.example/mcp`, command: null, connection }]
        : [{ id: `m-b`, name: `Linear`, url: `https://b.example/mcp`, command: null, connection }],
  }),
}))

const { AssistantTeamPicks } = await import(`@/components/team/assistant-team-picks`)

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const assistant = (id: string, teamId: string, skillIds: string[], mcpServerIds: string[]) =>
  ({ id, teamId, ownerId: `me`, role: `assistant`, name: `Assistant`, skillIds, mcpServerIds }) as unknown as SyncedAgent

const section = (teamId: string) => within(screen.getByTestId(`assistant-team-picks-${teamId}`))

beforeEach(() => {
  update.mockReset()
  update.mockResolvedValue({ txid: 1 })
})

describe(`AssistantTeamPicks (#EXP-1988)`, () => {
  it(`after the move: one section per team, each reading that team's picks of the one Assistant`, () => {
    const global = assistant(`as-p`, `p-me`, [`s-a1`, `s-b2`], [`m-b`])
    state.agents = [global]
    render(<AssistantTeamPicks pageAgent={global} />)
    expect(section(`t-a`).getByText(`Alpha`)).toBeTruthy()
    expect(section(`t-a`).getByText(`Skills`).closest(`button`)!.textContent).toContain(`alpha-skill`)
    expect(section(`t-a`).getByText(`MCP servers`).closest(`button`)!.textContent).toContain(`None`)
    expect(section(`t-b`).getByText(`Beta`)).toBeTruthy()
    expect(section(`t-b`).getByText(`Skills`).closest(`button`)!.textContent).toContain(`gamma-skill`)
    expect(section(`t-b`).getByText(`MCP servers`).closest(`button`)!.textContent).toContain(`Linear`)
  })

  it(`a pick writes only its team's picks, with pickTeamId`, async () => {
    const global = assistant(`as-p`, `p-me`, [`s-a1`], [`m-b`])
    state.agents = [global]
    render(<AssistantTeamPicks pageAgent={global} />)
    fireEvent.click(section(`t-a`).getByText(`MCP servers`).closest(`button`)!)
    fireEvent.click(await within(await screen.findByTestId(`agent-mcp-servers`)).findByText(`GlitchTip`))
    await waitFor(() =>
      expect(update).toHaveBeenCalledWith({ id: `as-p`, pickTeamId: `t-a`, mcpServerIds: [`m-a`] }, expect.anything())
    )
  })

  it(`old data in the store (an app kept open across the move): each team's section reads and edits that team's own Assistant`, async () => {
    const asA = assistant(`as-a`, `t-a`, [`s-a1`], [])
    const asB = assistant(`as-b`, `t-b`, [`s-b1`], [`m-b`])
    state.agents = [asA, asB]
    render(<AssistantTeamPicks pageAgent={asA} />)
    expect(section(`t-b`).getByText(`Skills`).closest(`button`)!.textContent).toContain(`beta-skill`)
    fireEvent.click(section(`t-b`).getByText(`MCP servers`).closest(`button`)!)
    fireEvent.click(await within(await screen.findByTestId(`agent-mcp-servers`)).findByText(`Linear`))
    await waitFor(() =>
      expect(update).toHaveBeenCalledWith({ id: `as-b`, pickTeamId: `t-b`, mcpServerIds: [] }, expect.anything())
    )
  })
})
