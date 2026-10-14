import { useLayoutEffect, useRef, type ComponentProps, type ReactNode } from "react"
import type { SessionTabMarkState, ThreadRow } from "@exp/domain-contract/session-tree"
import { AgentRunMark, type RunMarkState } from "./agent-brand-mark"
import { ASK_CARD_COPY, AskCard, AskPrompt, type AskOption } from "./ask-card"
import { Button } from "./button"
import { cn } from "./cn"
import { stripTabClass } from "./entity-chip"
import { ListRow } from "./glass-rows"
import { HoverCard, HoverCardContent, HoverCardTrigger } from "./hover-card"
import { SessionRow } from "./session-row"
import { treeGuides } from "./tree-guides"
import { conceptIcon } from "./icons.generated"
import { Tooltip, TooltipContent, TooltipTrigger } from "./tooltip"
import { UserMessageBubble } from "./user-message-bubble"
import { SESSION_CARD_TONE, SessionCard } from "./work-card"

// EXP-1293 Threads v2: the THREAD screen's presentational parts ×web (the
// styleguide paints these same components). The model is the contract's
// (`buildSessionThread` / `partitionThreads`, fixtures `session-thread.json`
// + `threads-list.json`, ×4); nothing here queries: every glyph, chip,
// renderer and handler is handed in.
//
//   ThreadTabs           the header's ONE strip: Conversation · one chip per
//                        linked issue · "+" · Guide (a dot while it holds
//                        anything); `pager` = the phones' tabs
//   ThreadConversation   the conversation rows: messages (a person's bubble
//                        right, the agent's markdown, system lines muted),
//                        THE session card per session, question and plan
//                        cards = THE session view's card (`ask-card.tsx`,
//                        EXP-1316) answered IN the card, a person's own
//                        queued row with its X (EXP-1317), subagent /
//                        picture / event lines
//   ThreadsListItem      a Threads list row: lead glyph, the title, ONE
//                        quiet word and the lock (private)
//   ChannelMessageRow    an agent channel's comment-style row

const ConversationIcon = conceptIcon(`action-chat`)
const GuideIcon = conceptIcon(`guide-changes`)
const AddIcon = conceptIcon(`ui-add`)
const CloseIcon = conceptIcon(`ui-close`)
const SubagentIcon = conceptIcon(`coding-subagent`)
const PrivateIcon = conceptIcon(`ui-private`)

// EXP-1316: THE strip tab box, shared with the issue chips' `tab` size.
const tabClass = stripTabClass

/** The add tab's accessible name (copy never says "thread"). */
export const THREAD_TABS_ADD_LABEL = `Link an issue`

export function ThreadTabs({
  conversation,
  issues,
  add,
  sessions,
  guide,
  variant = `bar`,
  className,
}: {
  conversation: { label: string; active: boolean; onSelect: () => void }
  /** One chip per linked issue, already rendered (the host resolves status,
   *  the active ring and the context menu). */
  issues: readonly ReactNode[]
  /** The "+" (a Menu around `ThreadTabsAddButton`); absent = read-only. */
  add?: ReactNode
  /** EXP-1316: the session chips group (`ThreadSessionGroup`) after the
   *  "+": md+ with a divider either side; the pager without. */
  sessions?: ReactNode
  guide: { label: string; active: boolean; dot: boolean; onSelect: () => void }
  /** `bar` = md+ under the title; `pager` = the phones' tabs. */
  variant?: `bar` | `pager`
  className?: string
}) {
  const pager = variant === `pager`
  // EXP-1316: ONE horizontally scrolling row; no chip ever shrinks, and the
  // active one is scrolled into view.
  const stripRef = useRef<HTMLDivElement | null>(null)
  const shownRef = useRef<Element | null>(null)
  useLayoutEffect(() => {
    // Only when the active tab CHANGES: a reader's own scroll stays put.
    const active = stripRef.current?.querySelector(`[aria-selected="true"]`) ?? null
    if (active === shownRef.current) return
    shownRef.current = active
    if (active && typeof (active as HTMLElement).scrollIntoView === `function`) {
      ;(active as HTMLElement).scrollIntoView({ block: `nearest`, inline: `nearest` })
    }
  })
  return (
    <div
      ref={stripRef}
      data-testid="thread-tabs"
      data-variant={variant}
      role="tablist"
      className={cn(
        `flex min-w-0 items-center gap-1 overflow-x-auto [scrollbar-width:none] [&>*]:shrink-0`,
        className
      )}
    >
        <Button
          type="button"
          variant="ghost"
          role="tab"
          aria-selected={conversation.active}
          data-testid="thread-tab-conversation"
          className={tabClass(conversation.active)}
          onClick={conversation.onSelect}
        >
          {!pager && <ConversationIcon className="size-3.5 shrink-0" />}
          {conversation.label}
        </Button>
        {issues.length > 0 && !pager && (
          <span aria-hidden className="mx-1 h-4 w-px shrink-0 bg-border" />
        )}
        {issues}
      {add}
      {sessions && (
        <>
          {!pager && <span aria-hidden className="mx-1 h-4 w-px shrink-0 bg-border" />}
          {sessions}
        </>
      )}
      {!pager && <span aria-hidden className="mx-1 h-4 w-px shrink-0 bg-border" />}
      <Button
        type="button"
        variant="ghost"
        role="tab"
        aria-selected={guide.active}
        data-testid="thread-tab-guide"
        className={tabClass(guide.active)}
        onClick={guide.onSelect}
      >
        {!pager && <GuideIcon className="size-3.5 shrink-0" />}
        {guide.label}
        {guide.dot && (
          <span
            data-testid="thread-guide-dot"
            aria-label="The Guide has content"
            className="size-1.5 shrink-0 rounded-full bg-emerald-400"
          />
        )}
      </Button>
    </div>
  )
}

/** A session chip's mark state ×4 (`sessionTabMarkState`) in the run
 *  marks' words (waiting = the bare mark). */
export function sessionTabRunMark(state: SessionTabMarkState): RunMarkState | undefined {
  if (state === `needs_you`) return `needs_input`
  if (state === `working`) return `working`
  if (state === `ended`) return `ended`
  return undefined
}

/** EXP-1316: ONE session chip of the strip ×4: the agent's mark (its state)
 *  and the name; muted at rest, the tab box when it is the open session
 *  (phones: bordered, `bordered`). */
export function ThreadSessionTab({
  agent,
  markState,
  name,
  active,
  onSelect,
  bordered = false,
}: {
  agent: string | null
  markState: SessionTabMarkState
  name: string
  active: boolean
  onSelect: () => void
  /** The phones' pager chip: a bordered rect at the tab's height, like the
   *  issue chips there. */
  bordered?: boolean
}) {
  return (
    <Button
      type="button"
      variant="ghost"
      role="tab"
      aria-selected={active}
      data-testid="thread-tab-session"
      className={cn(stripTabClass(active, bordered), `max-w-[11rem]`)}
      onClick={onSelect}
    >
      <AgentRunMark
        agent={agent}
        state={sessionTabRunMark(markState)}
        ringClassName="ring-background"
        className="size-3.5"
      />
      <span className="min-w-0 truncate">{name}</span>
    </Button>
  )
}

export interface ThreadTreeRow {
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
export function threadTreeRows(
  rows: readonly ThreadTreeRow[]
): { row: ThreadTreeRow; depth: number }[] {
  const ids = new Set(rows.map((row) => row.sessionId))
  const children = new Map<string | null, ThreadTreeRow[]>()
  for (const row of rows) {
    const parent = row.parentId && ids.has(row.parentId) ? row.parentId : null
    const list = children.get(parent) ?? []
    list.push(row)
    children.set(parent, list)
  }
  const out: { row: ThreadTreeRow; depth: number }[] = []
  const walk = (parent: string | null, depth: number) => {
    for (const row of children.get(parent) ?? []) {
      out.push({ row, depth })
      walk(row.sessionId, depth + 1)
    }
  }
  walk(null, 0)
  return out
}

/** EXP-1316: the session tree popover's body: the WHOLE tree as
 *  `SessionRow size="small"` rows with their guides, the open one washed. */
export function ThreadSessionTree({
  rows,
  activeId,
  onOpen,
}: {
  rows: readonly ThreadTreeRow[]
  activeId: string | null
  onOpen: (sessionId: string) => void
}) {
  const ordered = threadTreeRows(rows)
  const guides = treeGuides(ordered.map((entry) => entry.depth))
  return (
    <div data-testid="thread-session-tree" className="flex flex-col">
      {ordered.map(({ row, depth }, index) => (
        <SessionRow
          key={row.sessionId}
          size="small"
          agent={row.agent}
          markState={sessionTabRunMark(row.markState)}
          title={row.name}
          trailing={
            row.stateWord ? (
              <span
                data-slot="thread-session-tree-word"
                className={cn(
                  `shrink-0 text-muted-foreground`,
                  row.stateTone && SESSION_CARD_TONE[row.stateTone]
                )}
              >
                {row.stateWord}
              </span>
            ) : undefined
          }
          depth={depth}
          guide={guides[index]}
          active={row.sessionId === activeId}
          ringClassName="ring-popover"
          data-testid="thread-session-tree-row"
          onClick={() => onOpen(row.sessionId)}
        />
      ))}
    </div>
  )
}

/** The session chips as ONE group; on a pointer, hovering it opens the
 *  tree popover (`tree`). */
export function ThreadSessionGroup({
  children,
  tree,
}: {
  children: ReactNode
  tree?: ReactNode
}) {
  const group = (
    <div data-testid="thread-session-group" className="flex shrink-0 items-center gap-1">
      {children}
    </div>
  )
  if (!tree) return group
  return (
    <HoverCard openDelay={250}>
      <HoverCardTrigger asChild>{group}</HoverCardTrigger>
      <HoverCardContent align="end" className="w-80 p-1.5">
        {tree}
      </HoverCardContent>
    </HoverCard>
  )
}

/** The strip's "+" (the host wraps it in its Menu). */
export function ThreadTabsAddButton(props: ComponentProps<typeof Button>) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      aria-label={THREAD_TABS_ADD_LABEL}
      title={THREAD_TABS_ADD_LABEL}
      data-testid="thread-tab-add"
      className="shrink-0 text-muted-foreground hover:text-foreground"
      {...props}
    >
      <AddIcon />
    </Button>
  )
}

export interface ThreadConversationProps {
  rows: readonly ThreadRow[]
  /** The app's GFM renderer (agent text, a person's bubble, a plan). */
  renderText: (text: string) => ReactNode
  /** A person's bubble caption (`name · time`, `queued`). */
  messageCaption?: (row: Extract<ThreadRow, { type: `message` }>) => string | null
  attachmentSrc: (attachmentId: string) => string
  /** A session card was tapped (its raw view). */
  onOpenSession?: (sessionId: string) => void
  /** A question card's answer: an option or the typed free text (absent =
   *  read-only). */
  onAnswer?: (rowId: string, input: { answer: string }) => void
  /** A plan card's decision + the reject's note (absent = read-only). */
  onDecide?: (rowId: string, input: { decision: `approved` | `rejected`; feedback?: string }) => void
  /** The rows whose answer is in flight. */
  pending?: ReadonlySet<string>
  /** The viewer: their OWN queued rows carry the Remove from queue X. */
  viewerId?: string | null
  /** EXP-1317: revoke a queued row (absent = no X). */
  onUnqueue?: (rowId: string) => void
  className?: string
}

/** EXP-1317: the queued row's X (tooltip + accessible name). */
export const THREAD_UNQUEUE_LABEL = `Remove from queue`

export function ThreadConversation({
  rows,
  renderText,
  messageCaption,
  attachmentSrc,
  onOpenSession,
  onAnswer,
  onDecide,
  pending,
  viewerId,
  onUnqueue,
  className,
}: ThreadConversationProps) {
  return (
    <div data-testid="thread-conversation" className={cn(`flex flex-col gap-4`, className)}>
      {rows.map((row) => {
        switch (row.type) {
          case `message`:
            if (row.author === `user`) {
              const own = row.queued && onUnqueue && viewerId && row.authorId === viewerId
              return (
                <UserMessageBubble
                  key={row.id}
                  text={row.body}
                  renderText={renderText}
                  caption={messageCaption?.(row) ?? null}
                  action={own ? <UnqueueButton onClick={() => onUnqueue(row.id)} /> : undefined}
                />
              )
            }
            if (row.author === `system`) {
              return (
                <div
                  key={row.id}
                  data-slot="thread-system"
                  className="text-xs text-muted-foreground"
                >
                  {row.body}
                </div>
              )
            }
            return (
              <div key={row.id} data-slot="thread-agent" className="text-sm text-foreground">
                {renderText(row.body)}
              </div>
            )
          case `session`:
            return (
              <SessionCard
                key={row.id}
                card={row.card}
                onOpen={onOpenSession ? () => onOpenSession(row.card.sessionId) : undefined}
              />
            )
          case `question`:
            return (
              <ThreadQuestionCard
                key={row.id}
                row={row}
                renderText={renderText}
                busy={pending?.has(row.id) ?? false}
                onAnswer={onAnswer ? (answer) => onAnswer(row.id, { answer }) : undefined}
              />
            )
          case `plan`:
            return (
              <ThreadPlanCard
                key={row.id}
                row={row}
                renderText={renderText}
                busy={pending?.has(row.id) ?? false}
                onDecide={onDecide ? (input) => onDecide(row.id, input) : undefined}
              />
            )
          case `subagent`:
            return (
              <div
                key={row.id}
                data-slot="thread-subagent"
                className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground"
              >
                <SubagentIcon className="size-3.5 shrink-0" />
                <span className="min-w-0 truncate">{row.title}</span>
                {row.status && <span className="shrink-0">{row.status}</span>}
              </div>
            )
          case `picture`:
            return (
              <figure key={row.id} data-slot="thread-picture" className="flex flex-col gap-1">
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
          case `event`:
            return (
              <div
                key={row.id}
                data-slot="thread-event"
                data-event={row.event}
                className="text-xs text-muted-foreground"
              >
                {row.text}
              </div>
            )
          default:
            return null
        }
      })}
    </div>
  )
}

/** The queued row's X: ghost icon-xs, THE tooltip (Radix drops it with the
 *  trigger, so it never outlives the row flipping to consumed). */
function UnqueueButton({ onClick }: { onClick: () => void }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label={THREAD_UNQUEUE_LABEL}
          data-testid="thread-unqueue"
          className="shrink-0 text-muted-foreground hover:text-foreground"
          onClick={onClick}
        >
          <CloseIcon className="size-3" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{THREAD_UNQUEUE_LABEL}</TooltipContent>
    </Tooltip>
  )
}

/** The free-text row's key (never an option value). */
const FREE_TEXT_KEY = `free-text`

/** EXP-1316: a question row = THE session view's card (`AskCard` +
 *  `AskPrompt`): its options, the free-text row (`custom`), closed once
 *  answered. */
function ThreadQuestionCard({
  row,
  renderText,
  busy,
  onAnswer,
}: {
  row: Extract<ThreadRow, { type: `question` }>
  renderText: (text: string) => ReactNode
  busy: boolean
  onAnswer?: (answer: string) => void
}) {
  const options: AskOption[] = [
    ...row.options.map((option, index) => ({ key: `${index}:${option}`, label: option })),
    ...(row.custom ? [{ key: FREE_TEXT_KEY, label: ASK_CARD_COPY.freeTextRow, freeText: true }] : []),
  ]
  return (
    <AskCard data-slot="thread-question" data-answered={row.answered || undefined}>
      <div className="text-sm text-foreground/90">{renderText(row.question)}</div>
      <AskPrompt
        options={options}
        answerable={!row.answered && onAnswer !== undefined}
        locked={busy ? { labels: [] } : null}
        resolved={row.answered ? { answer: row.answer } : null}
        onAnswer={(_keys, labels, text) => {
          const answer = text ?? labels[0]
          if (answer) onAnswer?.(answer)
        }}
      />
    </AskCard>
  )
}

/** The plan card's words (the decision values are `sessions.answer`'s). */
export const THREAD_PLAN_COPY = {
  approve: `Approve`,
  reject: `Reject`,
  approved: `Approved`,
  rejected: `Rejected`,
} as const

/** EXP-1316: a plan row = THE session view's plan card: the plan in full,
 *  Approve (primary) / Reject (opens the note field), closed once decided. */
function ThreadPlanCard({
  row,
  renderText,
  busy,
  onDecide,
}: {
  row: Extract<ThreadRow, { type: `plan` }>
  renderText: (text: string) => ReactNode
  busy: boolean
  onDecide?: (input: { decision: `approved` | `rejected`; feedback?: string }) => void
}) {
  const decided = row.decision === `approved` || row.decision === `rejected`
  return (
    <AskCard
      plan
      label={ASK_CARD_COPY.planReady}
      data-slot="thread-plan"
      data-decision={row.decision ?? undefined}
    >
      <div className="text-sm">{renderText(row.plan)}</div>
      <AskPrompt
        planMode
        variant="plan"
        options={[
          { key: `approved`, label: THREAD_PLAN_COPY.approve },
          { key: `rejected`, label: THREAD_PLAN_COPY.reject },
        ]}
        answerable={!decided && onDecide !== undefined}
        locked={busy ? { labels: [] } : null}
        resolved={
          decided
            ? {
                answer:
                  row.decision === `approved`
                    ? THREAD_PLAN_COPY.approved
                    : THREAD_PLAN_COPY.rejected,
              }
            : null
        }
        onAnswer={(keys, _labels, text) =>
          onDecide?.(
            keys[0] === `rejected`
              ? { decision: `rejected`, ...(text ? { feedback: text } : {}) }
              : { decision: `approved` }
          )
        }
      />
    </AskCard>
  )
}

export function ThreadsListItem({
  lead,
  title,
  word,
  wordIcon,
  locked = false,
  active = false,
  onOpen,
  testId = `threads-row`,
  className,
}: {
  /** The agent mark / state glyph (spinner busy, amber dot needs you). */
  lead: ReactNode
  /** The thread title (it names the issue, EXP-1316: no identifier slot). */
  title: string
  /** ONE quiet trailing word (`ready to merge`, `stack`, `merged`). */
  word?: string | null
  wordIcon?: ReactNode
  /** A private thread: the lock after the word (EXP-1318: both). */
  locked?: boolean
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
      className={cn(`h-10 gap-2 py-0 pr-3 pl-3 md:h-9`, className)}
    >
      <span className="flex size-3.5 shrink-0 items-center justify-center">{lead}</span>
      <span className="min-w-0 flex-1 truncate text-sm">{title}</span>
      {word && (
        <span
          data-slot="threads-row-word"
          className="flex shrink-0 items-center gap-1 text-[11.5px] text-muted-foreground"
        >
          {wordIcon}
          {word}
        </span>
      )}
      {locked && (
        <PrivateIcon
          aria-label="Private"
          data-slot="threads-row-lock"
          className="size-3.5 shrink-0 text-muted-foreground"
        />
      )}
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
