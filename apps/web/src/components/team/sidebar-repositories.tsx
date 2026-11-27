import { useMemo, useRef, type ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { repoChecksRollup, type RepoPr } from "@exp/domain-contract/repo-page"
import {
  repositoryName,
  sidebarRepositoryTrailing,
  type RepositoryChecks,
  type RepositoryTrailing,
} from "@exp/domain-contract/sidebar-tree"
import { conceptIcon, Pill, SidebarMenuButton, SidebarMenuItem, SidebarMenuTrailing, type MenuTargetProps } from "@exp/ui"
import type { SyncedRepository } from "@/db/schema"
import { pullRequestCollection, repositoryCollection } from "@/lib/collections"
import { checksDotColor, checksLabel } from "@/components/repo/checks-dot"
import { useSidebarTreeDensity, type SidebarRowItemProps } from "@/components/team/sidebar-folders"
import { cn } from "@/lib/utils"

// #EXP-2171 (sidebar-tree.json `repositoryRow`): a team's repositories as a
// sidebar item kind: the repositories shape (the 34th), each row's trailing
// side its live PRs' checks rollup (THE Chip{dot}), else a GitHub
// repository's mark, else nothing.

const RepositoryIcon = conceptIcon(`action-repository`)
const GithubIcon = conceptIcon(`ui-github`)

export interface SidebarRepository {
  repo: SyncedRepository
  name: string
  trailing: RepositoryTrailing | null
}

const NONE: SidebarRepository[] = []

/** #PERF-13: every member team's synced repositories, read ONCE for the
 *  whole tree (two live queries, whatever the team count), each with its
 *  row's trailing state; a team's array keeps its identity while its rows
 *  draw the same (so a memoised team section skips a render). */
export function useSidebarRepositoriesByTeam(teamIds: readonly string[]): ReadonlyMap<string, SidebarRepository[]> {
  const key = teamIds.join(`,`)
  const { data: repos } = useLiveQuery(
    (q) => (teamIds.length > 0 ? q.from({ r: repositoryCollection }).where(({ r }) => inArray(r.teamId, [...teamIds])) : undefined),
    [key]
  )
  const { data: prs } = useLiveQuery(
    (q) => (teamIds.length > 0 ? q.from({ prs: pullRequestCollection }).where(({ prs }) => inArray(prs.teamId, [...teamIds])) : undefined),
    [key]
  )
  const cache = useRef(new Map<string, { sig: string; rows: SidebarRepository[] }>())
  return useMemo(() => {
    const byRepo = new Map<string, RepoPr[]>()
    for (const pr of (prs ?? []) as unknown as (RepoPr & { repositoryId: string })[]) {
      const list = byRepo.get(pr.repositoryId) ?? []
      list.push(pr)
      byRepo.set(pr.repositoryId, list)
    }
    const byTeam = new Map<string, SidebarRepository[]>()
    for (const repo of (repos ?? []) as SyncedRepository[]) {
      const list = byTeam.get(repo.teamId) ?? []
      list.push({
        repo,
        name: repositoryName(repo.fullName),
        trailing: sidebarRepositoryTrailing({
          provider: repo.provider,
          checks: repoChecksRollup(byRepo.get(repo.id) ?? []) as RepositoryChecks | null,
        }),
      })
      byTeam.set(repo.teamId, list)
    }
    const out = new Map<string, SidebarRepository[]>()
    for (const [teamId, rows] of byTeam) {
      const sig = JSON.stringify(rows.map(({ repo, trailing }) => [repo.id, repo.slug, repo.fullName, repo.sortOrder, repo.provider, trailing]))
      const prev = cache.current.get(teamId)
      if (prev && prev.sig === sig) out.set(teamId, prev.rows)
      else {
        cache.current.set(teamId, { sig, rows })
        out.set(teamId, rows)
      }
    }
    return out
  }, [repos, prs])
}

/** A team's rows off `useSidebarRepositoriesByTeam`. */
export function sidebarRepositoriesOf(map: ReadonlyMap<string, SidebarRepository[]>, teamId: string): SidebarRepository[] {
  return map.get(teamId) ?? NONE
}

/** The trailing side: the checks chip (phones: the dot alone), else the
 *  GitHub mark. */
export function RepositoryTrailingMark({ trailing }: { trailing: RepositoryTrailing | null }) {
  if (!trailing) return null
  if (trailing.kind === `github`) return <GithubIcon className="size-3.5 text-muted-foreground" aria-label="GitHub" />
  const state = trailing.state!
  return (
    <Pill size="sm" dot={checksDotColor(state)} data-testid="sidebar-repository-checks">
      <span className="max-md:sr-only">{checksLabel(state)}</span>
    </Pill>
  )
}

/** A repository: its glyph, its name (the full name's last segment), its
 *  page; the trailing checks chip. */
export function SidebarRepositoryRow({
  entry,
  teamSlug,
  active,
  itemProps,
  guides,
  menu,
}: {
  entry: SidebarRepository
  teamSlug: string
  active: boolean
  itemProps: SidebarRowItemProps
  guides: ReactNode
  menu?: MenuTargetProps
}) {
  const density = useSidebarTreeDensity()
  return (
    <SidebarMenuItem {...itemProps} data-testid="sidebar-repository-row">
      {guides}
      <SidebarMenuButton asChild density={density} isActive={active} className={entry.trailing ? `pr-20!` : undefined}>
        <Link to="/t/$teamSlug/repos/$name" params={{ teamSlug, name: entry.repo.slug }} {...menu}>
          <RepositoryIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate">{entry.name}</span>
        </Link>
      </SidebarMenuButton>
      {entry.trailing && (
        <SidebarMenuTrailing className={cn(`pointer-events-none`, density === `touch` && `right-4`)}>
          <RepositoryTrailingMark trailing={entry.trailing} />
        </SidebarMenuTrailing>
      )}
    </SidebarMenuItem>
  )
}
