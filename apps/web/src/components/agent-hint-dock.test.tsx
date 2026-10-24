import { fireEvent, render, renderHook, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { AGENT_HINT_BANNER } from "@exp/domain-contract/chats"
import { dockRows } from "@exp/domain-contract/dock"
import type { SessionRow } from "@/lib/session-tree"

// #EXP-1383 (chats.json `agentHint`): an agent chat's ROOT that runs on
// instructions older than the agent's carries ONE more Dock banner, whose
// action sends the agent menu's `/new` into that conversation.

let agent: { id: string; instructionsUpdatedAt: Date | null } | null = null
const sendMutate = vi.fn(async (_input: unknown, _opts?: unknown) => ({ txid: 1 }))

vi.mock(`@/hooks/use-agent-row`, () => ({
  useAgentRow: (id: string | null) => ({ agent: id ? agent : null, isReady: true }),
}))
vi.mock(`@/hooks/use-team-data`, () => ({ useTeamUsers: () => ({ users: [] }) }))
vi.mock(`@/hooks/use-session-tree`, () => ({ useSessionMessages: () => [] }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: { sessions: { send: { mutate: sendMutate } } } }))
vi.mock(`@/lib/collections`, () => ({
  messageCollection: { utils: { awaitTxId: async () => undefined } },
}))
vi.mock(`@/components/session-account-switch`, () => ({
  SessionAccountSwitchButton: () => null,
}))

const { useAgentHintDock } = await import(`./agent-session`)

function root(over: Partial<SessionRow> = {}): SessionRow {
  return {
    id: `s1`,
    teamId: `t1`,
    parentId: null,
    agentId: `g1`,
    agentInstructionsAt: new Date(`2026-10-24T10:00:00Z`),
    ...over,
  } as unknown as SessionRow
}

beforeEach(() => {
  agent = { id: `g1`, instructionsUpdatedAt: new Date(`2026-10-24T11:00:00Z`) }
  sendMutate.mockClear()
})

describe(`useAgentHintDock`, () => {
  it(`raises the banner while the session predates the instructions`, () => {
    const { result } = renderHook(() => useAgentHintDock(root()))
    expect(result.current.source).toMatchObject({
      kind: `banner`,
      id: AGENT_HINT_BANNER.id,
      label: AGENT_HINT_BANNER.label,
      text: AGENT_HINT_BANNER.text,
      action: AGENT_HINT_BANNER.action,
    })
  })

  it(`stays quiet once the brief is current, or off an agent chat's root`, () => {
    const current = root({ agentInstructionsAt: new Date(`2026-10-24T12:00:00Z`) })
    expect(renderHook(() => useAgentHintDock(current)).result.current.source).toBeNull()
    const never = root({ agentInstructionsAt: null })
    expect(renderHook(() => useAgentHintDock(never)).result.current.source).toBeNull()
    const child = root({ parentId: `s0` })
    expect(renderHook(() => useAgentHintDock(child)).result.current.source).toBeNull()
    expect(renderHook(() => useAgentHintDock(null)).result.current.source).toBeNull()
  })

  it(`its action sends /new into the conversation, like New conversation`, () => {
    const { result } = renderHook(() => useAgentHintDock(root()))
    const row = dockRows([result.current.source!])[0]!
    render(<>{result.current.renderAction(row)}</>)
    fireEvent.click(screen.getByText(AGENT_HINT_BANNER.action))
    expect(sendMutate).toHaveBeenCalledWith(
      { teamId: `t1`, sessionId: `s1`, text: `/new` },
      expect.anything()
    )
  })
})
