// #EXP-2082: the Assistant's chat lists follow the composer's Teams pick at
// once: removing the chip (pick = null) widens them to every synced team
// before the next send; no draft = the root's stored pick.
import { renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

const teams = [
  { id: `t-a`, name: `Acme` },
  { id: `t-b`, name: `Beta` },
  { id: `t-c`, name: `Cedar` },
]
vi.mock(`@/lib/collections`, () => ({ teamCollection: { id: `teams`, toArray: [] }, sessionCollection: { id: `sessions` }, issueCollection: {}, boardCollection: {} }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: { from: (source: Record<string, { id: string }>) => { where: () => unknown } }) => unknown) => {
      let source = ``
      build({ from: (tables) => ((source = Object.values(tables)[0]!.id), { where: () => undefined }) })
      return { data: source === `teams` ? teams : [] }
    },
  }
})

const { useSessionChatTeams } = await import(`@/hooks/use-chat-teams`)

describe(`useSessionChatTeams (#EXP-2082)`, () => {
  const root = { id: `r`, rootId: `r`, teamId: `p-me`, scopeTeamIds: [`t-b`] }

  it(`reads the root's stored pick without a draft`, () => {
    const { result } = renderHook(() => useSessionChatTeams(root))
    expect(result.current.teamIds).toEqual([`t-b`])
  })

  it(`reads the composer's draft at once: the chip removed = every synced team`, () => {
    const { result, rerender } = renderHook(({ draft }) => useSessionChatTeams(root, null, draft), {
      initialProps: { draft: [`t-b`] as string[] | null },
    })
    expect(result.current.teamIds).toEqual([`t-b`])
    rerender({ draft: null })
    expect(result.current.teamIds).toEqual([`t-a`, `t-b`, `t-c`])
    rerender({ draft: [`t-c`, `t-a`] })
    expect(result.current.teamIds).toEqual([`t-a`, `t-c`])
  })

  it(`a session in a synced team keeps its one team whatever the draft`, () => {
    const { result } = renderHook(() => useSessionChatTeams({ id: `s`, teamId: `t-c` }, null, null))
    expect(result.current.teamIds).toEqual([`t-c`])
  })
})
