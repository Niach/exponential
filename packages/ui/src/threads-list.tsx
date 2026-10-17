import type { ComponentType, ReactNode } from "react"
import {
  THREAD_ROW_WORDS,
  THREADS_LIST_COPY,
  threadInStack,
  threadRowMarkState,
  threadRowWord,
  type AgentRow,
  type StackPr,
  type ThreadRowWord,
  type ThreadsListGroup,
  type ThreadsListRow,
} from "@exp/domain-contract/session-tree"
import { AgentRunMark, AgentWorkingMark, type RunMarkState } from "./agent-brand-mark"
import { BoardGlyph } from "./board-glyph"
import { EmptyState } from "./empty-state"
import { GlassSectionHeader } from "./glass-rows"
import { conceptIcon } from "./icons.generated"
import { SessionRow } from "./session-row"
import { ThreadsListItem } from "./session-thread-view"
import { SkeletonRows } from "./skeleton"
import { treeGuides } from "./tree-guides"

// EXP-1293 Threads v2: THE Threads list (fixture `threads-list.json` ×4),
// shared by the app page and the styleguide so they cannot drift
// (EXP-1339). The lists language: a filled group band (no count) per group
// (Needs you · Running · Recent) over flat gapless rows, no dividers
// (EXP-1320). A row = the lead (the agent's working mark while busy, its
// amber badge when it needs you, else the PR glyph its word names: merged =
// the violet glyph), the title (it names the issue), the trailing word
// (never `merged`, never a lock, EXP-1339), then the root's device glyph
// (EXP-1318). Phones put the team's Agents band on top. EXP-1323: a live
// tree nests its children under the root (`SessionRow` depth + TreeGuides,
// opening that session), a bold title = unread, the live caption the muted
// second line, and a row that changes band enters on the motion tokens.

const NeedsYouIcon = conceptIcon(`coding-needs-input`)
const RecentIcon = conceptIcon(`ui-check`)
const ThreadsIcon = conceptIcon(`nav-threads`)
const AgentsIcon = conceptIcon(`settings-agents`)
const PrOpenIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)
const PrStackIcon = conceptIcon(`pr-stack`)

export type ThreadsListPr = StackPr

export interface ThreadsListDevice {
  icon: ComponentType<{ className?: string }>
  name: string | null
}

/** A thread row's ONE quiet word off its root's PRs (the contract's
 *  `threadRowWord` + `threadInStack`, `threads-list.json` ×4). `teamPrs` =
 *  the team's PRs (a stack edge may cross threads). */
export function threadWordOf(
  row: Pick<ThreadsListRow, `busy` | `needsYou`>,
  prs: readonly ThreadsListPr[],
  teamPrs: readonly ThreadsListPr[] = prs
): ThreadRowWord | null {
  return threadRowWord({
    pullRequests: prs,
    busy: row.busy,
    needsYou: row.needsYou,
    inStack: threadInStack(prs, teamPrs),
  })
}

/** The contract's `threadRowMarkState` as the `AgentRunMark` state. */
export function threadRunMarkState(row: {
  needsYou: boolean
  busy: boolean
}): RunMarkState | undefined {
  const state = threadRowMarkState(row)
  if (state === `needs_you`) return `needs_input`
  if (state === `working`) return `working`
  return undefined
}

function rowLead(
  row: Pick<ThreadsListRow, `busy` | `needsYou`>,
  cli: string | null,
  word: ThreadRowWord | null
): ReactNode {
  // EXP-1344: the contract's `threadRowMarkState` order, needs-you beats busy.
  const mark = threadRunMarkState(row)
  if (mark === `needs_input`) return <AgentRunMark agent={cli} state="needs_input" ringClassName="ring-background" />
  if (mark === `working`) return <AgentWorkingMark agent={cli} />
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

const noDevice = () => null
const noEnter = () => ``

export function ThreadsList({
  groups,
  cliByRoot,
  prsByRoot,
  teamPrs,
  agents,
  loading = false,
  activeId = null,
  deviceGlyph = noDevice,
  entering = noEnter,
  onOpenThread,
  onOpenSession,
  onOpenAgent,
}: {
  groups: readonly ThreadsListGroup[]
  /** Every listed session's agent CLI (its mark), roots and children. */
  cliByRoot: ReadonlyMap<string, string | null>
  prsByRoot: ReadonlyMap<string, readonly ThreadsListPr[]>
  teamPrs: readonly ThreadsListPr[]
  /** The Agents band (phones); absent/empty = none. */
  agents?: readonly AgentRow[]
  loading?: boolean
  activeId?: string | null
  /** A row's device glyph off its `deviceId` (null = none). */
  deviceGlyph?: (deviceId: string | null | undefined) => ThreadsListDevice | null
  /** The `motion-enter` class for a `<band>:<sessionId>` key that arrived
   *  after the first paint, else ``. */
  entering?: (key: string) => string
  onOpenThread: (sessionId: string) => void
  /** EXP-1323: a nested child row opens that session in its thread
   *  (absent = the thread). */
  onOpenSession?: (rootId: string, sessionId: string) => void
  onOpenAgent?: (sessionId: string) => void
}) {
  if (loading) return <SkeletonRows rowHeight={36} />
  const empty = groups.length === 0 && (agents?.length ?? 0) === 0
  return (
    <div data-testid="threads-list" className="flex flex-col gap-6">
      {agents && agents.length > 0 && (
        <div data-testid="threads-band-agents">
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
                word={agent.needsYou ? (THREAD_ROW_WORDS.needsInput ?? null) : null}
                active={agent.sessionId === activeId}
                onOpen={onOpenAgent ? () => onOpenAgent(agent.sessionId) : undefined}
              />
            ))}
          </div>
        </div>
      )}
      {groups.map((group) => (
        <div key={group.key} data-testid={`threads-band-${group.key}`}>
          <GlassSectionHeader label={group.title} leading={GROUP_GLYPH[group.key]} />
          <GroupRows
            group={group}
            cliByRoot={cliByRoot}
            prsByRoot={prsByRoot}
            teamPrs={teamPrs}
            activeId={activeId}
            deviceGlyph={deviceGlyph}
            entering={entering}
            onOpenThread={onOpenThread}
            onOpenSession={onOpenSession}
          />
        </div>
      ))}
      {empty && <EmptyState icon={ThreadsIcon} title={THREADS_LIST_COPY.empty} size="page" />}
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
  deviceGlyph,
  entering,
  onOpenThread,
  onOpenSession,
}: {
  group: ThreadsListGroup
  cliByRoot: ReadonlyMap<string, string | null>
  prsByRoot: ReadonlyMap<string, readonly ThreadsListPr[]>
  teamPrs: readonly ThreadsListPr[]
  activeId: string | null
  deviceGlyph: (deviceId: string | null | undefined) => ThreadsListDevice | null
  entering: (key: string) => string
  onOpenThread: (sessionId: string) => void
  onOpenSession?: (rootId: string, sessionId: string) => void
}) {
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
                onOpenSession ? onOpenSession(row.rootId, row.sessionId) : onOpenThread(row.rootId)
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
            word={word ? (THREAD_ROW_WORDS[word] ?? null) : null}
            wordIcon={word === `stack` ? <PrStackIcon className="size-3 shrink-0" /> : undefined}
            device={deviceGlyph(row.deviceId)}
            active={row.sessionId === activeId}
            onOpen={() => onOpenThread(row.sessionId)}
          />
        )
      })}
    </div>
  )
}
