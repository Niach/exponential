import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Issue } from "@/db/schema"
import { issueCollection } from "@/lib/collections"

/**
 * How many issues share this issue's pull request (a batch PR links several),
 * read only while the confirm is open: the prompt's body names the count.
 */
export function useLinkedIssueCount(
  issueId: string | undefined,
  enabled: boolean
): number {
  const active = enabled && issueId !== undefined
  const { data: issueRows } = useLiveQuery(
    (query) =>
      active
        ? query.from({ i: issueCollection }).where(({ i }) => eq(i.id, issueId))
        : undefined,
    [active, issueId]
  )
  const prUrl = ((issueRows ?? []) as Issue[])[0]?.prUrl ?? null
  const { data: linkedRows } = useLiveQuery(
    (query) =>
      active && prUrl
        ? query.from({ i: issueCollection }).where(({ i }) => eq(i.prUrl, prUrl))
        : undefined,
    [active, prUrl]
  )
  return Math.max(1, linkedRows?.length ?? 1)
}
