import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  type TouchEvent,
} from "react"
import { Dialog as DialogPrimitive } from "radix-ui"
import { DIALOG_CLASSES } from "@exponential_app/ui/web/primitives"
import { CHATS_SHELL } from "@exp/domain-contract/chats"
import { joinAnswers, PLAN_CARD_COPY, type CardSurface } from "@exp/domain-contract/plan-card"
import { PLAN_OPTIONS } from "@exp/domain-contract/ask-dock"
import { cardSurface, questionOptions, type ThreadQuestionOption } from "@exp/domain-contract/session-tree"
import {
  STORIES_COPY,
  STORIES_TEST_IDS,
  STORY_PROGRESS,
  STORY_RING,
  STORY_VIEWER,
  type Story,
  type StoryRing as StoryRingModel,
} from "@exp/domain-contract/stories"
import { ASK_DOCK_COPY } from "@exp/domain-contract/ask-dock"
import { AgentBrandMark } from "./agent-brand-mark"
import { BoardGlyph } from "./board-glyph"
import { Button } from "./button"
import { cn } from "./cn"
import { Composer } from "./composer"
import { Input } from "./input"
import { conceptIcon } from "./icons.generated"
import { PlanCardCustomField, PlanCardOptions, PlanCardSurface, questionAnswer, useSurfaceDataRef } from "./plan-card"
import { SurfaceCard } from "./surface-card"
import { Textarea } from "./textarea"

// S-3 STORIES ×web (fixtures/stories.json; desktop crates/ui stories, iOS
// ExpUI Stories, Android ui/stories): a session's own show cards (a plan, a
// question, a result) as story rings and a story viewer. The model is the
// contract's `stories()` fold; these only paint it.
//
//   StoryRing        the avatar inside a ring split into one arc per story
//                    (`ring.gap` degrees apart, `ring.width` px): waiting =
//                    the plan/question tint, unseen = foreground, seen = the
//                    glass-stroke hairline
//   StoryRingButton  a ring with its title under it (the chats header's row)
//   StoryRingRow     the rings side by side, scrolling sideways
//   StoryProgress    one segment per story: done = foreground, the current
//                    = its fill, todo = the glass-stroke track
//   StoryViewerPanel the viewer's frame (progress, header, body, footer, tap
//                    thirds, swipes), fully controlled; the styleguide paints
//                    it statically
//   StoryViewer      the stateful viewer: phones full screen, md+ a centred
//                    modal (`viewer`) over a scrim with side arrows; tap
//                    thirds / swipe / ←→ / Esc; only a result advances on its
//                    own (`autoAdvanceMs`), a long press pauses it

const CloseIcon = conceptIcon(`ui-close`)
const PrevIcon = conceptIcon(`ui-chevron-left`)
const NextIcon = conceptIcon(`ui-chevron-right`)

/** A ring's avatar: an agent ring = the agent's mark (its icon + colour,
 *  the chats bubble's), a root ring = the session's brand mark (its CLI, the
 *  chat row's), both on the muted disc. */
export function StoryAvatar({
  agent,
  cli = null,
  className,
}: {
  agent?: { icon?: string | null; color?: string | null } | null
  cli?: string | null
  className?: string
}) {
  return (
    <span
      data-slot="story-avatar"
      className={cn(`flex size-full items-center justify-center rounded-full bg-muted`, className)}
    >
      {agent ? (
        <BoardGlyph board={agent} className="size-[45%]" />
      ) : (
        <AgentBrandMark agent={cli} className="size-[45%]" />
      )}
    </span>
  )
}

/** The stroke class of one arc (the ask-dock tints: plan primary, question
 *  yellow). */
export function storyArcClass(story: Pick<Story, `kind` | `waiting` | `seen`>): string {
  if (story.waiting) return story.kind === `question` ? `stroke-yellow-400` : `stroke-primary`
  return story.seen ? `stroke-glass-stroke` : `stroke-foreground`
}

/** The ring's arcs as [startDeg, sweepDeg] from 12 o'clock, clockwise. */
export function storyArcs(count: number, gap: number = STORY_RING.gap): [number, number][] {
  if (count <= 0) return []
  if (count === 1) return [[0, 360]]
  const slot = 360 / count
  const sweep = Math.max(0, slot - gap)
  return Array.from({ length: count }, (_, index) => [index * slot + gap / 2, sweep])
}

/** THE story ring: `children` (the avatar) inside the segmented ring. */
export function StoryRing({
  stories,
  size = CHATS_SHELL.bubble,
  children,
  className,
}: {
  stories: readonly Pick<Story, `kind` | `waiting` | `seen`>[]
  /** The outer diameter (px). */
  size?: number
  children: ReactNode
  className?: string
}) {
  const width = STORY_RING.width
  const radius = (size - width) / 2
  const circumference = 2 * Math.PI * radius
  const arcs = storyArcs(stories.length)
  const state = stories.some((story) => story.waiting)
    ? `waiting`
    : stories.some((story) => !story.seen)
      ? `unseen`
      : `seen`
  return (
    <span
      data-slot="story-ring"
      data-state={state}
      data-count={stories.length}
      className={cn(`relative flex shrink-0 items-center justify-center rounded-full`, className)}
      style={{ width: size, height: size }}
    >
      <svg aria-hidden className="absolute inset-0 -rotate-90" width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        {arcs.map(([start, sweep], index) => {
          const story = stories[index]!
          const hairline = !story.waiting && story.seen
          const length = (circumference * sweep) / 360
          return (
            <circle
              key={index}
              data-slot="story-arc"
              cx={size / 2}
              cy={size / 2}
              r={radius}
              fill="none"
              strokeWidth={hairline ? 1 : width}
              strokeLinecap={arcs.length > 1 ? `round` : undefined}
              strokeDasharray={sweep >= 360 ? undefined : `${length} ${circumference}`}
              strokeDashoffset={sweep >= 360 ? undefined : -((circumference * start) / 360)}
              className={storyArcClass(story)}
            />
          )
        })}
      </svg>
      <span
        className="flex items-center justify-center overflow-hidden rounded-full"
        style={{ width: size - 4 * width, height: size - 4 * width }}
      >
        {children}
      </span>
    </span>
  )
}

/** One ring of the row: the ring (its avatar) with the title under it; a
 *  tap opens the viewer at it. */
export function StoryRingButton({
  ring,
  avatar,
  onOpen,
  size = CHATS_SHELL.bubble,
  className,
}: {
  ring: Pick<StoryRingModel, `key` | `title` | `stories` | `unseen`>
  avatar: ReactNode
  onOpen?: (key: string) => void
  size?: number
  className?: string
}) {
  return (
    <button
      type="button"
      data-testid={STORIES_TEST_IDS.ring}
      data-key={ring.key}
      aria-label={ring.title}
      onClick={onOpen ? () => onOpen(ring.key) : undefined}
      className={cn(
        `flex shrink-0 flex-col items-center gap-1.5 rounded-lg outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50`,
        className
      )}
      style={{ width: size + 6 }}
    >
      <StoryRing stories={ring.stories} size={size}>
        {avatar}
      </StoryRing>
      <span
        className={cn(
          `w-full truncate text-center text-xs`,
          ring.unseen ? `font-medium text-foreground` : `text-muted-foreground`
        )}
      >
        {ring.title}
      </span>
    </button>
  )
}

/** The rings side by side (scrolling sideways); none = nothing. */
export function StoryRingRow({
  rings,
  avatarFor,
  onOpen,
  className,
}: {
  rings: readonly StoryRingModel[]
  avatarFor: (ring: StoryRingModel) => ReactNode
  onOpen?: (key: string) => void
  className?: string
}) {
  if (rings.length === 0) return null
  return (
    <div
      data-slot="story-ring-row"
      className={cn(`-mx-4 flex gap-3 overflow-x-auto px-4 pb-1 [scrollbar-width:none]`, className)}
    >
      {rings.map((ring) => (
        <StoryRingButton key={ring.key} ring={ring} avatar={avatarFor(ring)} onOpen={onOpen} />
      ))}
    </div>
  )
}

/** THE progress: one segment per story, the current filled to `fraction`. */
export function StoryProgress({
  count,
  index,
  fraction,
  className,
}: {
  count: number
  index: number
  /** The current segment's fill (0…1). */
  fraction: number
  className?: string
}) {
  return (
    <div
      data-testid={STORIES_TEST_IDS.progress}
      data-index={index}
      className={cn(`flex w-full`, className)}
      style={{ gap: STORY_PROGRESS.gap }}
    >
      {Array.from({ length: count }, (_, at) => {
        const fill = at < index ? 1 : at === index ? Math.min(1, Math.max(0, fraction)) : 0
        return (
          <span
            key={at}
            data-slot="story-progress-segment"
            className="relative flex-1 overflow-hidden rounded-full bg-glass-stroke"
            style={{ height: STORY_PROGRESS.height }}
          >
            <span className="absolute inset-y-0 left-0 rounded-full bg-foreground" style={{ width: `${fill * 100}%` }} />
          </span>
        )
      })}
    </div>
  )
}

/** What the viewer reads off a story's raw card beyond the fold: the
 *  surface and a question's options. */
export interface StoryCardDetail {
  surface: CardSurface | null
  title: string | null
  options: ThreadQuestionOption[]
  multi: boolean
  custom: boolean
}

/** A raw `messages.card` → the viewer's detail (tolerant). */
export function storyCardDetail(card: unknown): StoryCardDetail {
  const c = card && typeof card === `object` ? (card as Record<string, unknown>) : {}
  return {
    surface: cardSurface(c.surface),
    title: typeof c.title === `string` && c.title.trim() ? c.title.trim() : null,
    options: c.kind === `question` ? questionOptions(c) : [],
    multi: c.multi === true,
    custom: c.custom !== false,
  }
}

/** A plan story's decision (THE plan options: Approve = approve, Change =
 *  reject + its feedback). */
export interface StoryDecision {
  decision: `approved` | `rejected`
  fresh?: boolean
  feedback?: string
  surfaceData?: Record<string, unknown>
}

/** A question story's answer (the in-chat card's shape). */
export interface StoryAnswer {
  answer: string
  answers?: string[]
  surfaceData?: Record<string, unknown>
}

/** The md+ modal frame (`viewer`: width × height max, radius); phones =
 *  the whole screen. */
export const STORY_VIEWER_FRAME_CLASS = `md:h-[min(var(--story-h),calc(100dvh-2rem))] md:w-(--story-w) md:max-w-[calc(100vw-8rem)] md:rounded-(--story-radius) md:border md:border-glass-stroke-card md:shadow-2xl md:shadow-black/40`
export const STORY_VIEWER_FRAME_STYLE = {
  [`--story-w`]: `${STORY_VIEWER.width}px`,
  [`--story-h`]: `${STORY_VIEWER.height}px`,
  [`--story-radius`]: `${STORY_VIEWER.radius}px`,
} as CSSProperties

const INTERACTIVE = `button, a, input, textarea, select, [role="button"], [role="slider"], [contenteditable="true"]`

/** The viewer's frame (fully controlled). */
export function StoryViewerPanel({
  ring,
  story,
  index,
  fraction,
  avatar,
  detail,
  renderText,
  attachmentSrc = (id) => `/api/attachments/${id}`,
  busy = false,
  onPrev,
  onNext,
  onClose,
  onOpenSession,
  onDecide,
  onAnswer,
  onPause,
  className,
  style,
}: {
  ring: Pick<StoryRingModel, `key` | `title` | `stories`>
  story: Story
  /** The story's place in its ring. */
  index: number
  /** The current progress segment's fill (0…1). */
  fraction: number
  avatar: ReactNode
  detail: StoryCardDetail
  renderText?: (text: string) => ReactNode
  attachmentSrc?: (attachmentId: string) => string
  /** The answer is on its way (the footer goes inert). */
  busy?: boolean
  onPrev?: () => void
  onNext?: () => void
  onClose?: () => void
  /** A tap on the avatar or name: open that session. */
  onOpenSession?: (sessionId: string) => void
  onDecide?: (decision: StoryDecision) => void
  onAnswer?: (answer: StoryAnswer) => void
  /** A long press holds the timer (true) / lets it go (false). */
  onPause?: (paused: boolean) => void
  className?: string
  style?: CSSProperties
}) {
  const press = useRef<{ x: number; y: number; at: number; held: boolean; timer: number | null } | null>(null)
  const [picture, setPicture] = useState(0)
  useEffect(() => setPicture(0), [story.id])

  const endPress = () => {
    const current = press.current
    if (current?.timer != null) window.clearTimeout(current.timer)
    if (current?.held) onPause?.(false)
  }
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest(INTERACTIVE)) return
    const timer = window.setTimeout(() => {
      if (press.current) press.current.held = true
      onPause?.(true)
    }, 250)
    press.current = { x: event.clientX, y: event.clientY, at: Date.now(), held: false, timer }
  }
  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    const current = press.current
    press.current = null
    if (!current) return
    if (current.timer != null) window.clearTimeout(current.timer)
    if (current.held) {
      onPause?.(false)
      return
    }
    const dx = event.clientX - current.x
    const dy = event.clientY - current.y
    if (Math.abs(dx) > 10 || Math.abs(dy) > 10) return
    const box = event.currentTarget.getBoundingClientRect()
    const at = (event.clientX - box.left) / Math.max(1, box.width)
    if (at < 1 / 3) onPrev?.()
    else if (at > 2 / 3) onNext?.()
  }
  const swipeFrom = useRef<{ x: number; y: number } | null>(null)
  const onTouchStart = (event: TouchEvent<HTMLDivElement>) => {
    const touch = event.touches[0]
    swipeFrom.current = touch ? { x: touch.clientX, y: touch.clientY } : null
  }
  const onTouchEnd = (event: TouchEvent<HTMLDivElement>) => {
    const from = swipeFrom.current
    swipeFrom.current = null
    const touch = event.changedTouches[0]
    if (!from || !touch) return
    const dx = touch.clientX - from.x
    const dy = touch.clientY - from.y
    if (Math.abs(dy) > Math.abs(dx)) {
      if (dy > 80) onClose?.()
      return
    }
    if (dx > 48) onPrev?.()
    else if (dx < -48) onNext?.()
  }

  const shown = story.pictures[Math.min(picture, story.pictures.length - 1)] ?? null
  const surface = story.surface ? detail.surface : null
  return (
    <div
      data-testid={STORIES_TEST_IDS.viewer}
      data-story={story.id}
      data-kind={story.kind}
      data-ring={ring.key}
      className={cn(`relative flex h-full w-full flex-col overflow-hidden bg-background text-foreground`, className)}
      style={style}
      onTouchStart={onTouchStart}
      onTouchEnd={onTouchEnd}
    >
      <div className="flex shrink-0 flex-col gap-3 px-3 pt-[max(0.75rem,env(safe-area-inset-top))] pb-2">
        <StoryProgress count={ring.stories.length} index={index} fraction={fraction} />
        <div data-testid={STORIES_TEST_IDS.header} className="flex min-w-0 items-center gap-2.5">
          <button
            type="button"
            className="flex min-w-0 flex-1 items-center gap-2.5 rounded-md text-left outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
            onClick={onOpenSession ? () => onOpenSession(story.sessionId) : undefined}
          >
            <span className="flex size-8 shrink-0 items-center justify-center overflow-hidden rounded-full">{avatar}</span>
            <span className="flex min-w-0 flex-col">
              <span className="truncate text-sm font-medium">{story.sessionName}</span>
              <span data-slot="story-meta" className="truncate text-xs text-muted-foreground">
                {story.meta}
              </span>
            </span>
          </button>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label={STORIES_COPY.close}
            className="shrink-0 text-muted-foreground"
            onClick={onClose}
          >
            <CloseIcon />
          </Button>
        </div>
      </div>
      <div
        data-slot="story-body"
        className="relative flex min-h-0 flex-1 cursor-pointer select-none flex-col gap-3 overflow-y-auto px-4 pb-4"
        onPointerDown={onPointerDown}
        onPointerUp={onPointerUp}
        onPointerCancel={endPress}
        onPointerLeave={endPress}
      >
        {story.title && (
          <div data-slot="story-title" className="truncate text-base font-semibold">
            {story.title}
          </div>
        )}
        {shown && (
          <figure data-slot="story-picture" className="flex min-h-0 flex-col items-center gap-2">
            <img
              key={shown.attachmentId}
              src={attachmentSrc(shown.attachmentId)}
              alt={shown.label}
              draggable={false}
              className="max-h-[50dvh] w-auto max-w-full rounded-xl border border-glass-stroke-card object-contain"
            />
            {story.pictures.length > 1 && (
              <div className="flex items-center gap-1.5" data-slot="story-picture-pager">
                {story.pictures.map((entry, at) => (
                  <button
                    key={entry.attachmentId}
                    type="button"
                    aria-label={entry.label}
                    aria-current={at === picture || undefined}
                    className={cn(`size-1.5 rounded-full`, at === picture ? `bg-foreground` : `bg-glass-stroke-strong`)}
                    onClick={() => setPicture(at)}
                  />
                ))}
              </div>
            )}
            {shown.caption && (
              <figcaption className="text-center text-xs text-muted-foreground">{shown.caption}</figcaption>
            )}
          </figure>
        )}
        {surface && story.kind === `result` && (
          <SurfaceCard surface={surface} testKey={`story-${story.id}`} />
        )}
        {story.text !== null && (
          <div data-slot="story-text" className="text-sm text-foreground/90">
            {renderText ? renderText(story.text) : <p className="whitespace-pre-wrap">{story.text}</p>}
          </div>
        )}
        {story.footer !== `none` && (
          <StoryAskBody key={story.id} story={story} surface={surface} detail={detail} attachmentSrc={attachmentSrc} busy={busy} onDecide={onDecide} onAnswer={onAnswer} />
        )}
      </div>
    </div>
  )
}

/** A plan's / question's surface + footer (the in-chat card's answer
 *  controls: THE option rows / tiles, THE free-text row, THE buttons). */
function StoryAskBody({
  story,
  surface,
  detail,
  attachmentSrc,
  busy,
  onDecide,
  onAnswer,
}: {
  story: Story
  surface: CardSurface | null
  detail: StoryCardDetail
  attachmentSrc: (attachmentId: string) => string
  busy: boolean
  onDecide?: (decision: StoryDecision) => void
  onAnswer?: (answer: StoryAnswer) => void
}) {
  const surfaceData = useSurfaceDataRef()
  const [picked, setPicked] = useState<string[]>([])
  const [customText, setCustomText] = useState(``)
  const [changing, setChanging] = useState(false)
  const [feedback, setFeedback] = useState(``)
  const withData = () => (surface && surfaceData.current ? { surfaceData: surfaceData.current } : {})
  const approve = PLAN_OPTIONS.find((option) => option.key === `approve`)
  const reject = PLAN_OPTIONS.find((option) => option.key === `reject`)
  const labels = questionAnswer(picked, customText, detail.multi)
  return (
    <>
      {surface && (
        <PlanCardSurface surface={surface} dataRef={surfaceData} disabled={busy} testKey={`story-${story.id}`} />
      )}
      <div data-slot="story-footer" data-footer={story.footer} className="mt-auto flex flex-col gap-2 pt-2">
        {story.footer === `plan` ? (
          <>
            {changing && (
              <Composer>
                <Textarea
                  value={feedback}
                  onChange={(event) => setFeedback(event.target.value)}
                  placeholder={ASK_DOCK_COPY.feedbackPlaceholder}
                  aria-label={STORIES_COPY.change}
                  rows={1}
                  autoFocus
                  className="max-h-32 border-none bg-transparent px-3 pb-1 pt-3 text-sm shadow-none focus-visible:border-transparent"
                />
              </Composer>
            )}
            <div className="flex items-center justify-end gap-2">
              {changing ? (
                <>
                  <Button type="button" variant="secondary" size="sm" onClick={() => setChanging(false)}>
                    {ASK_DOCK_COPY.cancel}
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    data-slot="story-decide"
                    data-decision="rejected"
                    disabled={busy || !onDecide || feedback.trim() === ``}
                    onClick={() =>
                      reject && onDecide?.({ decision: reject.decision, feedback: feedback.trim(), ...withData() })
                    }
                  >
                    {STORIES_COPY.change}
                  </Button>
                </>
              ) : (
                <>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    data-slot="story-change"
                    disabled={busy || !onDecide}
                    onClick={() => setChanging(true)}
                  >
                    {STORIES_COPY.change}
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    data-slot="story-decide"
                    data-decision="approved"
                    disabled={busy || !onDecide}
                    onClick={() =>
                      approve && onDecide?.({ decision: approve.decision, fresh: approve.fresh, ...withData() })
                    }
                  >
                    {STORIES_COPY.approve}
                  </Button>
                </>
              )}
            </div>
          </>
        ) : (
          <>
            {story.footer === `choices` && (
              <PlanCardOptions
                options={detail.options}
                multi={detail.multi}
                picked={picked}
                onPick={busy || !onAnswer ? undefined : setPicked}
                attachmentSrc={attachmentSrc}
                label={story.title ?? STORIES_COPY.question}
              />
            )}
            {story.footer === `reply` ? (
              <Input
                value={customText}
                onChange={(event) => setCustomText(event.target.value)}
                placeholder={STORIES_COPY.reply}
                aria-label={story.title ?? STORIES_COPY.question}
                data-slot="story-reply"
                className="h-9 text-sm"
              />
            ) : detail.custom ? (
              <PlanCardCustomField
                value={customText}
                onChange={setCustomText}
                label={story.title ?? STORIES_COPY.question}
              />
            ) : null}
            <div className="flex justify-end">
              <Button
                type="button"
                size="sm"
                data-slot="story-answer"
                disabled={busy || !onAnswer || labels.length === 0}
                onClick={() => onAnswer?.({ answer: joinAnswers(labels), answers: labels, ...withData() })}
              >
                {PLAN_CARD_COPY.answer}
              </Button>
            </div>
          </>
        )}
      </div>
    </>
  )
}

/** Where the viewer stands: a ring (by key) and a story (by id). */
export interface StoryPosition {
  ringKey: string
  storyId: string
}

/** The next / previous position over the rings (past the last ring =
 *  null = close; before the first story of the first ring = stay). */
export function stepStory(
  rings: readonly Pick<StoryRingModel, `key` | `stories`>[],
  at: StoryPosition,
  direction: 1 | -1
): StoryPosition | null | `stay` {
  const r = rings.findIndex((ring) => ring.key === at.ringKey)
  if (r < 0) return null
  const ring = rings[r]!
  const s = ring.stories.findIndex((story) => story.id === at.storyId)
  const next = s + direction
  if (s >= 0 && next >= 0 && next < ring.stories.length) return { ringKey: ring.key, storyId: ring.stories[next]!.id }
  if (direction === 1) {
    const following = rings[r + 1]
    const first = following?.stories[0]
    return following && first ? { ringKey: following.key, storyId: first.id } : null
  }
  const before = rings[r - 1]
  const last = before?.stories.at(-1)
  return before && last ? { ringKey: before.key, storyId: last.id } : `stay`
}

/** THE story viewer: open while `openRing` names a ring. */
export function StoryViewer({
  rings,
  openRing,
  onClose,
  avatarFor,
  cardFor,
  onShown,
  onOpenSession,
  onDecide,
  onAnswer,
  pending,
  renderText,
  attachmentSrc,
}: {
  rings: readonly StoryRingModel[]
  /** The ring the viewer opens at (null = closed). */
  openRing: string | null
  onClose: () => void
  avatarFor: (ring: StoryRingModel) => ReactNode
  /** The story's raw card (its surface and options). */
  cardFor: (story: Story) => unknown
  /** A story came on screen (mark it seen). */
  onShown?: (story: Story) => void
  onOpenSession?: (sessionId: string) => void
  onDecide?: (story: Story, decision: StoryDecision) => Promise<unknown> | void
  onAnswer?: (story: Story, answer: StoryAnswer) => Promise<unknown> | void
  pending?: ReadonlySet<string>
  renderText?: (text: string) => ReactNode
  attachmentSrc?: (attachmentId: string) => string
}) {
  // The rings as they stood when the viewer opened: an answered story
  // drops out of the live fold, the viewer still walks the opened order.
  const [snapshot, setSnapshot] = useState<readonly StoryRingModel[] | null>(null)
  const [position, setPosition] = useState<StoryPosition | null>(null)
  useEffect(() => {
    if (openRing === null) {
      setSnapshot(null)
      setPosition(null)
      return
    }
    const ring = rings.find((entry) => entry.key === openRing)
    const first = ring?.stories[0]
    if (!ring || !first) {
      onClose()
      return
    }
    setSnapshot(rings)
    setPosition({ ringKey: ring.key, storyId: first.id })
    // Opening only: later changes of `rings` refresh the stories below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openRing])

  // Live stories by id (an answer's state, a fresher meta), else the snapshot's.
  const live = useMemo(() => {
    const map = new Map<string, Story>()
    for (const ring of rings) for (const story of ring.stories) map.set(story.id, story)
    return map
  }, [rings])
  const view = useMemo(
    () =>
      (snapshot ?? []).map((ring) => ({
        ...ring,
        stories: ring.stories.map((story) => live.get(story.id) ?? story),
      })),
    [snapshot, live]
  )
  const ring = position ? view.find((entry) => entry.key === position.ringKey) : undefined
  const index = ring && position ? ring.stories.findIndex((story) => story.id === position.storyId) : -1
  const story = ring && index >= 0 ? ring.stories[index]! : null

  const go = useCallback(
    (direction: 1 | -1) => {
      if (!position) return
      const next = stepStory(view, position, direction)
      if (next === `stay`) return
      if (next === null) onClose()
      else setPosition(next)
    },
    [position, view, onClose]
  )

  // Seen when shown.
  const shownRef = useRef(onShown)
  shownRef.current = onShown
  const storyId = story?.id ?? null
  useEffect(() => {
    if (story) shownRef.current?.(story)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [storyId])

  // The result's timer (a decision never advances on its own).
  const [paused, setPaused] = useState(false)
  const [fraction, setFraction] = useState(0)
  const duration = story?.autoAdvanceMs ?? null
  useEffect(() => {
    setFraction(duration === null ? 1 : 0)
  }, [storyId, duration])
  const elapsed = useRef(0)
  useEffect(() => {
    elapsed.current = 0
  }, [storyId])
  useEffect(() => {
    if (duration === null || paused || storyId === null) return
    let frame = 0
    let last = performance.now()
    const tick = (now: number) => {
      if (typeof document === `undefined` || document.visibilityState !== `hidden`) elapsed.current += now - last
      last = now
      const value = elapsed.current / duration
      setFraction(Math.min(1, value))
      if (value >= 1) {
        go(1)
        return
      }
      frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [duration, paused, storyId, go])

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement
    if (target.closest(`input, textarea, [contenteditable="true"]`)) return
    if (event.key === `ArrowRight`) {
      event.preventDefault()
      go(1)
    } else if (event.key === `ArrowLeft`) {
      event.preventDefault()
      go(-1)
    }
  }

  // Answered = the next story; a failed answer stays put (the host toasts).
  const settle = async (work: Promise<unknown> | void) => {
    try {
      await work
    } catch {
      return
    }
    go(1)
  }

  const open = openRing !== null && ring !== undefined && story !== null
  return (
    <DialogPrimitive.Root open={open} onOpenChange={(next) => (next ? undefined : onClose())}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className={DIALOG_CLASSES.overlay} />
        <DialogPrimitive.Content
          aria-describedby={undefined}
          onKeyDown={onKeyDown}
          className="fixed inset-0 z-50 flex items-center justify-center outline-none"
          onClick={(event) => {
            // The scrim around the centred modal (md+) closes it.
            if (event.target === event.currentTarget) onClose()
          }}
        >
          <DialogPrimitive.Title className="sr-only">{ring?.title ?? ``}</DialogPrimitive.Title>
          {open && ring && story && (
            <div className="relative flex h-full w-full items-center justify-center md:h-auto md:w-auto">
              <Button
                type="button"
                variant="ghost"
                size="icon"
                data-testid={STORIES_TEST_IDS.previous}
                aria-label={STORIES_COPY.previous}
                className="absolute top-1/2 -left-14 hidden -translate-y-1/2 rounded-full bg-glass-row md:inline-flex"
                onClick={() => go(-1)}
              >
                <PrevIcon />
              </Button>
              <StoryViewerPanel
                ring={ring}
                story={story}
                index={index}
                fraction={fraction}
                avatar={avatarFor(ring)}
                detail={storyCardDetail(cardFor(story))}
                renderText={renderText}
                attachmentSrc={attachmentSrc}
                busy={pending?.has(story.id) ?? false}
                onPrev={() => go(-1)}
                onNext={() => go(1)}
                onClose={onClose}
                onOpenSession={
                  onOpenSession
                    ? (sessionId) => {
                        onClose()
                        onOpenSession(sessionId)
                      }
                    : undefined
                }
                onDecide={onDecide ? (decision) => void settle(onDecide(story, decision)) : undefined}
                onAnswer={onAnswer ? (answer) => void settle(onAnswer(story, answer)) : undefined}
                onPause={setPaused}
                className={STORY_VIEWER_FRAME_CLASS}
                style={STORY_VIEWER_FRAME_STYLE}
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                data-testid={STORIES_TEST_IDS.next}
                aria-label={STORIES_COPY.next}
                className="absolute top-1/2 -right-14 hidden -translate-y-1/2 rounded-full bg-glass-row md:inline-flex"
                onClick={() => go(1)}
              >
                <NextIcon />
              </Button>
            </div>
          )}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}
