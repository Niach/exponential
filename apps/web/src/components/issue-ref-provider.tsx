import { createContext, useContext, useMemo } from "react"
import { useNavigate } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { issueCollection } from "@/lib/collections"
import { useWorkspaceProjects } from "@/hooks/use-workspace-data"
import type { Issue } from "@/db/schema"
import type { IssueStatus } from "@/lib/domain"

// Workspace-scoped issue-reference resolution, mounted once in the workspace
// layout. Powers the `#IDENTIFIER` pill rendering in the markdown editors, the
// #-autocomplete in the comment composer, and the mark-as-duplicate issue
// picker — all from the already-synced issues shape (no server round-trips).
// Resolution is scoped to the current workspace's projects so a same-prefix
// identifier from another workspace never leaks in.

export interface ResolvedIssueRef {
  id: string
  identifier: string
  title: string
  status: IssueStatus
  projectSlug: string
}

export interface IssueRefContextValue {
  /** Resolve an identifier (case-insensitive) to a visible issue, or null. */
  resolve: (identifier: string) => ResolvedIssueRef | null
  /** Search visible issues by identifier/title; empty query = most recent. */
  search: (
    query: string,
    opts?: { excludeIssueIds?: string[]; limit?: number }
  ) => ResolvedIssueRef[]
  /** Navigate to an issue's full-page detail route by identifier. */
  open: (identifier: string) => void
}

const IssueRefContext = createContext<IssueRefContextValue | null>(null)

export function useIssueRefs(): IssueRefContextValue | null {
  return useContext(IssueRefContext)
}

export function IssueRefProvider({
  workspaceId,
  workspaceSlug,
  children,
}: {
  workspaceId: string | undefined
  workspaceSlug: string
  children: React.ReactNode
}) {
  const navigate = useNavigate()
  const projects = useWorkspaceProjects(workspaceId)
  const projectIds = useMemo(() => projects.map((p) => p.id), [projects])
  const projectSlugById = useMemo(
    () => new Map(projects.map((p) => [p.id, p.slug])),
    [projects]
  )

  const { data: issues } = useLiveQuery(
    (query) =>
      projectIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.projectId, projectIds))
        : undefined,
    [projectIds.join(`,`)]
  )

  const refs = useMemo(() => {
    const list: ResolvedIssueRef[] = []
    for (const issue of (issues ?? []) as Issue[]) {
      const projectSlug = projectSlugById.get(issue.projectId)
      if (!projectSlug) continue
      list.push({
        id: issue.id,
        identifier: issue.identifier,
        title: issue.title,
        status: issue.status,
        projectSlug,
      })
    }
    // Most recently created first, so empty-query search surfaces fresh work.
    const createdAt = new Map(
      ((issues ?? []) as Issue[]).map((issue) => [
        issue.id,
        new Date(issue.createdAt).getTime(),
      ])
    )
    list.sort((a, b) => (createdAt.get(b.id) ?? 0) - (createdAt.get(a.id) ?? 0))
    return list
  }, [issues, projectSlugById])

  const byIdentifier = useMemo(
    () => new Map(refs.map((ref) => [ref.identifier.toUpperCase(), ref])),
    [refs]
  )

  const value = useMemo<IssueRefContextValue>(
    () => ({
      resolve: (identifier) =>
        byIdentifier.get(identifier.toUpperCase()) ?? null,
      search: (query, opts) => {
        const q = query.trim().toLowerCase()
        const exclude = new Set(opts?.excludeIssueIds ?? [])
        const limit = opts?.limit ?? 8
        const matches: ResolvedIssueRef[] = []
        for (const ref of refs) {
          if (exclude.has(ref.id)) continue
          if (
            q &&
            !ref.identifier.toLowerCase().includes(q) &&
            !ref.title.toLowerCase().includes(q)
          ) {
            continue
          }
          matches.push(ref)
          if (matches.length >= limit) break
        }
        return matches
      },
      open: (identifier) => {
        const ref = byIdentifier.get(identifier.toUpperCase())
        if (!ref) return
        void navigate({
          to: `/t/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier`,
          params: {
            workspaceSlug,
            projectSlug: ref.projectSlug,
            issueIdentifier: ref.identifier,
          },
        })
      },
    }),
    [byIdentifier, refs, navigate, workspaceSlug]
  )

  return (
    <IssueRefContext.Provider value={value}>
      {children}
    </IssueRefContext.Provider>
  )
}
