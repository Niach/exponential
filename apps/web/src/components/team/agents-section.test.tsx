// EXP-1293: the agent settings page's create mode — validation is the
// contract's `validateAgentForm` (inline FieldError, nothing sent), and a
// valid form is ONE `agents.create` (NEW-2: agents are definition rows).
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Team } from "@/db/schema"

const create = vi.hoisted(() => vi.fn())

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    agents: {
      create: { mutate: create },
      get: { query: vi.fn() },
      update: { mutate: vi.fn() },
      delete: { mutate: vi.fn() },
    },
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
