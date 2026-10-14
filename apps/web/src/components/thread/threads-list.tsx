import { useMemo, type ReactNode } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  AgentRunMark,
  AgentWorkingMark,
  BoardGlyph,
  conceptIcon,
  EmptyState,
  GlassSectionHeader,
  SkeletonRows,
  ThreadsListItem,
  type RunMarkState,
} from "@exp/ui"
import { pullRequestCollection } from "@/lib/collections"
import {
  THREADS_LIST_COPY,
  type AgentRow,
  type PullRequestRow,
  type ThreadsListGroup,
  type ThreadsListRow as ThreadsListRowModel,
} from "@/lib/session-tree"

// EXP-1293 Threads v2: the Threads list (fixture `threads-list.json` ×4) in
// the Reviews list language: a filled group band (no count) per group (Needs
// you · Running · Recent) over flat hairline rows. A row = the lead (the
// agent's working mark while busy, its amber badge when it needs you, else
// the PR glyph its word names), the first linked issue's identifier, the
// title and ONE quiet word derived from the root's pull requests, or a lock
// when private. Phones put the team's Agents band on top.

const NeedsYouIcon = conceptIcon(`coding-needs-input`)
const RecentIcon = conceptIcon(`ui-check`)
const ThreadsIcon = conceptIcon(`nav-threads`)
const AgentsIcon = conceptIcon(`settings-agents`)
const PrOpenIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)
const PrStackIcon = conceptIcon(`pr-stack`)

/** The quiet trailing words ×web. */
export const THREAD_ROW_WORDS = {
  readyToMerge: `ready to merge`,
  stack: `stack`,
  merged: `merged`,
  question: `question`,
  needsInput: `needs input`,
} as const

export type ThreadRowWord = keyof typeof THREAD_ROW_WORDS

type WordPr = Pick<PullRequestRow, `id` | `state` | `basePullRequestId`>

/**
 * ONE quiet word for a thread row, deterministic: an open PR that sits in a
 * stack (based on another open PR, or one is based on it) → `stack`; else an
 * open PR while nothing works → `ready to merge`; no open PR but a merged one
 * → `merged`; else waiting on a person → `question`; else nothing.
 * `teamPrs` = the team's PRs (a stack edge may cross threads).
 */
export function threadRowWord(
  row: Pick<ThreadsListRowModel, `busy` | `needsYou`>,
  prs: readonly WordPr[],
  teamPrs: readonly WordPr[] = prs
): ThreadRowWord | null {
  const open = prs.filter((pr) => pr.state === `open`)
  if (open.length > 0) {
    const openIds = new Set(teamPrs.filter((pr) => pr.state === `open`).map((pr) => pr.id))
    const stacked = open.some(
      (pr) =>
        (pr.basePullRequestId !== null && openIds.has(pr.basePullRequestId)) ||
        teamPrs.some((other) => other.state === `open` && other.basePullRequestId === pr.id)
    )
    if (stacked) return `stack`
    if (!row.busy) return `readyToMerge`
    return null
  }
  if (prs.some((pr) => pr.state === `merged`)) return `merged`
  if (row.needsYou) return `question`
  return null
}

/** The team's PRs grouped by the thread (root) that opened them. */
export function useTeamPullRequestsByRoot(teamId: string | undefined): {
  byRoot: ReadonlyMap<string, PullRequestRow[]>
  all: PullRequestRow[]
} {
  const { data } = useLiveQuery(
    (query) =>
      teamId
        ? query.from({ prs: pullRequestCollection }).where(({ prs }) => eq(prs.teamId, teamId))
        : undefined,
    [teamId]
  )
  return useMemo(() => {
    const all = (data ?? []) as PullRequestRow[]
    const byRoot = new Map<string, PullRequestRow[]>()
    for (const pr of all) {
      if (!pr.rootSessionId) continue
      const list = byRoot.get(pr.rootSessionId) ?? []
      list.push(pr)
      byRoot.set(pr.rootSessionId, list)
    }
    return { byRoot, all }
  }, [data])
}

function rowLead(
  row: Pick<ThreadsListRowModel, `busy` | `needsYou`>,
  cli: string | null,
  word: ThreadRowWord | null
): ReactNode {
  if (row.busy) return <AgentWorkingMark agent={cli} />
  if (row.needsYou) return <AgentRunMark agent={cli} state="needs_input" ringClassName="ring-background" />
  if (word === `readyToMerge`) return <PrOpenIcon className="size-3.5 text-emerald-500" />
  if (word === `merged`) return <PrMergedIcon className="size-3.5 text-violet-400" />
  const state: RunMarkState | undefined = word === null ? `ended` : undefined
  return <AgentRunMark agent={cli} state={state} ringClassName="ring-background" />
}

const GROUP_GLYPH: Record<ThreadsListGroup[`key`], ReactNode> = {
  needs_you: <NeedsYouIcon className="size-3.5 shrink-0 text-amber-400" />,
  running: <AgentWorkingMark agent={null} />,
  recent: <RecentIcon className="size-3.5 shrink-0 text-muted-foreground" />,
}

export function ThreadsListView({
  groups,
  identifierByRoot,
  cliByRoot,
  prsByRoot,
  teamPrs,
  agents,
  loading = false,
  activeId = null,
  onOpenThread,
  onOpenAgent,
}: {
  groups: readonly ThreadsListGroup[]
  identifierByRoot: ReadonlyMap<string, string>
  /** The root's agent CLI (its mark). */
  cliByRoot: ReadonlyMap<string, string | null>
  prsByRoot: ReadonlyMap<string, readonly WordPr[]>
  teamPrs: readonly WordPr[]
  /** The Agents band (phones); absent/empty = none. */
  agents?: readonly AgentRow[]
  loading?: boolean
  activeId?: string | null
  onOpenThread: (sessionId: string) => void
  onOpenAgent?: (sessionId: string) => void
}) {
  if (loading) return <SkeletonRows rowHeight={36} />
  const empty = groups.length === 0 && (agents?.length ?? 0) === 0
  return (
    <div data-testid="threads-list" className="flex flex-col gap-6">
      {agents && agents.length > 0 && (
        <section data-testid="threads-band-agents">
          <GlassSectionHeader
            label={THREADS_LIST_COPY.agents}
            leading={<AgentsIcon className="size-3.5 shrink-0 text-muted-foreground" />}
          />
          <div className="flex flex-col">
            {agents.map((agent) => (
              <ThreadsListItem
                key={agent.sessionId}
                testId="agents-row"
                lead={
                  agent.busy ? (
                    <AgentWorkingMark agent={null} />
                  ) : (
                    <BoardGlyph board={{ icon: agent.icon, color: agent.color }} className="size-3.5" />
                  )
                }
                title={agent.name}
                word={agent.needsYou ? THREAD_ROW_WORDS.needsInput : null}
                active={agent.sessionId === activeId}
                onOpen={onOpenAgent ? () => onOpenAgent(agent.sessionId) : undefined}
              />
            ))}
          </div>
        </section>
      )}
      {groups.map((group) => (
        <section key={group.key} data-testid={`threads-band-${group.key}`}>
          <GlassSectionHeader label={group.title} leading={GROUP_GLYPH[group.key]} />
          <div className="flex flex-col">
            {group.rows.map((row) => {
              const word = threadRowWord(row, prsByRoot.get(row.sessionId) ?? [], teamPrs)
              return (
                <ThreadsListItem
                  key={row.sessionId}
                  lead={rowLead(row, cliByRoot.get(row.sessionId) ?? null, word)}
                  identifier={identifierByRoot.get(row.sessionId) ?? null}
                  title={row.title}
                  word={word ? THREAD_ROW_WORDS[word] : null}
                  wordIcon={
                    word === `stack` ? <PrStackIcon className="size-3 shrink-0" /> : undefined
                  }
                  locked={row.private}
                  active={row.sessionId === activeId}
                  onOpen={() => onOpenThread(row.sessionId)}
                />
              )
            })}
          </div>
        </section>
      ))}
      {empty && (
        <EmptyState icon={ThreadsIcon} title={THREADS_LIST_COPY.empty} size="page" />
      )}
    </div>
  )
}
