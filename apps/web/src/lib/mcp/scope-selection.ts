import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, inArray } from "drizzle-orm"
import { boards } from "@/db/schema"
import { boardVisible } from "@/lib/board-visibility"
import type { DbReader } from "@/lib/mcp-oauth/oauth-client"

// ONE team/board selection, the shape the MCP consent screen grants and
// (FEED-76) a personal API key is minted with. Both writers validate it the
// same way: a selection never exceeds the caller's membership, a board in
// the trash/archive is dropped, and a selection that reaches nothing is
// refused rather than stored as a silent no-access credential.

export const scopeSelectionInput = z.object({
  allTeams: z.boolean().default(false),
  teamIds: z.array(z.string().uuid()).max(500).default([]),
  boardIds: z.array(z.string().uuid()).max(2000).default([]),
})

export type ScopeSelectionInput = z.infer<typeof scopeSelectionInput>

export interface ScopeSelection {
  allTeams: boolean
  teamIds: string[]
  boardIds: string[]
}

export const EMPTY_SELECTION_MESSAGE = `Select at least one team or board, or deny access.`
export const UNREACHABLE_SELECTION_MESSAGE = `None of the selected teams/boards are accessible to your account.`

/** Refuse a selection that names nothing — BEFORE any read, so a consent
 *  code or a key mint is never consumed by an empty pick. */
export function assertScopeSelectionNonEmpty(input: ScopeSelectionInput) {
  if (
    !input.allTeams &&
    input.teamIds.length === 0 &&
    input.boardIds.length === 0
  ) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: EMPTY_SELECTION_MESSAGE })
  }
}

/**
 * Clamp a selection to `memberTeamIds` (the caller's memberships, resolved
 * by the caller) and to visible boards inside them. `allTeams` clears both
 * lists; otherwise a clamped-away selection rejects BAD_REQUEST.
 */
export async function clampScopeSelection(
  db: DbReader,
  memberTeamIds: ReadonlySet<string>,
  input: ScopeSelectionInput
): Promise<ScopeSelection> {
  assertScopeSelectionNonEmpty(input)
  if (input.allTeams) return { allTeams: true, teamIds: [], boardIds: [] }
  const teamIds = input.teamIds.filter((id) => memberTeamIds.has(id))
  let boardIds: string[] = []
  if (input.boardIds.length > 0) {
    const rows = await db
      .select({ id: boards.id, teamId: boards.teamId })
      .from(boards)
      .where(and(inArray(boards.id, input.boardIds), boardVisible()))
    boardIds = rows
      .filter((row) => memberTeamIds.has(row.teamId))
      .map((row) => row.id)
  }
  if (teamIds.length === 0 && boardIds.length === 0) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: UNREACHABLE_SELECTION_MESSAGE,
    })
  }
  return { allTeams: false, teamIds, boardIds }
}
