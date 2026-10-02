import {
  useCallback,
  useEffect,
  useRef,
  type CSSProperties,
  type ReactNode,
  type TouchEvent,
} from "react"
import { ChangesFaceLabel, type WorkFaceStripRun } from "@exp/ui"
import { cn } from "@/lib/utils"
import type { CodingSession } from "@/db/schema"
import type { PastRunRow } from "@/hooks/use-agents-data"
import {
  swipeTarget,
  toggleFaceDots,
  type ChangesFaceCounts,
  type FaceDotTone,
  type SwipeDirection,
  type WorkFaceKind,
} from "@/lib/work-faces"
import {
  ISSUE_FACE_LABEL,
  RESULTS_FACE_LABEL,
  runFaceLabel,
  WorkFaceToggle,
  type WorkFace,
  type WorkFaceItem,
} from "@/components/team/work-face-toggle"

// EXP-1150: the phone's faces are TABS. The strip under the Work screen's
// header is the SAME segmented control the md+ work header carries
// (`WorkFaceToggle` → `@exp/ui` `WorkFaceStrip`, EXP-1152), listing every
// available face in its fixed order — Issue · Run/Runs · +N −M · Results —
// and the `Runs` segment keeps its caret to the run menu (EXP-950). The
// Changes segment wears the diff's counts once its files are known and the
// word until then (EXP-1152, the desktop `FaceToggle::diff` rule). The strip
// is absent with a single face (nothing to switch), and it never moves
// between faces: every face renders it INSIDE the header band, under the
// title row and over the band's one hairline (`MobileDetailHeader` `below`),
// so title and tabs read as one header. iOS `WorkFaceTabs` and Android
// `FaceTabs` draw the same strip in the same place.
//
// EXP-1152: the body is a PAGER. iOS pages with `TabView(.page)` and Android
// with `HorizontalPager`; on the web the faces are separate route trees (Run
// is another route), so a neighbour cannot be pre-mounted. Instead the face's
// BODY (`[data-face-body]`) follows the finger — `translateX` written straight
// onto the node, no React render per move, rubber-banded where there is no
// neighbour — and on release it either springs back or leaves to the side it
// was thrown, the face flips, and the NEW body slides in from the other side
// (`primeFaceEnter`, read on the next face). The header band and the fixed
// bottom bar never move: only the body is transformed (a transformed ancestor
// would also turn the bar's `position: fixed` into a local one). The content
// follows the finger, so a swipe LEFT opens the next face and a swipe RIGHT
// the previous one; a sideways scroller (a code line of the diff) keeps the
// gesture only while it can still scroll that way — past its edge the page
// turns, the pager rule — and a text field keeps every gesture.

/** A swipe counts from this many CSS px of horizontal travel… */
export const SWIPE_MIN_DISTANCE = 56
/** …and only when it is clearly horizontal (twice the vertical travel). */
export const SWIPE_DOMINANCE = 2

/** The pure gate: a direction, or null for a scroll / a tap / a shrug. */
export function swipeDirection(dx: number, dy: number): SwipeDirection | null {
  if (Math.abs(dx) < SWIPE_MIN_DISTANCE) return null
  if (Math.abs(dx) <= SWIPE_DOMINANCE * Math.abs(dy)) return null
  return dx < 0 ? `left` : `right`
}

/** What a touch landed on: a text field keeps every gesture; the nearest
 *  ancestor that scrolls sideways (a code line of the diff, a wide table) is
 *  remembered with where it stood, so the lift can apply the PAGER rule —
 *  the scroller keeps a swipe only while it can still move that way, and a
 *  swipe past its edge turns the page. */
function touchClaim(
  target: EventTarget | null,
  root: HTMLElement
): { field: boolean; scroller: { node: Element; scrollLeft: number } | null } {
  let node = target instanceof Element ? target : null
  while (node && node !== root) {
    if (
      node instanceof HTMLInputElement ||
      node instanceof HTMLTextAreaElement ||
      (node instanceof HTMLElement && node.isContentEditable)
    ) {
      return { field: true, scroller: null }
    }
    if (node.scrollWidth > node.clientWidth + 1) {
      const overflow = getComputedStyle(node).overflowX
      if (overflow === `auto` || overflow === `scroll`) {
        return { field: false, scroller: { node, scrollLeft: node.scrollLeft } }
      }
    }
    node = node.parentElement
  }
  return { field: false, scroller: null }
}

/** The pager rule: the scroller under the finger could have scrolled the
 *  way the finger went (measured where it STOOD at touch start — by the lift
 *  the browser has already moved it), so the gesture was a scroll. */
function scrollerTookIt(
  scroller: { node: Element; scrollLeft: number },
  direction: SwipeDirection
): boolean {
  const { node, scrollLeft } = scroller
  return direction === `left`
    ? scrollLeft + node.clientWidth < node.scrollWidth - 1
    : scrollLeft > 0
}

/** A flick commits from this much horizontal travel… */
export const FLICK_MIN_DISTANCE = 24
/** …at this speed (CSS px per ms) in the direction it went. */
export const FLICK_MIN_VELOCITY = 0.5
/** The axis is decided ONCE per gesture, after this much movement. */
export const AXIS_LOCK_DISTANCE = 8
/** Past either end of the strip the body follows at this fraction. */
export const RUBBER_BAND = 0.3
/** The motion: a committed body leaves fast, the new one and a spring-back
 *  settle a little slower (ms, ease-out). */
export const FACE_EXIT_MS = 120
export const FACE_ENTER_MS = 200
/** The body a face root's swipe moves — its scrolling content, never the
 *  header band or the bar. */
export const FACE_BODY_SELECTOR = `[data-face-body]`

/** The release rule: a swipe past the distance gate (`swipeDirection`), or a
 *  quick flick — at least `FLICK_MIN_DISTANCE` of clearly horizontal travel
 *  at `FLICK_MIN_VELOCITY` or more, the same way. */
export function releaseDirection(
  dx: number,
  dy: number,
  velocity: number
): SwipeDirection | null {
  const swiped = swipeDirection(dx, dy)
  if (swiped) return swiped
  if (Math.abs(dx) < FLICK_MIN_DISTANCE) return null
  if (Math.abs(dx) <= SWIPE_DOMINANCE * Math.abs(dy)) return null
  if (Math.abs(velocity) < FLICK_MIN_VELOCITY) return null
  if (Math.sign(velocity) !== Math.sign(dx)) return null
  return dx < 0 ? `left` : `right`
}

function reducedMotion(): boolean {
  return (
    typeof window !== `undefined` &&
    typeof window.matchMedia === `function` &&
    window.matchMedia(`(prefers-reduced-motion: reduce)`).matches
  )
}

/** Where the NEXT face's body enters from. Module state on purpose: the
 *  issue route remounts the face component (and the Run face is another
 *  route), so the direction has to outlive the hook that set it. Stale after
 *  a second — a navigation that never landed must not animate a later one. */
let pendingEnter: { direction: SwipeDirection; at: number } | null = null
const PENDING_ENTER_TTL_MS = 1000

/** Announce that the face about to show arrives from a `direction` swipe
 *  (`left` = the next face, entering from the right). A tab tap calls it
 *  from the index delta, the pager on commit. */
export function primeFaceEnter(direction: SwipeDirection): void {
  pendingEnter = { direction, at: Date.now() }
}

function takeFaceEnter(): SwipeDirection | null {
  const pending = pendingEnter
  pendingEnter = null
  if (!pending || Date.now() - pending.at > PENDING_ENTER_TTL_MS) return null
  return pending.direction
}

/** Run `done` once the body's transform transition ends — or after the
 *  duration plus a margin, since a `transitionend` is not guaranteed (a
 *  detached node, a zero-distance move, jsdom). */
function afterTransition(body: HTMLElement, ms: number, done: () => void) {
  let fired = false
  const finish = () => {
    if (fired) return
    fired = true
    body.removeEventListener(`transitionend`, onEnd)
    clearTimeout(timer)
    done()
  }
  const onEnd = (event: Event) => {
    if (event.target === body) finish()
  }
  body.addEventListener(`transitionend`, onEnd)
  const timer = setTimeout(finish, ms + 50)
}

/** Bumped by every transform write, so a deferred clear can tell whether
 *  the body moved on (a NEW drag) since it was scheduled. */
const transformWrites = new WeakMap<HTMLElement, number>()

function setTransform(body: HTMLElement, transform: string, ms: number) {
  transformWrites.set(body, (transformWrites.get(body) ?? 0) + 1)
  body.style.transition = ms > 0 ? `transform ${ms}ms ease-out` : `none`
  body.style.transform = transform
}

function clearTransform(body: HTMLElement) {
  body.style.transition = ``
  body.style.transform = ``
}

/** Settle the body once its transition to rest ends — unless something wrote
 *  its transform in between: a drag that began meanwhile owns the body, and
 *  clearing under it would snap the finger's offset back to 0. */
function clearAfterTransition(body: HTMLElement, ms: number) {
  const write = transformWrites.get(body)
  afterTransition(body, ms, () => {
    if (transformWrites.get(body) === write) clearTransform(body)
  })
}

/** EXP-1152: what a face's ROOT spreads to page its body — the touch
 *  handlers, the root node (to find the body on a face change), and the CSS
 *  that keeps the browser from claiming a sideways pan (React's touch
 *  listeners are passive, so `touch-action` is what stops it) and from
 *  scrolling the page sideways under a translated body. */
export interface FaceSwipeHandlers {
  ref: (node: HTMLElement | null) => void
  style: CSSProperties
  onTouchStart: (event: TouchEvent<HTMLElement>) => void
  onTouchMove: (event: TouchEvent<HTMLElement>) => void
  onTouchEnd: (event: TouchEvent<HTMLElement>) => void
  onTouchCancel: (event: TouchEvent<HTMLElement>) => void
}

const ROOT_STYLE: CSSProperties = {
  touchAction: `pan-y pinch-zoom`,
  overflowX: `clip`,
}

/** The face bodies wear this too: `touch-action` counts only up to the
 *  nearest scroll container, and the body IS one. */
export const FACE_BODY_TOUCH_CLASS = `touch-pan-y touch-pinch-zoom`

interface Gesture {
  x: number
  y: number
  claim: ReturnType<typeof touchClaim>
  body: HTMLElement | null
  axis: `x` | `y` | null
  /** The last two samples, for the release velocity. */
  prev: { x: number; t: number }
  last: { x: number; t: number }
}

/** The pager a face's root element spreads (`FaceSwipeHandlers`): the body
 *  follows a horizontal drag and commits to the neighbour in the strip. */
export function useFaceSwipe(
  faces: readonly WorkFaceKind[],
  face: WorkFaceKind,
  onFace: (face: WorkFaceKind) => void
): FaceSwipeHandlers {
  const root = useRef<HTMLElement | null>(null)
  const gesture = useRef<Gesture | null>(null)
  /** A commit is leaving: touches wait for the next face. */
  const leaving = useRef(false)
  const latest = useRef({ faces, face, onFace })
  latest.current = { faces, face, onFace }

  const ref = useCallback((node: HTMLElement | null) => {
    root.current = node
  }, [])

  // The face changed (or this Work screen just mounted — the issue route
  // remounts per face, and the Run face is another route): settle the body
  // and, when a swipe or a tab tap announced it, slide the new one in from
  // the side the old one left by.
  useEffect(() => {
    leaving.current = false
    const direction = takeFaceEnter()
    const body = root.current?.querySelector<HTMLElement>(FACE_BODY_SELECTOR)
    if (!body) return
    if (!direction || reducedMotion()) {
      clearTransform(body)
      return
    }
    setTransform(body, `translateX(${direction === `left` ? 100 : -100}%)`, 0)
    // Flush the start position before the transition takes over.
    void body.getBoundingClientRect()
    setTransform(body, `translateX(0px)`, FACE_ENTER_MS)
    clearAfterTransition(body, FACE_ENTER_MS)
  }, [face])

  const springBack = (body: HTMLElement | null) => {
    if (!body || !body.style.transform) return
    setTransform(body, `translateX(0px)`, FACE_ENTER_MS)
    clearAfterTransition(body, FACE_ENTER_MS)
  }

  const neighbour = (direction: SwipeDirection) =>
    swipeTarget(latest.current.faces, latest.current.face, direction)

  /** Decide the axis once: a sideways scroller that can still scroll the way
   *  the finger went keeps the gesture (so the page does not follow). */
  const lockAxis = (began: Gesture, dx: number, dy: number) => {
    if (Math.abs(dx) <= Math.abs(dy)) return `y` as const
    const direction: SwipeDirection = dx < 0 ? `left` : `right`
    if (began.claim.scroller && scrollerTookIt(began.claim.scroller, direction)) {
      return `y` as const
    }
    return `x` as const
  }

  return {
    ref,
    style: ROOT_STYLE,
    onTouchStart: (event) => {
      const touch = event.touches[0]
      if (!touch || event.touches.length > 1 || leaving.current) {
        // A second finger is a pinch, never a page turn.
        springBack(gesture.current?.body ?? null)
        gesture.current = null
        return
      }
      const now = event.timeStamp || Date.now()
      const body = event.currentTarget.querySelector<HTMLElement>(
        FACE_BODY_SELECTOR
      )
      gesture.current = {
        x: touch.clientX,
        y: touch.clientY,
        claim: touchClaim(event.target, event.currentTarget),
        body: body && !reducedMotion() ? body : null,
        axis: null,
        prev: { x: touch.clientX, t: now },
        last: { x: touch.clientX, t: now },
      }
    },
    onTouchMove: (event) => {
      const began = gesture.current
      const touch = event.touches[0]
      if (!began || began.claim.field || !touch) return
      if (event.touches.length > 1) {
        springBack(began.body)
        gesture.current = null
        return
      }
      const dx = touch.clientX - began.x
      const dy = touch.clientY - began.y
      if (began.axis === null) {
        if (Math.max(Math.abs(dx), Math.abs(dy)) < AXIS_LOCK_DISTANCE) return
        began.axis = lockAxis(began, dx, dy)
      }
      if (began.axis !== `x`) return
      began.prev = began.last
      began.last = { x: touch.clientX, t: event.timeStamp || Date.now() }
      if (!began.body) return
      const open = neighbour(dx < 0 ? `left` : `right`) !== null
      setTransform(began.body, `translateX(${open ? dx : dx * RUBBER_BAND}px)`, 0)
    },
    onTouchCancel: () => {
      springBack(gesture.current?.body ?? null)
      gesture.current = null
    },
    onTouchEnd: (event) => {
      const began = gesture.current
      gesture.current = null
      const touch = event.changedTouches[0]
      if (!began || began.claim.field || !touch) return
      const dx = touch.clientX - began.x
      const dy = touch.clientY - began.y
      // A lift with no move in between (or under the lock distance) still
      // gets its one axis decision.
      const axis = began.axis ?? lockAxis(began, dx, dy)
      if (axis !== `x`) {
        springBack(began.body)
        return
      }
      const now = event.timeStamp || Date.now()
      const sample = now - began.last.t > 100 ? began.last : began.prev
      const elapsed = Math.max(1, now - sample.t)
      const velocity = (touch.clientX - sample.x) / elapsed
      const direction = releaseDirection(dx, dy, velocity)
      const target = direction ? neighbour(direction) : null
      if (!direction || !target) {
        springBack(began.body)
        return
      }
      const body = began.body
      if (!body) {
        latest.current.onFace(target)
        return
      }
      leaving.current = true
      setTransform(
        body,
        `translateX(${direction === `left` ? -100 : 100}%)`,
        FACE_EXIT_MS
      )
      afterTransition(body, FACE_EXIT_MS, () => {
        const from = latest.current.face
        primeFaceEnter(direction)
        const primed = pendingEnter
        latest.current.onFace(target)
        // The flip never landed (a refused navigation): bring the body back
        // rather than leave the face blank.
        setTimeout(() => {
          if (latest.current.face !== from || !body.isConnected) return
          leaving.current = false
          if (pendingEnter === primed) pendingEnter = null
          springBack(body)
        }, 600)
      })
    },
  }
}

/** The toggle speaks `diff` for the changes face (`WorkFace`); the phone's
 *  rules speak `changes` (`WorkFaceKind`). */
function toToggleFace(face: WorkFaceKind): WorkFace {
  return face === `changes` ? `diff` : face
}

export interface MobileFaceTabsProps {
  /** The faces this subject has (`availableFaces`), in order. */
  faces: readonly WorkFaceKind[]
  /** The face on show. */
  face: WorkFaceKind
  /** EXP-1162: the tabs' state (`faceDots`), and the run whose brand mark
   *  the Run tab wears. */
  dots?: Partial<Record<WorkFaceKind, FaceDotTone>>
  run?: WorkFaceStripRun
  /** The issue's runs of mine (`useIssueRuns`) — two or more turn the Run
   *  segment into `Runs` with the caret to the run menu (EXP-950). */
  runs?: readonly PastRunRow[]
  /** The run the Run face shows (or would show): the menu's check. */
  viewedRunId?: string | null
  /** EXP-1152: the Changes face's `+N −M` (`changesFaceCounts` of the files
   *  it draws); absent / null = the word `Changes`. */
  changesCounts?: ChangesFaceCounts | null
  onFace: (face: WorkFaceKind) => void
  onOpenRun?: (session: CodingSession) => void
  /** EXP-1150: the band's Merge PR pill (`MergePrPill placement="header"`),
   *  at the row's trailing end on every face while the PR is open — the ONE
   *  merge of the phone Work screen. The strip gives way to it. */
  trailing?: ReactNode
}

export function MobileFaceTabs({
  faces,
  face,
  dots,
  run,
  runs = [],
  viewedRunId = null,
  changesCounts = null,
  onFace,
  onOpenRun,
  trailing,
}: MobileFaceTabsProps) {
  const multipleRuns = runs.length > 1
  const shownIndex = faces.indexOf(face)
  const items: WorkFaceItem[] = faces.map((kind, index) => ({
    face: toToggleFace(kind),
    label:
      kind === `issue` ? (
        ISSUE_FACE_LABEL
      ) : kind === `run` ? (
        runFaceLabel(multipleRuns)
      ) : kind === `changes` ? (
        <ChangesFaceLabel counts={changesCounts} />
      ) : (
        RESULTS_FACE_LABEL
      ),
    onSelect: () => {
      // EXP-1152: a tap pages too — the new face slides in from the side
      // its tab sits on.
      if (shownIndex >= 0 && index !== shownIndex) {
        primeFaceEnter(index > shownIndex ? `left` : `right`)
      }
      onFace(kind)
    },
  }))
  // The toggle itself renders nothing under two segments (unless the Runs
  // caret alone earns it); the row must vanish with it or it leaves a gap.
  const hasRunMenu = multipleRuns && faces.includes(`run`) && Boolean(onOpenRun)
  const hasTabs = items.length >= 2 || hasRunMenu
  const hasTrailing = trailing !== undefined && trailing !== null
  if (!hasTabs && !hasTrailing) return null
  return (
    <div
      className={cn(
        `flex shrink-0 items-center gap-2 px-4 pb-2`,
        hasTrailing ? `justify-between` : `justify-center`
      )}
      data-testid="mobile-face-tabs"
    >
      <div
        className={cn(
          `flex min-w-0 overflow-x-auto [scrollbar-width:none]`,
          hasTrailing ? `justify-start` : `justify-center`
        )}
      >
      <WorkFaceToggle
        face={toToggleFace(face)}
        items={items}
        dots={dots ? toggleFaceDots(dots) : undefined}
        run={run}
        runMenu={
          onOpenRun
            ? {
                runs,
                checkedRunId: viewedRunId ?? undefined,
                onOpen: (target) => {
                  if (target.id !== viewedRunId) onOpenRun(target)
                },
              }
            : undefined
        }
      />
      </div>
      {trailing}
    </div>
  )
}
