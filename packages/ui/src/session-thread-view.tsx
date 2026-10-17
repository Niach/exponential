import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ComponentProps,
  type ComponentType,
  type ReactElement,
  type ReactNode,
  type Ref,
} from "react"
import {
  partitionThreadStrip,
  SUGGESTIONS_COPY,
  type SessionTabMarkState,
  type SuggestionRow,
  type ThreadRow,
  type ThreadStripLayout,
} from "@exp/domain-contract/session-tree"
import {
  askDock,
  PLAN_OPTIONS,
  planDecisionLabel,
  threadPendingAsks,
} from "@exp/domain-contract/ask-dock"
import { AgentRunMark, type RunMarkState } from "./agent-brand-mark"
import { answerIssue, joinAnswers, PLAN_CARD_COPY, PLAN_CARD_TEST_IDS } from "@exp/domain-contract/plan-card"
import { ASK_CARD_COPY, AskAnsweredLine, AskCard, AskPrompt, type AskOption } from "./ask-card"
import { Composer } from "./composer"
import {
  closedPicks,
  PlanCardCaption,
  PlanCardCustomField,
  PlanCardOptions,
  PlanCardQuestionBlock,
  PlanCardSurface,
  questionAnswer,
  stripPendingUploads,
  useSurfaceDataRef,
} from "./plan-card"
import { Textarea } from "./textarea"
import { AskDock, AskDockBody } from "./ask-dock"
import { Button } from "./button"
import { cn } from "./cn"
import { stripTabClass } from "./entity-chip"
import { ListRow } from "./glass-rows"
import { Menu, MenuRow, type MenuEntry, type MenuIcon } from "./menu"
import { SessionRow, SessionRowDevice } from "./session-row"
import { StatusGlyph, type StatusGlyphProps } from "./status-glyph"
import { StripChip, StripOverflowButton } from "./strip-chip"
import { TREE_BASE, TREE_INDENT, treeGuides } from "./tree-guides"
import { TreeGuides } from "./tree-guides-view"
import { conceptIcon } from "./icons.generated"
import { Tooltip, TooltipContent, TooltipTrigger } from "./tooltip"
import { UserMessageBubble } from "./user-message-bubble"
import { Spinner } from "./feedback"
import { SESSION_CARD_TONE, SessionCard } from "./work-card"

// EXP-1293 Threads v2: the THREAD screen's presentational parts ×web (the
// styleguide paints these same components). The model is the contract's
// (`buildSessionThread` / `partitionThreads`, fixtures `session-thread.json`
// + `threads-list.json`, ×4); nothing here queries: every glyph, chip,
// renderer and handler is handed in.
//
//   ThreadTabs           the header's ONE strip, nothing scrolls (EXP-1334):
//                        "+" · Conversation | issue chips … +X | session
//                        chips … +Y | Guide (a dot while it holds anything);
//                        `pager` = the phones' tabs (Conversation and Guide
//                        icon-only there). Chips = THE strip chip
//                        (`strip-chip.tsx`); a folded chip opens its subtree
//                        (`ThreadChipSubtree`) on a click
//   ThreadConversation   the conversation rows: messages (a person's bubble
//                        right, the agent's markdown, system lines muted),
//                        THE session card per session, question and plan
//                        cards = THE session view's card (`ask-card.tsx`,
//                        EXP-1316) answered IN the card, a person's own
//                        queued row with its X (EXP-1317), subagent /
//                        picture / event lines; EXP-1323: a child's
//                        `report` line (opens it) and THE suggested tasks
//                        card (`ThreadSuggestionsCard`), then the host's
//                        live tail (Tasks card + status line)
//   ThreadsListItem      a Threads list row: lead glyph, the title (bold
//                        while unread), ONE quiet word and the lock
//                        (private), the live caption under it
//   ChannelMessageRow    an agent channel's comment-style row

const ConversationIcon = conceptIcon(`action-chat`)
const GuideIcon = conceptIcon(`guide-changes`)
const AddIcon = conceptIcon(`ui-add`)
const CloseIcon = conceptIcon(`ui-close`)
const SubagentIcon = conceptIcon(`coding-subagent`)
const ReportIcon = conceptIcon(`notification-reporter-reply`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const SuggestionsIcon = conceptIcon(`ui-checklist`)

// EXP-1316: THE strip tab box, shared with the issue chips' `tab` size.
const tabClass = stripTabClass

/** The add tab's accessible name (copy never says "thread"): THE Menu with
 *  Link an issue › and New issue (EXP-1334). */
export const THREAD_TABS_ADD_LABEL = `Add an issue`

/** EXP-1334: one chip of a strip zone (issues or sessions). The strip
 *  measures every chip off-screen, `partitionThreadStrip` picks the tier and
 *  which chips draw; the rest fold into the zone's "+N" (THE Menu, one
 *  `entry` per hidden chip). */
export interface ThreadStripItem {
  key: string
  /** The chip itself: `short` = the identifier alone (issues only; a
   *  session chip ignores it); `measure` = the hidden measuring copy (no
   *  handlers, no menus). */
  render: (options: { short: boolean; measure: boolean }) => ReactNode
  /** Its row in the zone's "+N" menu. */
  entry: MenuEntry
  /** The open chip (or one of its nested items): it never folds away. */
  active: boolean
}

/** The "+N" buttons' accessible names. */
export const THREAD_STRIP_MORE_COPY = {
  issues: (count: number) => `${count} more ${count === 1 ? `issue` : `issues`}`,
  sessions: (count: number) => `${count} more ${count === 1 ? `session` : `sessions`}`,
} as const

interface StripMeasure {
  available: number
  gap: number
  overflowW: number
  dividerW: number
  issueFull: number[]
  issueShort: number[]
  sessions: number[]
}

function sameMeasure(a: StripMeasure | null, b: StripMeasure): boolean {
  if (!a) return false
  const eq = (x: number[], y: number[]) => x.length === y.length && x.every((v, i) => v === y[i])
  return (
    a.available === b.available &&
    a.gap === b.gap &&
    a.overflowW === b.overflowW &&
    a.dividerW === b.dividerW &&
    eq(a.issueFull, b.issueFull) &&
    eq(a.issueShort, b.issueShort) &&
    eq(a.sessions, b.sessions)
  )
}

/**
 * EXP-1334: the thread header's ONE strip ×4 — NOTHING scrolls. Pinned left:
 * the "+" (THE Menu: Link an issue ›, New issue) and Conversation; pinned
 * right: Guide (a dot while it holds anything). Between them two zones,
 * issues then sessions: everything full → the issues identifier-only → fold,
 * each zone with its own "+N" (`partitionThreadStrip`, the contract's
 * `stripCases`), the open chip always drawn. `pager` = the phones' header
 * band: Conversation and Guide ICON-ONLY (the strip chip box, the label as
 * aria-label/title, the Guide dot on the icon's corner).
 */
export function ThreadTabs({
  conversation,
  issues,
  add,
  sessions = [],
  guide,
  variant = `bar`,
  layout: forced,
  className,
}: {
  conversation: { label: string; active: boolean; onSelect: () => void }
  /** The issue chips, strip order (top-level linked issues). */
  issues: readonly ThreadStripItem[]
  /** The "+" (a Menu around `ThreadTabsAddButton`); absent = read-only. */
  add?: ReactNode
  /** The session chips (the dispatcher's direct children). */
  sessions?: readonly ThreadStripItem[]
  guide: { label: string; active: boolean; dot: boolean; onSelect: () => void }
  /** `bar` = md+ under the title; `pager` = the phones' tabs. */
  variant?: `bar` | `pager`
  /** A fixed layout instead of the measured one (a static render — the
   *  styleguide's islands run no effects). */
  layout?: ThreadStripLayout
  className?: string
}) {
  const pager = variant === `pager`
  const middleRef = useRef<HTMLDivElement | null>(null)
  const measureRef = useRef<HTMLDivElement | null>(null)
  const [measure, setMeasure] = useState<StripMeasure | null>(null)

  const read = useCallback(() => {
    const middle = middleRef.current
    const ruler = measureRef.current
    if (!middle || !ruler) return
    const widths = (slot: string) =>
      Array.from(ruler.querySelectorAll<HTMLElement>(`[data-measure="${slot}"]`)).map(
        (node) => node.getBoundingClientRect().width
      )
    const columnGap = Number.parseFloat(getComputedStyle(middle).columnGap)
    const next: StripMeasure = {
      available: middle.getBoundingClientRect().width,
      gap: Number.isFinite(columnGap) ? columnGap : 4,
      overflowW: widths(`overflow`)[0] ?? 32,
      dividerW: widths(`divider`)[0] ?? 1,
      issueFull: widths(`issue-full`),
      issueShort: widths(`issue-short`),
      sessions: widths(`session`),
    }
    setMeasure((current) => (sameMeasure(current, next) ? current : next))
  }, [])

  // Before paint whenever the CHIPS changed (a chip came or went, its state
  // flipped, its text moved: the ruler's text is the chips' text, the keys
  // and active flags the rest) and never otherwise: the host re-renders
  // every second (its clock), and a layout read a second is what this
  // skips …
  const chipKey = [
    pager ? `pager` : `bar`,
    ...issues.map((item) => `${item.key}\u0001${item.active ? 1 : 0}`),
    `|`,
    ...sessions.map((item) => `${item.key}\u0001${item.active ? 1 : 0}`),
  ].join(`\u0002`)
  const measuredRef = useRef<string | null>(null)
  useLayoutEffect(() => {
    const signature = `${chipKey}\u0003${measureRef.current?.textContent ?? ``}`
    if (measuredRef.current === signature) return
    measuredRef.current = signature
    read()
  })
  // … and whenever the strip resizes (the window, the sidebar).
  useEffect(() => {
    const middle = middleRef.current
    if (!middle || typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(read)
    observer.observe(middle)
    return () => observer.disconnect()
  }, [read])

  const both = issues.length > 0 && sessions.length > 0
  const activeIssue = issues.findIndex((item) => item.active)
  const activeSession = sessions.findIndex((item) => item.active)
  const layout: ThreadStripLayout = forced
    ? forced
    : measure &&
    measure.issueFull.length === issues.length &&
    measure.issueShort.length === issues.length &&
    measure.sessions.length === sessions.length &&
    measure.available > 0
      ? partitionThreadStrip({
          issues: issues.map((_, index) => ({
            full: measure.issueFull[index],
            short: measure.issueShort[index],
          })),
          sessions: measure.sessions,
          // The divider between the zones (and its second gap) is not a chip.
          available: measure.available - (both ? measure.dividerW + measure.gap : 0),
          gap: measure.gap,
          overflowW: measure.overflowW,
          activeIssue: activeIssue < 0 ? null : activeIssue,
          activeSession: activeSession < 0 ? null : activeSession,
        })
      : {
          tier: `full`,
          issues: issues.map((_, index) => index),
          sessions: sessions.map((_, index) => index),
        }
  const short = layout.tier !== `full`
  const shownIssues = new Set(layout.issues)
  const shownSessions = new Set(layout.sessions)
  const hiddenIssues = issues.filter((_, index) => !shownIssues.has(index))
  const hiddenSessions = sessions.filter((_, index) => !shownSessions.has(index))

  const divider = <span aria-hidden className="mx-1 h-4 w-px shrink-0 bg-border" />
  const more = (hidden: readonly ThreadStripItem[], label: string, testId: string) =>
    hidden.length > 0 ? (
      <Menu
        align="end"
        aria-label={label}
        title={label}
        contentClassName="w-72"
        entries={hidden.map((item) => item.entry)}
        trigger={
          <StripOverflowButton
            count={hidden.length}
            aria-label={label}
            data-testid={testId}
          />
        }
      />
    ) : null

  return (
    <div
      data-testid="thread-tabs"
      data-variant={variant}
      data-tier={layout.tier}
      className={cn(`relative flex min-w-0 items-center gap-1`, className)}
    >
      {/* The ruler: every chip at its natural width (full AND short), the
          "+N" and the zone divider — invisible, inert, out of the a11y tree. */}
      <div
        ref={measureRef}
        aria-hidden
        inert
        className="pointer-events-none invisible absolute top-0 left-0 flex w-max gap-1"
      >
        {issues.map((item) => (
          <div key={`f-${item.key}`} data-measure="issue-full" className="flex shrink-0">
            {item.render({ short: pager, measure: true })}
          </div>
        ))}
        {issues.map((item) => (
          <div key={`s-${item.key}`} data-measure="issue-short" className="flex shrink-0">
            {item.render({ short: true, measure: true })}
          </div>
        ))}
        {sessions.map((item) => (
          <div key={`x-${item.key}`} data-measure="session" className="flex shrink-0">
            {item.render({ short: false, measure: true })}
          </div>
        ))}
        <div data-measure="overflow" className="flex shrink-0">
          <StripOverflowButton count={99} tabIndex={-1} />
        </div>
        <span data-measure="divider" className="h-4 w-px shrink-0" />
      </div>
      {/* The "+" is a MENU trigger, not a tab: it sits beside the tablist. */}
      {add && <span className="flex shrink-0 items-center">{add}</span>}
      <div role="tablist" className="flex min-w-0 flex-1 items-center gap-1">
      <Button
        type="button"
        variant="ghost"
        role="tab"
        aria-selected={conversation.active}
        data-testid="thread-tab-conversation"
        // EXP-1334: phones = an ICON-ONLY tab in the strip chip box (the
        // middle zones keep their room); the label names it.
        aria-label={pager ? conversation.label : undefined}
        title={pager ? conversation.label : undefined}
        className={cn(tabClass(conversation.active, pager), pager && `px-2`)}
        onClick={conversation.onSelect}
      >
        <ConversationIcon className="size-3.5 shrink-0" />
        {!pager && conversation.label}
      </Button>
      {(issues.length > 0 || sessions.length > 0) && !pager && divider}
      <div
        ref={middleRef}
        data-testid="thread-tabs-zones"
        className="flex min-w-0 flex-1 items-center gap-1 overflow-hidden"
      >
        {issues.map((item, index) =>
          shownIssues.has(index) ? (
            <Fragment key={item.key}>
              {item.render({ short: pager || short, measure: false })}
            </Fragment>
          ) : null
        )}
        {more(hiddenIssues, THREAD_STRIP_MORE_COPY.issues(hiddenIssues.length), `thread-tabs-more-issues`)}
        {both && <span aria-hidden className="h-4 w-px shrink-0 bg-border" />}
        {sessions.map((item, index) =>
          shownSessions.has(index) ? (
            <Fragment key={item.key}>{item.render({ short: false, measure: false })}</Fragment>
          ) : null
        )}
        {more(
          hiddenSessions,
          THREAD_STRIP_MORE_COPY.sessions(hiddenSessions.length),
          `thread-tabs-more-sessions`
        )}
      </div>
      {!pager && divider}
      <Button
        type="button"
        variant="ghost"
        role="tab"
        aria-selected={guide.active}
        data-testid="thread-tab-guide"
        aria-label={pager ? guide.label : undefined}
        title={pager ? guide.label : undefined}
        className={cn(tabClass(guide.active, pager), pager && `px-2`)}
        onClick={guide.onSelect}
      >
        {pager ? (
          // Phones: the icon alone, the dot on its corner.
          <span className="relative flex shrink-0">
            <GuideIcon className="size-3.5 shrink-0" />
            {guide.dot && (
              <span
                data-testid="thread-guide-dot"
                className="absolute -top-0.5 -right-0.5 size-1.5 rounded-full bg-emerald-400"
              >
                <span className="sr-only">The Guide has content</span>
              </span>
            )}
          </span>
        ) : (
          <>
            <GuideIcon className="size-3.5 shrink-0" />
            {guide.label}
            {guide.dot && (
              <span
                data-testid="thread-guide-dot"
                className="size-1.5 shrink-0 rounded-full bg-emerald-400"
              >
                <span className="sr-only">The Guide has content</span>
              </span>
            )}
          </>
        )}
      </Button>
      </div>
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

/** EXP-1316/1334: ONE session chip of the strip ×4 = THE strip chip: the
 *  agent's mark (its state) and the name; `path` = the open nested session
 *  (`<chip> › <open one>`); `chevron` = it opens its subtree (a click). */
export function ThreadSessionTab({
  agent,
  markState,
  name,
  active,
  path,
  chevron = false,
  onSelect,
  ref,
  ...button
}: {
  agent: string | null
  markState: SessionTabMarkState
  name: string
  active: boolean
  path?: string | null
  chevron?: boolean
  onSelect?: () => void
  ref?: Ref<HTMLButtonElement>
} & Omit<ComponentProps<`button`>, `ref` | `children` | `onSelect`>) {
  return (
    <StripChip
      ref={ref}
      // A folded chip opens a MENU (its subtree): a plain button, its state
      // on `data-active`; only a chip that opens itself is a tab.
      role={chevron ? undefined : `tab`}
      aria-selected={chevron ? undefined : active}
      data-testid="thread-tab-session"
      active={active}
      lead={
        <AgentRunMark
          agent={agent}
          state={sessionTabRunMark(markState)}
          ringClassName="ring-background"
          className="size-3.5"
        />
      }
      label={name}
      path={path}
      chevron={chevron}
      tooltip={path ? `${name} › ${path}` : name}
      onClick={onSelect}
      {...button}
    />
  )
}

/** EXP-1334: ONE issue chip of the strip = THE strip chip: status glyph ·
 *  mono identifier · title (`short` = the identifier alone). */
export function ThreadIssueTab({
  identifier,
  title,
  status,
  active,
  short = false,
  path,
  chevron = false,
  onSelect,
  boxProps,
  ref,
  ...button
}: {
  identifier: string
  title: string
  status: StatusGlyphProps
  active: boolean
  short?: boolean
  /** The open nested issue's identifier (`EXP-1 › EXP-4`). */
  path?: string | null
  chevron?: boolean
  onSelect?: () => void
  /** The box's props (THE issue context menu's `menuProps`). */
  boxProps?: ComponentProps<typeof StripChip>[`boxProps`]
  ref?: Ref<HTMLButtonElement>
} & Omit<ComponentProps<`button`>, `ref` | `children` | `onSelect` | `title`>) {
  return (
    <StripChip
      ref={ref}
      // A folded chip opens a MENU (its subtree): a plain button, its state
      // on `data-active`; only a chip that opens itself is a tab.
      role={chevron ? undefined : `tab`}
      aria-selected={chevron ? undefined : active}
      aria-label={path ? `${identifier} ${title} › ${path}` : `${identifier} ${title}`}
      active={active}
      lead={<StatusGlyph {...status} className={cn(`size-3.5 shrink-0`, status.className)} />}
      identifier={identifier}
      label={short ? null : title}
      path={path}
      chevron={chevron}
      tooltip={`${identifier} · ${title}`}
      boxProps={boxProps}
      onClick={onSelect}
      {...button}
    />
  )
}

/** EXP-1334: a folded chip's dropdown (a CLICK, every platform; phones:
 *  the bottom sheet): THE Menu around the chip, its body the subtree rows
 *  (`ThreadSessionTree` / `ThreadIssueTree`), row 1 = the chip itself. A
 *  pick closes it. */
export function ThreadChipSubtree({
  trigger,
  title,
  testId,
  children,
}: {
  /** The chip (ONE element, its ref forwarded to the button). */
  trigger: ReactElement
  /** The sheet's headline / the menu's accessible name. */
  title: string
  testId?: string
  children: (close: () => void) => ReactNode
}) {
  const [open, setOpen] = useState(false)
  return (
    <Menu
      open={open}
      onOpenChange={setOpen}
      align="start"
      title={title}
      aria-label={title}
      data-testid={testId}
      contentClassName="w-80 p-1"
      trigger={trigger}
      body={children(() => setOpen(false))}
    />
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

/** A tree row inside THE Menu: the menu's own item (keyboard, highlight,
 *  close on pick) around the row, which keeps its geometry. */
function TreeMenuRow({
  menu,
  onSelect,
  testId,
  children,
}: {
  menu: boolean
  onSelect: () => void
  testId: string
  children: ReactElement
}) {
  if (!menu) return children
  return (
    <MenuRow
      onSelect={onSelect}
      data-testid={testId}
      className="h-auto p-0 [&>*]:w-full [&>*]:bg-transparent"
    >
      {children}
    </MenuRow>
  )
}

/** EXP-1316/1334: a session subtree as THE session rows (`SessionRow
 *  size="small"` + the tree guides, the Threads list's nesting), the open
 *  one washed. `menu` = inside THE Menu (a folded chip's dropdown). */
export function ThreadSessionTree({
  rows,
  activeId,
  onOpen,
  menu = false,
}: {
  rows: readonly ThreadTreeRow[]
  activeId: string | null
  onOpen: (sessionId: string) => void
  menu?: boolean
}) {
  const ordered = threadTreeRows(rows)
  const guides = treeGuides(ordered.map((entry) => entry.depth))
  return (
    <div data-testid="thread-session-tree" className="flex flex-col">
      {ordered.map(({ row, depth }, index) => (
        <TreeMenuRow
          key={row.sessionId}
          menu={menu}
          testId="thread-session-tree-item"
          onSelect={() => onOpen(row.sessionId)}
        >
          <SessionRow
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
            onClick={menu ? undefined : () => onOpen(row.sessionId)}
          />
        </TreeMenuRow>
      ))}
    </div>
  )
}

export interface ThreadIssueTreeRow {
  issueId: string
  identifier: string
  title: string
  status: StatusGlyphProps
  /** 0 = the chip's own issue. */
  depth: number
}

/** EXP-1334: an issue chip's subtree = the issue rows (status glyph, mono
 *  identifier, title) with THE tree guides, the open one washed. */
export function ThreadIssueTree({
  rows,
  activeId,
  onOpen,
  menu = false,
}: {
  rows: readonly ThreadIssueTreeRow[]
  activeId: string | null
  onOpen: (issueId: string) => void
  menu?: boolean
}) {
  const guides = treeGuides(rows.map((row) => row.depth))
  return (
    <div data-testid="thread-issue-tree" className="flex flex-col">
      {rows.map((row, index) => (
        <TreeMenuRow
          key={row.issueId}
          menu={menu}
          testId="thread-issue-tree-item"
          onSelect={() => onOpen(row.issueId)}
        >
          <ListRow
            interactive
            active={row.issueId === activeId}
            data-testid="thread-issue-tree-row"
            onClick={menu ? undefined : () => onOpen(row.issueId)}
            className="relative h-8 gap-2 py-0 pr-3"
            style={{ paddingLeft: `${TREE_BASE + row.depth * TREE_INDENT}px` }}
          >
            <TreeGuides guide={guides[index]} />
            <StatusGlyph
              {...row.status}
              className={cn(`size-3.5 shrink-0`, row.status.className)}
            />
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {row.identifier}
            </span>
            <span className="min-w-0 truncate text-sm">{row.title}</span>
          </ListRow>
        </TreeMenuRow>
      ))}
    </div>
  )
}

/** The strip's "+" (the host wraps it in THE Menu). */
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
  /** A question card's answer: the picks / the typed free text + the
   *  surface's data (absent = read-only). */
  onAnswer?: (rowId: string, input: ThreadQuestionAnswer) => void
  /** A plan card's decision + the reject's note (absent = read-only). */
  onDecide?: (rowId: string, input: ThreadPlanDecision) => void
  /** The rows whose answer is in flight. */
  pending?: ReadonlySet<string>
  /** The waiting cards THE ask dock holds (`threadPendingAsks`): they
   *  leave the Conversation and take the composer's slot. */
  docked?: ReadonlySet<string>
  /** The viewer: their OWN queued rows carry the Remove from queue X. */
  viewerId?: string | null
  /** EXP-1317: revoke a queued row (absent = no X). */
  onUnqueue?: (rowId: string) => void
  /** EXP-1323: the suggested tasks cards' actions (absent = read-only). */
  suggestions?: ThreadSuggestionActions
  /** EXP-1323: a filed suggestion's issue (absent = its identifier). */
  renderIssueRef?: (issueId: string, identifier: string | null) => ReactNode
  /** EXP-1323: the live tail after the last row (Tasks card + status
   *  line), the host's. */
  tail?: ReactNode
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
  docked,
  viewerId,
  onUnqueue,
  suggestions,
  renderIssueRef,
  tail,
  className,
}: ThreadConversationProps) {
  return (
    <div data-testid="thread-conversation" className={cn(`flex flex-col gap-4`, className)}>
      {rows.map((row) => {
        if (docked?.has(row.id)) return null
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
                attachmentSrc={attachmentSrc}
                busy={pending?.has(row.id) ?? false}
                onAnswer={onAnswer ? (input) => onAnswer(row.id, input) : undefined}
              />
            )
          case `plan`:
            return (
              <ThreadPlanCard
                key={row.id}
                row={row}
                renderText={renderText}
                attachmentSrc={attachmentSrc}
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
          case `report`:
            return (
              <ThreadReportLine
                key={row.id}
                text={row.text}
                onOpen={onOpenSession ? () => onOpenSession(row.sessionId) : undefined}
              />
            )
          case `suggestions`:
            return (
              <ThreadSuggestionsCard
                key={row.id}
                row={row}
                actions={suggestions}
                onOpenSession={onOpenSession}
                renderIssueRef={renderIssueRef}
              />
            )
          default:
            return null
        }
      })}
      {tail}
    </div>
  )
}

/** EXP-1323: a child's report = ONE event line `Report from <session> ›`
 *  opening that session; its summary is never repeated (the parent's next
 *  message relays it). */
export function ThreadReportLine({ text, onOpen }: { text: string; onOpen?: () => void }) {
  const content = (
    <>
      <ReportIcon className="size-3.5 shrink-0" />
      <span className="min-w-0 truncate">{text}</span>
      {onOpen && <ChevronRightIcon className="size-3.5 shrink-0" />}
    </>
  )
  const className = `flex min-w-0 max-w-full items-center gap-1.5 self-start text-xs text-muted-foreground`
  if (!onOpen) {
    return (
      <div data-slot="thread-report" className={className}>
        {content}
      </div>
    )
  }
  return (
    <button
      type="button"
      data-slot="thread-report"
      className={cn(className, `text-left hover:text-foreground`)}
      onClick={onOpen}
    >
      {content}
    </button>
  )
}

/** EXP-1323: what a suggested tasks card can do (`sessions.startSuggestion`
 *  / `startAllSuggestions` / `dismissSuggestion` / `fileSuggestion`). */
export interface ThreadSuggestionActions {
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
 *  open), then hairline-divided items. Open = title, text, × · Options ·
 *  Do it here · Start (phones stack the buttons under the text); started =
 *  the child's session card (its caption under the name) opening it; here =
 *  `Sent here`; filed = `Filed EXP-9`. */
export function ThreadSuggestionsCard({
  row,
  actions,
  onOpenSession,
  renderIssueRef,
}: {
  row: Extract<ThreadRow, { type: `suggestions` }>
  actions?: ThreadSuggestionActions
  onOpenSession?: (sessionId: string) => void
  renderIssueRef?: (issueId: string, identifier: string | null) => ReactNode
}) {
  const pending = actions?.pending
  return (
    <div
      data-slot="thread-suggestions"
      className="min-w-0 overflow-clip rounded-lg border border-glass-stroke-card bg-glass-row text-xs"
    >
      <div className="flex min-h-9 min-w-0 items-center gap-2 px-3 py-1.5">
        <SuggestionsIcon className="size-3.5 shrink-0 text-muted-foreground" />
        <span data-slot="thread-suggestions-header" className="min-w-0 truncate font-medium">
          {row.header}
        </span>
        {row.startAll && actions && (
          <Button
            type="button"
            size="xs"
            className="ml-auto shrink-0"
            data-testid="thread-suggestions-start-all"
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
  actions?: ThreadSuggestionActions
  onOpenSession?: (sessionId: string) => void
  renderIssueRef?: (issueId: string, identifier: string | null) => ReactNode
}) {
  if (item.state === `started`) {
    if (item.card) {
      const card = item.card
      return (
        <div data-slot="thread-suggestion" data-state={item.state}>
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
        data-slot="thread-suggestion"
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
        data-slot="thread-suggestion"
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
          label: SUGGESTIONS_COPY.file,
          onSelect: () => actions.onFile(rowId, item.id),
        },
        {
          kind: `item`,
          label: SUGGESTIONS_COPY.dismiss,
          onSelect: () => actions.onDismiss(rowId, item.id),
        },
      ]
    : []
  return (
    <div
      data-slot="thread-suggestion"
      data-state={item.state}
      className="flex min-w-0 flex-col gap-2 px-3 py-2.5 md:flex-row md:items-start md:gap-3"
    >
      <div className="flex min-w-0 flex-1 items-start gap-2">
        <div className="min-w-0 flex-1">
          <div className="truncate font-medium text-foreground">{item.title}</div>
          {item.text && <div className="mt-0.5 text-muted-foreground">{item.text}</div>}
        </div>
        {actions && (
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={SUGGESTIONS_COPY.dismiss}
            title={SUGGESTIONS_COPY.dismiss}
            data-testid="thread-suggestion-dismiss"
            className="shrink-0 text-muted-foreground hover:text-foreground"
            disabled={busy}
            onClick={() => actions.onDismiss(rowId, item.id)}
          >
            <CloseIcon className="size-3" />
          </Button>
        )}
      </div>
      {actions && (
        <div
          data-slot="thread-suggestion-actions"
          className="flex shrink-0 flex-col gap-1.5 md:flex-row md:items-center"
        >
          <Menu
            entries={entries}
            align="end"
            title={item.title}
            aria-label={SUGGESTIONS_COPY.options}
            contentClassName="w-56"
            trigger={
              <Button
                type="button"
                variant="outline"
                size="xs"
                data-testid="thread-suggestion-options"
                disabled={busy}
              >
                {SUGGESTIONS_COPY.options}
                <ChevronDownIcon />
              </Button>
            }
          />
          <Button
            type="button"
            variant="outline"
            size="xs"
            data-testid="thread-suggestion-here"
            disabled={busy}
            onClick={() => actions.onHere(rowId, item.id)}
          >
            {SUGGESTIONS_COPY.here}
          </Button>
          <Button
            type="button"
            size="xs"
            data-testid="thread-suggestion-start"
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

/** EXP-1280: a question card's answer: the labels (`answers`; `answer` =
 *  them joined, the compat string) + the inline surface's data model. */
export type ThreadQuestionAnswer = {
  answer: string
  answers?: string[]
  surfaceData?: Record<string, unknown>
}

/** The big card's body: capped and scrolling inside THE ask dock. */
function DockBody({ docked, children }: { docked: boolean; children: ReactNode }) {
  return docked ? <AskDockBody>{children}</AskDockBody> : <>{children}</>
}

/** A closed card's free answers (not an option) as THE answered line. */
function freeAnswers(options: readonly { label: string }[], picks: readonly string[]): string[] {
  return picks.filter((pick) => !options.some((option) => option.label === pick))
}

/** EXP-1316/1280: a question row = THE question card ×4 (plan-card.json):
 *  the question, the inline surface, the options (THE option rows or the
 *  picture tiles, multi aware), the `custom` free-text row and its Answer
 *  button; closed once answered (the picks highlighted, the rest inert). */
function ThreadQuestionCard({
  row,
  renderText,
  attachmentSrc,
  busy,
  onAnswer,
  docked = false,
  onBack,
}: {
  row: Extract<ThreadRow, { type: `question` }>
  renderText: (text: string) => ReactNode
  attachmentSrc: (attachmentId: string) => string
  busy: boolean
  onAnswer?: (input: ThreadQuestionAnswer) => void
  /** In THE ask dock: the body scrolls inside its cap. */
  docked?: boolean
  onBack?: () => void
}) {
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
  return (
    <AskCard
      onBack={onBack}
      data-slot="thread-question"
      data-testid={PLAN_CARD_TEST_IDS.card}
      data-kind="question"
      data-answered={row.answered || undefined}
    >
      <DockBody docked={docked}>
        <div className="text-sm text-foreground/90">{renderText(stripPendingUploads(row.question))}</div>
        {row.surface && (
          <PlanCardSurface
            surface={row.surface}
            disabled={!interactive}
            dataRef={surfaceData}
            testKey={row.id}
          />
        )}
        {row.multi && !row.answered && row.options.length > 0 && (
          <div className="mt-2">
            <PlanCardCaption>{PLAN_CARD_COPY.chooseAny}</PlanCardCaption>
          </div>
        )}
        <PlanCardOptions
          options={row.options}
          multi={row.multi}
          picked={shown}
          onPick={interactive ? setPicked : undefined}
          attachmentSrc={attachmentSrc}
          label={row.question}
        />
        {interactive && row.custom && (
          <PlanCardCustomField value={customText} onChange={setCustomText} label={row.question} />
        )}
      </DockBody>
      {row.answered ? (
        free.length > 0 || shown.length === 0 ? (
          <AskAnsweredLine answer={free.length > 0 ? joinAnswers(free) : row.answer} />
        ) : null
      ) : busy ? (
        <AskPrompt options={[]} answerable={false} locked={{ labels }} onAnswer={noop} />
      ) : onAnswer ? (
        <Button
          type="button"
          size="sm"
          className="mt-2"
          data-testid="thread-question-answer"
          disabled={issue !== null}
          onClick={send}
        >
          {PLAN_CARD_COPY.answer}
        </Button>
      ) : null}
    </AskCard>
  )
}

const noop = () => {}

/** A plan card's decision (`sessions.answer`): `fresh` = start the work in
 *  a fresh context; `feedback` = the note; EXP-1280: `answers` by question
 *  id + the inline surface's data model. */
export type ThreadPlanDecision = {
  decision: `approved` | `rejected`
  fresh?: boolean
  feedback?: string
  answers?: Record<string, string[]>
  surfaceData?: Record<string, unknown>
}

/** EXP-1316/1280: a plan row = THE plan card ×4 (plan-card.json): the
 *  markdown (pictures inline, a pending upload renders nothing), the inline
 *  surface, one block per question, then THE plan options (`PLAN_OPTIONS`:
 *  Yes / Yes, and start with a fresh context / No, keep planning) + the
 *  feedback field. Approving waits on every question (`answerIssue`);
 *  rejecting never does. Closed once decided: the picks highlighted, the
 *  decision label + the feedback. */
function ThreadPlanCard({
  row,
  renderText,
  attachmentSrc,
  busy,
  onDecide,
  docked = false,
  onBack,
}: {
  row: Extract<ThreadRow, { type: `plan` }>
  renderText: (text: string) => ReactNode
  attachmentSrc: (attachmentId: string) => string
  busy: boolean
  onDecide?: (input: ThreadPlanDecision) => void
  docked?: boolean
  onBack?: () => void
}) {
  const decided = row.decision === `approved` || row.decision === `rejected`
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
  const options = useMemo<AskOption[]>(
    () =>
      PLAN_OPTIONS.map((option) => ({
        key: option.key,
        label: option.label,
        ...(option.description ? { description: option.description } : {}),
        ...(option.decision === `approved` && approveIssue ? { disabled: true } : {}),
      })),
    [approveIssue]
  )
  const decide = (key: string) => {
    const option = PLAN_OPTIONS.find((o) => o.key === key)
    if (!option || !interactive) return
    if (option.decision === `approved` && approveIssue) return
    const note = feedback.trim()
    onDecide?.({
      decision: option.decision === `rejected` ? `rejected` : `approved`,
      ...(option.decision === `approved` && option.fresh ? { fresh: true } : {}),
      ...(note ? { feedback: note } : {}),
      ...(Object.keys(answers).length > 0 ? { answers } : {}),
      ...(row.surface && surfaceData.current ? { surfaceData: surfaceData.current } : {}),
    })
  }
  return (
    <AskCard
      plan
      label={ASK_CARD_COPY.planReady}
      onBack={onBack}
      data-slot="thread-plan"
      data-testid={PLAN_CARD_TEST_IDS.card}
      data-kind="plan"
      data-decision={row.decision ?? undefined}
    >
      <DockBody docked={docked}>
        <div className="text-sm">{renderText(stripPendingUploads(row.plan))}</div>
        {row.surface && (
          <PlanCardSurface
            surface={row.surface}
            disabled={!interactive}
            dataRef={surfaceData}
            testKey={row.id}
          />
        )}
        {row.questions.map((question) => {
          const picked = decided ? (row.answers?.[question.id] ?? []) : (picks[question.id] ?? [])
          const free = decided ? freeAnswers(question.options, picked) : []
          return (
            <PlanCardQuestionBlock
              key={question.id}
              question={question}
              picked={picked}
              customText={custom[question.id] ?? ``}
              onPick={
                interactive ? (next) => setPicks((prev) => ({ ...prev, [question.id]: next })) : undefined
              }
              onCustom={(text) => setCustom((prev) => ({ ...prev, [question.id]: text }))}
              attachmentSrc={attachmentSrc}
              closedFree={free.length > 0 ? <AskAnsweredLine answer={joinAnswers(free)} /> : undefined}
            />
          )
        })}
      </DockBody>
      <AskPrompt
        variant="plan"
        options={options}
        answerable={!decided && onDecide !== undefined}
        locked={busy ? { labels: [] } : null}
        resolved={
          decided
            ? { answer: planDecisionLabel(row.decision as `approved` | `rejected`, row.fresh) }
            : null
        }
        onAnswer={(keys) => {
          if (keys[0]) decide(keys[0])
        }}
      >
        {approveIssue && (
          <div data-testid={PLAN_CARD_TEST_IDS.issue} className="mt-1.5 text-[0.6875rem] text-yellow-400">
            {PLAN_CARD_COPY.answerIssues[approveIssue]}
          </div>
        )}
        <Composer className="mt-2">
          <Textarea
            value={feedback}
            onChange={(event) => setFeedback(event.target.value)}
            placeholder={ASK_CARD_COPY.planFeedbackPlaceholder}
            aria-label={PLAN_CARD_COPY.feedback}
            rows={1}
            data-testid={PLAN_CARD_TEST_IDS.feedback}
            className="max-h-32 min-h-9 border-none bg-transparent px-3 pb-1 pt-3 text-sm shadow-none focus-visible:border-transparent"
          />
        </Composer>
      </AskPrompt>
      {decided && row.feedback && (
        <div data-testid={PLAN_CARD_TEST_IDS.feedback} className="mt-1 text-xs text-muted-foreground">
          {`${PLAN_CARD_COPY.feedback}: ${row.feedback}`}
        </div>
      )}
    </AskCard>
  )
}


const defaultAttachmentSrc = (attachmentId: string) => `/api/attachments/${attachmentId}`

/** THE ask dock in a thread (fixtures/ask-dock.json): the tree's waiting
 *  cards (`threadPendingAsks`) in the composer's slot: one = the big card,
 *  several = the stack, a tap focuses one (the back arrow returns). */
export function ThreadAskDock({
  rows,
  sessionName,
  renderText,
  attachmentSrc = defaultAttachmentSrc,
  pending,
  onAnswer,
  onDecide,
  initialFocusedId = null,
  className,
}: {
  rows: readonly ThreadRow[]
  sessionName: (sessionId: string) => string | null
  renderText: (text: string) => ReactNode
  /** EXP-1280: a picture's url (the cards' tiles); default the app's own. */
  attachmentSrc?: (attachmentId: string) => string
  pending?: ReadonlySet<string>
  onAnswer: (rowId: string, input: ThreadQuestionAnswer) => void
  onDecide: (rowId: string, input: ThreadPlanDecision) => void
  /** The card focused on mount (the styleguide's focused state). */
  initialFocusedId?: string | null
  className?: string
}) {
  const [focusedId, setFocusedId] = useState<string | null>(initialFocusedId)
  const view = askDock({ pending: threadPendingAsks(rows, sessionName), focusedId })
  return (
    <AskDock
      view={view}
      className={className}
      onFocus={setFocusedId}
      onBack={() => setFocusedId(null)}
      renderCard={(id, chrome) => {
        const row = rows.find((r) => r.id === id)
        if (row?.type === `plan`) {
          return (
            <ThreadPlanCard
              row={row}
              renderText={renderText}
              attachmentSrc={attachmentSrc}
              busy={pending?.has(row.id) ?? false}
              docked
              onBack={chrome.onBack}
              onDecide={(input) => onDecide(row.id, input)}
            />
          )
        }
        if (row?.type === `question`) {
          return (
            <ThreadQuestionCard
              row={row}
              renderText={renderText}
              attachmentSrc={attachmentSrc}
              busy={pending?.has(row.id) ?? false}
              docked
              onBack={chrome.onBack}
              onAnswer={(input) => onAnswer(row.id, input)}
            />
          )
        }
        return null
      }}
    />
  )
}

export function ThreadsListItem({
  lead,
  title,
  word,
  wordIcon,
  device = null,
  unread = false,
  caption = null,
  active = false,
  onOpen,
  testId = `threads-row`,
  className,
}: {
  /** The agent mark / state glyph (spinner busy, amber dot needs you). */
  lead: ReactNode
  /** The thread title (it names the issue, EXP-1316: no identifier slot). */
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
          <span data-slot="threads-row-caption" className="truncate text-xs text-muted-foreground">
            {caption}
          </span>
        )}
      </span>
      {word && (
        <span
          data-slot="threads-row-word"
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
