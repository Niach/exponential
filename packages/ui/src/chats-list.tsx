import type { ComponentType, ReactNode } from "react"
import {
  CHAT_ROW_WORDS,
  CHATS_LIST_COPY,
  chatInStack,
  chatRowLeadPr,
  chatRowMarkState,
  chatRowWord,
  type AgentRow,
  type StackPr,
  type ChatRowWord,
  type ChatsListGroup,
  type ChatsListRow,
} from "@exp/domain-contract/session-tree"
import { AgentRunMark, AgentWorkingMark, type RunMarkState } from "./agent-brand-mark"
import { AgentAvatar } from "./chat-list"
import { EmptyState } from "./empty-state"
import { GlassSectionHeader } from "./glass-rows"
import type { IconConcept } from "@exp/icons"
import { conceptIcon } from "./icons.generated"
import { menuProps, type MenuEntry, type MenuIcon } from "./menu"
import { CHATS_COPY, type ChatMenuEntry } from "@exp/domain-contract/chats"
import {
  SESSION_SCREEN_MENU_ICONS,
  SESSION_SCREEN_TEST_IDS,
  sessionScreenMenuLabel,
  type SessionScreenMenuEntry,
} from "@exp/domain-contract/session-screen"
import { ChatsListItem } from "./session-chat-view"
import { SkeletonRows } from "./skeleton"

// EXP-1293: THE Chats list (fixture `chat-list.json` ×4),
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

/** Chat 58cb32df (chats.json MENUS ×4): the context-menu kinds a chat row
 *  and an agent row carry (`menuProps(kind, id)`); the app's ONE
 *  `MenuGestureHost` builds them. */
export const CHAT_ROW_MENU_KIND = `chat`
export const AGENT_ROW_MENU_KIND = `agent`

const ChatDeleteIcon = conceptIcon(`ui-delete`)

/** A chat's menu (`chatMenuEntries`) as THE Menu's rows: the destructive
 *  Delete (S-3: no Stop). */
export function chatMenuRows(
  entries: readonly ChatMenuEntry[],
  run: (entry: ChatMenuEntry) => void
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

type SessionMenuItem = Exclude<SessionScreenMenuEntry, `separator`>

const SESSION_MENU_ICONS = Object.fromEntries(
  Object.entries(SESSION_SCREEN_MENU_ICONS).map(([entry, concept]) => [entry, conceptIcon(concept as IconConcept)])
) as Record<SessionMenuItem, MenuIcon>

/** #FEED-239 (session-screen.json `menuCases`): THE session screen menu
 *  (`sessionScreenMenu`) as THE Menu's rows, for every kind: the labels
 *  `copy.menu` (settings and delete by kind), the icons `menuIcons`, a
 *  divider per `separator`, Delete destructive. The header's `…` and a
 *  sidebar agent row's right-click draw the same rows. */
export function sessionScreenMenuRows(
  kind: string,
  entries: readonly SessionScreenMenuEntry[],
  run: (entry: SessionMenuItem) => void,
  /** #EXP-1834: the `linkIssue` entry's submenu rows (THE link menu:
   *  Link an issue ›, New issue). */
  linkIssue?: readonly MenuEntry[]
): MenuEntry[] {
  return entries.map((entry, index): MenuEntry =>
    entry === `separator`
      ? { kind: `separator`, id: `sep-${index}` }
      : entry === `linkIssue`
        ? {
            kind: `submenu`,
            id: entry,
            label: sessionScreenMenuLabel(kind, entry),
            icon: SESSION_MENU_ICONS[entry],
            entries: linkIssue ?? [],
            "data-testid": `${SESSION_SCREEN_TEST_IDS.menu}-${entry}`,
          }
        : {
          kind: `item`,
          id: entry,
          label: sessionScreenMenuLabel(kind, entry),
          icon: SESSION_MENU_ICONS[entry],
          destructive: entry === `delete`,
          "data-testid": `${SESSION_SCREEN_TEST_IDS.menu}-${entry}`,
          onSelect: () => run(entry),
        }
  )
}

const NeedsYouIcon = conceptIcon(`coding-needs-input`)
const RecentIcon = conceptIcon(`ui-check`)
const ChatsIcon = conceptIcon(`nav-chats`)
const AgentsIcon = conceptIcon(`settings-agents`)
const PrOpenIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)
const PrStackIcon = conceptIcon(`pr-stack`)

export type ChatsListPr = StackPr

export interface ChatsListDevice {
  icon: ComponentType<{ className?: string }>
  name: string | null
}

/** A chat row's ONE quiet word off its root's PRs (the contract's
 *  `chatRowWord` + `chatInStack`, `chat-list.json` ×4). `teamPrs` =
 *  the team's PRs (a stack edge may cross chats). */
export function chatWordOf(
  row: Pick<ChatsListRow, `busy` | `needsYou` | `offline`>,
  prs: readonly ChatsListPr[],
  teamPrs: readonly ChatsListPr[] = prs
): ChatRowWord | null {
  return chatRowWord({
    pullRequests: prs,
    busy: row.busy,
    needsYou: row.needsYou,
    inStack: chatInStack(prs, teamPrs),
    offline: row.offline ?? false,
  })
}

/** The contract's `chatRowMarkState` as the `AgentRunMark` state. */
export function chatRunMarkState(row: {
  needsYou: boolean
  busy: boolean
}): RunMarkState | undefined {
  const state = chatRowMarkState(row)
  if (state === `needs_you`) return `needs_input`
  if (state === `working`) return `working`
  return undefined
}

/** The root's PR glyph for a chat row's lead (chat-list.json, NEW-41
 *  ×4): any open PR (a stacked one too) = pr-open emerald, else a merged
 *  one = pr-merged violet, else none. */
export function chatLeadPrState(prs: readonly ChatsListPr[]): `open` | `merged` | null {
  // Release train 2026-10-23: the contract's rule, locked by `leadCases` ×4.
  return chatRowLeadPr(prs.map((pr) => pr.state))
}

function rowLead(
  row: Pick<ChatsListRow, `busy` | `needsYou`>,
  cli: string | null,
  word: ChatRowWord | null,
  prs: readonly ChatsListPr[]
): ReactNode {
  // EXP-1344: the contract's `chatRowMarkState` order, needs-you beats busy.
  const mark = chatRunMarkState(row)
  if (mark === `needs_input`) return <AgentRunMark agent={cli} state="needs_input" ringClassName="ring-background" />
  if (mark === `working`) return <AgentWorkingMark agent={cli} />
  // NEW-41: the PR glyph follows the root's PRs, not the word (a stacked
  // open PR reads `stack` yet leads emerald, like the IDE).
  const pr = chatLeadPrState(prs)
  if (pr === `open`) return <PrOpenIcon className="size-3.5 text-emerald-500" />
  if (pr === `merged`) return <PrMergedIcon className="size-3.5 text-violet-400" />
  const state: RunMarkState | undefined = word === null ? `ended` : undefined
  return <AgentRunMark agent={cli} state={state} ringClassName="ring-background" />
}

const GROUP_GLYPH: Record<ChatsListGroup[`key`], ReactNode> = {
  needs_you: <NeedsYouIcon className="size-3.5 shrink-0 text-yellow-400" />,
  running: <AgentWorkingMark agent={null} />,
  recent: <RecentIcon className="size-3.5 shrink-0 text-muted-foreground" />,
}

const noDevice = () => null
const noEnter = () => ``

export function ChatsList({
  groups,
  cliByRoot,
  prsByRoot,
  teamPrs,
  agents,
  loading = false,
  activeId = null,
  deviceGlyph = noDevice,
  entering = noEnter,
  onOpenChat,
  onOpenAgent,
}: {
  groups: readonly ChatsListGroup[]
  /** Every listed root's agent CLI (its mark). */
  cliByRoot: ReadonlyMap<string, string | null>
  prsByRoot: ReadonlyMap<string, readonly ChatsListPr[]>
  teamPrs: readonly ChatsListPr[]
  /** The Agents band (phones); absent/empty = none. */
  agents?: readonly AgentRow[]
  loading?: boolean
  activeId?: string | null
  /** A row's device glyph off its `deviceId` (null = none). */
  deviceGlyph?: (deviceId: string | null | undefined) => ChatsListDevice | null
  /** The `motion-enter` class for a `<band>:<sessionId>` key that arrived
   *  after the first paint, else ``. */
  entering?: (key: string) => string
  onOpenChat: (sessionId: string) => void
  onOpenAgent?: (sessionId: string) => void
}) {
  if (loading) return <SkeletonRows rowHeight={36} />
  const empty = groups.length === 0 && (agents?.length ?? 0) === 0
  return (
    <div data-testid="chats-list" className="flex flex-col gap-6">
      {agents && agents.length > 0 && (
        <div data-testid="chats-band-agents">
          <GlassSectionHeader
            label={CHATS_LIST_COPY.agents}
            leading={<AgentsIcon className="size-3.5 shrink-0 text-muted-foreground" />}
          />
          <div className="flex flex-col">
            {agents.map((agent) => (
              <ChatsListItem
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
                word={agent.needsYou ? (CHAT_ROW_WORDS.needsInput ?? null) : null}
                active={agent.agentId === activeId}
                menu={menuProps(AGENT_ROW_MENU_KIND, agent.agentId)}
                onOpen={onOpenAgent ? () => onOpenAgent(agent.agentId) : undefined}
              />
            ))}
          </div>
        </div>
      )}
      {groups.map((group) => (
        <div key={group.key} data-testid={`chats-band-${group.key}`}>
          <GlassSectionHeader label={group.title} leading={GROUP_GLYPH[group.key]} />
          <GroupRows
            group={group}
            cliByRoot={cliByRoot}
            prsByRoot={prsByRoot}
            teamPrs={teamPrs}
            activeId={activeId}
            deviceGlyph={deviceGlyph}
            entering={entering}
            onOpenChat={onOpenChat}
          />
        </div>
      ))}
      {empty && <EmptyState icon={ChatsIcon} title={CHATS_LIST_COPY.empty} size="page" />}
    </div>
  )
}

/** One band's rows: THE chat row per root. */
function GroupRows({
  group,
  cliByRoot,
  prsByRoot,
  teamPrs,
  activeId,
  deviceGlyph,
  entering,
  onOpenChat,
}: {
  group: ChatsListGroup
  cliByRoot: ReadonlyMap<string, string | null>
  prsByRoot: ReadonlyMap<string, readonly ChatsListPr[]>
  teamPrs: readonly ChatsListPr[]
  activeId: string | null
  deviceGlyph: (deviceId: string | null | undefined) => ChatsListDevice | null
  entering: (key: string) => string
  onOpenChat: (sessionId: string) => void
}) {
  return (
    <div className="flex flex-col">
      {group.rows.map((row) => {
        const enter = entering(`${group.key}:${row.sessionId}`)
        const prs = prsByRoot.get(row.sessionId) ?? []
        const word = chatWordOf(row, prs, teamPrs)
        return (
          <ChatsListItem
            key={row.sessionId}
            lead={rowLead(row, cliByRoot.get(row.sessionId) ?? null, word, prs)}
            title={row.title}
            unread={row.unread}
            className={enter}
            word={word ? (CHAT_ROW_WORDS[word] ?? null) : null}
            wordIcon={word === `stack` ? <PrStackIcon className="size-3 shrink-0" /> : undefined}
            device={deviceGlyph(row.deviceId)}
            active={row.sessionId === activeId}
            menu={menuProps(CHAT_ROW_MENU_KIND, row.sessionId)}
            onOpen={() => onOpenChat(row.sessionId)}
          />
        )
      })}
    </div>
  )
}
