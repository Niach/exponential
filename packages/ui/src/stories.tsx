import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  type ReactNode,
  type TouchEvent,
} from "react"
import { Dialog as DialogPrimitive } from "radix-ui"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID, type NestedNode } from "@exponential_app/eui"
import { ExponentialSurface, useSurface } from "@exponential_app/eui-react"
import { DIALOG_CLASSES } from "@exponential_app/eui/web/primitives"
import { CHATS_SHELL } from "@exp/domain-contract/chats"
import { joinAnswers, PLAN_CARD_COPY, type CardSurface } from "@exp/domain-contract/plan-card"
import { PLAN_PAGE_COPY, planCountsLine } from "@exp/domain-contract/plan-page"
import { cardSurface, questionOptions, type ThreadQuestionOption } from "@exp/domain-contract/session-tree"
import {
  STORIES_COPY,
  STORIES_TEST_IDS,
  STORY_BADGE,
  STORY_PROGRESS,
  STORY_RING,
  STORY_ROW,
  STORY_VIEWER,
  storyFrameIndex,
  storyFrames,
  storyViewerSize,
  type Story,
  type StoryFooter as StoryFooterKind,
  type StoryFrame,
  type StoryRing as StoryRingModel,
} from "@exp/domain-contract/stories"
import { AgentBrandMark } from "./agent-brand-mark"
import { BoardGlyph } from "./board-glyph"
import { Button } from "./button"
import { cn } from "./cn"
import { Input } from "./input"
import { conceptIcon } from "./icons.generated"
import { PictureCarousel } from "./picture-group"
import { Pill } from "./pill"
import { PlanCardCustomField, PlanCardOptions, planCardHost, questionAnswer, useSurfaceDataRef } from "./plan-card"

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
//   StoryProgress    one segment per FRAME (`progress.gap` within a story,
//                    `progress.storyGap` between stories): done =
//                    foreground, the current = its fill, todo = the
//                    glass-stroke track
//   StoryViewerPanel one frame of `storyFrames(ring)`: the body (picture
//                    fitted on black / THE surface bare / the plan text)
//                    fills it, progress + header + title + description float
//                    over the top scrim, the footer over the bottom one; tap
//                    thirds, swipes; fully controlled (the styleguide paints
//                    it statically)
//   StoryViewer      the stateful viewer stepping FRAMES across the rings:
//                    phones full screen, md+ a centred modal (`viewer`) over
//                    a scrim with side arrows outside it; every frame has its
//                    own timer (a result's `autoAdvanceMs`), a long press
//                    pauses it

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

/** The viewer header's ring (px): the avatar inside its StoryRing. */
const STORY_VIEWER_AVATAR = 36

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
        `flex shrink-0 flex-col items-center rounded-lg outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50`,
        className
      )}
      style={{ width: size + 6, gap: STORY_ROW.labelGap }}
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
      className={cn(`-mx-4 flex overflow-x-auto px-4 [scrollbar-width:none]`, className)}
      style={{ gap: STORY_ROW.gap, paddingBottom: STORY_ROW.padBottom }}
    >
      {rings.map((ring) => (
        <StoryRingButton key={ring.key} ring={ring} avatar={avatarFor(ring)} onOpen={onOpen} />
      ))}
    </div>
  )
}

/** THE progress (#EXP-1493): one segment per STORY, `progress.gap` apart,
 *  the current filled to `fraction` (a gallery: (picture + its fill) / n). */
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
    <div data-testid={STORIES_TEST_IDS.progress} data-index={index} className={cn(`flex w-full`, className)}>
      {Array.from({ length: count }, (_, at) => {
        const fill = at < index ? 1 : at === index ? Math.min(1, Math.max(0, fraction)) : 0
        const lead = at === 0 ? 0 : STORY_PROGRESS.gap
        return (
          <span
            key={at}
            data-slot="story-progress-segment"
            className="relative flex-1 overflow-hidden rounded-full bg-glass-stroke"
            style={{ height: STORY_PROGRESS.height, marginLeft: lead }}
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

/** #EXP-1493: THE kind badge ×4 (stories.json `badge`): THE Pill (sm,
 *  readonly), the kind's glyph + word in its tint. */
const BADGE_TINT: Record<Story[`kind`], string> = {
  plan: `text-primary bg-primary/14 border-primary/30`,
  question: `text-yellow-400 bg-yellow-400/14 border-yellow-400/30`,
  result: `text-emerald-500 bg-emerald-500/14 border-emerald-500/30`,
}
const BADGE_ICON = {
  plan: conceptIcon(STORY_BADGE.icons.plan as Parameters<typeof conceptIcon>[0]),
  question: conceptIcon(STORY_BADGE.icons.question as Parameters<typeof conceptIcon>[0]),
  result: conceptIcon(STORY_BADGE.icons.result as Parameters<typeof conceptIcon>[0]),
}
export function StoryKindBadge({ kind, className }: { kind: Story[`kind`]; className?: string }) {
  const Icon = BADGE_ICON[kind]
  return (
    <Pill mode="readonly" size="sm" data-slot="story-badge" data-kind={kind} className={cn(BADGE_TINT[kind], className)} leading={<Icon />}>
      {STORIES_COPY[kind]}
    </Pill>
  )
}

/** A question story's answer (the in-chat card's shape). */
export interface StoryAnswer {
  answer: string
  answers?: string[]
  surfaceData?: Record<string, unknown>
}


/** The md+ modal frame (#EXP-1463: sized by its frame, `storyViewerSize`,
 *  radius `viewer.radius`); phones = the whole screen. */
export const STORY_VIEWER_FRAME_CLASS = `md:h-(--story-h) md:w-(--story-w) md:rounded-(--story-radius) md:border md:border-glass-stroke-card md:shadow-2xl md:shadow-black/40`
export function storyViewerFrameStyle(size: { width: number; height: number }): CSSProperties {
  return {
    [`--story-w`]: `${size.width}px`,
    [`--story-h`]: `${size.height}px`,
    [`--story-radius`]: `${STORY_VIEWER.radius}px`,
  } as CSSProperties
}

/** The window's size (the wide viewer fits its frame into it). */
function useViewportSize(): { width: number; height: number } {
  const read = () =>
    typeof window === `undefined`
      ? { width: STORY_VIEWER.width, height: STORY_VIEWER.height }
      : { width: window.innerWidth, height: window.innerHeight }
  const [size, setSize] = useState(read)
  useEffect(() => {
    const onResize = () => setSize(read())
    window.addEventListener(`resize`, onResize)
    return () => window.removeEventListener(`resize`, onResize)
  }, [])
  return size
}

/** A picture's size: the card's, else the loaded image's natural size. */
function usePictureSize(
  picture: { width: number | null; height: number | null } | null,
  src: string | null
): { width: number | null; height: number | null } | null {
  const [natural, setNatural] = useState<{ src: string; width: number; height: number } | null>(null)
  const known = Boolean(picture?.width && picture.height)
  useEffect(() => {
    if (!picture || known || !src) return
    const image = new Image()
    image.onload = () => setNatural({ src, width: image.naturalWidth, height: image.naturalHeight })
    image.src = src
    return () => {
      image.onload = null
    }
  }, [picture, known, src])
  if (!picture || known) return picture
  return natural?.src === src ? natural : picture
}

/** A press on these never steps (the tap thirds skip them). */
const INTERACTIVE = `button, a, input, textarea, select, label, [role="button"], [role="slider"], [role="radio"], [role="checkbox"], [role="switch"], [role="option"], [contenteditable="true"], [data-slot="story-footer"]`

/** A floating band's scrim (`viewer.scrimHeight`): near-opaque behind the
 *  band's text, fading out over the extra `STORY_SCRIM_FADE` px past it,
 *  so a picture's own text never collides with the title. `to` = the
 *  direction it fades towards. */
const STORY_SCRIM_FADE = STORY_VIEWER.scrimFade
function storyScrim(to: `bottom` | `top`): CSSProperties {
  return {
    minHeight: STORY_VIEWER.scrimHeight,
    [to === `bottom` ? `paddingBottom` : `paddingTop`]: STORY_SCRIM_FADE,
    backgroundImage: `linear-gradient(to ${to}, rgb(0 0 0 / 0.88) 0%, rgb(0 0 0 / 0.8) calc(100% - ${STORY_SCRIM_FADE}px), transparent 100%)`,
  }
}

/** A node's live height (a callback ref + its height; 0 until measured). */
function useMeasuredHeight(): [(node: HTMLElement | null) => void, number] {
  const [height, setHeight] = useState(0)
  const observer = useRef<ResizeObserver | null>(null)
  const ref = useCallback((node: HTMLElement | null) => {
    observer.current?.disconnect()
    observer.current = null
    if (!node) return
    setHeight(node.offsetHeight)
    if (typeof ResizeObserver === `undefined`) return
    observer.current = new ResizeObserver(() => setHeight(node.offsetHeight))
    observer.current.observe(node)
  }, [])
  return [ref, height]
}

/** The viewer's frame (fully controlled): the body fills it on black, the
 *  progress + header + the frame's title and description float over the
 *  top scrim, the footer over the bottom one. #EXP-1493: ONE frame per
 *  story; a gallery's picture is `picture` (its own swipe steps it). */
export function StoryViewerPanel({
  ring,
  index,
  frames: given,
  picture = 0,
  onPicture,
  fraction,
  avatar,
  detail,
  attachmentSrc = (id) => `/api/attachments/${id}`,
  busy = false,
  onPrev,
  onNext,
  onClose,
  onOpenSession,
  onOpenPlan,
  onAnswer,
  onPause,
  className,
  style,
}: {
  ring: Pick<StoryRingModel, `key` | `title` | `stories`>
  /** The frame on show: its index in `storyFrames(ring)` (= the story's). */
  index: number
  /** `storyFrames(ring)` when the host holds it already. */
  frames?: readonly StoryFrame[]
  /** A gallery's picture on show. */
  picture?: number
  onPicture?: (picture: number) => void
  /** The current progress segment's fill (0…1). */
  fraction: number
  avatar: ReactNode
  /** The frame's story's card detail. */
  detail: StoryCardDetail
  attachmentSrc?: (attachmentId: string) => string
  /** The answer is on its way (the footer goes inert). */
  busy?: boolean
  onPrev?: () => void
  onNext?: () => void
  onClose?: () => void
  /** A tap on the avatar or name: open that session. */
  onOpenSession?: (sessionId: string) => void
  /** A plan's `Open plan` (THE plan page). */
  onOpenPlan?: (story: Story) => void
  onAnswer?: (answer: StoryAnswer) => void
  /** A long press holds the timer (true) / lets it go (false). */
  onPause?: (paused: boolean) => void
  className?: string
  style?: CSSProperties
}) {
  const frames = useMemo(() => given ?? storyFrames(ring), [given, ring])
  const [topRef, top] = useMeasuredHeight()
  const press = useRef<{ x: number; y: number; held: boolean; timer: number | null } | null>(null)

  const endPress = () => {
    const current = press.current
    press.current = null
    if (current?.timer != null) window.clearTimeout(current.timer)
    if (current?.held) onPause?.(false)
  }
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest(INTERACTIVE)) return
    const timer = window.setTimeout(() => {
      if (press.current) press.current.held = true
      onPause?.(true)
    }, 250)
    press.current = { x: event.clientX, y: event.clientY, held: false, timer }
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
    // stories.json: a tap anywhere on a plan's cover opens THE plan page
    // (swipes, the arrows and the keys still step).
    if (frame?.kind === `plan` && story && onOpenPlan) {
      onOpenPlan(story)
      return
    }
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

  const at = Math.min(Math.max(0, index), frames.length - 1)
  const frame = frames[at]
  const story = frame ? ring.stories[frame.story] : undefined
  if (!frame || !story) return null
  return (
    <div
      data-testid={STORIES_TEST_IDS.viewer}
      data-story={story.id}
      data-kind={story.kind}
      data-frame={frame.kind}
      data-ring={ring.key}
      className={cn(`relative h-full w-full cursor-pointer overflow-hidden bg-black text-foreground select-none`, className)}
      style={style}
      onTouchStart={onTouchStart}
      onTouchEnd={onTouchEnd}
      onPointerDown={onPointerDown}
      onPointerUp={onPointerUp}
      onPointerCancel={endPress}
      onPointerLeave={endPress}
    >
      <StoryFrameView
        key={story.id}
        story={story}
        frame={frame}
        picture={picture}
        onPicture={onPicture}
        detail={detail}
        top={top}
        attachmentSrc={attachmentSrc}
        busy={busy}
        onOpenPlan={onOpenPlan}
        onAnswer={onAnswer}
      />
      <div data-slot="story-top" className="pointer-events-none absolute inset-x-0 top-0 z-10" style={storyScrim(`bottom`)}>
        <div ref={topRef} className="flex flex-col gap-3 px-3 pt-[max(0.75rem,env(safe-area-inset-top))] pb-3">
          <StoryProgress count={frames.length} index={at} fraction={fraction} />
          <div data-testid={STORIES_TEST_IDS.header} className="flex min-w-0 items-center gap-2.5">
            <button
              type="button"
              className="pointer-events-auto flex min-w-0 flex-1 items-center gap-2.5 rounded-md text-left outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
              onClick={onOpenSession ? () => onOpenSession(story.sessionId) : undefined}
            >
              <StoryRing stories={ring.stories} size={STORY_VIEWER_AVATAR}>
                {avatar}
              </StoryRing>
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
              className="pointer-events-auto shrink-0 text-muted-foreground"
              onClick={onClose}
            >
              <CloseIcon />
            </Button>
          </div>
          <div className="flex min-w-0 flex-col gap-1 px-0.5">
            <div className="flex min-w-0 items-center gap-2">
              <StoryKindBadge kind={story.kind} />
              {frame.title && (
                <div data-slot="story-title" className="min-w-0 truncate text-base font-semibold">
                  {frame.title}
                </div>
              )}
            </div>
            {frame.description && (
              <p data-slot="story-description" className="line-clamp-3 text-sm text-muted-foreground">
                {frame.description}
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

/** THE surface renderer with no card chrome around it (the story body). */
function StorySurface({
  surface,
  dataRef,
  disabled,
  testKey,
}: {
  surface: CardSurface
  dataRef: { current: Record<string, unknown> | null }
  disabled: boolean
  testKey: string
}) {
  const state = useSurface({
    surfaceId: `story-${testKey}`,
    catalogId: CORE_CATALOG_ID,
    initial: surface.root as NestedNode,
    data: { ...(surface.data ?? {}) },
  })
  dataRef.current = state.data as Record<string, unknown>
  const host = useMemo(() => planCardHost(), [])
  return (
    <div data-slot="story-surface" inert={disabled || undefined} className="min-w-0">
      <ExponentialSurface surface={state} host={host} theme={DEFAULT_THEME_ID} mode="dark" />
    </div>
  )
}

/** One frame's body: a plan's COVER, a GALLERY (THE carousel with its
 *  filmstrip), THE surface, or nothing (a bare title); then its footer. */
function StoryFrameView({
  story,
  frame,
  picture,
  onPicture,
  detail,
  top,
  attachmentSrc,
  busy,
  onOpenPlan,
  onAnswer,
}: {
  story: Story
  frame: StoryFrame
  picture: number
  onPicture?: (picture: number) => void
  detail: StoryCardDetail
  /** The floating top band's height (the scrolling body starts under it). */
  top: number
  attachmentSrc: (attachmentId: string) => string
  busy: boolean
  onOpenPlan?: (story: Story) => void
  onAnswer?: (answer: StoryAnswer) => void
}) {
  const surfaceData = useSurfaceDataRef()
  const [footerRef, bottom] = useMeasuredHeight()
  const surface = frame.kind === `surface` ? detail.surface : null
  const pictures = useMemo(
    () =>
      story.pictures.map((entry) => ({
        src: attachmentSrc(entry.attachmentId),
        alt: entry.caption ?? entry.label,
        width: entry.width,
        height: entry.height,
      })),
    [story.pictures, attachmentSrc]
  )
  const cover = frame.kind === `plan` ? (story.pictures[0] ?? null) : null
  const shown = frame.kind === `gallery` ? (story.pictures[Math.min(picture, story.pictures.length - 1)] ?? null) : null
  return (
    <>
      {cover && (
        <img
          data-slot="story-picture"
          src={attachmentSrc(cover.attachmentId)}
          alt={cover.label}
          draggable={false}
          className="absolute inset-0 size-full object-contain"
        />
      )}
      {frame.kind === `gallery` && (
        <div
          data-slot="story-gallery"
          className="absolute inset-x-0 flex flex-col"
          style={{ top: top + 8, bottom: (bottom || 0) + 12 }}
        >
          <PictureCarousel
            pictures={pictures}
            index={picture}
            onIndex={onPicture}
            fit="contain"
            thumbnails
            className="min-h-0 flex-1 px-0 [&>[data-slot=carousel-counter]]:px-4 [&>[data-slot=carousel-thumbnails]]:px-4 [&>[data-slot=carousel-thumbnails]]:pb-[max(0.5rem,env(safe-area-inset-bottom))]"
            slideClassName="min-h-0 flex-1"
          />
          {shown && (
            <p data-slot="story-caption" className="px-4 pt-1 text-xs text-muted-foreground">{shown.caption ?? shown.label}</p>
          )}
        </div>
      )}
      {surface && (
        <div
          data-slot="story-body"
          className="absolute inset-0 overflow-y-auto overscroll-contain px-4"
          style={{ paddingTop: top + 8, paddingBottom: bottom + 16 }}
        >
          <StorySurface surface={surface} dataRef={surfaceData} disabled={busy} testKey={story.id} />
        </div>
      )}
      {frame.footer !== `none` && (
        <div
          className="pointer-events-none absolute inset-x-0 bottom-0 z-10 flex flex-col justify-end"
          style={storyScrim(`top`)}
        >
          <div ref={footerRef} className="pointer-events-auto cursor-auto px-4 pt-3 pb-[max(1rem,env(safe-area-inset-bottom))]">
            {frame.footer === `open` ? (
              <StoryPlanFooter story={story} attachmentSrc={attachmentSrc} onOpenPlan={onOpenPlan} />
            ) : (
              <StoryFooter
                story={story}
                footer={frame.footer}
                detail={detail}
                surfaceData={surfaceData}
                attachmentSrc={attachmentSrc}
                busy={busy}
                onAnswer={onAnswer}
              />
            )}
          </div>
        </div>
      )}
    </>
  )
}

/** A plan's cover footer (#EXP-1493): the counts line, up to 4 thumbnails
 *  (+N), then ONE full-width `Open plan` (THE plan page). */
function StoryPlanFooter({
  story,
  attachmentSrc,
  onOpenPlan,
}: {
  story: Story
  attachmentSrc: (attachmentId: string) => string
  onOpenPlan?: (story: Story) => void
}) {
  const line = story.counts ? planCountsLine(story.counts) : ``
  const thumbs = story.pictures.slice(0, 4)
  const more = story.pictures.length - thumbs.length
  return (
    <div data-slot="story-footer" data-footer="open" className="flex flex-col gap-3">
      {line && <div className="text-xs text-foreground/85">{line}</div>}
      {thumbs.length > 0 && (
        <div className="flex items-center gap-1.5">
          {thumbs.map((entry, at) => (
            <img
              key={entry.attachmentId}
              src={attachmentSrc(entry.attachmentId)}
              alt={entry.label}
              draggable={false}
              className={cn(
                `h-10 w-14 rounded-md border object-cover`,
                at === 0 ? `border-2 border-foreground` : `border-glass-stroke`
              )}
            />
          ))}
          {more > 0 && <span className="text-xs text-muted-foreground">+{more}</span>}
        </div>
      )}
      <Button
        type="button"
        size="lg"
        data-slot="story-open-plan"
        data-testid="open-plan"
        className="w-full"
        disabled={!onOpenPlan}
        onClick={() => onOpenPlan?.(story)}
      >
        {PLAN_PAGE_COPY.openPlan}
      </Button>
    </div>
  )
}

/** A question's footer (the in-chat card's answer controls: THE option rows
 *  / tiles, THE free-text row, THE Answer button). */
function StoryFooter({
  story,
  footer,
  detail,
  surfaceData,
  attachmentSrc,
  busy,
  onAnswer,
}: {
  story: Story
  footer: Exclude<StoryFooterKind, `none` | `open`>
  detail: StoryCardDetail
  /** The shown surface's live data (null = none shown). */
  surfaceData: { current: Record<string, unknown> | null }
  attachmentSrc: (attachmentId: string) => string
  busy: boolean
  onAnswer?: (answer: StoryAnswer) => void
}) {
  const [picked, setPicked] = useState<string[]>([])
  const [customText, setCustomText] = useState(``)
  const withData = () => (surfaceData.current ? { surfaceData: surfaceData.current } : {})
  const labels = questionAnswer(picked, customText, detail.multi)
  return (
    <div data-slot="story-footer" data-footer={footer} inert={busy || undefined} className="flex flex-col gap-2">
      {footer === `choices` && (
        <PlanCardOptions
          options={detail.options}
          multi={detail.multi}
          picked={picked}
          onPick={busy || !onAnswer ? undefined : setPicked}
          attachmentSrc={attachmentSrc}
          label={story.title ?? STORIES_COPY.question}
        />
      )}
      {footer === `reply` ? (
        <Input
          value={customText}
          onChange={(event) => setCustomText(event.target.value)}
          placeholder={STORIES_COPY.reply}
          aria-label={story.title ?? STORIES_COPY.question}
          data-slot="story-reply"
          className="h-9 text-sm"
        />
      ) : detail.custom ? (
        <PlanCardCustomField value={customText} onChange={setCustomText} label={story.title ?? STORIES_COPY.question} />
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
    </div>
  )
}

/** Where the viewer stands: a ring (by key), a story (by id) and, on a
 *  gallery, the picture. */
export interface StoryPosition {
  ringKey: string
  storyId: string
  picture: number
}

const storyPosition = (ringKey: string, frame: StoryFrame, picture = 0): StoryPosition => ({
  ringKey,
  storyId: frame.storyId,
  picture,
})

/** The next / previous STORY over the rings (past the last story of the last
 *  ring = null = close; before the very first = stay; the first story of a
 *  ring steps back to the previous ring's last). */
export function stepFrame(
  rings: readonly Pick<StoryRingModel, `key` | `stories`>[],
  at: Pick<StoryPosition, `ringKey` | `storyId`>,
  direction: 1 | -1
): StoryPosition | null | `stay` {
  const r = rings.findIndex((ring) => ring.key === at.ringKey)
  if (r < 0) return null
  const ring = rings[r]!
  const frames = storyFrames(ring)
  const f = storyFrameIndex(frames, at.storyId)
  const next = f + direction
  if (f >= 0 && next >= 0 && next < frames.length) return storyPosition(ring.key, frames[next]!)
  if (direction === 1) {
    const following = rings[r + 1]
    const first = following ? storyFrames(following)[0] : undefined
    return following && first ? storyPosition(following.key, first) : null
  }
  const before = rings[r - 1]
  const last = before ? storyFrames(before).at(-1) : undefined
  return before && last ? storyPosition(before.key, last) : `stay`
}

/** THE story viewer: open while `openRing` names a ring. */
export function StoryViewer({
  rings,
  openRing,
  openStory = null,
  openPicture = 0,
  onClose,
  avatarFor,
  cardFor,
  onShown,
  onOpenSession,
  onOpenPlan,
  onAnswer,
  pending,
  attachmentSrc,
}: {
  rings: readonly StoryRingModel[]
  /** The ring the viewer opens at (null = closed). */
  openRing: string | null
  /** The story of `openRing` it opens on (null = its first; the Stories
   *  tab's tile). */
  openStory?: string | null
  /** The picture of `openStory`'s gallery it opens on (a thumbnail). */
  openPicture?: number
  onClose: () => void
  avatarFor: (ring: StoryRingModel) => ReactNode
  /** The story's raw card (its surface and options). */
  cardFor: (story: Story) => unknown
  /** A story came on screen (mark it seen). */
  onShown?: (story: Story) => void
  onOpenSession?: (sessionId: string) => void
  /** #EXP-1493: a plan's `Open plan` (THE plan page); the viewer closes. */
  onOpenPlan?: (story: Story) => void
  onAnswer?: (story: Story, answer: StoryAnswer) => Promise<unknown> | void
  pending?: ReadonlySet<string>
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
    const frames = ring ? storyFrames(ring) : []
    const hit = openStory === null ? 0 : storyFrameIndex(frames, openStory)
    const first = frames[Math.max(0, hit)]
    if (!ring || !first) {
      onClose()
      return
    }
    setSnapshot(rings)
    setPosition(storyPosition(ring.key, first, hit >= 0 && openStory !== null ? openPicture : 0))
    // Opening only: later changes of `rings` refresh the stories below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openRing, openStory, openPicture])

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
  const frames = useMemo(() => (ring ? storyFrames(ring) : []), [ring])
  const at = position ? storyFrameIndex(frames, position.storyId) : -1
  const frame = at >= 0 ? frames[at]! : null
  const story = ring && frame ? (ring.stories[frame.story] ?? null) : null
  const count = frame?.kind === `gallery` ? Math.max(1, frame.pictures) : 1
  const pictureAt = Math.min(position?.picture ?? 0, count - 1)
  const sized =
    story && frame && (frame.kind === `gallery` || frame.kind === `plan`)
      ? (story.pictures[frame.kind === `gallery` ? pictureAt : 0] ?? null)
      : null
  const pictureSize = usePictureSize(
    sized,
    sized ? (attachmentSrc ? attachmentSrc(sized.attachmentId) : `/api/attachments/${sized.attachmentId}`) : null
  )
  const viewport = useViewportSize()
  const frameSize = storyViewerSize(pictureSize, viewport)

  const go = useCallback(
    (direction: 1 | -1) => {
      if (!position) return
      const next = stepFrame(view, position, direction)
      if (next === `stay`) return
      if (next === null) onClose()
      else setPosition(next)
    },
    [position, view, onClose]
  )
  const setPicture = useCallback(
    (picture: number) => setPosition((current) => (current ? { ...current, picture } : current)),
    []
  )

  // Seen when its frame is shown.
  const shownRef = useRef(onShown)
  shownRef.current = onShown
  const storyId = story?.id ?? null
  useEffect(() => {
    if (story) shownRef.current?.(story)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [storyId])

  // One timer per frame; a gallery's steps its pictures first (a decision's
  // frame never advances on its own).
  const [paused, setPaused] = useState(false)
  const [fraction, setFraction] = useState(0)
  const frameKey = frame ? `${frame.storyId}:${pictureAt}` : null
  const duration = frame?.autoAdvanceMs ?? null
  useEffect(() => {
    setFraction(duration === null ? 1 : pictureAt / count)
  }, [frameKey, duration, pictureAt, count])
  const elapsed = useRef(0)
  useEffect(() => {
    elapsed.current = 0
  }, [frameKey])
  useEffect(() => {
    if (duration === null || paused || frameKey === null) return
    let raf = 0
    let last = performance.now()
    const tick = (now: number) => {
      if (typeof document === `undefined` || document.visibilityState !== `hidden`) elapsed.current += now - last
      last = now
      const value = elapsed.current / duration
      setFraction(Math.min(1, (pictureAt + Math.min(1, value)) / count))
      if (value >= 1) {
        if (pictureAt + 1 < count) setPicture(pictureAt + 1)
        else go(1)
        return
      }
      raf = requestAnimationFrame(tick)
    }
    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [duration, paused, frameKey, go, pictureAt, count, setPicture])

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

  // Only a press that starts AND ends on the bare scrim closes (md+): a
  // side arrow steps, a drag out of the modal stays.
  const scrimDown = useRef(false)
  const step = (direction: 1 | -1) => (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation()
    go(direction)
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
          onPointerDown={(event) => {
            scrimDown.current = event.target === event.currentTarget
          }}
          onClick={(event) => {
            const bare = scrimDown.current && event.target === event.currentTarget
            scrimDown.current = false
            if (bare) onClose()
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
                onClick={step(-1)}
              >
                <PrevIcon />
              </Button>
              <StoryViewerPanel
                ring={ring}
                index={at}
                frames={frames}
                picture={pictureAt}
                onPicture={setPicture}
                fraction={fraction}
                avatar={avatarFor(ring)}
                detail={storyCardDetail(cardFor(story))}
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
                onOpenPlan={
                  onOpenPlan
                    ? (entry) => {
                        onClose()
                        onOpenPlan(entry)
                      }
                    : undefined
                }
                onAnswer={onAnswer ? (answer) => void settle(onAnswer(story, answer)) : undefined}
                onPause={setPaused}
                className={STORY_VIEWER_FRAME_CLASS}
                style={storyViewerFrameStyle(frameSize)}
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                data-testid={STORIES_TEST_IDS.next}
                aria-label={STORIES_COPY.next}
                className="absolute top-1/2 -right-14 hidden -translate-y-1/2 rounded-full bg-glass-row md:inline-flex"
                onClick={step(1)}
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
