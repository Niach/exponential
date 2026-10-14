import type { ComponentProps, ReactNode } from "react"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { Button } from "./button"
import { cn } from "./cn"
import { ListRow } from "./glass-rows"
import { conceptIcon } from "./icons.generated"
import { UserMessageBubble } from "./user-message-bubble"
import { SessionCard } from "./work-card"

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
//                        cards answered IN the card, subagent / picture /
//                        event lines
//   ThreadsListItem      a Threads list row: lead glyph, the first issue's
//                        identifier, the title, ONE quiet word or a lock
//   ChannelMessageRow    an agent channel's comment-style row

const ConversationIcon = conceptIcon(`action-chat`)
const GuideIcon = conceptIcon(`guide-changes`)
const AddIcon = conceptIcon(`ui-add`)
const QuestionIcon = conceptIcon(`coding-needs-input`)
const PlanIcon = conceptIcon(`coding-plan`)
const SubagentIcon = conceptIcon(`coding-subagent`)
const PrivateIcon = conceptIcon(`ui-private`)

const TAB_CLASS = `h-8 min-w-0 shrink-0 gap-1.5 rounded-md border px-2.5 text-sm font-normal`

function tabClass(active: boolean): string {
  return cn(
    TAB_CLASS,
    active
      ? `border-glass-stroke-strong bg-glass-active text-foreground hover:bg-glass-active`
      : `border-transparent text-muted-foreground hover:text-foreground`
  )
}

/** The add tab's accessible name (copy never says "thread"). */
export const THREAD_TABS_ADD_LABEL = `Link an issue`

export function ThreadTabs({
  conversation,
  issues,
  add,
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
  guide: { label: string; active: boolean; dot: boolean; onSelect: () => void }
  /** `bar` = md+ under the title; `pager` = the phones' tabs. */
  variant?: `bar` | `pager`
  className?: string
}) {
  const pager = variant === `pager`
  return (
    <div
      data-testid="thread-tabs"
      data-variant={variant}
      role="tablist"
      className={cn(`flex min-w-0 items-center gap-1`, className)}
    >
      <div className="flex min-w-0 items-center gap-1 overflow-x-auto [scrollbar-width:none]">
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
      </div>
      {add}
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

/** An issue chip's class in the strip: the active one wears the tab box. */
export function threadTabChipClass(active: boolean): string {
  return cn(`shrink-0 max-w-[11rem]`, active && `ring-1 ring-glass-stroke-strong bg-glass-active`)
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
  /** A question card's option (absent = read-only). */
  onAnswer?: (rowId: string, answer: string) => void
  /** A plan card's decision (absent = read-only). */
  onDecide?: (rowId: string, decision: `approved` | `rejected`) => void
  /** The rows whose answer is in flight. */
  pending?: ReadonlySet<string>
  className?: string
}

export function ThreadConversation({
  rows,
  renderText,
  messageCaption,
  attachmentSrc,
  onOpenSession,
  onAnswer,
  onDecide,
  pending,
  className,
}: ThreadConversationProps) {
  return (
    <div data-testid="thread-conversation" className={cn(`flex flex-col gap-4`, className)}>
      {rows.map((row) => {
        switch (row.type) {
          case `message`:
            if (row.author === `user`) {
              return (
                <UserMessageBubble
                  key={row.id}
                  text={row.body}
                  renderText={renderText}
                  caption={messageCaption?.(row) ?? null}
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
              <QuestionCard
                key={row.id}
                row={row}
                busy={pending?.has(row.id) ?? false}
                onAnswer={onAnswer ? (answer) => onAnswer(row.id, answer) : undefined}
              />
            )
          case `plan`:
            return (
              <PlanCard
                key={row.id}
                row={row}
                renderText={renderText}
                busy={pending?.has(row.id) ?? false}
                onDecide={onDecide ? (decision) => onDecide(row.id, decision) : undefined}
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

const CARD_CLASS = `flex flex-col gap-3 rounded-xl border border-glass-stroke-strong bg-glass-row p-4`

function QuestionCard({
  row,
  busy,
  onAnswer,
}: {
  row: Extract<ThreadRow, { type: `question` }>
  busy: boolean
  onAnswer?: (answer: string) => void
}) {
  const open = !row.answered && onAnswer !== undefined
  return (
    <div data-slot="thread-question" data-answered={row.answered || undefined} className={CARD_CLASS}>
      <div className="flex items-start gap-2">
        <QuestionIcon className="mt-0.5 size-4 shrink-0 text-amber-400" />
        <p className="min-w-0 text-sm text-foreground">{row.question}</p>
      </div>
      {row.answered ? (
        row.answer && <p className="pl-6 text-sm text-muted-foreground">{row.answer}</p>
      ) : (
        <div className="flex flex-col gap-1.5 pl-6">
          {row.options.map((option, index) => (
            <Button
              key={`${index}-${option}`}
              type="button"
              variant="outline"
              disabled={!open || busy}
              data-testid="thread-question-option"
              className="h-auto justify-between gap-3 rounded-lg px-3 py-2 text-left font-normal whitespace-normal"
              onClick={() => onAnswer?.(option)}
            >
              <span className="min-w-0">{option}</span>
              <span className="flex size-5 shrink-0 items-center justify-center rounded-full border border-glass-stroke text-[0.6875rem] text-muted-foreground tabular-nums">
                {index + 1}
              </span>
            </Button>
          ))}
        </div>
      )}
    </div>
  )
}

/** The plan card's words (the decision values are `sessions.answer`'s). */
export const THREAD_PLAN_COPY = {
  approve: `Approve`,
  reject: `Reject`,
  approved: `Approved`,
  rejected: `Rejected`,
} as const

function PlanCard({
  row,
  renderText,
  busy,
  onDecide,
}: {
  row: Extract<ThreadRow, { type: `plan` }>
  renderText: (text: string) => ReactNode
  busy: boolean
  onDecide?: (decision: `approved` | `rejected`) => void
}) {
  const decided = row.decision === `approved` || row.decision === `rejected`
  return (
    <div data-slot="thread-plan" data-decision={row.decision ?? undefined} className={CARD_CLASS}>
      <div className="flex items-start gap-2">
        <PlanIcon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1 text-sm">{renderText(row.plan)}</div>
      </div>
      {decided ? (
        <p className="pl-6 text-xs text-muted-foreground">
          {row.decision === `approved` ? THREAD_PLAN_COPY.approved : THREAD_PLAN_COPY.rejected}
        </p>
      ) : (
        onDecide && (
          <div className="flex gap-2 pl-6">
            <Button
              type="button"
              size="sm"
              disabled={busy}
              data-testid="thread-plan-approve"
              onClick={() => onDecide(`approved`)}
            >
              {THREAD_PLAN_COPY.approve}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={busy}
              data-testid="thread-plan-reject"
              onClick={() => onDecide(`rejected`)}
            >
              {THREAD_PLAN_COPY.reject}
            </Button>
          </div>
        )
      )}
    </div>
  )
}

export function ThreadsListItem({
  lead,
  identifier,
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
  /** The first linked issue's identifier (null = an empty column; absent =
   *  no column, an agent row). */
  identifier?: string | null
  title: string
  /** ONE quiet trailing word (`ready to merge`, `stack`, `merged`). */
  word?: string | null
  wordIcon?: ReactNode
  /** A private thread: the lock instead of a word. */
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
      {identifier !== undefined && (
        <span className="min-w-[4.5rem] shrink-0 font-mono text-xs text-muted-foreground">
          {identifier ?? ``}
        </span>
      )}
      <span className="min-w-0 flex-1 truncate text-sm">{title}</span>
      {locked ? (
        <PrivateIcon
          aria-label="Private"
          data-slot="threads-row-lock"
          className="size-3.5 shrink-0 text-muted-foreground"
        />
      ) : (
        word && (
          <span
            data-slot="threads-row-word"
            className="flex shrink-0 items-center gap-1 text-[11.5px] text-muted-foreground"
          >
            {wordIcon}
            {word}
          </span>
        )
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
