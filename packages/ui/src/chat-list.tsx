import type { ComponentProps, MouseEvent as ReactMouseEvent, ReactNode } from "react"
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
  type ChatSelectionAction,
} from "@exp/domain-contract/chats"
import { THREAD_ROW_WORDS, THREADS_LIST_COPY } from "@exp/domain-contract/session-tree"
import { AgentBrandMark, AgentWorkingMark, RUN_MARK_BADGE_CLASS } from "./agent-brand-mark"
import { BoardGlyph } from "./board-glyph"
import { CHECKBOX_CLASSES } from "@exponential_app/ui/web/primitives"
import { SelectionCheck, selectableRowClass } from "./bulk-bar"
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
//                    two-line preview, word, unread badge; while the list
//                    SELECTS (#EXP-1388) it leads with the issue rows' control
//   ChatArchivedRow  the Archived entry (#EXP-1388): the same row, ui-archive
//                    in the disc, the count in the time slot, no preview
//   ChatList         the rows (or the skeleton / the empty state)

const ChatsIcon = conceptIcon(`nav-threads`)
const PrOpenIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)
const PrStackIcon = conceptIcon(`pr-stack`)
const ArchiveIcon = conceptIcon(`ui-archive`)
const CheckIcon = conceptIcon(`ui-check`)

/** #EXP-1388: the selection bar's action glyphs (`chatSelectionActions`). */
export const CHAT_SELECTION_ICONS: Record<ChatSelectionAction, ReturnType<typeof conceptIcon>> = {
  markRead: conceptIcon(`ui-mark-read`),
  markUnread: conceptIcon(`ui-mark-unread`),
  archive: ArchiveIcon,
  unarchive: conceptIcon(`ui-unarchive`),
  stop: conceptIcon(`coding-stop`),
  delete: conceptIcon(`ui-delete`),
}

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
  icon,
  className,
}: {
  lead: ChatLead
  /** The root's agent CLI (its brand mark). */
  agent: string | null
  /** #EXP-1388: a glyph instead of the lead (the Archived entry's). */
  icon?: ReactNode
  className?: string
}) {
  const size = CHATS_SHELL.leadDisc
  let body: ReactNode
  if (icon) body = icon
  else if (lead === `working`) body = <AgentWorkingMark agent={agent} className="size-6" />
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
      {lead === `needs_you` && !icon && (
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

/** The row's button classes, shared by a chat and the Archived entry. */
const ROW_CLASS = `flex w-full items-center gap-4 text-left outline-none transition-colors duration-fast active:bg-glass-active/40 focus-visible:bg-glass-active/40`

/** One chat (a ROOT session) as the flat list row. Right-click / long-press
 *  opens its menu through the platform's ONE menu host
 *  (`menuProps('thread', sessionId)`). A SELECTABLE row (#EXP-1388,
 *  `onSelectToggle`) is the issue row's: wide, the Checkbox column leads
 *  every row (shown on hover, always while selecting; a click toggles),
 *  the row fills on hover; phones lead with SelectionCheck while selecting
 *  and a selected row sits on the issue row's selected card
 *  (`selectableRowClass`). While selecting `onRowClick` gets every click (a
 *  toggle, never an open). */
export function ChatRow({
  row,
  agent = null,
  onOpen,
  onRowClick,
  onSelectToggle,
  selecting = false,
  selected = false,
  className,
  ...props
}: Omit<ComponentProps<`button`>, `onClick`> & {
  row: ChatRowModel
  agent?: string | null
  onOpen?: (sessionId: string) => void
  /** #EXP-1388: the click with its modifiers (cmd/ctrl toggles, shift
   *  ranges); wins over `onOpen`. */
  onRowClick?: (sessionId: string, event: ReactMouseEvent<HTMLButtonElement>) => void
  /** #EXP-1388: the wide checkbox cell's click (shift = the range); set =
   *  the row is selectable. */
  onSelectToggle?: (sessionId: string, event: ReactMouseEvent<HTMLElement>) => void
  /** #EXP-1388: the list is selecting (every row leads with the glyph). */
  selecting?: boolean
  selected?: boolean
}) {
  const selectable = Boolean(onSelectToggle) || selecting
  const unread = row.unreadCount > 0
  const { prefix, rest } = previewParts(row)
  const word = row.word ? (THREAD_ROW_WORDS[row.word] ?? null) : null
  return (
    <button
      type="button"
      data-testid={CHATS_TEST_IDS.chatRow}
      data-session-id={row.sessionId}
      data-selected={selecting ? selected : undefined}
      aria-pressed={selecting ? selected : undefined}
      {...menuProps(`thread`, row.sessionId)}
      {...props}
      // Shift-click ranges: never the browser's text selection.
      onMouseDown={(event) => {
        if (event.shiftKey) event.preventDefault()
      }}
      onClick={
        onRowClick
          ? (event) => onRowClick(row.sessionId, event)
          : onOpen
            ? () => onOpen(row.sessionId)
            : undefined
      }
      className={cn(
        ROW_CLASS,
        selectable &&
          cn(`group/row -mx-3 px-3`, selectableRowClass({ selected: selecting && selected, rest: `max-md:border-transparent` })),
        className
      )}
    >
      {/* The issue rows' selection cell (owner on #1283): phones the round
          SelectionCheck while selecting, wide the Checkbox column on every
          row (hover-revealed like the issue row's; its recipe painted here:
          the row is already the button, a Radix button cannot nest). */}
      {selecting && (
        <SelectionCheck selected={selected} aria-label={`Select ${row.title}`} className="-mr-1 md:hidden" />
      )}
      {selectable && (
        <span
          className="hidden shrink-0 items-center self-stretch md:flex"
          onMouseDown={(event) => {
            if (event.shiftKey) event.preventDefault()
          }}
          onClick={(event) => {
            event.stopPropagation()
            onSelectToggle?.(row.sessionId, event)
          }}
        >
          <span
            role="checkbox"
            aria-checked={selected}
            aria-label={`Select ${row.title}`}
            data-slot="checkbox"
            data-state={selected ? `checked` : `unchecked`}
            className={cn(
              CHECKBOX_CLASSES.root,
              `flex items-center justify-center transition-opacity`,
              selecting ? `opacity-100` : `opacity-0 group-hover/row:opacity-100`
            )}
          >
            {selected && <CheckIcon className={CHECKBOX_CLASSES.icon} />}
          </span>
        </span>
      )}
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

/** #EXP-1388: the Archived entry, the list's first row while chats are
 *  archived (and the filter is All): the chat row with `ui-archive` in its
 *  lead disc, `copy.archived.entry` and the count in the (muted) time slot,
 *  no preview line. */
export function ChatArchivedRow({
  count,
  onOpen,
  aligned = false,
  className,
  ...props
}: Omit<ComponentProps<`button`>, `onClick`> & {
  count: number
  onOpen?: () => void
  /** Over selectable rows: the same inset and (wide) the checkbox column's
   *  room, so the discs line up. */
  aligned?: boolean
}) {
  return (
    <button
      type="button"
      data-testid={CHATS_TEST_IDS.chatArchived}
      {...props}
      onClick={onOpen}
      className={cn(ROW_CLASS, aligned && `-mx-3 px-3 md:hover:bg-glass-row`, className)}
    >
      {aligned && <span aria-hidden className="hidden size-4 shrink-0 md:block" />}
      <ChatLeadDisc lead="waiting" agent={null} icon={<ArchiveIcon className="size-6 text-muted-foreground" />} />
      <span className="flex min-w-0 flex-1 items-center gap-2 self-stretch border-b border-glass-stroke py-3.5">
        <span className="min-w-0 flex-1 truncate text-[15px] font-medium text-foreground">
          {CHATS_COPY.archived.entry}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{count}</span>
      </span>
    </button>
  )
}

/** The flat list (`chatList(...).rows`): the Archived entry (when
 *  `archived` > 0), the rows, the skeleton while the shapes load, else the
 *  empty state. `selection` non-null = the list SELECTS (#EXP-1388). */
export function ChatList({
  rows,
  agentOf,
  loading = false,
  entering,
  emptyTitle,
  onOpen,
  archived = 0,
  onOpenArchived,
  selection = null,
  onRowClick,
  onSelectToggle,
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
  /** #EXP-1388: the archived count for the entry row (0 = no entry; the
   *  caller passes 0 unless the filter is All). */
  archived?: number
  onOpenArchived?: () => void
  /** #EXP-1388: the selected ids while selecting, else null. */
  selection?: ReadonlySet<string> | null
  onRowClick?: (sessionId: string, event: ReactMouseEvent<HTMLButtonElement>) => void
  /** #EXP-1388: the rows are selectable (the wide checkbox column). */
  onSelectToggle?: (sessionId: string, event: ReactMouseEvent<HTMLElement>) => void
  className?: string
}) {
  if (loading) return <SkeletonRows rowHeight={76} />
  if (rows.length === 0 && archived === 0) {
    return <EmptyState icon={ChatsIcon} title={emptyTitle ?? THREADS_LIST_COPY.empty} size="page" />
  }
  return (
    <div data-slot="chat-list" data-testid="chat-list" className={cn(`flex flex-col`, className)}>
      {archived > 0 && (
        <ChatArchivedRow
          count={archived}
          onOpen={onOpenArchived}
          aligned={Boolean(onSelectToggle) || selection !== null}
        />
      )}
      {rows.map((row) => (
        <ChatRow
          key={row.sessionId}
          row={row}
          agent={agentOf?.(row.sessionId) ?? null}
          className={entering?.(row.sessionId)}
          onOpen={onOpen}
          onRowClick={onRowClick}
          onSelectToggle={onSelectToggle}
          selecting={selection !== null}
          selected={selection?.has(row.sessionId) ?? false}
        />
      ))}
    </div>
  )
}
