import { useLayoutEffect, useMemo, useRef, type ReactNode } from "react"
import { CORE_CATALOG_ID, DEFAULT_THEME_ID, type NestedNode } from "@exponential_app/ui"
import { ExponentialSurface, useSurface } from "@exponential_app/ui-react"
import { CHATS_COPY, CHATS_TEST_IDS, surfaceReply } from "@exp/domain-contract/chats"
import type { CardSurface } from "@exp/domain-contract/plan-card"
import fixture from "@exp/domain-contract/fixtures/chats.json"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { planCardHost } from "./plan-card"
import { userMessageTime } from "./user-message-bubble"

// Thread 58cb32df (fixtures/chats.json TRANSCRIPT + SURFACE CARD ×4; iOS
// `SurfaceCard` / `TranscriptDivider` / `ChatBubble`, Android the same names
// in ui/thread, desktop `surface_card.rs`): the chat transcript's new pieces.
//
//   SurfaceCard        an agent's inline Exponential UI preview (message card
//                      `surface`), painted by THE plan card's renderer
//                      (theme exponential, dark) in its own rounded glass
//                      frame; a press whose event is `replyEvent` with a
//                      `context.text` posts that text as the person's message
//                      (`surfaceReply`). A LIVE arrival plays `surfaceCard.enter`
//                      once (frame fade + rise + scale, the primary hairline
//                      settling, the root's children staggered); reduce
//                      motion = the frame's opacity alone.
//   TranscriptDivider  a person's `/new` / `/compact` line: the centred glass
//                      pill `copy.dividers.<kind> · HH:mm`
//   ChatBubble         phones: the agent's text on the left in a bg-card
//                      bubble (max 94%, top-left 6px), its time bottom-right
//   useLiveArrivals    the rows that arrived after the screen first painted
//                      (history, a reopen or a scroll never animates)

const ENTER = fixture.surfaceCard.enter
const NewIcon = conceptIcon(`ui-clean`)
const CompactIcon = conceptIcon(`coding-compact`)

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

/** Plays `surfaceCard.enter` on the frame, its glow and the root's children. */
function playEnter(frame: HTMLElement, glow: HTMLElement | null, body: HTMLElement): void {
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
  const c = ENTER.children
  const stagger = (targets: HTMLElement[]) =>
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
  if (now.length > 0) stagger(now)
  else requestAnimationFrame(() => stagger(rootChildren(body)))
}

export function SurfaceCard({
  surface,
  title = null,
  testKey,
  onReply,
  animate = false,
  className,
}: {
  surface: CardSurface
  /** The card's title (absent = none). */
  title?: string | null
  /** A stable id (the message's): the surface's own id. */
  testKey: string
  /** A `reply` press's text, sent as the person's next message. */
  onReply?: (text: string) => void
  /** A LIVE arrival: play the enter once on mount. */
  animate?: boolean
  className?: string
}) {
  const state = useSurface({
    surfaceId: `surface-${testKey}`,
    catalogId: CORE_CATALOG_ID,
    initial: surface.root as NestedNode,
    data: { ...(surface.data ?? {}) },
  })
  const replyRef = useRef(onReply)
  replyRef.current = onReply
  const host = useMemo(
    () =>
      planCardHost((event) => {
        const reply = surfaceReply(event)
        if (reply) replyRef.current?.(reply.text)
      }),
    []
  )
  const frameRef = useRef<HTMLDivElement>(null)
  const glowRef = useRef<HTMLSpanElement>(null)
  const bodyRef = useRef<HTMLDivElement>(null)
  const played = useRef(false)
  useLayoutEffect(() => {
    if (!animate || played.current || !frameRef.current || !bodyRef.current) return
    played.current = true
    playEnter(frameRef.current, glowRef.current, bodyRef.current)
  }, [animate])
  return (
    <div
      ref={frameRef}
      data-testid={CHATS_TEST_IDS.surfaceCard}
      data-entering={animate || undefined}
      className={cn(
        `relative flex w-full max-w-full min-w-0 flex-col gap-2 rounded-2xl border border-glass-stroke-card bg-glass-card p-3`,
        className
      )}
    >
      <span
        ref={glowRef}
        aria-hidden
        className="pointer-events-none absolute -inset-px rounded-[inherit] border border-primary opacity-0"
      />
      {title && <div className="truncate text-xs font-medium text-muted-foreground">{title}</div>}
      <div ref={bodyRef} className="min-w-0">
        <ExponentialSurface surface={state} host={host} theme={DEFAULT_THEME_ID} mode="dark" />
      </div>
    </div>
  )
}

/** A person's `/new` or `/compact` line: the centred glass pill. */
export function TranscriptDivider({
  kind,
  at,
  className,
}: {
  kind: `new` | `compact`
  /** The line's createdAt (ISO); its local `HH:mm` follows the copy. */
  at: string | null
  className?: string
}) {
  const Icon = kind === `new` ? NewIcon : CompactIcon
  const time = at ? userMessageTime(new Date(at)) : ``
  const label = time ? `${CHATS_COPY.dividers[kind]} · ${time}` : CHATS_COPY.dividers[kind]
  return (
    <div
      data-testid={CHATS_TEST_IDS.divider}
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

/** The issue chips inside a chat bubble run INLINE with the prose like the
 *  natives' (iOS/Android wrap a chip's title with the text, its box cloned
 *  on each line; the identifier never splits), never a full line each: the app's `.issue-ref-pill`
 *  (an atomic truncating box elsewhere, EXP-899) becomes an inline run here. */
const CHAT_BUBBLE_INLINE_CHIPS_CLASS = cn(
  `[&_.issue-ref-pill]:inline! [&_.issue-ref-pill]:max-w-none! [&_.issue-ref-pill]:whitespace-nowrap! [&_.issue-ref-pill]:after:whitespace-normal!`,
  `[&_.issue-ref-pill]:[box-decoration-break:clone]! [&_.issue-ref-pill]:[-webkit-box-decoration-break:clone]!`,
  `[&_.issue-ref-pill]:py-0! [&_.issue-ref-pill]:leading-[inherit]!`
)

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
      <div className="flex max-w-[94%] min-w-0 flex-col gap-1 rounded-2xl rounded-tl-[6px] bg-card px-3.5 py-2.5 text-sm text-foreground">
        <div className={cn(`min-w-0`, CHAT_BUBBLE_INLINE_CHIPS_CLASS)}>{children}</div>
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
