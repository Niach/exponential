import {
  useCallback,
  useEffect,
  useRef,
  type CSSProperties,
  type ReactNode,
  type TouchEvent,
} from "react"
import {
  WorkFaceStrip,
  type WorkFaceStripDots,
  type WorkFaceStripRun,
} from "@exp/ui"
import { IssueRunMenuContent } from "@/components/issue-run-switcher"
import type { CodingSession } from "@/db/schema"
import type { PastRunRow } from "@/hooks/use-agents-data"
import {
  faceLabel,
  swipeTarget,
  type FaceDotTone,
  type SwipeDirection,
  type WorkFaceKind,
} from "@/lib/work-faces"

// EXP-1251: THE work faces of the web, one file: the segmented strip every
// Work screen wears (`WorkFaceToggle`, the md+ work header's and the phone's
// header band's, both `@exp/ui` `WorkFaceStrip`), the phone's tab row under
// the header (`MobileFaceTabs`) and the phone PAGER the face bodies (and
// every other phone tab strip, EXP-1190) swipe with (`useFaceSwipe`). The
// faces are Issue · Run/Runs · Guide in that fixed order (`availableFaces`);
// an unavailable face is HIDDEN, never disabled, and the strip is absent
// under two faces unless the `Runs` caret earns it (EXP-950). EXP-974: an
// issue-less run's menu lists its RESUME CHAIN. EXP-1024: a NULL face leaves
// every segment inactive (the workflow node panel). EXP-1162: the tabs carry
// the state (the Run tab's mark, the Guide's open-PR dot), never the title.
//
// EXP-1152: the phone body is a PAGER. iOS pages with `FacePager` and
// Android with `HorizontalPager`; on the web the faces are separate route
// trees (Run is another route), so a neighbour cannot be pre-mounted.
// Instead the face's BODY (`[data-face-body]`) follows the finger —
// `translateX` written straight onto the node, no React render per move,
// rubber-banded where there is no neighbour — and on release it either
// springs back or leaves to the side it was thrown, the face flips, and the
// NEW body slides in from the other side (`primeFaceEnter`). The header band
// and the fixed bottom bar never move. A sideways scroller (a code line of
// the diff) keeps the gesture only while it can still scroll that way, and a
// text field keeps every gesture.

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

/** A tab TAP pages too: the new tab slides in from the side it sits on. */
export function primeTabEnter<T>(tabs: readonly T[], from: T, to: T): void {
  const shown = tabs.indexOf(from)
  const next = tabs.indexOf(to)
  if (shown < 0 || next < 0 || shown === next) return
  primeFaceEnter(next > shown ? `left` : `right`)
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

/** EXP-1190: a tab body whose lists scroll INSIDE it (My Work) hands the
 *  same rule down to those scrollers. */
export const TAB_BODY_TOUCH_CLASS = `${FACE_BODY_TOUCH_CLASS} [&_.overflow-y-auto]:touch-pan-y [&_.overflow-y-auto]:touch-pinch-zoom [&_.overflow-auto]:touch-pan-y [&_.overflow-auto]:touch-pinch-zoom`

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
 *  follows a horizontal drag and commits to the neighbour in the strip. Any
 *  phone tab strip pages with it (EXP-1190): `faces` = its tabs in order. */
export function useFaceSwipe<T>(
  faces: readonly T[],
  face: T,
  onFace: (face: T) => void
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

/** Byte-identical with the IDE (`lib/work-faces.ts` owns the words). */
export { ISSUE_FACE_LABEL, RUN_FACE_LABEL, RUNS_FACE_LABEL, GUIDE_FACE_LABEL } from "@/lib/work-faces"

export function runFaceLabel(multipleRuns: boolean): string {
  return faceLabel(`run`, multipleRuns)
}

export interface WorkFaceItem {
  face: WorkFaceKind
  label: ReactNode
  onSelect: () => void
}

/** EXP-950: the runs behind the `Runs` segment's caret. */
export interface WorkFaceRunMenu {
  /** `useIssueRuns` rows (an issue-less run: `useRunChain`, EXP-974) — the
   *  caret exists from two up. */
  runs: readonly PastRunRow[]
  checkedRunId?: string
  onOpen: (session: CodingSession) => void
}

/** The md+ work header's strip (and, under `MobileFaceTabs`, the phone's):
 *  `WorkFaceStrip` with the app's run menu plugged in. */
export function WorkFaceToggle({
  face,
  items,
  runMenu,
  dots,
  run,
}: {
  /** The face on show; `null` = none (every segment inactive). */
  face: WorkFaceKind | null
  items: readonly WorkFaceItem[]
  runMenu?: WorkFaceRunMenu
  /** EXP-1162: the segments' state (`faceDots`). */
  dots?: Partial<Record<WorkFaceKind, FaceDotTone>>
  /** EXP-1162: the run whose brand mark the Run tab wears. */
  run?: WorkFaceStripRun
}) {
  return (
    <WorkFaceStrip
      face={face}
      items={items}
      dots={dots as WorkFaceStripDots | undefined}
      run={run}
      runMenu={
        runMenu && runMenu.runs.length > 1
          ? {
              content: (
                <IssueRunMenuContent
                  runs={runMenu.runs}
                  checkedRunId={runMenu.checkedRunId}
                  onOpen={runMenu.onOpen}
                />
              ),
            }
          : undefined
      }
    />
  )
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
  onFace: (face: WorkFaceKind) => void
  onOpenRun?: (session: CodingSession) => void
}

/** EXP-1150: the phone's face tabs, centred inside the header band (Merge
 *  rides the floating bar, never this row). */
export function MobileFaceTabs({
  faces,
  face,
  dots,
  run,
  runs = [],
  viewedRunId = null,
  onFace,
  onOpenRun,
}: MobileFaceTabsProps) {
  const multipleRuns = runs.length > 1
  const items: WorkFaceItem[] = faces.map((kind) => ({
    face: kind,
    label: faceLabel(kind, multipleRuns),
    onSelect: () => {
      // EXP-1152: a tap pages too.
      primeTabEnter(faces, face, kind)
      onFace(kind)
    },
  }))
  // The toggle renders nothing under two segments (unless the Runs caret
  // alone earns it); the row must vanish with it or it leaves a gap.
  const hasRunMenu = multipleRuns && faces.includes(`run`) && Boolean(onOpenRun)
  if (items.length < 2 && !hasRunMenu) return null
  return (
    <div
      className="flex shrink-0 items-center justify-center gap-2 px-4 pb-2"
      data-testid="mobile-face-tabs"
    >
      <div className="flex min-w-0 justify-center overflow-x-auto [scrollbar-width:none]">
        <WorkFaceToggle
          face={face}
          items={items}
          dots={dots}
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
    </div>
  )
}
