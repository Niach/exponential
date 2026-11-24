import { useLayoutEffect, useMemo, useRef, type ReactNode } from "react"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID, type NestedNode } from "@exponential_app/eui"
import { ExponentialSurface, useSurface } from "@exponential_app/eui-react"
import { CHATS_COPY, CHATS_TEST_IDS } from "@exp/domain-contract/chats"
import type { ActionEvent } from "@exp/domain-contract/chat-express"
import { PLAN_CARD_GEOMETRY, type CardSurface } from "@exp/domain-contract/plan-card"
import { SEALED_PLACEHOLDER } from "@exp/domain-contract/sealed-content"
import fixture from "@exp/domain-contract/fixtures/chats.json"
import { cn } from "./cn"
import { useColorMode } from "./color-mode"
import { conceptIcon } from "./icons.generated"
import { paintedCard, planCardHost, useCardLinks } from "./plan-card"
import { userMessageTime } from "./user-message-bubble"

// Chat 58cb32df (fixtures/chats.json TRANSCRIPT + SURFACE CARD ×4; iOS
// `SurfaceCard` / `TranscriptDivider` / `ChatBubble`, Android the same names
// in ui/chat, desktop `surface_card.rs`): the chat transcript's new pieces.
//
//   SurfaceCard        an agent's inline Exponential UI preview (message card
//                      `surface`), painted by THE plan card's renderer
//                      (theme exponential, dark) in its own rounded glass
//                      frame; #EXP-2067: EVERY press posts an action card
//                      (chat-express.json `surfaceAction` over
//                      `showActionOrigin`). A LIVE arrival plays `surfaceCard.enter`
//                      once (frame fade + rise + scale, the primary hairline
//                      settling, the root's children staggered); reduce
//                      motion = the frame's opacity alone.
//   TranscriptDivider  a person's `/new` / `/compact` line: the centred glass
//                      pill `copy.dividers.<kind> · HH:mm`
//   ChatBubble         the agent's text on the left on THE card surface
//                      (GlassCard's fill + hairline, N-12 ONE gray) (max 94%, top-left 6px), its time bottom-right
//   useLiveArrivals    the rows that arrived after the screen first painted
//                      (history, a reopen or a scroll never animates)

const ENTER = fixture.surfaceCard.enter
const NewIcon = conceptIcon(`ui-clean`)
const CompactIcon = conceptIcon(`coding-compact`)
const LockedIcon = conceptIcon(SEALED_PLACEHOLDER.icon as `ui-private`)

/** tokens.json `motion` (the CSS vars win; these cover a var-less host). */
const DURATION_FALLBACK: Record<string, number> = { fast: 120, standard: 180, slow: 280 }
const EASE_FALLBACK: Record<string, string> = {
  standard: `cubic-bezier(0.2, 0, 0, 1)`,
  decelerate: `cubic-bezier(0, 0, 0.2, 1)`,
  accelerate: `cubic-bezier(0.3, 0, 1, 1)`,
}

function motionVar(name: string): string {
  if (typeof document === `undefined`) return ``
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}

/** A motion token's duration in ms (`--motion-duration-<name>`). */
export function motionDuration(name: string): number {
  const raw = motionVar(`--motion-duration-${name}`)
  const ms = raw.endsWith(`ms`) ? Number(raw.slice(0, -2)) : raw.endsWith(`s`) ? Number(raw.slice(0, -1)) * 1000 : NaN
  return Number.isFinite(ms) ? ms : (DURATION_FALLBACK[name] ?? 180)
}

/** A motion token's easing (`--motion-ease-<name>`). */
export function motionEase(name: string): string {
  return motionVar(`--motion-ease-${name}`) || (EASE_FALLBACK[name] ?? EASE_FALLBACK.standard!)
}

function prefersReducedMotion(): boolean {
  return typeof window !== `undefined` && typeof window.matchMedia === `function`
    ? window.matchMedia(`(prefers-reduced-motion: reduce)`).matches
    : false
}

/** The surface ROOT's first-level children (the stagger's targets). */
function rootChildren(host: HTMLElement): HTMLElement[] {
  const root = host.querySelector<HTMLElement>(`[data-xui-id]`)
  if (!root) return []
  return Array.from(root.querySelectorAll<HTMLElement>(`[data-xui-id]`)).filter(
    (el) => el.parentElement?.closest(`[data-xui-id]`) === root
  )
}

/** Plays `surfaceCard.enter` on the frame, its glow and (`stagger`) the
 *  root's children. */
function playEnter(frame: HTMLElement, glow: HTMLElement | null, body: HTMLElement, stagger = true): void {
  if (typeof frame.animate !== `function`) return
  const reduce = prefersReducedMotion()
  const f = ENTER.frame
  frame.animate(
    reduce
      ? [{ opacity: f.fromOpacity }, { opacity: 1 }]
      : [
          { opacity: f.fromOpacity, transform: `translateY(${f.fromTranslateY}px) scale(${f.fromScale})` },
          { opacity: 1, transform: `none` },
        ],
    { duration: motionDuration(f.duration), easing: motionEase(f.ease), fill: `backwards` }
  )
  if (reduce) return
  glow?.animate([{ opacity: ENTER.glow.fromOpacity }, { opacity: 0 }], {
    duration: motionDuration(ENTER.glow.duration),
    easing: motionEase(ENTER.glow.ease),
    fill: `both`,
  })
  if (!stagger) return
  const c = ENTER.children
  const staggerIn = (targets: HTMLElement[]) =>
    targets.forEach((el, index) => {
      el.animate(
        [
          { opacity: c.fromOpacity, transform: `translateY(${c.fromTranslateY}px)` },
          { opacity: 1, transform: `none` },
        ],
        {
          duration: motionDuration(c.duration),
          easing: motionEase(c.ease),
          delay: c.startAfterMs + Math.min(index, c.maxStaggered - 1) * c.staggerMs,
          fill: `backwards`,
        }
      )
    })
  const now = rootChildren(body)
  if (now.length > 0) staggerIn(now)
  else requestAnimationFrame(() => staggerIn(rootChildren(body)))
}

/** THE surface card frame (chats.json `surfaceCard`): the glass frame with
 *  plan-card.json `surfaceRadius` / `surfacePadding`, an optional caption
 *  above it, the enter on a LIVE arrival. Its body is the caller's (a show
 *  card's painted surface, an Express block's host surface, a skeleton). */
export function SurfaceCardFrame({
  title = null,
  animate = false,
  stagger = true,
  className,
  children,
}: {
  title?: string | null
  /** A LIVE arrival: play the enter once on mount. */
  animate?: boolean
  /** The enter staggers the root's children (an Express block streams
   *  them in instead: false). */
  stagger?: boolean
  className?: string
  children: ReactNode
}) {
  const frameRef = useRef<HTMLDivElement>(null)
  const glowRef = useRef<HTMLSpanElement>(null)
  const bodyRef = useRef<HTMLDivElement>(null)
  const played = useRef(false)
  useLayoutEffect(() => {
    if (!animate || played.current || !frameRef.current || !bodyRef.current) return
    played.current = true
    playEnter(frameRef.current, glowRef.current, bodyRef.current, stagger)
  }, [animate, stagger])
  return (
    // The title = the plan card's surface caption (plan-card.json `card`:
    // 2xs medium muted, `surfaceLabelGap` ABOVE the frame), entering with it;
    // the frame = its `surfaceRadius` + `surfacePadding` ×4.
    <div
      ref={frameRef}
      data-testid={CHATS_TEST_IDS.surfaceCard}
      data-entering={animate || undefined}
      className={cn(`flex w-full max-w-full min-w-0 flex-col`, className)}
      style={{ gap: PLAN_CARD_GEOMETRY.surfaceLabelGap }}
    >
      {title && <div className="truncate text-[0.6875rem] font-medium text-muted-foreground">{title}</div>}
      <div
        className="relative min-w-0 border border-glass-stroke-card bg-glass-card-opaque"
        style={{ borderRadius: PLAN_CARD_GEOMETRY.surfaceRadius, padding: PLAN_CARD_GEOMETRY.surfacePadding }}
      >
        <span
          ref={glowRef}
          aria-hidden
          className="pointer-events-none absolute -inset-px rounded-[inherit] border border-primary opacity-0"
        />
        <div ref={bodyRef} className="min-w-0">
          {children}
        </div>
      </div>
    </div>
  )
}

export function SurfaceCard({
  surface,
  title = null,
  testKey,
  onAction,
  animate = false,
  className,
}: {
  surface: CardSurface
  /** The card's title (absent = none). */
  title?: string | null
  /** A stable id (the message's): the surface's own id. */
  testKey: string
  /** #EXP-2067: every action event of the surface (the host posts it as an
   *  action card, `surfaceAction` + `showActionOrigin`); absent = inert. */
  onAction?: (event: ActionEvent) => void
  /** A LIVE arrival: play the enter once on mount. */
  animate?: boolean
  className?: string
}) {
  // #FEED-292: a card edited in place (`replace`, a picture landing) paints
  // its new surface: the body remounts on a changed tree, the frame stays.
  const paintKey = useMemo(() => JSON.stringify(surface), [surface])
  const actionRef = useRef(onAction)
  actionRef.current = onAction
  return (
    <SurfaceCardFrame title={title} animate={animate} className={className}>
      <SurfaceCardBody key={paintKey} surface={surface} testKey={testKey} actionRef={actionRef} />
    </SurfaceCardFrame>
  )
}

function SurfaceCardBody({
  surface,
  testKey,
  actionRef,
}: {
  surface: CardSurface
  testKey: string
  actionRef: { current: ((event: ActionEvent) => void) | undefined }
}) {
  const upgraded = useMemo(() => paintedCard(surface), [surface])
  const state = useSurface({
    surfaceId: `surface-${testKey}`,
    catalogId: CORE_CATALOG_ID,
    initial: upgraded.root as NestedNode,
    data: { ...((upgraded.data as Record<string, unknown> | null | undefined) ?? {}) },
  })
  const links = useCardLinks()
  const host = useMemo(() => planCardHost((event) => actionRef.current?.(event), links), [links, actionRef])
  const mode = useColorMode()
  return <ExponentialSurface surface={state} host={host} theme={DEFAULT_THEME_ID} mode={mode} />
}

/** A person's `/new` or `/compact` line: the centred glass pill. EXP-1459:
 *  the same pill with the lock = content this install cannot open (yet)
 *  (sealed-content.json `placeholder`; no time). */
export function TranscriptDivider({
  kind,
  at,
  className,
}: {
  kind: `new` | `compact` | `locked-noKey` | `locked-unreadable`
  /** The line's createdAt (ISO); its local `HH:mm` follows the copy. */
  at: string | null
  className?: string
}) {
  const locked = kind === `locked-noKey` ? `noKey` : kind === `locked-unreadable` ? `unreadable` : null
  const Icon = locked ? LockedIcon : kind === `new` ? NewIcon : CompactIcon
  const copy = locked ? SEALED_PLACEHOLDER.copy[locked] : CHATS_COPY.dividers[kind as `new` | `compact`]
  const time = at && !locked ? userMessageTime(new Date(at)) : ``
  const label = time ? `${copy} · ${time}` : copy
  return (
    <div
      data-testid={locked ? SEALED_LOCKED_TEST_ID : CHATS_TEST_IDS.divider}
      data-kind={kind}
      className={cn(`flex justify-center`, className)}
    >
      <span className="inline-flex max-w-full items-center gap-1.5 rounded-full border border-glass-stroke-card bg-glass-row px-3 py-1 text-xs text-muted-foreground">
        <Icon className="size-3.5 shrink-0" />
        <span className="truncate">{label}</span>
      </span>
    </div>
  )
}

/** #EXP-1772 (transcript-grouping.json MARKER): THE inline marker, the
 *  feed's `Context compacted` row: a centred muted label between two
 *  hairlines. A schedule or event firing the agent answered, a PR or
 *  session status line, a system note: punctuation, never a bubble. A
 *  marker about a session opens it (`onPress`). */
export function TranscriptMarker({
  label,
  onPress,
  className,
}: {
  label: string
  onPress?: () => void
  className?: string
}) {
  const text = <span className="min-w-0 shrink truncate text-muted-foreground/70">{label}</span>
  return (
    <div
      data-testid="transcript-marker"
      className={cn(
        `flex items-center gap-2 text-[length:var(--transcript-tool-size)] leading-[var(--transcript-tool-line-height)]`,
        className
      )}
    >
      <span className="h-px flex-1 bg-border/60" />
      {onPress ? (
        <button type="button" onClick={onPress} className="min-w-0 shrink truncate rounded-sm outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50">
          {text}
        </button>
      ) : (
        text
      )}
      <span className="h-px flex-1 bg-border/60" />
    </div>
  )
}

/** EXP-1459: the locked pill's test id (component tests, captures). */
export const SEALED_LOCKED_TEST_ID = `sealed-locked`

/** Phones: the agent's text bubble on the left. */
export function ChatBubble({
  children,
  time = null,
  className,
}: {
  children: ReactNode
  time?: string | null
  className?: string
}) {
  return (
    <div data-testid="chat-bubble" className={cn(`flex justify-start`, className)}>
      <div className="flex max-w-[94%] min-w-0 flex-col gap-1 rounded-2xl rounded-tl-[6px] border border-glass-stroke-card bg-glass-card-opaque px-3.5 py-2.5 text-sm text-foreground">
        <div className="min-w-0">{children}</div>
        {time && (
          <div data-slot="chat-bubble-time" className="self-end text-[0.6875rem] text-muted-foreground tabular-nums">
            {time}
          </div>
        )}
      </div>
    </div>
  )
}

/** The rows that arrived after the screen first painted its rows: the set
 *  present at the first non-empty render never animates (history, a
 *  reopen); every later id does, once. */
export function useLiveArrivals(ids: readonly string[]): (id: string) => boolean {
  const initial = useRef<Set<string> | null>(null)
  if (initial.current === null && ids.length > 0) initial.current = new Set(ids)
  const seen = initial.current
  return (id: string) => seen !== null && !seen.has(id)
}

/** The phones' new-bubble rise (opacity + 6px at `standard`/`decelerate`,
 *  the `motion-enter` recipe; reduce motion = the fade). */
export const CHAT_BUBBLE_ENTER_CLASS = `motion-enter slide-in-from-bottom-[6px]`
