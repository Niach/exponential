import { createContext, useContext, useMemo } from "react"
import type { User } from "@/db/schema"
import { displayUserName } from "@/lib/user-display"
import { useTeamUsers } from "@/hooks/use-team-data"

// Team-scoped `@email` mention resolution, mounted once in the team
// layout (beside IssueRefProvider). Powers the name-pill rendering and the
// @-autocomplete in the TipTap markdown editors — built on useTeamUsers,
// which intersects the team member rows with the synced users shape, so
// only members the viewer may actually see are offered or rendered as
// pills.

export interface MentionContextValue {
  /** Resolve a mention email (case-insensitive) to a visible member, or null. */
  resolve: (email: string) => { name: string } | null
  /** Search visible members by name/email substring; empty query = all. */
  search: (query: string, opts?: { limit?: number }) => User[]
}

const MentionContext = createContext<MentionContextValue | null>(null)

export function useMentions(): MentionContextValue | null {
  return useContext(MentionContext)
}

export function MentionProvider({
  teamId,
  children,
}: {
  teamId: string | undefined
  children: React.ReactNode
}) {
  const { users } = useTeamUsers(teamId)

  const sorted = useMemo(
    () => [...users].sort((a, b) => a.name.localeCompare(b.name)),
    [users]
  )

  const byEmail = useMemo(
    () => new Map(sorted.map((user) => [user.email.toLowerCase(), user])),
    [sorted]
  )

  const value = useMemo<MentionContextValue>(
    () => ({
      resolve: (email) => {
        const user = byEmail.get(email.toLowerCase())
        return user ? { name: displayUserName(user, user.id) } : null
      },
      search: (query, opts) => {
        const q = query.trim().toLowerCase()
        const limit = opts?.limit ?? 6
        const matches: User[] = []
        for (const user of sorted) {
          if (
            q &&
            !user.name.toLowerCase().includes(q) &&
            !user.email.toLowerCase().includes(q)
          ) {
            continue
          }
          matches.push(user)
          if (matches.length >= limit) break
        }
        return matches
      },
    }),
    [byEmail, sorted]
  )

  return (
    <MentionContext.Provider value={value}>{children}</MentionContext.Provider>
  )
}
