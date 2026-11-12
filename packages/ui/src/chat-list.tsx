import { type ComponentProps, type MouseEvent as ReactMouseEvent, type ReactNode } from "react"
import {
  CHAT_SECTION_KEYS,
  CHATS_COPY,
  CHATS_SHELL,
  CHATS_TEST_IDS,
  chatSections,
  type AgentChatRow,
  type AppChatRow,
  type ChatLead,
  type ChatRow as ChatRowModel,
  type ChatSelectionAction,
} from "@exp/domain-contract/chats"
import { CHAT_ROW_WORDS, CHATS_LIST_COPY } from "@exp/domain-contract/session-tree"
import { rowBadge } from "@exp/domain-contract/nav-badges"
import { Badge } from "./badge"
import { AgentWorkingMark, RUN_MARK_BADGE_CLASS } from "./agent-brand-mark"
import { BoardGlyph } from "./board-glyph"
import { getAppIcon } from "./board-icons"
import { ExponentialLogo } from "./exponential-logo"
import { CHECKBOX_CLASSES } from "@exponential_app/eui/web/primitives"
import { cn } from "./cn"
import { EmptyState } from "./empty-state"
import { PageHeader } from "./page-header"
import { conceptIcon } from "./icons.generated"
import { menuProps } from "./menu"
import { SkeletonRows } from "./skeleton"

// Chat 58cb32df (#EXP-1373, fixture `chats.json` ×4): the PHONE Chats
// screen's pieces, shared by the web-mobile page and the styleguide so they
// cannot drift. The model is the contract's (`chatList`, `chatSections`);
// these only paint it. iOS ExpUI `ChatRow`, Android `ui/components` `ChatRow`
// are the twins.
//
//   ChatsHeader      the large "Chats" headline + search + inbox circles
//   AgentAvatar      an agent's avatar glyph: the Assistant = the brand
//                    mark in the foreground colour, an agent = its icon
//   ChatRow          one flat WhatsApp row: 52 lead disc, title + time,
//                    two-line preview, word, unread badge; while the list
//                    SELECTS (#EXP-1388) wide leads with the issue rows'
//                    Checkbox, phones never shift: a selected row is a
//                    full-width tint with the check badge on its disc
//   ChatArchivedRow  the Archived entry (#EXP-1388): the plain ui-archive
//                    glyph in the disc column, the count in the time slot
//   AgentChatRowButton  S-3: an agent row pinned on top (its avatar disc)
//   ChatList         #FEED-191: Agents, then `chatSections` under their headers
//                    (the Archived entry = first under Chats); the Archived
//                    page is flat. Or the skeleton / the empty state.

const ChatsIcon = conceptIcon(`nav-chats`)
/** S-3 (`shell.chatLead`): a chat row's plain lead glyph. */
const ChatLeadIcon = conceptIcon(CHATS_SHELL.chatLead as Parameters<typeof conceptIcon>[0])
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

/** S-3: an agent's AVATAR glyph ×4: the viewer's Assistant = the Exponential
 *  brand mark in the foreground colour (never its stored icon/color), any
 *  other agent = its icon in its colour. Callers put it on the bg-muted disc. */
export function AgentAvatar({
  agent,
  className,
}: {
  agent: { icon: string; color: string; assistant?: boolean }
  className?: string
}) {
  if (agent.assistant) {
    return (
      <ExponentialLogo
        variant="light"
        className={cn(`shrink-0 text-foreground`, className)}
      />
    )
  }
  return <BoardGlyph board={{ icon: agent.icon, color: agent.color }} className={className} />
}

/** #EXP-1388 (owner on #1286): the touch selection mark on the disc's
 *  bottom-right corner, THE checkbox look (owner 2026-10-27: every check is
 *  the square Checkbox): the primary square at `selectBadgeRadius` with the
 *  Checkbox's check, ringed in the selected row's ground (the background
 *  under the fillActive tint, outer corner = radius + ring) so it reads cut
 *  out of the disc. */
export function ChatSelectBadge({ className }: { className?: string }) {
  const { selectBadge, selectBadgeRing, selectBadgeGlyph, selectBadgeRadius } = CHATS_SHELL
  const outer = selectBadge + 2 * selectBadgeRing
  const ringRadius = selectBadgeRadius + selectBadgeRing
  return (
    <span
      aria-hidden
      data-slot="chat-select-badge"
      className={cn(`absolute right-0 bottom-0 bg-background`, className)}
      style={{ width: outer, height: outer, borderRadius: ringRadius }}
    >
      <span
        className="flex size-full items-center justify-center bg-glass-active"
        style={{ padding: selectBadgeRing, borderRadius: ringRadius }}
      >
        <span
          className="flex size-full items-center justify-center bg-primary text-primary-foreground"
          style={{ borderRadius: selectBadgeRadius }}
        >
          <CheckIcon style={{ width: selectBadgeGlyph, height: selectBadgeGlyph }} />
        </span>
      </span>
    </span>
  )
}

/** The 52 lead DISC (bg muted) and what it holds (`lead`): the chat glyph
 *  (`shell.chatLead`) with the amber badge, the working mark, the root's PR
 *  glyph, else the bare chat glyph (S-3: the brand mark is the Assistant's
 *  avatar now, never a chat's). `avatar` = an agent row's avatar instead. #EXP-1388: a SELECTED row's disc steps one grey notch
 *  up (fillActive over muted) so it never blends into the highlight. */
export function ChatLeadDisc({
  lead,
  agent,
  selected = false,
  badge,
  avatar,
  className,
}: {
  lead: ChatLead
  /** The root's agent CLI (its working mark). */
  agent: string | null
  /** S-3: an agent row's avatar (`AgentAvatar`) in place of the chat glyph. */
  avatar?: ReactNode
  /** #EXP-1388: the row is selected (the stronger disc). */
  selected?: boolean
  /** #EXP-1388: the touch selection badge (`ChatSelectBadge`). */
  badge?: ReactNode
  className?: string
}) {
  const size = CHATS_SHELL.leadDisc
  let body: ReactNode
  if (lead === `working`) body = <AgentWorkingMark agent={agent} className="size-6" />
  else if (lead === `open`) body = <PrOpenIcon className="size-6 text-emerald-500" />
  else if (lead === `merged`) body = <PrMergedIcon className="size-6 text-violet-400" />
  else if (avatar) body = avatar
  else body = <ChatLeadIcon data-slot="chat-lead-glyph" className="size-6 text-muted-foreground" />
  return (
    <span
      data-slot="chat-lead"
      data-lead={lead}
      data-selected={selected || undefined}
      className={cn(
        `relative flex shrink-0 items-center justify-center rounded-full bg-muted`,
        selected && `bg-[linear-gradient(var(--glass-fill-active),var(--glass-fill-active))]`,
        className
      )}
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
      {badge}
    </span>
  )
}

/** The preview's copy prefix (`Question: `, `Plan: `, `You: `), painted at
 *  full emphasis before the muted rest. */
function previewParts(preview: ChatRowModel[`preview`]): { prefix: string; rest: string } {
  const kind = preview.kind
  if (kind === `question` || kind === `plan` || kind === `you`) {
    const prefix = CHATS_COPY.preview[kind].split(`{text}`)[0] ?? ``
    if (prefix && preview.text.startsWith(prefix)) {
      return { prefix, rest: preview.text.slice(prefix.length) }
    }
  }
  return { prefix: ``, rest: preview.text }
}

/** A row's two text lines (a chat's and an agent row's alike): title +
 *  time, then the preview, the row word and the unread badge. */
function ChatRowText({
  title,
  time,
  needsYou,
  unreadCount,
  preview,
  word: wordKey = null,
  className,
}: {
  title: string
  time: string
  needsYou: boolean
  unreadCount: number
  preview: ChatRowModel[`preview`]
  word?: ChatRowModel[`word`]
  className?: string
}) {
  const unread = unreadCount > 0
  const { prefix, rest } = previewParts(preview)
  const word = wordKey ? (CHAT_ROW_WORDS[wordKey] ?? null) : null
  const row = { title, time, needsYou, unreadCount, preview, word: wordKey }
  const badge = rowBadge({ unreadCount, needsYou })
  return (
      <span
        className={cn(
          // EXP-1554: the natives' row metrics (iOS 12pt vertical padding,
          // 3pt between the lines).
          `flex min-w-0 flex-1 flex-col gap-[3px] border-b border-glass-stroke py-3`,
          // Phones: no hairline inside the selected band (WhatsApp's).
          className
        )}
      >
        <span className="flex min-w-0 items-baseline gap-2">
          <span
            className={cn(
              `min-w-0 flex-1 truncate text-[15px] text-foreground max-md:leading-snug`,
              unread ? `font-semibold` : `font-medium`
            )}
          >
            {row.title}
          </span>
          <span
            className={cn(
              `shrink-0 text-xs tabular-nums`,
              row.needsYou ? `text-yellow-400` : unread ? `text-foreground` : `text-muted-foreground`
            )}
          >
            {row.time}
          </span>
        </span>
        <span className="flex min-w-0 items-center gap-2">
          <span
            className={cn(
              `line-clamp-2 min-w-0 flex-1 text-sm leading-snug text-muted-foreground`,
              (row.preview.kind === `working` || row.preview.kind === `locked`) && `italic`
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
          {/* #FEED-232: THE count badge (nav-badges.json `rowBadge`). */}
          <Badge count={badge.count} tone={badge.tone} data-slot="chat-unread" />
        </span>
      </span>
  )
}

/** The row's button classes, shared by a chat and the Archived entry. */
const ROW_CLASS = `flex w-full items-center gap-4 text-left outline-none transition-colors duration-fast active:bg-glass-active/40 focus-visible:bg-glass-active/40`

/** One chat (a ROOT session) as the flat list row. Right-click / long-press
 *  opens its menu through the platform's ONE menu host
 *  (`menuProps('chat', sessionId)`). A SELECTABLE row (#EXP-1388,
 *  `onSelectToggle`) is the issue row's on md+: the Checkbox column leads
 *  every row (shown on hover, always while selecting; a click toggles),
 *  the row fills on hover. Phones never shift (owner on #1286): a selected
 *  row is a full-width fillActive tint with `ChatSelectBadge` on its disc.
 *  While selecting `onRowClick` gets every click (a toggle, never an
 *  open). */
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
  const isSelected = selecting && selected
  return (
    <button
      type="button"
      data-testid={CHATS_TEST_IDS.chatRow}
      data-session-id={row.sessionId}
      data-selected={selecting ? selected : undefined}
      aria-pressed={selecting ? selected : undefined}
      {...menuProps(`chat`, row.sessionId)}
      {...props}
      // S-3: a DONE chat (every PR merged, nothing works) stays, dimmed as
      // a whole at `shell.dimOpacity`.
      data-dim={row.dim || undefined}
      style={row.dim ? { ...props.style, opacity: CHATS_SHELL.dimOpacity } : props.style}
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
        // Phones: the row bleeds to the screen's edges (the page's px-4) so
        // the selected tint is full width; nothing inside moves.
        selectable && `group/row max-md:-mx-4 max-md:w-[calc(100%+2rem)] max-md:px-4 md:-mx-3 md:px-3 md:hover:bg-glass-row`,
        isSelected && `max-md:bg-glass-active`,
        className
      )}
    >
      {/* Wide: the issue rows' Checkbox column on every row (owner on
          #1283; hover-revealed like the issue row's; its recipe painted
          here: the row is already the button, a Radix button cannot
          nest). */}
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
      <ChatLeadDisc
        lead={row.lead}
        agent={agent}
        selected={isSelected}
        badge={isSelected ? <ChatSelectBadge className="md:hidden" /> : undefined}
      />
      <ChatRowText
        title={row.title}
        time={row.time}
        needsYou={row.needsYou}
        unreadCount={row.unreadCount}
        preview={row.preview}
        word={row.word}
        className={isSelected ? `max-md:border-transparent` : undefined}
      />
    </button>
  )
}

/** #EXP-1388: the Archived entry, the list's first row while chats are
 *  archived (and the filter is All): the PLAIN `ui-archive` glyph (no disc)
 *  centred in the discs' column, `copy.archived.entry` and the count in
 *  the (muted) time slot, no preview line. It never moves. */
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
      className={cn(
        ROW_CLASS,
        aligned && `max-md:-mx-4 max-md:w-[calc(100%+2rem)] max-md:px-4 md:-mx-3 md:px-3 md:hover:bg-glass-row`,
        className
      )}
    >
      {aligned && <span aria-hidden className="hidden size-4 shrink-0 md:block" />}
      <span
        data-slot="chat-archived-glyph"
        className="flex shrink-0 justify-center text-muted-foreground"
        style={{ width: CHATS_SHELL.leadDisc }}
      >
        <ArchiveIcon style={{ width: CHATS_SHELL.archivedGlyph, height: CHATS_SHELL.archivedGlyph }} />
      </span>
      <span className="flex min-w-0 flex-1 items-center gap-2 self-stretch border-b border-glass-stroke py-3">
        <span className="min-w-0 flex-1 truncate text-[15px] font-medium text-foreground">
          {CHATS_COPY.archived.entry}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{count}</span>
      </span>
    </button>
  )
}

/** S-3: the agent rows' test id (no fixture id: web-only hook). */
export const CHAT_AGENT_ROW_TEST_ID = `chat-agent-row`

/** S-3 (chats.json AGENT ROWS): an agent pinned on top of the list: a chat
 *  row whose disc holds the agent's avatar (`AgentAvatar`), its name, the
 *  viewer's conversation's preview / time / unread; the amber needs-you
 *  badge on the avatar, busy only in the preview; never dim, never selectable. Right-click /
 *  long-press = the agent menu (`menuProps('agent', id)`). */
export function AgentChatRowButton({
  row,
  onOpen,
  aligned = false,
  className,
  ...props
}: Omit<ComponentProps<`button`>, `onClick`> & {
  row: AgentChatRow
  onOpen?: (agentId: string) => void
  /** Over selectable rows: the same inset and (wide) the checkbox column's
   *  room, so the discs line up. */
  aligned?: boolean
}) {
  // The avatar always (chats.json AGENT ROWS): needs you adds the amber
  // badge; busy reads in the preview (`working…`), never as the working mark.
  const lead: ChatLead = row.needsYou ? `needs_you` : `waiting`
  return (
    <button
      type="button"
      data-testid={CHAT_AGENT_ROW_TEST_ID}
      data-agent-id={row.agentId}
      data-assistant={row.assistant || undefined}
      {...menuProps(`agent`, row.agentId)}
      {...props}
      onClick={onOpen ? () => onOpen(row.agentId) : undefined}
      className={cn(
        ROW_CLASS,
        aligned && `max-md:-mx-4 max-md:w-[calc(100%+2rem)] max-md:px-4 md:-mx-3 md:px-3 md:hover:bg-glass-row`,
        className
      )}
    >
      {aligned && <span aria-hidden className="hidden size-4 shrink-0 md:block" />}
      <ChatLeadDisc lead={lead} agent={null} avatar={<AgentAvatar agent={row} className="size-6" />} />
      <ChatRowText
        title={row.name}
        time={row.time}
        needsYou={row.needsYou}
        unreadCount={row.unreadCount}
        preview={row.preview}
      />
    </button>
  )
}

/** An app's glyph (the web apps sidebar's `AppIcon`): its icon in its colour. */
function AppChatGlyph({ icon, color }: { icon: string | null; color: string | null }) {
  const Icon = getAppIcon({ icon })
  return <Icon className="size-6 shrink-0" style={{ color: color ?? undefined }} />
}

/** #EXP-1733: the app rows' test id (chats.json `testIds.appRow`). */
export const CHAT_APP_ROW_TEST_ID = CHATS_TEST_IDS.appRow

/** #EXP-1733 (chats.json APPS): an app's row between the Agents and the
 *  sessions: the agent row's look with the app's glyph in the disc, the
 *  viewer's session with the app for preview / time / unread; never
 *  selectable. A tap opens the app's chat. */
export function AppChatRowButton({
  row,
  onOpen,
  aligned = false,
  className,
  ...props
}: Omit<ComponentProps<`button`>, `onClick`> & {
  row: AppChatRow
  onOpen?: (slug: string) => void
  aligned?: boolean
}) {
  const lead: ChatLead = row.needsYou ? `needs_you` : `waiting`
  return (
    <button
      type="button"
      data-testid={CHAT_APP_ROW_TEST_ID}
      data-app-id={row.appId}
      {...props}
      onClick={onOpen ? () => onOpen(row.slug) : undefined}
      className={cn(
        ROW_CLASS,
        aligned && `max-md:-mx-4 max-md:w-[calc(100%+2rem)] max-md:px-4 md:-mx-3 md:px-3 md:hover:bg-glass-row`,
        className
      )}
    >
      {aligned && <span aria-hidden className="hidden size-4 shrink-0 md:block" />}
      <ChatLeadDisc
        lead={lead}
        agent={null}
        avatar={<AppChatGlyph icon={row.icon} color={row.color} />}
      />
      <ChatRowText
        title={row.name}
        time={row.time}
        needsYou={row.needsYou}
        unreadCount={row.unreadCount}
        preview={row.preview}
      />
    </button>
  )
}

/** A section header of the list (`copy.sections`). */
function ChatSectionHeader({ title }: { title: string }) {
  return (
    <div
      role="heading"
      aria-level={2}
      data-slot="chat-section"
      className="pt-2 pb-1 text-xs font-medium text-muted-foreground"
    >
      {title}
    </div>
  )
}

/** #FEED-191 (chats.json): THE list: `entry` (the phones' Needs you row), the
 *  agents, then `chatSections(rows)` under their headers, the Archived entry
 *  (when `archived` > 0) first under `chats`; `flat` (the Archived page) = the
 *  rows alone, no headers. The skeleton while the shapes load, else the empty
 *  state. `selection` non-null = the list SELECTS (#EXP-1388). */
export function ChatList({
  rows,
  agentRows = [],
  onOpenAgent,
  appRows = [],
  onOpenApp,
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
  flat = false,
  entry,
  className,
}: {
  rows: readonly ChatRowModel[]
  /** S-3: the agents pinned on top (`agentChatRows`). */
  agentRows?: readonly AgentChatRow[]
  onOpenAgent?: (agentId: string) => void
  /** #EXP-1733: the team's apps between the agents and the sessions (`appChatRows`). */
  appRows?: readonly AppChatRow[]
  onOpenApp?: (slug: string) => void
  /** A root's agent CLI (its working mark). */
  agentOf?: (sessionId: string) => string | null
  loading?: boolean
  /** The `motion-enter` class for a row that arrived after the first paint. */
  entering?: (sessionId: string) => string
  emptyTitle?: string
  onOpen?: (sessionId: string) => void
  /** #EXP-1388: the archived count for the entry row (0 = no entry). */
  archived?: number
  onOpenArchived?: () => void
  /** #EXP-1388: the selected ids while selecting, else null. */
  selection?: ReadonlySet<string> | null
  onRowClick?: (sessionId: string, event: ReactMouseEvent<HTMLButtonElement>) => void
  /** #EXP-1388: the rows are selectable (the wide checkbox column). */
  onSelectToggle?: (sessionId: string, event: ReactMouseEvent<HTMLElement>) => void
  /** #FEED-191: no section headers (the Archived page). */
  flat?: boolean
  /** #FEED-191: what leads the list (the phones' `NeedsYouEntryRow`). */
  entry?: ReactNode
  className?: string
}) {
  if (loading) return <SkeletonRows rowHeight={76} />
  const empty = <EmptyState icon={ChatsIcon} title={emptyTitle ?? CHATS_LIST_COPY.empty} size="page" />
  if (rows.length === 0 && archived === 0 && agentRows.length === 0 && appRows.length === 0 && !entry) return empty
  const aligned = Boolean(onSelectToggle) || selection !== null
  const paint = (row: ChatRowModel) => (
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
  )
  const archivedEntry =
    archived > 0 ? <ChatArchivedRow count={archived} onOpen={onOpenArchived} aligned={aligned} /> : null
  const sections = flat ? [] : chatSections(rows)
  return (
    <div data-slot="chat-list" data-testid="chat-list" className={cn(`flex flex-col`, className)}>
      {entry}
      {agentRows.length > 0 && (
        <>
          <ChatSectionHeader title={CHATS_COPY.sections.agents} />
          {agentRows.map((row) => (
            <AgentChatRowButton key={row.agentId} row={row} onOpen={onOpenAgent} aligned={aligned} />
          ))}
        </>
      )}
      {appRows.length > 0 && (
        <>
          <ChatSectionHeader title={CHATS_COPY.sections.apps} />
          {appRows.map((row) => (
            <AppChatRowButton key={row.appId} row={row} onOpen={onOpenApp} aligned={aligned} />
          ))}
        </>
      )}
      {rows.length === 0 && archived === 0 && empty}
      {flat ? (
        <>
          {archivedEntry}
          {rows.map(paint)}
        </>
      ) : (
        CHAT_SECTION_KEYS.map((key) => {
          const own = sections.find((section) => section.key === key)?.rows ?? []
          const lead = key === `chats` ? archivedEntry : null
          if (own.length === 0 && !lead) return null
          return (
            <div key={key} data-section={key} className="flex flex-col">
              <ChatSectionHeader title={CHATS_COPY.sections[key]} />
              {lead}
              {own.map(paint)}
            </div>
          )
        })
      )}
    </div>
  )
}
