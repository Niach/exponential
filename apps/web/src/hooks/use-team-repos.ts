// EXP-822: the team's connected repositories over tRPC. `repositories` is
// server-only (never a shape), so this is the `use-mcp-servers` pattern:
// fetch once per team on mount, null until the first answer. `repositories.list`
// also heals default branches on every read, so callers get the branch the
// launcher will actually cut from.
import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import type { ChatRepoOption } from "@/lib/chat-repo"

/** null until the first fetch answers (and on a failed one — a chat still
 * starts without a repository, so a broken list is never fatal). */
export function useTeamRepos(
  teamId: string | undefined,
  enabled = true
): ChatRepoOption[] | null {
  const [repos, setRepos] = useState<ChatRepoOption[] | null>(null)

  useEffect(() => {
    if (!teamId || !enabled) {
      setRepos(null)
      return
    }
    let active = true
    trpc.repositories.list
      .query({ teamId })
      .then((rows) => {
        if (!active) return
        setRepos(rows.map((row) => ({ id: row.id, fullName: row.fullName })))
      })
      .catch(() => {})
    return () => {
      active = false
    }
  }, [teamId, enabled])

  return repos
}
