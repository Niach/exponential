import type * as React from "react"
import type { ReactNode } from "react"

import { Button } from "./button"
import { cn } from "./cn"
import { ListRow } from "./glass-rows"
import { conceptIcon } from "./icons.generated"

// EXP-1284 — the THREAD's presentational parts (web; the styleguide paints
// these same components). A thread holds n issues and m runs; the model
// (title, strip, captions, the conversation's blocks) is the app's
// `lib/thread-view.ts`, fixture `thread-view.json` ×4. Nothing here queries:
// every glyph, chip and handler is handed in.
//
//   ThreadHeader        the title + the muted `thread` word
//   ThreadStrip         the CHILD STRIP: Conversation · one tab per issue ·
//                       "+" · divider · Guide (green dot), the header caption
//                       trailing; `pager` = the phones' pager tabs
//   ThreadRunHeader     a `run` / `parallel` block's header: glyph, `Run N`,
//                       the caption parts, a hairline, the state word
//   ThreadFoldedRun     an ended run folded to ONE line + Show work
//   ThreadLinkRow       `<glyph> Linked | Filed and linked [issue chip]`
//   ThreadParallelCard  the runs one parent started, ONE card, a row each
//   ThreadListRow       a list's thread row: mark, title, caption

const ConversationIcon = conceptIcon(`action-chat`)
const GuideIcon = conceptIcon(`guide-changes`)
const AddIcon = conceptIcon(`ui-add`)

/** The trailing state word of a block (`live` | `needs input` | `ended`). */
export type ThreadStateWord = `live` | `needs_input` | `ended`

export const THREAD_STATE_LABEL: Record<ThreadStateWord, string> = {
  live: `live`,
  needs_input: `needs input`,
  ended: `ended`,
}

export const THREAD_COPY = {
  word: `thread`,
  conversation: `Conversation`,
  guide: `Guide`,
  showWork: `Show work`,
  reply: `Reply in this thread`,
  threads: `Threads`,
  add: `Add to this thread`,
} as const

export function ThreadHeader({
  title,
  className,
  trailing,
}: {
  /** The title (a plain string, or the app's inline rename field). */
  title: ReactNode
  className?: string
  /** Anything after the `thread` word (a phone's caption). */
  trailing?: ReactNode
}) {
  return (
    <div
      data-testid="thread-header"
      className={cn(`flex min-w-0 items-baseline gap-2`, className)}
    >
      <h1 className="min-w-0 truncate text-lg font-semibold tracking-tight text-foreground md:text-xl">
        {title}
      </h1>
      <span className="shrink-0 text-xs text-muted-foreground">{THREAD_COPY.word}</span>
      {trailing}
    </div>
  )
}

/** One strip entry, already resolved by the app. */
export type ThreadStripEntry =
  | {
      kind: `conversation`
      active: boolean
      onSelect: () => void
    }
  | {
      kind: `issue`
      key: string
      identifier: string
      title: string
      /** The issue's status glyph (resolved against the team's statuses). */
      lead: ReactNode
      active: boolean
      /** A child the viewer has not seen yet: fades in with a ring. */
      fresh?: boolean
      onSelect: () => void
      /** The tab's context-menu hooks (`menuProps(kind, id)`). */
      menu?: Record<string, string>
    }
  | {
      kind: `guide`
      active: boolean
      dot: boolean
      onSelect: () => void
    }

const TAB_CLASS = `flex h-8 min-w-0 items-center gap-1.5 rounded-md border px-2.5 text-sm font-normal`

function tabClass(active: boolean, fresh?: boolean): string {
  return cn(
    TAB_CLASS,
    active
      ? `border-border bg-muted/50 text-foreground`
      : `border-transparent text-muted-foreground hover:text-foreground`,
    fresh &&
      `border-sky-500/60 motion-safe:animate-in motion-safe:fade-in motion-safe:duration-slow motion-safe:ease-decelerate`
  )
}

export function ThreadStrip({
  entries,
  onAdd,
  addMenu,
  caption,
  variant = `bar`,
  className,
}: {
  /** Conversation first, the issues in link order, the Guide last. */
  entries: readonly ThreadStripEntry[]
  /** The "+": opens the thread's menu (the app wraps it in its Menu). */
  onAdd?: () => void
  /** Or the "+" as a ready trigger (a Menu around the button). */
  addMenu?: (button: React.ReactElement) => ReactNode
  /** The header caption (`2 sessions · 2 devices`), trailing on md+. */
  caption?: string
  /** `bar` = md+ (the strip under the title); `pager` = phones' tabs. */
  variant?: `bar` | `pager`
  className?: string
}) {
  const pager = variant === `pager`
  const addButton = (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      aria-label={THREAD_COPY.add}
      title={THREAD_COPY.add}
      data-testid="thread-strip-add"
      className="shrink-0 text-muted-foreground hover:text-foreground"
      onClick={onAdd}
    >
      <AddIcon />
    </Button>
  )
  const conversation = entries.find((entry) => entry.kind === `conversation`)
  const issues = entries.filter(
    (entry): entry is Extract<ThreadStripEntry, { kind: `issue` }> => entry.kind === `issue`
  )
  const guide = entries.find(
    (entry): entry is Extract<ThreadStripEntry, { kind: `guide` }> => entry.kind === `guide`
  )
  const conversationTab = conversation && (
    <button
      type="button"
      role="tab"
      aria-selected={conversation.active}
      data-testid="thread-child-conversation"
      className={cn(tabClass(conversation.active), `shrink-0`)}
      onClick={conversation.onSelect}
    >
      {!pager && <ConversationIcon className="size-3.5 shrink-0" />}
      <span>{THREAD_COPY.conversation}</span>
    </button>
  )
  const issueTabs = issues.map((entry) => (
    <button
      key={entry.key}
      type="button"
      role="tab"
      aria-selected={entry.active}
      data-testid={`thread-child-${entry.identifier}`}
      data-fresh={entry.fresh || undefined}
      title={`${entry.identifier} · ${entry.title}`}
      className={cn(
        tabClass(entry.active, entry.fresh),
        // Every issue tab shares ONE max width (its title truncates); the
        // tabs never shrink unevenly — the row scrolls instead.
        pager ? `shrink-0 px-2` : `max-w-[9rem] shrink-0`
      )}
      onClick={entry.onSelect}
      {...entry.menu}
    >
      <span className="flex size-3.5 shrink-0 items-center justify-center">{entry.lead}</span>
      <span className="shrink-0 font-mono text-xs">{entry.identifier}</span>
      {!pager && <span className="min-w-0 truncate text-foreground/90">{entry.title}</span>}
    </button>
  ))
  const guideTab = guide && (
    <button
      type="button"
      role="tab"
      aria-selected={guide.active}
      data-testid="thread-child-guide"
      className={cn(tabClass(guide.active), `shrink-0`)}
      onClick={guide.onSelect}
    >
      {!pager && <GuideIcon className="size-3.5 shrink-0" />}
      <span>{THREAD_COPY.guide}</span>
      {guide.dot && (
        <span
          data-testid="thread-guide-dot"
          aria-label="The thread has a Guide"
          className="size-1.5 shrink-0 rounded-full bg-emerald-400"
        />
      )}
    </button>
  )
  const add = addMenu ? addMenu(addButton) : addButton

  if (pager) {
    return (
      <div
        data-testid="thread-strip"
        data-variant={variant}
        role="tablist"
        className={cn(
          `flex min-w-0 items-center gap-0.5 overflow-x-auto [scrollbar-width:none]`,
          className
        )}
      >
        {conversationTab}
        {issueTabs}
        {guideTab}
        {add}
      </div>
    )
  }

  // md+: Conversation · the issues scroll sideways once they overflow (the
  // group shrinks only then); the "+", the Guide and the caption stay in view.
  return (
    <div
      data-testid="thread-strip"
      data-variant={variant}
      role="tablist"
      className={cn(`flex min-w-0 items-center gap-1`, className)}
    >
      <div
        data-testid="thread-strip-scroll"
        className="flex min-w-0 items-center gap-1 overflow-x-auto [scrollbar-width:none]"
      >
        {conversationTab}
        {issues.length > 0 && <span aria-hidden className="mx-1 h-4 w-px shrink-0 bg-border" />}
        {issueTabs}
      </div>
      {add}
      <span aria-hidden className="mx-1 h-4 w-px shrink-0 bg-border" />
      {guideTab}
      {caption && (
        <span
          data-testid="thread-header-caption"
          className="ml-auto shrink-0 pl-3 text-xs text-muted-foreground"
        >
          {caption}
        </span>
      )}
    </div>
  )
}

export function ThreadRunHeader({
  lead,
  label,
  caption,
  state,
  onOpen,
  className,
  "data-testid": testId,
}: {
  /** The agent glyph (a spinner/spark while the run works). */
  lead: ReactNode
  /** `Session 2`, `Sessions 3 and 4`. */
  label: string
  /** The muted parts after the label, joined ` · `. */
  caption: readonly string[]
  state: ThreadStateWord
  /** Opens the run (its own page). */
  onOpen?: () => void
  className?: string
  "data-testid"?: string
}) {
  const head = (
    <>
      <span className="flex size-3.5 shrink-0 items-center justify-center">{lead}</span>
      <span className="shrink-0 text-foreground/90">{label}</span>
      {caption.length > 0 && (
        <span className="min-w-0 truncate text-muted-foreground">
          {` · ${caption.join(` · `)}`}
        </span>
      )}
    </>
  )
  return (
    <div
      data-testid={testId ?? `thread-run-header`}
      data-state={state}
      className={cn(`flex min-w-0 items-center gap-2 text-xs`, className)}
    >
      {onOpen ? (
        <button
          type="button"
          className="flex min-w-0 items-center gap-2 rounded-sm hover:underline"
          onClick={onOpen}
        >
          {head}
        </button>
      ) : (
        <div className="flex min-w-0 items-center gap-2">{head}</div>
      )}
      <span aria-hidden className="h-px min-w-6 flex-1 bg-border" />
      <span
        className={cn(
          `shrink-0`,
          state === `needs_input` ? `text-amber-400` : `text-muted-foreground`
        )}
      >
        {THREAD_STATE_LABEL[state]}
      </span>
    </div>
  )
}

export function ThreadFoldedRun({
  name,
  onShowWork,
  className,
}: {
  /** The run's name (its session identity subject). */
  name: string
  onShowWork: () => void
  className?: string
}) {
  return (
    <div
      data-testid="thread-folded-run"
      className={cn(`flex min-w-0 items-center gap-1 pl-[1.375rem] text-sm text-muted-foreground`, className)}
    >
      <span className="min-w-0 truncate">{name}</span>
      <span aria-hidden className="shrink-0">
        ·
      </span>
      <Button
        type="button"
        variant="link"
        size="inline"
        className="h-auto shrink-0 p-0 text-sm text-foreground/80"
        onClick={onShowWork}
      >
        {THREAD_COPY.showWork}
      </Button>
    </div>
  )
}

export function ThreadLinkRow({
  lead,
  label,
  chip,
  className,
}: {
  /** The linking agent's glyph. */
  lead: ReactNode
  /** `Linked` | `Filed and linked`. */
  label: string
  /** The issue chip (opens that issue's child). */
  chip: ReactNode
  className?: string
}) {
  return (
    <div
      data-testid="thread-link-row"
      className={cn(`flex min-w-0 items-center gap-2 text-sm text-muted-foreground`, className)}
    >
      <span className="flex size-3.5 shrink-0 items-center justify-center">{lead}</span>
      <span className="shrink-0">{label}</span>
      <span className="min-w-0">{chip}</span>
    </div>
  )
}

export interface ThreadParallelRowModel {
  key: string
  /** The run glyph (a spark while it works). */
  lead: ReactNode
  name: string
  /** The issue it works on (a short chip), when it has one. */
  chip?: ReactNode
  /** `device · 9m`. */
  meta: string
  onOpen?: () => void
}

export function ThreadParallelCard({
  rows,
  className,
}: {
  rows: readonly ThreadParallelRowModel[]
  className?: string
}) {
  return (
    <div
      data-testid="thread-parallel-card"
      className={cn(
        `flex flex-col divide-y divide-border overflow-hidden rounded-lg border border-border bg-card/40`,
        className
      )}
    >
      {rows.map((row) => (
        <ListRow
          key={row.key}
          interactive={Boolean(row.onOpen)}
          onClick={row.onOpen}
          data-testid="thread-parallel-row"
          className="h-10 gap-2.5 rounded-none px-3"
        >
          <span className="flex size-3.5 shrink-0 items-center justify-center">{row.lead}</span>
          <span className="min-w-0 truncate text-sm text-foreground">{row.name}</span>
          {row.chip && <span className="shrink-0">{row.chip}</span>}
          <span className="ml-auto shrink-0 pl-2 text-xs text-muted-foreground">{row.meta}</span>
        </ListRow>
      ))}
    </div>
  )
}

export function ThreadListRow({
  lead,
  title,
  caption,
  captionTone = `muted`,
  active = false,
  onClick,
  className,
  ...props
}: Omit<React.ComponentProps<`div`>, `title` | `onClick`> & {
  /** The live run's mark (spark / badge), else the thread glyph. */
  lead: ReactNode
  title: string
  /** `listCaption`: `2 issues · 1 session · macbook`. */
  caption?: string | null
  captionTone?: `muted` | `amber`
  active?: boolean
  onClick?: () => void
}) {
  return (
    <ListRow
      interactive
      active={active}
      onClick={onClick}
      data-thread-row=""
      className={cn(`h-[52px] gap-2 py-0 pr-3 pl-3`, className)}
      {...props}
    >
      <span className="flex size-3.5 shrink-0 items-center justify-center">{lead}</span>
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm">{title}</div>
        {caption && (
          <div
            data-slot="thread-row-caption"
            className={cn(
              `truncate text-xs`,
              captionTone === `amber` ? `text-amber-400` : `text-muted-foreground`
            )}
          >
            {caption}
          </div>
        )}
      </div>
    </ListRow>
  )
}

/** The glyph a thread row leads with when nothing in it runs. */
export function ThreadGlyph({ className }: { className?: string }) {
  return <ConversationIcon className={cn(`size-3.5 text-muted-foreground`, className)} />
}
