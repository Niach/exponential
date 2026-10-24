import type { ComponentProps, ReactNode } from "react"
import {
  CHAT_FILTERS,
  CHATS_COPY,
  CHATS_SHELL,
  CHATS_TEST_IDS,
  chatFilterLabel,
  type AgentBubble,
  type ChatFilter,
  type ChatLead,
  type ChatList as ChatListModel,
  type ChatRow as ChatRowModel,
} from "@exp/domain-contract/chats"
import { THREAD_ROW_WORDS, THREADS_LIST_COPY } from "@exp/domain-contract/session-tree"
import { AgentBrandMark, AgentWorkingMark, RUN_MARK_BADGE_CLASS } from "./agent-brand-mark"
import { BoardGlyph } from "./board-glyph"
import { cn } from "./cn"
import { EmptyState } from "./empty-state"
import { PageHeader } from "./page-header"
import { conceptIcon } from "./icons.generated"
import { menuProps } from "./menu"
import { Pill } from "./pill"
import { SkeletonRows } from "./skeleton"

// Thread 58cb32df (#EXP-1373, fixture `chats.json` ×4): the PHONE Chats
// screen's pieces, shared by the web-mobile page and the styleguide so they
// cannot drift. The model is the contract's (`chatList`, `agentBubbles`,
// `chatFilterLabel`); these only paint it. iOS ExpUI `ChatRow` /
// `AgentBubbles`, Android `ui/components` `ChatRow` / `AgentBubbles` are the
// twins.
//
//   ChatsHeader      the large "Chats" headline + search + inbox circles
//   AgentBubbles     the team's agents as 66px rings, scrolling sideways
//   ChatFilterChips  All · Needs you N · Running N · Unread N (select Pills)
//   ChatRow          one flat WhatsApp row: 52 lead disc, title + time,
//                    two-line preview, word, unread badge
//   ChatList         the rows (or the skeleton / the empty state)

const ChatsIcon = conceptIcon(`nav-threads`)
const PrOpenIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)
const PrStackIcon = conceptIcon(`pr-stack`)

/** The Chats page header: THE page header (`page-header.tsx`, chats.json
 *  `shell.header`) titled `copy.title`. */
export function ChatsHeader({
  title = CHATS_COPY.title,
  ...props
}: Omit<ComponentProps<typeof PageHeader>, `title`> & { title?: string }) {
  return <PageHeader title={title} {...props} />
}

const BUBBLE_RING: Record<NonNullable<AgentBubble[`ring`]> | `none`, string> = {
  needs_you: `border-amber-400`,
  unread: `border-foreground`,
  none: `border-transparent`,
}

/** One agent bubble: the 66 circle (a 3px ring around a 3px background gap
 *  around the agent's icon on bg muted) with the name under it. A tap opens
 *  the agent's chat; right-click / long-press = its agent menu. */
export function AgentBubbleButton({
  bubble,
  onOpen,
  className,
}: {
  bubble: AgentBubble
  onOpen?: (agentId: string) => void
  className?: string
}) {
  const size = CHATS_SHELL.bubble
  const ring = CHATS_SHELL.bubbleRing
  return (
    <button
      type="button"
      data-testid={CHATS_TEST_IDS.agentBubble}
      data-ring={bubble.ring ?? `none`}
      {...menuProps(`agent`, bubble.agentId)}
      onClick={onOpen ? () => onOpen(bubble.agentId) : undefined}
      className={cn(
        `flex shrink-0 flex-col items-center gap-1.5 rounded-lg outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50`,
        className
      )}
      style={{ width: size + 6 }}
    >
      <span
        className={cn(`relative flex shrink-0 rounded-full`, BUBBLE_RING[bubble.ring ?? `none`])}
        style={{ width: size, height: size, borderWidth: ring, padding: ring }}
      >
        {bubble.ring === null && (
          <span aria-hidden className="absolute -inset-[3px] rounded-full border border-glass-stroke" />
        )}
        <span className="flex size-full items-center justify-center rounded-full bg-muted">
          {bubble.busy ? (
            <AgentWorkingMark agent={null} className="size-6" />
          ) : (
            <BoardGlyph board={{ icon: bubble.icon, color: bubble.color }} className="size-6" />
          )}
        </span>
      </span>
      <span
        className={cn(
          `w-full truncate text-center text-xs`,
          bubble.ring ? `font-medium text-foreground` : `text-muted-foreground`
        )}
      >
        {bubble.name}
      </span>
    </button>
  )
}

/** The Chats header's agents row (`agentBubbles`, in `listAgents` order). */
export function AgentBubbles({
  bubbles,
  onOpen,
  className,
}: {
  bubbles: readonly AgentBubble[]
  onOpen?: (agentId: string) => void
  className?: string
}) {
  if (bubbles.length === 0) return null
  return (
    <div
      data-slot="agent-bubbles"
      className={cn(`-mx-4 flex gap-3 overflow-x-auto px-4 pb-1 [scrollbar-width:none]`, className)}
    >
      {bubbles.map((bubble) => (
        <AgentBubbleButton key={bubble.agentId} bubble={bubble} onOpen={onOpen} />
      ))}
    </div>
  )
}

/** The filter chips over the list (`filters`): select Pills labelled by
 *  `chatFilterLabel` (a count after a non-zero one). */
export function ChatFilterChips({
  value,
  counts,
  onChange,
  className,
}: {
  value: ChatFilter
  counts: ChatListModel[`counts`]
  onChange: (filter: ChatFilter) => void
  className?: string
}) {
  return (
    <div
      role="toolbar"
      aria-label="Filter chats"
      className={cn(`-mx-4 flex gap-2 overflow-x-auto px-4 [scrollbar-width:none]`, className)}
    >
      {CHAT_FILTERS.map((filter) => (
        <Pill
          key={filter}
          mode="select"
          size="md"
          selected={filter === value}
          data-testid={CHATS_TEST_IDS.chatFilter}
          data-filter={filter}
          className="h-9 px-4"
          onClick={() => onChange(filter)}
        >
          {chatFilterLabel(filter, counts)}
        </Pill>
      ))}
    </div>
  )
}

/** The 52 lead DISC (bg muted) and what it holds (`lead`): the brand mark
 *  with the amber badge, the working mark, the root's PR glyph, else the
 *  bare brand mark. */
export function ChatLeadDisc({
  lead,
  agent,
  className,
}: {
  lead: ChatLead
  /** The root's agent CLI (its brand mark). */
  agent: string | null
  className?: string
}) {
  const size = CHATS_SHELL.leadDisc
  let body: ReactNode
  if (lead === `working`) body = <AgentWorkingMark agent={agent} className="size-6" />
  else if (lead === `open`) body = <PrOpenIcon className="size-6 text-emerald-500" />
  else if (lead === `merged`) body = <PrMergedIcon className="size-6 text-violet-400" />
  else body = <AgentBrandMark agent={agent} className="size-6" />
  return (
    <span
      data-slot="chat-lead"
      data-lead={lead}
      className={cn(`relative flex shrink-0 items-center justify-center rounded-full bg-muted`, className)}
      style={{ width: size, height: size }}
    >
      {body}
      {lead === `needs_you` && (
        <span
          aria-hidden
          data-slot="run-mark-badge"
          className={cn(
            `absolute right-2.5 top-2.5 size-2.5 rounded-full ring-2 ring-muted`,
            RUN_MARK_BADGE_CLASS.needs_input
          )}
        />
      )}
    </span>
  )
}

/** The preview's copy prefix (`Question: `, `Plan: `, `You: `), painted at
 *  full emphasis before the muted rest. */
function previewParts(row: ChatRowModel): { prefix: string; rest: string } {
  const kind = row.preview.kind
  if (kind === `question` || kind === `plan` || kind === `you`) {
    const prefix = CHATS_COPY.preview[kind].split(`{text}`)[0] ?? ``
    if (prefix && row.preview.text.startsWith(prefix)) {
      return { prefix, rest: row.preview.text.slice(prefix.length) }
    }
  }
  return { prefix: ``, rest: row.preview.text }
}

/** One chat (a ROOT session) as the flat list row. Right-click / long-press
 *  opens its menu through the platform's ONE menu host
 *  (`menuProps('thread', sessionId)`). */
export function ChatRow({
  row,
  agent = null,
  onOpen,
  className,
  ...props
}: Omit<ComponentProps<`button`>, `onClick`> & {
  row: ChatRowModel
  agent?: string | null
  onOpen?: (sessionId: string) => void
}) {
  const unread = row.unreadCount > 0
  const { prefix, rest } = previewParts(row)
  const word = row.word ? (THREAD_ROW_WORDS[row.word] ?? null) : null
  return (
    <button
      type="button"
      data-testid={CHATS_TEST_IDS.chatRow}
      data-session-id={row.sessionId}
      {...menuProps(`thread`, row.sessionId)}
      {...props}
      onClick={onOpen ? () => onOpen(row.sessionId) : undefined}
      className={cn(
        `flex w-full items-center gap-4 text-left outline-none transition-colors duration-fast active:bg-glass-active/40 focus-visible:bg-glass-active/40`,
        className
      )}
    >
      <ChatLeadDisc lead={row.lead} agent={agent} />
      <span className="flex min-w-0 flex-1 flex-col gap-1 border-b border-glass-stroke py-3.5">
        <span className="flex min-w-0 items-baseline gap-2">
          <span
            className={cn(
              `min-w-0 flex-1 truncate text-[15px] text-foreground`,
              unread ? `font-semibold` : `font-medium`
            )}
          >
            {row.title}
          </span>
          <span
            className={cn(
              `shrink-0 text-xs tabular-nums`,
              row.needsYou ? `text-amber-400` : unread ? `text-foreground` : `text-muted-foreground`
            )}
          >
            {row.time}
          </span>
        </span>
        <span className="flex min-w-0 items-center gap-2">
          <span
            className={cn(
              `line-clamp-2 min-w-0 flex-1 text-sm leading-snug text-muted-foreground`,
              row.preview.kind === `working` && `italic`
            )}
          >
            {prefix && <span className="text-foreground/85">{prefix}</span>}
            {rest}
          </span>
          {word && (
            <span className="flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
              {row.word === `stack` && <PrStackIcon className="size-3.5 shrink-0" />}
              {word}
            </span>
          )}
          {unread && (
            <span
              data-slot="chat-unread"
              className={cn(
                `flex h-5 min-w-5 shrink-0 items-center justify-center rounded-full px-1.5 text-[11px] font-semibold tabular-nums`,
                row.needsYou ? `bg-amber-400 text-black` : `bg-foreground text-background`
              )}
            >
              {row.unreadCount > 99 ? `99+` : row.unreadCount}
            </span>
          )}
        </span>
      </span>
    </button>
  )
}

/** The flat list (`chatList(...).rows`): rows, the skeleton while the
 *  shapes load, else the empty state. */
export function ChatList({
  rows,
  agentOf,
  loading = false,
  entering,
  emptyTitle,
  onOpen,
  className,
}: {
  rows: readonly ChatRowModel[]
  /** A root's agent CLI (its brand mark). */
  agentOf?: (sessionId: string) => string | null
  loading?: boolean
  /** The `motion-enter` class for a row that arrived after the first paint. */
  entering?: (sessionId: string) => string
  emptyTitle?: string
  onOpen?: (sessionId: string) => void
  className?: string
}) {
  if (loading) return <SkeletonRows rowHeight={76} />
  if (rows.length === 0) {
    return <EmptyState icon={ChatsIcon} title={emptyTitle ?? THREADS_LIST_COPY.empty} size="page" />
  }
  return (
    <div data-slot="chat-list" data-testid="chat-list" className={cn(`flex flex-col`, className)}>
      {rows.map((row) => (
        <ChatRow
          key={row.sessionId}
          row={row}
          agent={agentOf?.(row.sessionId) ?? null}
          className={entering?.(row.sessionId)}
          onOpen={onOpen}
        />
      ))}
    </div>
  )
}
