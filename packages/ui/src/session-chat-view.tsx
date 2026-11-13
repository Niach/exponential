import {
  useMemo,
  useRef,
  useState,
  type ComponentProps,
  type ComponentType,
  type ReactNode,
} from "react"
import {
  SESSION_CARD_COPY,
  SUGGESTIONS_COPY,
  type SessionState,
  type SessionTabMarkState,
  type SessionXref as SessionXrefModel,
  type SuggestionRow,
  type SessionChatRow,
} from "@exp/domain-contract/session-tree"
import {
  ASK_DOCK_COPY,
  askDockTitle,
  planDecisionLabel,
  chatPendingAsks,
  type PlanOption,
} from "@exp/domain-contract/ask-dock"
import { SESSION_SCREEN_COPY } from "@exp/domain-contract/session-screen"
import { dockRows, suggestionSources, type DockRow, type DockSource } from "@exp/domain-contract/dock"
import {
  CHAT_ROW_ACTION_COPY,
  CHAT_ROW_ACTION_TEST_IDS,
  chatRowAction,
  waitingCardInline,
  type ChatRowAction,
} from "@exp/domain-contract/chat-row-action"
import type { RunMarkState } from "./agent-brand-mark"
import {
  answerIssue,
  joinAnswers,
  PLAN_CARD_COPY,
  PLAN_CARD_TEST_IDS,
  stepLabel,
  stepperApplies,
} from "@exp/domain-contract/plan-card"
import {
  PLAN_PAGE_COPY,
  PLAN_PAGE_LAYOUT,
  planRowLabel,
} from "@exp/domain-contract/plan-page"
import {
  AskDismissButton, AskAnsweredLine, AskCard, AskDismissedCard, AskPrompt } from "./ask-card"
import {
  closedPicks,
  PlanCardCaption,
  PlanCardCustomField,
  PlanCardFooter,
  PlanCardOptions,
  PlanCardQuestionBlock,
  PlanCardSurface,
  questionAnswer,
  stripPendingUploads,
  useSurfaceDataRef,
} from "./plan-card"
import { Dock, DockSheetBody, DockSheetFooter, useDockSheetMeta } from "./dock"
import { Button } from "./button"
import { cn } from "./cn"
import { ListRow } from "./glass-rows"
import { Menu, type MenuEntry, type MenuIcon, type MenuTargetProps } from "./menu"
import { SessionRow, SessionRowDevice } from "./session-row"
import type { UserAvatarAgent, UserAvatarUser } from "./user-avatar"
import { treeGuides } from "./tree-guides"
import { conceptIcon } from "./icons.generated"
import {
  MESSAGE_DELIVERY_COPY,
  bubbleActions,
  deliveryState,
  type DeliveryState,
} from "@exp/domain-contract/message-delivery"
import { MessageBubble, SenderAvatar, TranscriptRow, transcriptSenderName } from "./user-message-bubble"
import { CHAT_BUBBLE_ENTER_CLASS, SurfaceCard, TranscriptDivider, TranscriptMarker, useLiveArrivals } from "./surface-card"
import {
  transcriptLayout,
  type TranscriptItem,
  type TranscriptRowLayout,
} from "@exp/domain-contract/transcript-grouping"
import { Spinner } from "./feedback"
import { SkeletonRows } from "./skeleton"
import { StableRow } from "./transcript-window"
import { SESSION_CARD_TONE, SessionCard } from "./work-card"

// EXP-1293: the CHAT screen's presentational parts ×web (the
// styleguide paints these same components). The model is the contract's
// (`buildSessionChat` / `partitionChats`, fixtures `session-chat.json`
// + `chat-list.json`, ×4); nothing here queries: every glyph, chip,
// renderer and handler is handed in.
//
//   ChatSessionTree    #EXP-1732: THE session screen's Sessions tab rows
//                        (SessionRow small + the tree guides)
//   ChatConversation   the conversation rows: messages (a person's bubble
//                        right, the agent's markdown, system lines muted),
//                        THE session card per session, question and plan
//                        cards = THE session view's card (`ask-card.tsx`,
//                        EXP-1316) answered IN the card, a person's own
//                        queued row with its X (EXP-1317), subagent /
//                        picture / event lines; EXP-1323: a child's
//                        report line (S-3 SessionXref) and THE suggested tasks
//                        card (`ChatSuggestionsCard`), then the host's
//                        live tail (Tasks card + status line)
//   ChatsListItem      a Chats list row: lead glyph, the title (bold
//                        while unread), ONE quiet word and the lock
//                        (private), the live caption under it
//   ChannelMessageRow    an agent channel's comment-style row

const SubagentIcon = conceptIcon(`coding-subagent`)
const SuggestionsIcon = conceptIcon(`ui-checklist`)
const MoreIcon = conceptIcon(`ui-more`)
const AddIcon = conceptIcon(`ui-add`)
const CloseIcon = conceptIcon(`ui-close`)

/** #EXP-1732: the Issues tab's `+` accessible name (session-screen.json
 *  `copy.linkIssue`): THE Menu with Link an issue › and New issue. */
export const CHAT_TABS_ADD_LABEL = SESSION_SCREEN_COPY.linkIssue

/** A session chip's mark state ×4 (`sessionTabMarkState`) in the run
 *  marks' words (idle = the bare mark). */
export function sessionTabRunMark(state: SessionTabMarkState): RunMarkState | undefined {
  if (state === `needs_you`) return `needs_input`
  if (state === `working`) return `working`
  if (state === `done`) return `ended`
  return undefined
}

export interface ChatTreeRow {
  sessionId: string
  parentId: string | null
  name: string
  agent: string | null
  markState: SessionTabMarkState
  /** The session card's state word (`kindWord`) and its tone. */
  stateWord?: string
  stateTone?: string
}

/** Depth-first (children in the given order) with each row's depth. */
export function chatTreeRows(
  rows: readonly ChatTreeRow[]
): { row: ChatTreeRow; depth: number }[] {
  const ids = new Set(rows.map((row) => row.sessionId))
  const children = new Map<string | null, ChatTreeRow[]>()
  for (const row of rows) {
    const parent = row.parentId && ids.has(row.parentId) ? row.parentId : null
    const list = children.get(parent) ?? []
    list.push(row)
    children.set(parent, list)
  }
  const out: { row: ChatTreeRow; depth: number }[] = []
  const visited = new Set<string>()
  const walk = (parent: string | null, depth: number) => {
    for (const row of children.get(parent) ?? []) {
      if (visited.has(row.sessionId)) continue
      visited.add(row.sessionId)
      out.push({ row, depth })
      walk(row.sessionId, depth + 1)
    }
  }
  walk(null, 0)
  // A parent cycle (a ↔ b, a row naming itself) reaches no root and would
  // vanish: promote its first unreached row to a root and walk under it.
  for (const row of rows) {
    if (visited.has(row.sessionId)) continue
    visited.add(row.sessionId)
    out.push({ row, depth: 0 })
    walk(row.sessionId, 1)
  }
  return out
}

/** EXP-1316/#EXP-1433: the session tree as THE session rows (`SessionRow
 *  size="small"` + the tree guides, the Chats list's nesting), the open
 *  one washed (#EXP-1732: THE session screen's Sessions tab). */
export function ChatSessionTree({
  rows,
  activeId,
  onOpen,
}: {
  rows: readonly ChatTreeRow[]
  activeId: string | null
  onOpen: (sessionId: string) => void
}) {
  const ordered = chatTreeRows(rows)
  const guides = treeGuides(ordered.map((entry) => entry.depth))
  return (
    <div data-testid="chat-session-tree" className="flex flex-col">
      {ordered.map(({ row, depth }, index) => {
        const active = row.sessionId === activeId
        const word = row.stateWord ? (
          <span
            data-slot="chat-session-tree-word"
            className={cn(
              `shrink-0 text-muted-foreground`,
              row.stateTone && SESSION_CARD_TONE[row.stateTone]
            )}
          >
            {row.stateWord}
          </span>
        ) : null
        return (
          <SessionRow
            key={row.sessionId}
            size="small"
            agent={row.agent}
            markState={sessionTabRunMark(row.markState)}
            title={row.name}
            trailing={word ?? undefined}
            depth={depth}
            guide={guides[index]}
            active={active}
            data-testid="chat-session-tree-row"
            onClick={() => onOpen(row.sessionId)}
          />
        )
      })}
    </div>
  )
}

/** #EXP-1732: the Issues tab's "+" (the host wraps it in THE Menu: Link an
 *  issue ›, New issue). */
export function ChatTabsAddButton(props: ComponentProps<typeof Button>) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      aria-label={CHAT_TABS_ADD_LABEL}
      title={CHAT_TABS_ADD_LABEL}
      data-testid="chat-tab-add"
      className="shrink-0 text-muted-foreground hover:text-foreground"
      {...props}
    >
      <AddIcon />
    </Button>
  )
}

export interface ChatConversationProps {
  rows: readonly SessionChatRow[]
  /** The app's GFM renderer (agent text, a person's bubble, a plan). */
  renderText: (text: string) => ReactNode
  /** #EXP-1736 (transcript-grouping.json): the conversation's OWN sender
   *  (its session, title and agent): the agent's bubbles wear the agent's
   *  mark, else the session's title + glyph. */
  chat?: { sessionId: string; title: string; agentId: string | null } | null
  /** #EXP-1736: a member's name + picture (their bubbles' avatar + name). */
  member?: (userId: string) => UserAvatarUser | null
  /** #EXP-1736: the rows begin at the conversation's first message (no
   *  `Load earlier` above them): only then the first one can be the
   *  origin's (absent = true). */
  fromStart?: boolean
  attachmentSrc: (attachmentId: string) => string
  /** A session card was tapped (its raw view). */
  onOpenSession?: (sessionId: string) => void
  /** S-3: an inter-session link was tapped (the host pushes its back
   *  stack); absent = `onOpenSession`. */
  onOpenXref?: (sessionId: string) => void
  /** S-3: the agent mark of an xref's `agentId` (absent = the session
   *  glyph). */
  xrefAgent?: (agentId: string) => UserAvatarAgent | null
  /** #EXP-1464 (chat-row-action.json): the conversation's own root (a row
   *  about it does nothing). */
  rootId?: string
  /** #EXP-1464: loads an expanded row's waiting question / plan card (the
   *  host's data; `undefined` = still loading, null = none); absent = a
   *  Needs you card opens like any other. */
  WaitingCard?: ComponentType<{ sessionId: string; children: (row: WaitingCardRow | null | undefined) => ReactNode }>
  /** S-3: who started the viewed session (`SessionChatView.origin`): #EXP-1736
   *  its prompt reads as that session's bubble. */
  origin?: SessionXrefModel | null
  /** A question card's answer: the picks / the typed free text + the
   *  surface's data (absent = read-only). */
  onAnswer?: (rowId: string, input: ChatQuestionAnswer) => void
  /** A plan card's decision + the reject's note (absent = read-only). */
  onDecide?: (rowId: string, input: ChatPlanDecision) => void
  /** #EXP-1561 (waiting-cards.json DISMISS): a waiting card's × (absent =
   *  no ×). */
  onDismiss?: (rowId: string) => void
  /** #EXP-1493: a plan card's `Open plan` (THE plan page; absent = none). */
  onOpenPlan?: (row: Extract<SessionChatRow, { type: `plan` }>) => void
  /** The rows whose answer is in flight. */
  pending?: ReadonlySet<string>
  /** The rows THE Dock holds. FEED-140 (chats.json): a waiting plan or
   *  question card STAYS inline at its place with its actions (the dock
   *  holds it too); only the other docked rows (suggestions) leave. */
  docked?: ReadonlySet<string>
  /** chats.json CHAT MODE: `bubbles` = an agent conversation's chat bubbles
   *  (the person right, the agent's text left in a bg-card bubble with its
   *  time, new live bubbles rise in); `transcript` = a chat, every form
   *  factor (EXP-1676). */
  layout?: `transcript` | `bubbles`
  /** A row's `HH:mm` (the bubbles' time). */
  messageTime?: (row: Extract<SessionChatRow, { type: `message` }>) => string | null
  /** A surface card's `reply` press: send the text as the person's message
   *  (absent = the presses do nothing). */
  onSurfaceReply?: (text: string) => void
  /** The rows that arrived live (the surface enter, the bubbles' rise);
   *  absent = computed here off the first painted rows. */
  arrived?: (rowId: string) => boolean
  /** The viewer: their OWN queued rows carry the Remove from queue X. */
  viewerId?: string | null
  /** EXP-1317: revoke a queued row (#FEED-237: the bubble menu's Cancel
   *  sending; absent = no menu). */
  onUnqueue?: (rowId: string) => void
  /** #FEED-237: the viewer's own row's delivery state (message-delivery.json
   *  `deliveryState` off the synced row); absent = consumed reads `read`,
   *  unconsumed `sending`. */
  deliveryOf?: (row: Extract<SessionChatRow, { type: `message` }>) => DeliveryState | null
  /** EXP-1323: the suggested tasks cards' actions (absent = read-only). */
  suggestions?: ChatSuggestionActions
  /** EXP-1323: a filed suggestion's issue (absent = its identifier). */
  renderIssueRef?: (issueId: string, identifier: string | null) => ReactNode
  /** EXP-1323: the live tail after the last row (Tasks card + status
   *  line), the host's. */
  tail?: ReactNode
  className?: string
}

/** #EXP-1736 (transcript-grouping.json): the conversation's rows as the
 *  transcript's items: messages and reports carry a sender, subagent lines
 *  are the agent's trace, every card is a full row; a docked row that
 *  left the conversation drops out. */
export function chatTranscriptLayout(
  rows: readonly SessionChatRow[],
  options: {
    chat?: { sessionId: string; title: string; agentId: string | null } | null
    origin?: SessionXrefModel | null
    viewerId?: string | null
    member?: (userId: string) => UserAvatarUser | null
    docked?: ReadonlySet<string>
    fromStart?: boolean
  }
): Map<string, TranscriptRowLayout> {
  const sessions = new Map<string, { id: string; title: string; agentId: string | null }>()
  const addXref = (xref: SessionXrefModel | null | undefined) => {
    if (xref) sessions.set(xref.sessionId, { id: xref.sessionId, title: xref.name, agentId: xref.agentId })
  }
  addXref(options.origin)
  const members = new Map<string, string>()
  const items: TranscriptItem[] = []
  for (const row of rows) {
    if (options.docked?.has(row.id) && row.type !== `plan` && row.type !== `question`) continue
    if (row.type === `message`) {
      addXref(row.xref)
      if (row.author === `user` && row.authorId && !members.has(row.authorId)) {
        const name = options.member?.(row.authorId)?.name
        if (name) members.set(row.authorId, name)
      }
      items.push({
        id: row.id,
        kind: `message`,
        authorKind: row.author,
        authorId: row.authorId,
        authorSessionId: row.authorSessionId,
      })
    } else if (row.type === `report`) {
      addXref(row.xref)
      items.push({ id: row.id, kind: `message`, authorKind: `agent`, authorId: null, authorSessionId: row.sessionId })
    } else {
      items.push({ id: row.id, kind: row.type === `subagent` ? `trace` : `full` })
    }
  }
  const chat = options.chat ?? { sessionId: ``, title: ``, agentId: null }
  const layouts = transcriptLayout({
    viewerId: options.viewerId ?? null,
    chat: { ...chat, originSessionId: options.origin?.sessionId ?? null, fromStart: options.fromStart ?? true },
    members: [...members].map(([id, name]) => ({ id, name })),
    agents: chat.agentId ? [{ id: chat.agentId, name: chat.title }] : [],
    sessions: [...sessions.values()],
    items,
  })
  return new Map(layouts.map((layout) => [layout.id, layout]))
}

/** A row's layout as one comparable value (`StableRow`'s deps). */
function layoutKey(layout: TranscriptRowLayout | undefined): string {
  if (!layout) return ``
  const sender = layout.sender
  return [
    layout.side,
    layout.avatar,
    layout.name,
    layout.gap,
    sender ? `${sender.kind}:${sender.id}:${sender.name}:${sender.link ?? ``}` : ``,
  ].join(`|`)
}

/** #EXP-1464: the card an expanded session row opens. */
export type WaitingCardRow = Extract<SessionChatRow, { type: `question` | `plan` }>

/** S-3: the card rows a `bubbles` conversation keeps in the agent column. */
const AGENT_COLUMN_ROWS = new Set<SessionChatRow[`type`]>([
  `session`,
  `question`,
  `plan`,
  `picture`,
  `report`,
  `suggestions`,
])

/** EXP-1317: the queued row's X (tooltip + accessible name). */
export const CHAT_UNQUEUE_LABEL = `Remove from queue`

export function ChatConversation(props: ChatConversationProps) {
  const {
    rows,
    renderText,
    chat,
    member,
    fromStart = true,
    attachmentSrc,
    onOpenSession,
    onOpenXref,
    xrefAgent,
    rootId = ``,
    WaitingCard,
    origin,
    onAnswer,
    onDecide,
    onDismiss,
    onOpenPlan,
    pending,
    docked,
    viewerId,
    onUnqueue,
    deliveryOf,
    suggestions,
    renderIssueRef,
    tail,
    layout = `transcript`,
    messageTime,
    onSurfaceReply,
    arrived,
    className,
  } = props
  // #EXP-1380 (chats.json PERF): rows render once per change of their own
  // data (`StableRow`); the callbacks a skipped row holds read the latest
  // props through this ref.
  const latest = useRef(props)
  latest.current = props
  const ownArrivals = useLiveArrivals(rows.map((row) => row.id))
  const isLive = arrived ?? ownArrivals
  const bubbles = layout === `bubbles`
  const canOpenXref = Boolean(onOpenXref ?? onOpenSession)
  const openXref = (id: string) => (latest.current.onOpenXref ?? latest.current.onOpenSession)?.(id)
  // #EXP-1464 (chat-row-action.json): the rows expanded into their
  // waiting card, and the sessions the viewer answered from one (local).
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set())
  const [answered, setAnswered] = useState<ReadonlySet<string>>(() => new Set())
  const toggle = (rowId: string) =>
    setExpanded((prev) => {
      const next = new Set(prev)
      if (!next.delete(rowId)) next.add(rowId)
      return next
    })
  const actionOf = (row: SessionChatRow): ChatRowAction => {
    const sessionId =
      row.type === `session` ? row.card.sessionId : row.type === `report` || row.type === `event` ? row.sessionId : null
    const card = row.type === `session` ? row.card : null
    const action = chatRowAction({
      type: row.type,
      sessionId,
      rootId,
      state: (card?.state as SessionState | undefined) ?? null,
      // A flagged session's word names its flag (session-card.json).
      blocked: card !== null && card.state === `needs_you` && card.kindWord !== SESSION_CARD_COPY.states.needs_you,
      inline: sessionId !== null && waitingCardInline(rows, sessionId),
      answered: sessionId !== null && answered.has(sessionId),
    })
    if (action.action === `expand` && !WaitingCard) return { ...action, action: `open` }
    if (action.action === `open` && !(row.type === `session` ? onOpenSession : canOpenXref)) {
      return { action: `none`, sessionId: null, chevron: false, kindWord: null, tone: null }
    }
    return action
  }
  // #EXP-1736 (transcript-grouping.json): who sent each row, the groups.
  const layouts = useMemo(
    () => chatTranscriptLayout(rows, { chat, origin, viewerId, member, docked, fromStart }),
    [rows, chat, origin, viewerId, member, docked, fromStart]
  )
  const renderRow = (row: SessionChatRow): ReactNode => {
    const body = renderRowBody(row)
    if (body === null) return null
    const layout = layouts.get(row.id)
    const sender = layout?.sender ?? null
    const avatar = sender ? (
      <SenderAvatar
        sender={sender}
        user={sender.kind === `member` ? (latest.current.member?.(sender.id) ?? null) : null}
        agent={sender.kind === `agent` ? (latest.current.xrefAgent?.(sender.id) ?? null) : null}
      />
    ) : null
    return (
      <TranscriptRow key={row.id} layout={layout} avatar={avatar}>
        {body}
      </TranscriptRow>
    )
  }
  const renderRowBody = (row: SessionChatRow): ReactNode => {
    // FEED-140: a waiting plan / question never leaves the conversation.
    if (docked?.has(row.id) && row.type !== `plan` && row.type !== `question`) return null
    const enter = bubbles && isLive(row.id) ? CHAT_BUBBLE_ENTER_CLASS : undefined
    switch (row.type) {
      case `message`: {
        const layout = layouts.get(row.id)
        if (layout?.side === `full`) {
          // #EXP-1772: a system note is THE inline marker, never a bubble.
          return (
            <div key={row.id} data-slot="chat-system">
              <TranscriptMarker label={row.body} className={enter} />
            </div>
          )
        }
        const time = messageTime?.(row) ?? null
        if (layout?.side === `own`) {
          // #FEED-237 (message-delivery.json): the viewer's own row wears
          // its delivery mark; the menu's Cancel sending revokes it.
          const mine = Boolean(viewerId && row.authorId === viewerId)
          const state: DeliveryState =
            deliveryOf?.(row) ??
            deliveryState({ local: null, stored: true, deliveredAt: null, consumedAt: row.queued ? null : `consumed`, echoed: false })
          const actions = mine && onUnqueue ? bubbleActions({ state, stored: true }) : []
          return (
            <MessageBubble
              key={row.id}
              side="out"
              text={row.body}
              renderText={renderText}
              time={time}
              status={mine ? state : null}
              menu={actions.map((action) => ({
                kind: `item` as const,
                id: action,
                label: MESSAGE_DELIVERY_COPY.menu[action],
                destructive: action === `cancel`,
                onSelect: () => latest.current.onUnqueue?.(row.id),
              }))}
              className={enter}
            />
          )
        }
        // #EXP-1772: a relay names its session ABOVE the bubble; only the
        // name opens it.
        const link = layout?.sender?.link ?? null
        const relay =
          link && layout?.name && layout.sender
            ? { name: layout.sender.name, onPress: canOpenXref ? () => openXref(link) : undefined }
            : null
        return (
          <MessageBubble
            key={row.id}
            side="in"
            text={row.body}
            renderText={renderText}
            author={layout?.name ? transcriptSenderName(layout.sender) : null}
            relay={relay}
            time={time}
            className={enter}
          />
        )
      }
      case `report`: {
        // #EXP-1736: a child's report = its bubble (the summary), named
        // `<child> ›`, opening it.
        const layout = layouts.get(row.id)
        const open = actionOf(row).action !== `none`
        return (
          <MessageBubble
            key={row.id}
            side="in"
            text={row.summary || row.text}
            renderText={renderText}
            author={null}
            relay={layout?.name && layout.sender ? { name: layout.sender.name, onPress: open ? () => openXref(row.sessionId) : undefined } : null}
            className={enter}
          />
        )
      }
      case `divider`:
        return <TranscriptDivider key={row.id} kind={row.kind} at={row.at} className={enter} />
      case `locked`:
        return <TranscriptDivider key={row.id} kind={`locked-${row.reason}`} at={null} className={enter} />
      case `surface`:
        return (
          <SurfaceCard
            key={row.id}
            surface={row.surface}
            title={row.title}
            testKey={row.id}
            onReply={onSurfaceReply ? (text) => latest.current.onSurfaceReply?.(text) : undefined}
            animate={isLive(row.id)}
            className={bubbles ? `max-w-[94%]` : undefined}
          />
        )
      case `session`: {
        const action = actionOf(row)
        const card = action.kindWord ? { ...row.card, kindWord: action.kindWord, tone: action.tone ?? row.card.tone } : row.card
        const open =
          action.action === `expand`
            ? () => toggle(row.id)
            : action.action === `open`
              ? () => latest.current.onOpenSession?.(row.card.sessionId)
              : undefined
        const isExpanded = action.action === `expand` && expanded.has(row.id)
        const sessionCard = <SessionCard key={row.id} card={card} onOpen={open} expanded={isExpanded} />
        if (!isExpanded || !WaitingCard) return sessionCard
        const sessionId = row.card.sessionId
        const openChat = (
          <button
            type="button"
            data-testid={CHAT_ROW_ACTION_TEST_IDS.openChat}
            className="ml-auto shrink-0 text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:text-foreground active:opacity-70"
            onClick={() => openXref(sessionId)}
          >
            {CHAT_ROW_ACTION_COPY.openChat}
          </button>
        )
        const done = () => {
          setAnswered((prev) => new Set(prev).add(sessionId))
          setExpanded((prev) => {
            const next = new Set(prev)
            next.delete(row.id)
            return next
          })
        }
        return (
          <div key={row.id} data-testid={CHAT_ROW_ACTION_TEST_IDS.expanded} className="flex min-w-0 flex-col gap-2">
            {sessionCard}
            <WaitingCard sessionId={sessionId}>
              {(waiting) =>
                waiting === undefined ? (
                  <SkeletonRows rows={1} />
                ) : waiting === null ? null : waiting.type === `question` ? (
                  <ChatQuestionCard
                    row={waiting}
                    renderText={renderText}
                    attachmentSrc={attachmentSrc}
                    busy={pending?.has(waiting.id) ?? false}
                    headerAction={openChat}
                    onDismiss={
                      onDismiss
                        ? () => {
                            latest.current.onDismiss?.(waiting.id)
                            done()
                          }
                        : undefined
                    }
                    onAnswer={
                      onAnswer
                        ? (input) => {
                            latest.current.onAnswer?.(waiting.id, input)
                            done()
                          }
                        : undefined
                    }
                  />
                ) : (
                  <ChatPlanRow
                    row={waiting}
                    onOpen={() => onOpenPlan?.(waiting)}
                    onDismiss={
                      onDismiss
                        ? () => {
                            latest.current.onDismiss?.(waiting.id)
                            done()
                          }
                        : undefined
                    }
                  />
                )
              }
            </WaitingCard>
          </div>
        )
      }
      case `question`:
        return (
          <ChatQuestionCard
            key={row.id}
            row={row}
            renderText={renderText}
            attachmentSrc={attachmentSrc}
            busy={pending?.has(row.id) ?? false}
            onDismiss={onDismiss ? () => latest.current.onDismiss?.(row.id) : undefined}
            onAnswer={onAnswer ? (input) => latest.current.onAnswer?.(row.id, input) : undefined}
          />
        )
      case `plan`:
        return (
          <ChatPlanRow
            key={row.id}
            row={row}
            onOpen={() => onOpenPlan?.(row)}
            onDismiss={onDismiss ? () => latest.current.onDismiss?.(row.id) : undefined}
          />
        )
      case `subagent`:
        return (
          <div
            key={row.id}
            data-slot="chat-subagent"
            className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground"
          >
            <SubagentIcon className="size-3.5 shrink-0" />
            <span className="min-w-0 truncate">{row.title}</span>
            {row.status && <span className="shrink-0">{row.status}</span>}
          </div>
        )
      case `picture`:
        return (
          <figure key={row.id} data-slot="chat-picture" className="flex flex-col gap-1">
            <img
              src={attachmentSrc(row.attachmentId)}
              alt={row.caption ?? ``}
              width={row.width ?? undefined}
              height={row.height ?? undefined}
              loading="lazy"
              className="h-auto max-h-96 w-auto max-w-full rounded-lg border border-glass-stroke-card object-contain"
            />
            {row.caption && (
              <figcaption className="text-xs text-muted-foreground">{row.caption}</figcaption>
            )}
          </figure>
        )
      case `event`: {
        // #EXP-1464: a line about another session opens it. #EXP-1772:
        // every event is THE inline marker, never a bubble or a bare line.
        const action = actionOf(row)
        const sessionId = action.sessionId
        const open = action.action !== `none` && sessionId ? () => openXref(sessionId) : undefined
        return (
          <div key={row.id} data-slot="chat-event" data-event={row.event} data-testid={open ? CHAT_ROW_ACTION_TEST_IDS.event : undefined}>
            <TranscriptMarker label={row.text} onPress={open} className={enter} />
          </div>
        )
      }
      case `suggestions`:
        return (
          <ChatSuggestionsCard
            key={row.id}
            row={row}
            actions={suggestions}
            onOpenSession={onOpenSession ? (id) => latest.current.onOpenSession?.(id) : undefined}
            renderIssueRef={renderIssueRef}
          />
        )
      default:
        return null
    }
  }
  // S-3 (chats.json CHAT MODE): in `bubbles` the agent's cards sit in the
  // agent column (left, max 94%), beside the person's right-hand bubbles.
  const renderInColumn = (row: SessionChatRow): ReactNode => {
    const node = renderRow(row)
    if (!bubbles || node === null || !AGENT_COLUMN_ROWS.has(row.type)) return node
    return (
      <div data-slot="chat-agent-column" className="w-full max-w-[94%] self-start">
        {node}
      </div>
    )
  }
  return (
    <div
      data-testid="chat-conversation"
      data-layout={layout}
      className={cn(`flex flex-col`, className)}
    >
      {rows.map((row) => (
        <StableRow
          key={row.id}
          row={row}
          render={renderInColumn}
          deps={[
            bubbles,
            isLive(row.id),
            docked?.has(row.id) ?? false,
            pending?.has(row.id) ?? false,
            expanded.has(row.id),
            row.type === `session` ? answered.has(row.card.sessionId) : false,
            row.type === `session` ? waitingCardInline(rows, row.card.sessionId) : false,
            row.type === `session` && expanded.has(row.id) ? pending : null,
            WaitingCard ?? null,
            layoutKey(layouts.get(row.id)),
            row.type === `message` ? (messageTime?.(row) ?? null) : null,
            viewerId ?? null,
            !onUnqueue,
            row.type === `message` ? (deliveryOf?.(row) ?? null) : null,
            !onAnswer,
            !onDecide,
            !onDismiss,
            !onOpenSession,
            !onOpenXref,
            row.type === `message` || row.type === `report` ? xrefAgent : null,
            member ?? null,
            !onSurfaceReply,
            renderText,
            attachmentSrc,
            row.type === `suggestions` ? suggestions : null,
            row.type === `suggestions` ? renderIssueRef : null,
          ]}
        />
      ))}
      {tail}
    </div>
  )
}

/** EXP-1323: what a suggested tasks card can do (`sessions.startSuggestion`
 *  / `startAllSuggestions` / `dismissSuggestion` / `fileSuggestion`). */
export interface ChatSuggestionActions {
  /** Start the item as a child (Options: on a device, with a model). */
  onStart: (rowId: string, itemId: string, options?: { deviceId?: string; model?: string }) => void
  onStartAll: (rowId: string) => void
  /** Do it here: the item goes to the posting session. */
  onHere: (rowId: string, itemId: string) => void
  onDismiss: (rowId: string, itemId: string) => void
  onFile: (rowId: string, itemId: string) => void
  /** Start on… rows (the runnable devices). */
  devices?: readonly { id: string; name: string; icon?: MenuIcon }[]
  /** Start with… rows (contract `codingModel`, never a default row); a
   *  function resolves them for the card's POSTING session (its agent's). */
  models?:
    | readonly { value: string; label: string }[]
    | ((sessionId: string) => readonly { value: string; label: string }[])
  /** In-flight keys (`suggestionKey`; Start all = `suggestionKey(row)`). */
  pending?: ReadonlySet<string>
}

/** The pending key of one item (or, without an item, the card's Start all). */
export function suggestionKey(rowId: string, itemId?: string): string {
  return itemId ? `${rowId}:${itemId}` : `${rowId}:*`
}

/** EXP-1323: THE suggested tasks card ×4 = the Work card's shell: a header
 *  (`3 suggested tasks`, `· 2 started`, Start all while more than one is
 *  open), then hairline-divided items. N-12: while any item is open the row
 *  is a DockCard (`ChatDock`) and this list is its sheet
 *  (`ChatSuggestionsSheet`); in the Conversation the card stays for the
 *  rows with no open item. Open = title, text, Start and the ⋯ menu (Do it
 *  here · Start on… · Start with… · File as issue · Dismiss), one row on
 *  every width; started = the child's session card (its caption under the
 *  name) opening it; here = `Sent here`; filed = `Filed EXP-9`. */
export function ChatSuggestionsCard({
  row,
  actions,
  onOpenSession,
  renderIssueRef,
}: {
  row: Extract<SessionChatRow, { type: `suggestions` }>
  actions?: ChatSuggestionActions
  onOpenSession?: (sessionId: string) => void
  renderIssueRef?: (issueId: string, identifier: string | null) => ReactNode
}) {
  const pending = actions?.pending
  return (
    <div
      data-slot="chat-suggestions"
      className="min-w-0 overflow-clip rounded-lg border border-glass-stroke-card bg-glass-card-opaque text-xs"
    >
      <div className="flex min-h-9 min-w-0 items-center gap-2 px-3 py-1.5">
        <SuggestionsIcon className="size-3.5 shrink-0 text-muted-foreground" />
        <span data-slot="chat-suggestions-header" className="min-w-0 truncate font-medium">
          {row.header}
        </span>
        {row.startAll && actions && (
          <Button
            type="button"
            size="xs"
            className="ml-auto shrink-0"
            data-testid="chat-suggestions-start-all"
            pending={pending?.has(suggestionKey(row.id)) ?? false}
            onClick={() => actions.onStartAll(row.id)}
          >
            {SUGGESTIONS_COPY.startAll}
          </Button>
        )}
      </div>
      <div className="divide-y divide-glass-stroke border-t border-glass-stroke">
        {row.items.map((item) => (
          <SuggestionItemRow
            key={item.id}
            rowId={row.id}
            sessionId={row.sessionId}
            item={item}
            actions={actions}
            onOpenSession={onOpenSession}
            renderIssueRef={renderIssueRef}
          />
        ))}
      </div>
    </div>
  )
}

function SuggestionItemRow({
  rowId,
  sessionId,
  item,
  actions,
  onOpenSession,
  renderIssueRef,
}: {
  rowId: string
  /** The session that posted the card (its agent's models). */
  sessionId: string
  item: SuggestionRow
  actions?: ChatSuggestionActions
  onOpenSession?: (sessionId: string) => void
  renderIssueRef?: (issueId: string, identifier: string | null) => ReactNode
}) {
  if (item.state === `started`) {
    if (item.card) {
      const card = item.card
      return (
        <div data-slot="chat-suggestion" data-state={item.state}>
          <SessionCard
            card={card}
            onOpen={onOpenSession ? () => onOpenSession(card.sessionId) : undefined}
            className="rounded-none border-0 bg-transparent"
          />
        </div>
      )
    }
    // Started, the child's row not synced yet.
    return (
      <div
        data-slot="chat-suggestion"
        data-state={item.state}
        className="flex min-h-9 min-w-0 items-center gap-2 px-3 text-muted-foreground"
      >
        <Spinner size={12} label="Starting" />
        <span className="min-w-0 truncate">{item.title}</span>
      </div>
    )
  }
  if (item.state === `here` || item.state === `filed`) {
    const filed =
      item.state === `filed` && item.issueId
        ? (renderIssueRef?.(item.issueId, item.identifier) ?? item.identifier)
        : null
    const [before, after] = SUGGESTIONS_COPY.filed.split(`{issue}`)
    return (
      <div
        data-slot="chat-suggestion"
        data-state={item.state}
        className="flex min-h-9 min-w-0 items-center gap-2 px-3 py-1.5"
      >
        <span className="min-w-0 flex-1 truncate text-muted-foreground">{item.title}</span>
        <span className="flex shrink-0 items-center gap-1 text-muted-foreground">
          {item.state === `here` ? (
            SUGGESTIONS_COPY.sentHere
          ) : (
            <>
              {before}
              {filed}
              {after}
            </>
          )}
        </span>
      </div>
    )
  }
  const key = suggestionKey(rowId, item.id)
  const busy = actions?.pending?.has(key) ?? false
  const models =
    typeof actions?.models === `function` ? actions.models(sessionId) : (actions?.models ?? [])
  const entries: MenuEntry[] = actions
    ? [
        {
          kind: `item`,
          id: `here`,
          label: SUGGESTIONS_COPY.here,
          onSelect: () => actions.onHere(rowId, item.id),
        },
        {
          kind: `submenu`,
          label: SUGGESTIONS_COPY.startOn,
          disabled: (actions.devices?.length ?? 0) === 0,
          entries: (actions.devices ?? []).map((device) => ({
            kind: `item` as const,
            id: device.id,
            label: device.name,
            icon: device.icon,
            onSelect: () => actions.onStart(rowId, item.id, { deviceId: device.id }),
          })),
        },
        {
          kind: `submenu`,
          label: SUGGESTIONS_COPY.startWith,
          disabled: models.length === 0,
          entries: models.map((model) => ({
            kind: `item` as const,
            id: model.value,
            label: model.label,
            onSelect: () => actions.onStart(rowId, item.id, { model: model.value }),
          })),
        },
        { kind: `separator` },
        {
          kind: `item`,
          id: `file`,
          label: SUGGESTIONS_COPY.file,
          onSelect: () => actions.onFile(rowId, item.id),
        },
        {
          kind: `item`,
          id: `dismiss`,
          label: SUGGESTIONS_COPY.dismiss,
          onSelect: () => actions.onDismiss(rowId, item.id),
        },
      ]
    : []
  return (
    <div
      data-slot="chat-suggestion"
      data-state={item.state}
      className="flex min-w-0 items-start gap-3 px-3 py-2.5"
    >
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium text-foreground">{item.title}</div>
        {item.text && <div className="mt-0.5 text-muted-foreground">{item.text}</div>}
      </div>
      {actions && (
        <div data-slot="chat-suggestion-actions" className="flex shrink-0 items-center gap-1.5">
          <Menu
            entries={entries}
            align="end"
            title={item.title}
            aria-label={SUGGESTIONS_COPY.options}
            contentClassName="w-56"
            trigger={
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={SUGGESTIONS_COPY.options}
                title={SUGGESTIONS_COPY.options}
                data-testid="chat-suggestion-options"
                className="text-muted-foreground hover:text-foreground"
                disabled={busy}
              >
                <MoreIcon className="size-3.5" />
              </Button>
            }
          />
          <Button
            type="button"
            size="xs"
            data-testid="chat-suggestion-start"
            pending={busy}
            onClick={() => actions.onStart(rowId, item.id)}
          >
            {SUGGESTIONS_COPY.start}
          </Button>
        </div>
      )}
    </div>
  )
}

/** N-12: THE suggestions sheet (a DockCard opened): the items as the card
 *  lists them (`SuggestionItemRow`); the footer = Start all while more than
 *  one is open. */
export function ChatSuggestionsSheet({
  row,
  actions,
  onOpenSession,
  renderIssueRef,
}: {
  row: Extract<SessionChatRow, { type: `suggestions` }>
  actions?: ChatSuggestionActions
  onOpenSession?: (sessionId: string) => void
  renderIssueRef?: (issueId: string, identifier: string | null) => ReactNode
}) {
  const pending = actions?.pending
  return (
    <>
      <DockSheetBody className="px-1">
        <div data-slot="chat-suggestions" data-testid="chat-suggestions-sheet" className="divide-y divide-glass-stroke text-xs">
          {row.items.map((item) => (
            <SuggestionItemRow
              key={item.id}
              rowId={row.id}
              sessionId={row.sessionId}
              item={item}
              actions={actions}
              onOpenSession={onOpenSession}
              renderIssueRef={renderIssueRef}
            />
          ))}
        </div>
      </DockSheetBody>
      {row.startAll && actions && (
        <DockSheetFooter>
          <div className="flex justify-end">
            <Button
              type="button"
              size="sm"
              data-testid="chat-suggestions-start-all"
              pending={pending?.has(suggestionKey(row.id)) ?? false}
              onClick={() => actions.onStartAll(row.id)}
            >
              {SUGGESTIONS_COPY.startAll}
            </Button>
          </div>
        </DockSheetFooter>
      )}
    </>
  )
}

/** EXP-1280: a question card's answer: the labels (`answers`; `answer` =
 *  them joined, the compat string) + the inline surface's data model. */
export type ChatQuestionAnswer = {
  answer: string
  answers?: string[]
  surfaceData?: Record<string, unknown>
}

/** A closed card's free answers (not an option) as THE answered line. */
function freeAnswers(options: readonly { label: string }[], picks: readonly string[]): string[] {
  return picks.filter((pick) => !options.some((option) => option.label === pick))
}

/** EXP-1316/1280: a question row IN THE TRANSCRIPT = THE question card ×4
 *  (plan-card.json): the question, the inline surface, the options (THE
 *  option rows or the picture tiles, multi aware), the `custom` free-text
 *  row and its Answer button; closed once answered (the picks highlighted,
 *  the rest inert). N-12: a card the viewer may answer lives in THE Dock
 *  instead (`ChatDock`) and opens as `ChatQuestionSheet`. */
function ChatQuestionCard({
  row,
  renderText,
  attachmentSrc,
  busy,
  headerAction,
  onDismiss,
  onAnswer,
}: {
  row: Extract<SessionChatRow, { type: `question` }>
  renderText: (text: string) => ReactNode
  attachmentSrc: (attachmentId: string) => string
  busy: boolean
  /** #EXP-1464: the header's trailing link (`Open chat`); it brings the
   *  `Question` label line. */
  headerAction?: ReactNode
  /** #EXP-1561: THE × while it waits (absent = none). */
  onDismiss?: () => void
  onAnswer?: (input: ChatQuestionAnswer) => void
}) {
  const form = useQuestionForm(row, busy, onAnswer)
  // #EXP-1561 (waiting-cards.json): a dismissed card = ONE quiet line.
  if (row.dismissed) {
    return <AskDismissedCard title={askDockTitle(row.question)} data-card-id={row.id} />
  }
  return (
    <AskCard
      label={headerAction ? ASK_DOCK_COPY.question : undefined}
      meta={headerAction}
      onDismiss={!row.answered && !busy ? onDismiss : undefined}
      data-slot="chat-question"
      data-testid={PLAN_CARD_TEST_IDS.card}
      data-kind="question"
      data-card-id={row.id}
      data-answered={row.answered || undefined}
    >
      <div className="text-sm text-foreground/90">{renderText(stripPendingUploads(row.question))}</div>
      {row.surface && (
        <PlanCardSurface
          surface={row.surface}
          data={row.answered ? row.surfaceData : null}
          disabled={!form.interactive}
          dataRef={form.surfaceData}
          testKey={row.id}
        />
      )}
      <QuestionOptions row={row} form={form} attachmentSrc={attachmentSrc} />
      {row.answered ? (
        form.free.length > 0 || form.shown.length === 0 ? (
          <AskAnsweredLine answer={form.free.length > 0 ? joinAnswers(form.free) : row.answer} />
        ) : null
      ) : busy ? (
        <AskPrompt options={[]} answerable={false} locked={{ labels: form.labels }} onAnswer={noop} />
      ) : onAnswer ? (
        <Button
          type="button"
          size="sm"
          className="mt-2"
          data-testid="chat-question-answer"
          disabled={form.issue !== null}
          onClick={form.send}
        >
          {PLAN_CARD_COPY.answer}
        </Button>
      ) : null}
    </AskCard>
  )
}

/** The question card's form (the transcript card and the sheet share it). */
function useQuestionForm(
  row: Extract<SessionChatRow, { type: `question` }>,
  busy: boolean,
  onAnswer: ((input: ChatQuestionAnswer) => void) | undefined
) {
  const [picked, setPicked] = useState<string[]>([])
  const [customText, setCustomText] = useState(``)
  const surfaceData = useSurfaceDataRef()
  const interactive = !row.answered && !busy && onAnswer !== undefined
  const shown = row.answered ? closedPicks(row.options, row.answer, row.answers) : picked
  const labels = questionAnswer(picked, customText, row.multi)
  const issue = answerIssue(
    { kind: `question`, question: row.question, options: row.options.map((o) => o.label), multi: row.multi, custom: row.custom },
    { answers: labels }
  )
  const free = row.answered
    ? shown.length > 0
      ? freeAnswers(row.options, shown)
      : row.answer
        ? [row.answer]
        : []
    : []
  const send = () => {
    if (!interactive || issue) return
    onAnswer?.({
      answer: joinAnswers(labels),
      answers: labels,
      ...(row.surface && surfaceData.current ? { surfaceData: surfaceData.current } : {}),
    })
  }
  return { interactive, shown, labels, issue, free, send, surfaceData, pick: setPicked, customText, setCustomText }
}

const noop = () => {}

/** A plan card's decision (`sessions.answer`): `fresh` = start the work in
 *  a fresh context; `feedback` = the note; EXP-1280: `answers` by question
 *  id + the inline surface's data model. */
export type ChatPlanDecision = {
  decision: `approved` | `rejected`
  fresh?: boolean
  feedback?: string
  answers?: Record<string, string[]>
  surfaceData?: Record<string, unknown>
}

/** N-12: THE question sheet (a DockCard opened): the question, the inline
 *  surface, the options and the free-text row in the body; the footer =
 *  Answer (plan-card.json). */
export function ChatQuestionSheet({
  row,
  renderText,
  attachmentSrc,
  busy,
  onAnswer,
}: {
  row: Extract<SessionChatRow, { type: `question` }>
  renderText: (text: string) => ReactNode
  attachmentSrc: (attachmentId: string) => string
  busy: boolean
  onAnswer?: (input: ChatQuestionAnswer) => void
}) {
  const form = useQuestionForm(row, busy, onAnswer)
  return (
    <>
      <DockSheetBody>
        <div data-testid={PLAN_CARD_TEST_IDS.card} data-kind="question" className="text-sm text-foreground/90">
          {renderText(stripPendingUploads(row.question))}
        </div>
        {row.surface && (
          <PlanCardSurface
            surface={row.surface}
            data={row.answered ? row.surfaceData : null}
            disabled={!form.interactive}
            dataRef={form.surfaceData}
            testKey={row.id}
          />
        )}
        <QuestionOptions row={row} form={form} attachmentSrc={attachmentSrc} />
      </DockSheetBody>
      <DockSheetFooter>
        <div className="flex items-center justify-end gap-2">
          {busy ? (
            <AskPrompt options={[]} answerable={false} locked={{ labels: form.labels }} onAnswer={noop} />
          ) : (
            <Button
              type="button"
              size="sm"
              data-testid="chat-question-answer"
              disabled={form.issue !== null || !form.interactive}
              onClick={form.send}
            >
              {PLAN_CARD_COPY.answer}
            </Button>
          )}
        </div>
      </DockSheetFooter>
    </>
  )
}

/** The question's options block with the real picture urls. */
function QuestionOptions({
  row,
  form,
  attachmentSrc,
}: {
  row: Extract<SessionChatRow, { type: `question` }>
  form: ReturnType<typeof useQuestionForm>
  attachmentSrc: (attachmentId: string) => string
}) {
  return (
    <>
      {row.multi && !row.answered && row.options.length > 0 && (
        <div className="mt-2">
          <PlanCardCaption>{PLAN_CARD_COPY.chooseAny}</PlanCardCaption>
        </div>
      )}
      <PlanCardOptions
        options={row.options}
        multi={row.multi}
        picked={form.shown}
        onPick={form.interactive ? form.pick : undefined}
        attachmentSrc={attachmentSrc}
        label={row.question}
      />
      {form.interactive && row.custom && (
        <PlanCardCustomField value={form.customText} onChange={form.setCustomText} label={row.question} />
      )}
    </>
  )
}

/** The plan card's form (the transcript card and the sheet share it). */
function usePlanForm(
  row: Extract<SessionChatRow, { type: `plan` }>,
  busy: boolean,
  onDecide: ((input: ChatPlanDecision) => void) | undefined
) {
  // FEED-140: `superseded` (a newer plan replaced it) is a decision too.
  const decided = row.decision !== null
  const [picks, setPicks] = useState<Record<string, string[]>>({})
  const [custom, setCustom] = useState<Record<string, string>>({})
  const [feedback, setFeedback] = useState(``)
  const surfaceData = useSurfaceDataRef()
  const interactive = !decided && !busy && onDecide !== undefined
  const answers = useMemo(() => {
    const out: Record<string, string[]> = {}
    for (const question of row.questions) {
      const answer = questionAnswer(picks[question.id] ?? [], custom[question.id] ?? ``, question.multi)
      if (answer.length > 0) out[question.id] = answer
    }
    return out
  }, [row.questions, picks, custom])
  const approveIssue = answerIssue(
    { kind: `plan`, plan: row.plan, questions: row.questions },
    { decision: `approved`, answers }
  )
  const decide = (option: PlanOption) => {
    if (!interactive) return
    if (option.decision === `approved` && approveIssue) return
    const note = option.decision === `rejected` ? feedback.trim() : ``
    onDecide?.({
      decision: option.decision === `rejected` ? `rejected` : `approved`,
      ...(option.decision === `approved` && option.fresh ? { fresh: true } : {}),
      ...(note ? { feedback: note } : {}),
      ...(Object.keys(answers).length > 0 ? { answers } : {}),
      ...(row.surface && surfaceData.current ? { surfaceData: surfaceData.current } : {}),
    })
  }
  const block = (question: (typeof row.questions)[number], attachmentSrc: (id: string) => string, className?: string) => {
    const picked = decided ? (row.answers?.[question.id] ?? []) : (picks[question.id] ?? [])
    const free = decided ? freeAnswers(question.options, picked) : []
    return (
      <PlanCardQuestionBlock
        key={question.id}
        question={question}
        picked={picked}
        customText={custom[question.id] ?? ``}
        onPick={interactive ? (next) => setPicks((prev) => ({ ...prev, [question.id]: next })) : undefined}
        onCustom={(text) => setCustom((prev) => ({ ...prev, [question.id]: text }))}
        attachmentSrc={attachmentSrc}
        closedFree={free.length > 0 ? <AskAnsweredLine answer={joinAnswers(free)} /> : undefined}
        className={className}
      />
    )
  }
  return { decided, interactive, approveIssue, feedback, setFeedback, decide, block, surfaceData }
}

/** #EXP-1662: a plan in the Conversation = ONE row (plan-page.json); a press
 *  opens the chat's plan tab on it. A superseded or dismissed plan keeps its
 *  one closed line. */
function ChatPlanRow({
  row,
  onOpen,
  onDismiss,
}: {
  row: Extract<SessionChatRow, { type: `plan` }>
  onOpen?: () => void
  /** #EXP-1561: THE × while it waits (absent = none). */
  onDismiss?: () => void
}) {
  // #EXP-1561 (waiting-cards.json): a dismissed plan = ONE quiet line.
  if (row.decision === `dismissed`) {
    return <AskDismissedCard plan title={askDockTitle(row.plan)} data-card-id={row.id} />
  }
  // FEED-140: a plan the agent replaced paints closed, never its body.
  if (row.decision === `superseded`) {
    return (
      <AskCard
        plan
        label={ASK_DOCK_COPY.planReady}
        data-slot="chat-plan"
        data-testid={PLAN_CARD_TEST_IDS.card}
        data-kind="plan"
        data-decision="superseded"
        className="opacity-80"
      >
        <div data-slot="ask-answered" className="flex items-start gap-1.5 text-xs text-muted-foreground">
          <CloseIcon className="mt-0.5 size-3.5 shrink-0" />
          <span className="min-w-0 truncate">{planDecisionLabel(`superseded`, false)}</span>
        </div>
      </AskCard>
    )
  }
  const waiting = row.decision === null
  const Lead = row.decision === `approved` ? PlanApprovedIcon : PlanRowIcon
  return (
    <div
      data-slot="chat-plan"
      data-testid={PLAN_CARD_TEST_IDS.card}
      data-kind="plan"
      data-card-id={row.id}
      data-decision={row.decision ?? undefined}
      className="flex w-full min-w-0 items-center rounded-xl border border-glass-stroke-card bg-glass-card text-sm transition-colors duration-fast hover:bg-glass-active"
      style={{ minHeight: PLAN_PAGE_LAYOUT.rowHeight }}
    >
      <button
        type="button"
        data-testid="plan-row"
        className="flex min-w-0 flex-1 items-center gap-2 self-stretch rounded-xl px-3 text-left outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
        onClick={onOpen}
      >
        <Lead
          className={cn(
            `size-4 shrink-0`,
            waiting ? `text-yellow-400` : row.decision === `approved` ? `text-emerald-400` : `text-muted-foreground`
          )}
        />
        <span className="min-w-0 truncate font-medium">{planRowLabel(row.plan)}</span>
        <span className={cn(`shrink-0 text-xs`, waiting ? `text-yellow-400` : `text-muted-foreground`)}>
          {waiting ? PLAN_PAGE_COPY.needsYou : planDecisionLabel(row.decision as `approved` | `rejected`, row.fresh)}
        </span>
        <span className="flex-1" />
        <PlanRowChevron className="size-4 shrink-0 text-muted-foreground" />
      </button>
      {waiting && onDismiss && <span className="mr-2 flex shrink-0"><AskDismissButton onClick={onDismiss} /></span>}
    </div>
  )
}

/** N-12: THE plan sheet (a DockCard opened, plan-card.json `stepper`): page
 *  0 = the plan, the surface and (below `minQuestions` questions) the
 *  questions; else one page per question, "Question n of m" as the header's
 *  meta; the footer = Back / Next, the last page THE decision
 *  (`PlanCardFooter`). The page is local, never synced. */
export function ChatPlanSheet({
  row,
  renderText,
  attachmentSrc,
  busy,
  onDecide,
  questionsOnly = false,
}: {
  row: Extract<SessionChatRow, { type: `plan` }>
  renderText: (text: string) => ReactNode
  attachmentSrc: (attachmentId: string) => string
  busy: boolean
  onDecide?: (input: ChatPlanDecision) => void
  /** #EXP-1493: THE plan page's sheet: the document is the page, so the
   *  sheet holds the questions (and the decision) only. */
  questionsOnly?: boolean
}) {
  const form = usePlanForm(row, busy, onDecide)
  const [step, setStep] = useState(0)
  const [rejecting, setRejecting] = useState(false)
  const stepped = !form.decided && stepperApplies({ kind: `plan`, plan: row.plan, questions: row.questions })
  const count = row.questions.length
  const lead = questionsOnly ? 0 : 1
  const pages = stepped ? lead + count : 1
  const page = Math.min(step, pages - 1)
  const last = page === pages - 1
  const question = stepped && page >= lead ? row.questions[page - lead] : undefined
  useDockSheetMeta(
    question ? <span data-testid={PLAN_CARD_TEST_IDS.step}>{stepLabel(page - lead + 1, count)}</span> : null
  )
  return (
    <>
      <DockSheetBody>
        <div data-testid={PLAN_CARD_TEST_IDS.card} data-kind="plan" data-page={page}>
          {question ? (
            form.block(question, attachmentSrc, `mt-0`)
          ) : questionsOnly ? (
            <>
              {row.questions.map((q) => form.block(q, attachmentSrc))}
              {form.decided && <PlanClosedLines row={row} />}
            </>
          ) : (
            <>
              <div className="text-sm">{renderText(stripPendingUploads(row.plan))}</div>
              {row.surface && (
                <PlanCardSurface
                  surface={row.surface}
                  data={form.decided ? row.surfaceData : null}
                  disabled={!form.interactive}
                  dataRef={form.surfaceData}
                  testKey={row.id}
                />
              )}
              {!stepped && row.questions.map((q) => form.block(q, attachmentSrc))}
            </>
          )}
        </div>
      </DockSheetBody>
      <DockSheetFooter>
        <PlanCardFooter
          onBack={page > 0 ? () => setStep(page - 1) : undefined}
          onNext={!last ? () => setStep(page + 1) : undefined}
          decision={
            last && form.interactive
              ? {
                  approveIssue: form.approveIssue,
                  rejecting,
                  onRejecting: setRejecting,
                  feedback: form.feedback,
                  onFeedback: form.setFeedback,
                  onDecide: form.decide,
                  busy,
                }
              : undefined
          }
        />
        {busy && !form.decided && (
          <AskPrompt options={[]} answerable={false} locked={{ labels: [] }} onAnswer={noop} />
        )}
      </DockSheetFooter>
    </>
  )
}

const PlanRowIcon = conceptIcon(`coding-plan`)
const PlanApprovedIcon = conceptIcon(`ui-check`)

const PlanRowChevron = conceptIcon(`ui-chevron-right`)

/** A decided plan's closing lines: the decision label + the feedback. */
function PlanClosedLines({ row }: { row: Extract<SessionChatRow, { type: `plan` }> }) {
  if (row.decision === null) return null
  return (
    <>
      <AskAnsweredLine
        answer={planDecisionLabel(row.decision as `approved` | `rejected` | `superseded` | `dismissed`, row.fresh)}
      />
      {row.feedback && (
        <div data-testid={PLAN_CARD_TEST_IDS.feedback} className="mt-1 text-xs text-muted-foreground">
          {PLAN_CARD_COPY.feedbackLine.replace(`{feedback}`, () => row.feedback ?? ``)}
        </div>
      )}
    </>
  )
}

/** #EXP-1662: THE floating questions panel's body (plan-page.json `panel`): THE
 *  stepper (one question at a time, Back / Next) and, pinned at its foot, THE
 *  decision; decided = the closed card (every question's picks, the decision
 *  label, the feedback). */
export function PlanRail({
  row,
  attachmentSrc = defaultAttachmentSrc,
  busy,
  onDecide,
  heading = false,
  className,
}: {
  row: Extract<SessionChatRow, { type: `plan` }>
  attachmentSrc?: (attachmentId: string) => string
  busy: boolean
  onDecide?: (input: ChatPlanDecision) => void
  /** #FEED-191: THE wide rail leads with the `copy.questions` heading, the
   *  step label trailing it (phones' floating panel: the label alone). */
  heading?: boolean
  className?: string
}) {
  const form = usePlanForm(row, busy, onDecide)
  const [step, setStep] = useState(0)
  const [rejecting, setRejecting] = useState(false)
  const count = row.questions.length
  const page = Math.min(step, Math.max(0, count - 1))
  const question = form.decided ? undefined : row.questions[page]
  const last = page >= count - 1
  return (
    <div data-testid="plan-rail" className={cn(`flex min-h-0 flex-col gap-3`, className)}>
      {heading ? (
        <div className="flex shrink-0 items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-sm font-semibold">{PLAN_PAGE_COPY.questions}</span>
          {question && count > 1 && (
            <span data-testid={PLAN_CARD_TEST_IDS.step} className="shrink-0 text-xs text-muted-foreground">
              {stepLabel(page + 1, count)}
            </span>
          )}
        </div>
      ) : (
        question &&
        count > 1 && (
          <span data-testid={PLAN_CARD_TEST_IDS.step} className="shrink-0 self-end text-xs text-muted-foreground">
            {stepLabel(page + 1, count)}
          </span>
        )
      )}
      <div className="min-h-0 flex-auto overflow-y-auto overscroll-contain">
        <div data-testid={PLAN_CARD_TEST_IDS.card} data-kind="plan" data-page={page}>
          {question
            ? form.block(question, attachmentSrc, `mt-0`)
            : form.decided
              ? row.questions.map((q) => form.block(q, attachmentSrc))
              : null}
          {form.decided && <PlanClosedLines row={row} />}
        </div>
      </div>
      {!form.decided && (
        <div className="flex shrink-0 flex-col gap-2">
          <PlanCardFooter
            onBack={page > 0 ? () => setStep(page - 1) : undefined}
            onNext={!last ? () => setStep(page + 1) : undefined}
            decision={
              last && form.interactive
                ? {
                    approveIssue: form.approveIssue,
                    rejecting,
                    onRejecting: setRejecting,
                    feedback: form.feedback,
                    onFeedback: form.setFeedback,
                    onDecide: form.decide,
                    busy,
                  }
                : undefined
            }
          />
          {busy && <AskPrompt options={[]} answerable={false} locked={{ labels: [] }} onAnswer={noop} />}
        </div>
      )}
    </div>
  )
}

const defaultAttachmentSrc = (attachmentId: string) => `/api/attachments/${attachmentId}`

/** N-12: THE Dock in a chat or a session (fixtures/dock.json): the
 *  host's `sources` (THE Queue, the queued messages, banners) and the open
 *  suggestions as DockCards above the composer. S-3: a
 *  waiting plan or question is never a DockCard, and nothing opens
 *  itself; `asks` (the specimens' legacy pile only) still docks them, each
 *  opening as its sheet (`ChatPlanSheet` / `ChatQuestionSheet`). */
export function ChatDock({
  rows,
  sessionName,
  renderText,
  attachmentSrc = defaultAttachmentSrc,
  pending,
  onAnswer,
  onDecide,
  sources,
  renderSheet,
  renderAction,
  suggestions,
  onOpenSession,
  renderIssueRef,
  composer,
  onHeight,
  phoneClearance,
  asks: withAsks = false,
  initialOpenId = null,
  initialExpanded,
  className,
}: {
  rows: readonly SessionChatRow[]
  sessionName: (sessionId: string) => string | null
  renderText: (text: string) => ReactNode
  /** EXP-1280: a picture's url (the cards' tiles); default the app's own. */
  attachmentSrc?: (attachmentId: string) => string
  pending?: ReadonlySet<string>
  onAnswer: (rowId: string, input: ChatQuestionAnswer) => void
  onDecide: (rowId: string, input: ChatPlanDecision) => void
  /** The host's other cards (THE Queue, the queued messages, banners). */
  sources?: readonly DockSource[]
  /** Those cards' sheets. */
  renderSheet?: (row: DockRow, close: () => void) => ReactNode
  renderAction?: (row: DockRow) => ReactNode
  /** EXP-1323: the suggested-tasks cards' actions (absent = read-only, and
   *  the rows stay in the Conversation). */
  suggestions?: ChatSuggestionActions
  onOpenSession?: (sessionId: string) => void
  renderIssueRef?: (issueId: string, identifier: string | null) => ReactNode
  composer?: ReactNode
  onHeight?: (height: number) => void
  phoneClearance?: boolean
  /** S-3: dock the waiting plans/questions too (legacy; hosts never do). */
  asks?: boolean
  /** The sheet open on mount (the styleguide's open state). */
  initialOpenId?: string | null
  initialExpanded?: boolean
  className?: string
}) {
  const asks = useMemo<DockSource[]>(
    () =>
      (withAsks ? chatPendingAsks(rows, sessionName) : []).map((ask) =>
        ask.kind === `plan`
          ? { kind: `plan`, id: ask.id, text: ask.text, sessionName: ask.sessionName, questions: ask.questions ?? 0, createdAt: ask.createdAt }
          : { kind: `question`, id: ask.id, text: ask.text, sessionName: ask.sessionName, createdAt: ask.createdAt }
      ),
    [rows, sessionName, withAsks]
  )
  const suggested = useMemo<DockSource[]>(() => (suggestions ? suggestionSources(rows) : []), [rows, suggestions])
  const dockedRows = useMemo(() => dockRows([...asks, ...suggested, ...(sources ?? [])]), [asks, suggested, sources])
  const [openId, setOpenId] = useState<string | null>(initialOpenId)
  return (
    <Dock
      rows={dockedRows}
      openId={openId}
      onOpenChange={setOpenId}
      className={className}
      composer={composer}
      onHeight={onHeight}
      phoneClearance={phoneClearance}
      initialExpanded={initialExpanded}
      renderAction={renderAction}
      renderSheet={(dockRow, close) => {
        const row = rows.find((r) => r.id === dockRow.id)
        if (row?.type === `plan`) {
          return (
            <ChatPlanSheet
              row={row}
              renderText={renderText}
              attachmentSrc={attachmentSrc}
              busy={pending?.has(row.id) ?? false}
              onDecide={(input) => onDecide(row.id, input)}
            />
          )
        }
        if (row?.type === `question`) {
          return (
            <ChatQuestionSheet
              row={row}
              renderText={renderText}
              attachmentSrc={attachmentSrc}
              busy={pending?.has(row.id) ?? false}
              onAnswer={(input) => onAnswer(row.id, input)}
            />
          )
        }
        if (row?.type === `suggestions`) {
          return (
            <ChatSuggestionsSheet
              row={row}
              actions={suggestions}
              onOpenSession={onOpenSession}
              renderIssueRef={renderIssueRef}
            />
          )
        }
        return renderSheet?.(dockRow, close) ?? null
      }}
    />
  )
}

export function ChatsListItem({
  lead,
  title,
  word,
  wordIcon,
  device = null,
  unread = false,
  caption = null,
  active = false,
  onOpen,
  menu,
  testId = `chats-row`,
  className,
}: {
  /** Chat 58cb32df: the row's context menu (`menuProps(kind, id)`). */
  menu?: MenuTargetProps
  /** The agent mark / state glyph (spinner busy, amber dot needs you). */
  lead: ReactNode
  /** The chat title (it names the issue, EXP-1316: no identifier slot). */
  title: string
  /** EXP-1323: the tree has an agent reply the viewer has not read. */
  unread?: boolean
  /** EXP-1323: the live caption, the muted line under the title. */
  caption?: string | null
  /** ONE quiet trailing word (`ready to merge`, `stack`, `question`;
   *  EXP-1339: no lock, `merged` has no word). */
  word?: string | null
  wordIcon?: ReactNode
  /** EXP-1318: the ROOT's device, the trailing glyph after the word (its
   *  name = tooltip + accessible name); null = none. */
  device?: { icon: ComponentType<{ className?: string }>; name: string | null } | null
  active?: boolean
  onOpen?: () => void
  testId?: string
  className?: string
}) {
  return (
    <ListRow
      interactive={Boolean(onOpen)}
      active={active}
      onClick={onOpen}
      data-testid={testId}
      {...menu}
      className={cn(
        `gap-2 py-0 pr-3 pl-3`,
        caption ? `h-[52px]` : `h-10 md:h-9`,
        className
      )}
    >
      <span className="flex size-3.5 shrink-0 items-center justify-center">{lead}</span>
      <span className="flex min-w-0 flex-1 flex-col">
        <span
          data-unread={unread || undefined}
          className={cn(`truncate text-sm`, unread && `font-semibold text-foreground`)}
        >
          {title}
        </span>
        {caption && (
          <span data-slot="chats-row-caption" className="truncate text-xs text-muted-foreground">
            {caption}
          </span>
        )}
      </span>
      {word && (
        <span
          data-slot="chats-row-word"
          className="flex shrink-0 items-center gap-1 text-[11.5px] text-muted-foreground"
        >
          {wordIcon}
          {word}
        </span>
      )}
      {device && <SessionRowDevice icon={device.icon} name={device.name} />}
    </ListRow>
  )
}

export function ChannelMessageRow({
  lead,
  name,
  time,
  children,
  testId = `channel-message`,
  className,
}: {
  /** The avatar or the agent's icon. */
  lead: ReactNode
  name: string
  time?: string | null
  /** The body (rendered markdown) or a session card. */
  children: ReactNode
  testId?: string
  className?: string
}) {
  return (
    <div data-testid={testId} className={cn(`flex min-w-0 items-start gap-3`, className)}>
      <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-glass-row">
        {lead}
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1 rounded-xl border border-glass-stroke-card bg-glass-row px-4 py-3">
        <div className="flex items-baseline gap-2">
          <span className="truncate text-sm font-semibold text-foreground">{name}</span>
          {time && <span className="shrink-0 text-xs text-muted-foreground">{time}</span>}
        </div>
        <div className="min-w-0 text-sm text-foreground">{children}</div>
      </div>
    </div>
  )
}

/** N-12: the Conversation rows THE Dock holds (they leave the transcript):
 *  the suggestion rows with an open item. S-3: a waiting ask is
 *  never docked (`sessionName` kept for the callers' shape). */
export function chatDockedIds(
  rows: readonly SessionChatRow[],
  _sessionName: (sessionId: string) => string | null,
  withSuggestions = true
): Set<string> {
  const ids = new Set<string>()
  if (withSuggestions) for (const source of suggestionSources(rows)) ids.add(source.id)
  return ids
}
