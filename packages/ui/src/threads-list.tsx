import type { ComponentType, ReactNode } from "react"
import {
  THREAD_ROW_WORDS,
  THREADS_LIST_COPY,
  threadInStack,
  threadRowLeadPr,
  threadRowMarkState,
  threadRowWord,
  type AgentRow,
  type StackPr,
  type ThreadRowWord,
  type ThreadsListGroup,
  type ThreadsListRow,
} from "@exp/domain-contract/session-tree"
import { AgentRunMark, AgentWorkingMark, type RunMarkState } from "./agent-brand-mark"
import { AgentAvatar } from "./chat-list"
import { EmptyState } from "./empty-state"
import { GlassSectionHeader } from "./glass-rows"
import { conceptIcon } from "./icons.generated"
import { menuProps, type MenuEntry, type MenuIcon } from "./menu"
import { CHATS_COPY, type AgentMenuEntry, type ThreadMenuEntry } from "@exp/domain-contract/chats"
import { ThreadsListItem } from "./session-thread-view"
import { SkeletonRows } from "./skeleton"

// EXP-1293 Threads v2: THE Threads list (fixture `threads-list.json` ×4),
// shared by the app page and the styleguide so they cannot drift
// (EXP-1339). The lists language: a filled group band (no count) per group
// (Needs you · Running · Recent) over flat gapless rows, no dividers
// (EXP-1320). A row = the lead (the agent's working mark while busy, its
// amber badge when it needs you, else the root's PR glyph: any open PR, a
// stacked one too, = pr-open emerald, a merged one = the violet glyph, NEW-41
// ×4), the title (it names the issue), the trailing word
// (never `merged`, never a lock, EXP-1339), then the root's device glyph
// (EXP-1318). Phones put the team's Agents band on top. #EXP-1461: ROOTS
// only (a child never lists); a bold title = unread, the live caption the
// muted second line, and a row that changes band enters on the motion tokens.

/** Thread 58cb32df (chats.json MENUS ×4): the context-menu kinds a chat row
 *  and an agent row carry (`menuProps(kind, id)`); the app's ONE
 *  `MenuGestureHost` builds them. */
export const CHAT_ROW_MENU_KIND = `thread`
export const AGENT_ROW_MENU_KIND = `agent`

const ChatDeleteIcon = conceptIcon(`ui-delete`)
const AGENT_MENU_ICONS: Record<AgentMenuEntry, MenuIcon> = {
  compact: conceptIcon(`coding-compact`),
  newConversation: conceptIcon(`ui-clean`),
  settings: conceptIcon(`nav-settings`),
}

/** A chat's menu (`threadMenuEntries`) as THE Menu's rows: the destructive
 *  Delete (S-3: no Stop). */
export function chatMenuRows(
  entries: readonly ThreadMenuEntry[],
  run: (entry: ThreadMenuEntry) => void
): MenuEntry[] {
  return entries.map((entry): MenuEntry => ({
    kind: `item`,
    id: entry,
    label: CHATS_COPY.menu[entry],
    icon: ChatDeleteIcon,
    destructive: true,
    "data-testid": `chat-menu-${entry}`,
    onSelect: () => run(entry),
  }))
}

/** The agent menu (`agentMenuEntries`) as THE Menu's rows: the
 *  conversation pair, a divider, then Agent settings. */
export function agentMenuRows(
  entries: readonly AgentMenuEntry[],
  run: (entry: AgentMenuEntry) => void
): MenuEntry[] {
  const rows: MenuEntry[] = []
  let divided = false
  for (const entry of entries) {
    if (!divided && entry === `settings`) {
      rows.push({ kind: `separator`, id: `sep` })
      divided = true
    }
    rows.push({
      kind: `item`,
      id: entry,
      label: CHATS_COPY.menu[entry],
      icon: AGENT_MENU_ICONS[entry],
      "data-testid": `agent-menu-${entry}`,
      onSelect: () => run(entry),
    })
  }
  return rows
}

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

/** The root's PR glyph for a thread row's lead (threads-list.json, NEW-41
 *  ×4): any open PR (a stacked one too) = pr-open emerald, else a merged
 *  one = pr-merged violet, else none. */
export function threadLeadPrState(prs: readonly ThreadsListPr[]): `open` | `merged` | null {
  // Release train 2026-10-23: the contract's rule, locked by `leadCases` ×4.
  return threadRowLeadPr(prs.map((pr) => pr.state))
}

function rowLead(
  row: Pick<ThreadsListRow, `busy` | `needsYou`>,
  cli: string | null,
  word: ThreadRowWord | null,
  prs: readonly ThreadsListPr[]
): ReactNode {
  // EXP-1344: the contract's `threadRowMarkState` order, needs-you beats busy.
  const mark = threadRunMarkState(row)
  if (mark === `needs_input`) return <AgentRunMark agent={cli} state="needs_input" ringClassName="ring-background" />
  if (mark === `working`) return <AgentWorkingMark agent={cli} />
  // NEW-41: the PR glyph follows the root's PRs, not the word (a stacked
  // open PR reads `stack` yet leads emerald, like the IDE).
  const pr = threadLeadPrState(prs)
  if (pr === `open`) return <PrOpenIcon className="size-3.5 text-emerald-500" />
  if (pr === `merged`) return <PrMergedIcon className="size-3.5 text-violet-400" />
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
  onOpenAgent,
}: {
  groups: readonly ThreadsListGroup[]
  /** Every listed root's agent CLI (its mark). */
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
                key={agent.agentId}
                testId="agents-row"
                lead={
                  agent.busy ? (
                    <AgentWorkingMark agent={null} />
                  ) : (
                    <AgentAvatar agent={agent} className="size-3.5" />
                  )
                }
                title={agent.name}
                word={agent.needsYou ? (THREAD_ROW_WORDS.needsInput ?? null) : null}
                active={agent.agentId === activeId}
                menu={menuProps(AGENT_ROW_MENU_KIND, agent.agentId)}
                onOpen={onOpenAgent ? () => onOpenAgent(agent.agentId) : undefined}
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
          />
        </div>
      ))}
      {empty && <EmptyState icon={ThreadsIcon} title={THREADS_LIST_COPY.empty} size="page" />}
    </div>
  )
}

/** One band's rows: THE thread row per root. */
function GroupRows({
  group,
  cliByRoot,
  prsByRoot,
  teamPrs,
  activeId,
  deviceGlyph,
  entering,
  onOpenThread,
}: {
  group: ThreadsListGroup
  cliByRoot: ReadonlyMap<string, string | null>
  prsByRoot: ReadonlyMap<string, readonly ThreadsListPr[]>
  teamPrs: readonly ThreadsListPr[]
  activeId: string | null
  deviceGlyph: (deviceId: string | null | undefined) => ThreadsListDevice | null
  entering: (key: string) => string
  onOpenThread: (sessionId: string) => void
}) {
  return (
    <div className="flex flex-col">
      {group.rows.map((row) => {
        const enter = entering(`${group.key}:${row.sessionId}`)
        const prs = prsByRoot.get(row.sessionId) ?? []
        const word = threadWordOf(row, prs, teamPrs)
        return (
          <ThreadsListItem
            key={row.sessionId}
            lead={rowLead(row, cliByRoot.get(row.sessionId) ?? null, word, prs)}
            title={row.title}
            unread={row.unread}
            className={enter}
            word={word ? (THREAD_ROW_WORDS[word] ?? null) : null}
            wordIcon={word === `stack` ? <PrStackIcon className="size-3 shrink-0" /> : undefined}
            device={deviceGlyph(row.deviceId)}
            active={row.sessionId === activeId}
            menu={menuProps(CHAT_ROW_MENU_KIND, row.sessionId)}
            onOpen={() => onOpenThread(row.sessionId)}
          />
        )
      })}
    </div>
  )
}
