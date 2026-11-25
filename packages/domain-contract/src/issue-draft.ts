// #EXP-2125: the New issue page's board picker over every member team's
// boards, and what a pick does to the draft: the pure reference of
// `fixtures/issue-draft.json` `boards` + `picks` (its `_comment` is the spec);
// desktop `domain::issue_draft`, iOS `IssueDraftPage`, Android `IssueDraftPage`.

import { sidebarTeams } from "./sidebar-tree"

export interface IssueDraftBoards<Board> {
  boards: Board[]
  /** The rows say their team (the list spans more than one). */
  showTeam: boolean
}

/** The boards the picker offers, in its order. */
export function issueDraftBoards<Board extends { id: string; teamId: string; sortOrder: number | null }>(input: {
  teams: readonly { id: string; name: string; kind: string }[]
  boards: readonly Board[]
  /** Hold the list to one team (a chat's, a parent's, a share's). */
  only?: string | null
}): IssueDraftBoards<Board> {
  const bySort = (a: Board, b: Board) =>
    (a.sortOrder ?? Infinity) !== (b.sortOrder ?? Infinity)
      ? (a.sortOrder ?? Infinity) - (b.sortOrder ?? Infinity)
      : a.id < b.id
        ? -1
        : a.id > b.id
          ? 1
          : 0
  const boards = sidebarTeams(input.teams)
    .filter((teamId) => !input.only || teamId === input.only)
    .flatMap((teamId) => input.boards.filter((board) => board.teamId === teamId).sort(bySort))
  return { boards, showTeam: new Set(boards.map((board) => board.teamId)).size > 1 }
}

export interface IssueDraftPickFields {
  teamId: string
  boardId: string
  statusId: string | null
  labelIds: readonly string[]
  assigneeId: string | null
  assigneeAgentId: string | null
}

/** The draft after a pick in its board picker. `soleMemberId` = the picked
 *  board's team's only member, else null. */
export function issueDraftPick<Draft extends IssueDraftPickFields>(
  draft: Draft,
  board: { id: string; teamId: string },
  soleMemberId: string | null
): Draft {
  if (board.teamId === draft.teamId) return { ...draft, boardId: board.id }
  return {
    ...draft,
    teamId: board.teamId,
    boardId: board.id,
    statusId: null,
    labelIds: [],
    assigneeId: soleMemberId,
    assigneeAgentId: null,
  }
}
