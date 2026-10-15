import { useMemo, type ReactNode } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  AgentRunMark,
  AgentWorkingMark,
  BoardGlyph,
  conceptIcon,
  EmptyState,
  GlassSectionHeader,
  SessionRow,
  SkeletonRows,
  ThreadsListItem,
  treeGuides,
  type RunMarkState,
} from "@exp/ui"
import { threadRunMarkState } from "@/components/team/sidebar-running"
import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"
import { useEnteringRows } from "@/hooks/use-entering-rows"
import { pullRequestCollection } from "@/lib/collections"
import {
  THREAD_ROW_WORDS,
  THREADS_LIST_COPY,
  threadInStack,
  threadRowWord,
  type AgentRow,
  type PullRequestRow,
  type ThreadRowWord,
  type ThreadsListGroup,
  type ThreadsListRow as ThreadsListRowModel,
} from "@/lib/session-tree"

// EXP-1293 Threads v2: the Threads list (fixture `threads-list.json` ×4) in
// the lists language: a filled group band (no count) per group (Needs you ·
// Running · Recent) over flat gapless rows, no dividers (EXP-1320). A row =
// the lead (the agent's working mark while busy, its amber badge when it
// needs you, else the PR glyph its word names), the title (it names the
// issue), the trailing word AND the lock when private, then the root's
// device glyph (EXP-1318). Phones
// put the team's Agents band on top. EXP-1323: a live tree nests its
// children under the root (`SessionRow` depth + TreeGuides, opening that
// session), a bold title = unread, the live caption the muted second line,
// and a row that changes band enters on the motion tokens.

const NeedsYouIcon = conceptIcon(`coding-needs-input`)
const RecentIcon = conceptIcon(`ui-check`)
const ThreadsIcon = conceptIcon(`nav-threads`)
const AgentsIcon = conceptIcon(`settings-agents`)
const PrOpenIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)
const PrStackIcon = conceptIcon(`pr-stack`)

type WordPr = Pick<PullRequestRow, `id` | `state` | `basePullRequestId`>

/** A thread row's ONE quiet word off its root's PRs (the contract's
 *  `threadRowWord` + `threadInStack`, `threads-list.json` ×4). `teamPrs` =
 *  the team's PRs (a stack edge may cross threads). */
export function threadWordOf(
  row: Pick<ThreadsListRowModel, `busy` | `needsYou`>,
  prs: readonly WordPr[],
  teamPrs: readonly WordPr[] = prs
): ThreadRowWord | null {
  return threadRowWord({
    pullRequests: prs,
    busy: row.busy,
    needsYou: row.needsYou,
    inStack: threadInStack(prs, teamPrs),
  })
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
  cliByRoot,
  prsByRoot,
  teamPrs,
  agents,
  loading = false,
  activeId = null,
  onOpenThread,
  onOpenSession,
  onOpenAgent,
}: {
  groups: readonly ThreadsListGroup[]
  /** Every listed session's agent CLI (its mark), roots and children. */
  cliByRoot: ReadonlyMap<string, string | null>
  prsByRoot: ReadonlyMap<string, readonly WordPr[]>
  teamPrs: readonly WordPr[]
  /** The Agents band (phones); absent/empty = none. */
  agents?: readonly AgentRow[]
  loading?: boolean
  activeId?: string | null
  onOpenThread: (sessionId: string) => void
  /** EXP-1323: a nested child row opens that session in its thread
   *  (absent = the thread). */
  onOpenSession?: (rootId: string, sessionId: string) => void
  onOpenAgent?: (sessionId: string) => void
}) {
  const entering = useEnteringRows(
    groups.flatMap((group) => group.rows.map((row) => `${group.key}:${row.sessionId}`))
  )
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
          <GroupRows
            group={group}
            cliByRoot={cliByRoot}
            prsByRoot={prsByRoot}
            teamPrs={teamPrs}
            activeId={activeId}
            entering={entering}
            onOpenThread={onOpenThread}
            onOpenSession={onOpenSession}
          />
        </section>
      ))}
      {empty && (
        <EmptyState icon={ThreadsIcon} title={THREADS_LIST_COPY.empty} size="page" />
      )}
    </div>
  )
}

/** One band's rows: roots as THE thread row, children nested under them. */
function GroupRows({
  group,
  cliByRoot,
  prsByRoot,
  teamPrs,
  activeId,
  entering,
  onOpenThread,
  onOpenSession,
}: {
  group: ThreadsListGroup
  cliByRoot: ReadonlyMap<string, string | null>
  prsByRoot: ReadonlyMap<string, readonly WordPr[]>
  teamPrs: readonly WordPr[]
  activeId: string | null
  entering: (key: string) => string
  onOpenThread: (sessionId: string) => void
  onOpenSession?: (rootId: string, sessionId: string) => void
}) {
  const deviceGlyph = useDeviceGlyphs()
  const guides = treeGuides(group.rows.map((row) => row.depth))
  return (
    <div className="flex flex-col">
      {group.rows.map((row, index) => {
        const enter = entering(`${group.key}:${row.sessionId}`)
        if (row.depth > 0) {
          const device = deviceGlyph(row.deviceId)
          return (
            <SessionRow
              key={row.sessionId}
              size={row.caption ? `big` : `small`}
              agent={cliByRoot.get(row.sessionId) ?? null}
              markState={threadRunMarkState(row)}
              title={row.title}
              caption={row.caption}
              unread={row.unread}
              depth={row.depth}
              guide={guides[index]}
              deviceIcon={device?.icon}
              deviceName={device?.name ?? null}
              active={row.sessionId === activeId}
              data-testid="threads-row-child"
              className={enter}
              onClick={() =>
                onOpenSession
                  ? onOpenSession(row.rootId, row.sessionId)
                  : onOpenThread(row.rootId)
              }
            />
          )
        }
        const word = threadWordOf(row, prsByRoot.get(row.sessionId) ?? [], teamPrs)
        return (
          <ThreadsListItem
            key={row.sessionId}
            lead={rowLead(row, cliByRoot.get(row.sessionId) ?? null, word)}
            title={row.title}
            unread={row.unread}
            caption={row.caption}
            className={enter}
            word={word ? THREAD_ROW_WORDS[word] : null}
            wordIcon={
              word === `stack` ? <PrStackIcon className="size-3 shrink-0" /> : undefined
            }
            locked={row.private}
            device={deviceGlyph(row.deviceId)}
            active={row.sessionId === activeId}
            onOpen={() => onOpenThread(row.sessionId)}
          />
        )
      })}
    </div>
  )
}
