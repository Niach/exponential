import { useEffect, useState } from "react"
import { useMcpActions } from "./actions"
import { assigneeBoardIds, type IssueListRow, type MemberRow } from "./list-issue"

// EXP-1183 — the issue rows carry only `assigneeId`; the names and avatars
// come from the members of the boards' teams: `exponential_boards_get` per
// board holding an assigned row (→ teamId), then ONE
// `exponential_members_list` per team. Optional: no host, a refused call or
// a scoped grant just leaves the avatars as initials-less placeholders.
export function useAssignees(rows: readonly IssueListRow[]): ReadonlyMap<string, MemberRow> {
  const { call } = useMcpActions()
  const [members, setMembers] = useState<ReadonlyMap<string, MemberRow>>(new Map())
  const boards = assigneeBoardIds(rows).join(`,`)
  useEffect(() => {
    if (!boards) return
    let live = true
    void (async () => {
      const teamIds = new Set<string>()
      for (const id of boards.split(`,`)) {
        const board = await call<{ teamId?: string }>(`exponential_boards_get`, { id })
        if (board.kind === `ok` && board.data?.teamId) teamIds.add(board.data.teamId)
      }
      const next = new Map<string, MemberRow>()
      for (const teamId of teamIds) {
        const result = await call<MemberRow[]>(`exponential_members_list`, {
          teamId,
          limit: 200,
        })
        if (result.kind !== `ok` || !Array.isArray(result.data)) continue
        for (const member of result.data) next.set(member.id, member)
      }
      if (live && next.size > 0) setMembers(next)
    })()
    return () => {
      live = false
    }
  }, [boards, call])
  return members
}
