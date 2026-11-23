// After the Assistant move (#EXP-1984) the member's Assistant lives in their
// personal team, which the `teams` shape never carries. The team-less
// Settings › Assistant page must not wait for that row (it showed skeletons
// for ever), and its device list is the member's own machines whatever teams
// they are shared with (never `deviceUsableInTeam` against the personal team).
import { render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Team } from "@/db/schema"

const PERSONAL = `99999999-9999-4999-8999-999999999999`
const state = vi.hoisted(() => ({
  assistantId: `as-1` as string | null | undefined,
  agent: null as Record<string, unknown> | null,
  home: null as Record<string, unknown> | null,
  page: vi.fn((_props: Record<string, unknown>) => null),
  deviceRows: [] as Array<Record<string, unknown>>,
}))

vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: { user: { id: `me` } } }) }))
vi.mock(`@/hooks/use-ensure-assistant`, () => ({ useEnsureAssistant: () => state.assistantId }))
vi.mock(`@/hooks/use-agent-row`, () => ({ useAgentRow: () => ({ agent: state.agent }) }))
vi.mock(`@/hooks/use-team-data`, () => ({ useTeamById: () => state.home }))
vi.mock(`@/hooks/use-team-permissions`, () => ({ useTeamPermissions: () => ({ isOwner: false }) }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/lib/collections`, () => ({ deviceCollection: { id: `devices` }, userCollection: { id: `users` } }))
vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => vi.fn(), Link: () => null }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: { from: (source: Record<string, { id: string }>) => unknown }) => unknown) => {
      let source = ``
      build({ from: (tables) => ((source = Object.values(tables)[0]!.id), undefined) })
      return { data: source === `devices` ? state.deviceRows : [] }
    },
  }
})

describe(`Settings › Assistant, the team-less page`, () => {
  beforeEach(() => {
    state.assistantId = `as-1`
    state.home = null
    state.agent = { id: `as-1`, teamId: PERSONAL, role: `assistant`, ownerId: `me` }
    state.page.mockClear()
    vi.resetModules()
  })

  const mount = async (team?: Team) => {
    vi.doMock(`@/components/team/agents-section`, () => ({
      AgentSettingsPage: (props: Record<string, unknown>) => {
        state.page(props)
        return <div data-testid="agent-settings-page" />
      },
    }))
    const { AssistantSettingsBody } = await import(`@/components/settings/personal-settings`)
    return render(team ? <AssistantSettingsBody team={team} isOwner /> : <AssistantSettingsBody />)
  }

  it(`renders the page for an Assistant in the personal team, which has no synced teams row`, async () => {
    await mount()
    expect(screen.getByTestId(`agent-settings-page`)).toBeTruthy()
    expect(state.page.mock.calls.at(-1)![0]).toMatchObject({ team: { id: PERSONAL }, agent: { id: `as-1` } })
  })

  it(`uses the synced team when the Assistant's team is one (old per-team data)`, async () => {
    state.home = { id: `t-a`, slug: `acme`, name: `Acme` }
    state.agent = { id: `as-1`, teamId: `t-a`, role: `assistant`, ownerId: `me` }
    await mount()
    expect(state.page.mock.calls.at(-1)![0]).toMatchObject({ team: { id: `t-a`, slug: `acme` } })
  })

  it(`a phone's team page keeps its current team; no Assistant row yet = skeletons, no device = the empty state`, async () => {
    const team = { id: `t-a`, slug: `acme`, name: `Acme` } as Team
    await mount(team)
    expect(state.page.mock.calls.at(-1)![0]).toMatchObject({ team: { id: `t-a` }, isOwner: true })
    state.agent = null
    const waiting = await mount()
    expect(waiting.queryByTestId(`agent-settings-page`)).toBeNull()
  })
})

describe(`useAgentDevices for an Assistant`, () => {
  it(`lists the member's own machines whatever teams they are shared with; a team's list hides one shared elsewhere`, async () => {
    vi.resetModules()
    vi.doUnmock(`@/components/team/agents-section`)
    const now = new Date()
    const row = (deviceId: string, sharedTeamIds: string[], userId = `me`) => ({
      id: `row-${deviceId}`,
      userId,
      deviceId,
      label: deviceId,
      kind: `desktop`,
      lastSeenAt: now,
      sharedTeamIds,
      agentProfiles: [],
    })
    state.deviceRows = [row(`mine`, []), row(`shared-a`, [`t-a`]), row(`theirs`, [`t-a`], `mate`)]
    const { useAgentDevices } = await import(`@/components/team/agents-section`)
    const seen: Record<string, string[]> = {}
    function Probe({ name, teamId, own }: { name: string; teamId: string; own: boolean }) {
      seen[name] = (useAgentDevices(teamId, own) ?? []).map((device) => device.deviceId).sort()
      return null
    }
    render(
      <>
        <Probe name="assistant" teamId={PERSONAL} own />
        <Probe name="personal-as-team" teamId={PERSONAL} own={false} />
      </>
    )
    expect(seen.assistant).toEqual([`mine`, `shared-a`])
    // What the page did before: the machine shared with a team went missing.
    expect(seen[`personal-as-team`]).toEqual([`mine`])
  })
})
