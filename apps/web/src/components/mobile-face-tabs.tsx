import { useRef, type ReactNode, type TouchEvent } from "react"
import { cn } from "@/lib/utils"
import type { CodingSession } from "@/db/schema"
import type { PastRunRow } from "@/hooks/use-agents-data"
import {
  CHANGES_FACE_LABEL,
  swipeTarget,
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
// (`WorkFaceToggle`: the Inbox / My issues pill), listing every available
// face in its fixed order — Issue · Run/Runs · Changes · Results — and the
// `Runs` segment keeps its caret to the run menu (EXP-950). It replaces the
// bottom-right switcher circle of EXP-893, whose one-or-menu behaviour read
// as a toggle nobody could predict. The strip is absent with a single face
// (nothing to switch), and it never moves between faces: every face renders
// it INSIDE the header band, under the title row and over the band's one
// hairline (`MobileDetailHeader` `below`), so title and tabs read as one
// header. iOS `WorkScreen` and Android
// `WorkScreen` draw their `GlassSegmentedControl` in the same place.
//
// A horizontal swipe on the face's body walks the strip (`swipeTarget`):
// the content follows the finger, so a swipe LEFT opens the next face and a
// swipe RIGHT the previous one. `useFaceSwipe` is the touch handler pair a
// face's root spreads; it decides on release, never on move, so vertical
// scrolling and text selection are left alone, and a sideways scroller (a
// code line of the diff) keeps the swipe only while it can still scroll that
// way — past its edge the page turns, the pager rule.

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

export interface FaceSwipeHandlers {
  onTouchStart: (event: TouchEvent<HTMLElement>) => void
  onTouchEnd: (event: TouchEvent<HTMLElement>) => void
}

/** The handler pair a face's root element spreads to make the body
 *  swipeable between the strip's faces. */
export function useFaceSwipe(
  faces: readonly WorkFaceKind[],
  face: WorkFaceKind,
  onFace: (face: WorkFaceKind) => void
): FaceSwipeHandlers {
  const start = useRef<{
    x: number
    y: number
    claim: ReturnType<typeof touchClaim>
  } | null>(null)
  return {
    onTouchStart: (event) => {
      const touch = event.touches[0]
      if (!touch || event.touches.length > 1) {
        start.current = null
        return
      }
      start.current = {
        x: touch.clientX,
        y: touch.clientY,
        claim: touchClaim(event.target, event.currentTarget),
      }
    },
    onTouchEnd: (event) => {
      const began = start.current
      start.current = null
      const touch = event.changedTouches[0]
      if (!began || began.claim.field || !touch) return
      const direction = swipeDirection(
        touch.clientX - began.x,
        touch.clientY - began.y
      )
      if (!direction) return
      if (began.claim.scroller && scrollerTookIt(began.claim.scroller, direction)) {
        return
      }
      const target = swipeTarget(faces, face, direction)
      if (target) onFace(target)
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
  /** The issue's runs of mine (`useIssueRuns`) — two or more turn the Run
   *  segment into `Runs` with the caret to the run menu (EXP-950). */
  runs?: readonly PastRunRow[]
  /** The run the Run face shows (or would show): the menu's check. */
  viewedRunId?: string | null
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
  runs = [],
  viewedRunId = null,
  onFace,
  onOpenRun,
  trailing,
}: MobileFaceTabsProps) {
  const multipleRuns = runs.length > 1
  const items: WorkFaceItem[] = faces.map((kind) => ({
    face: toToggleFace(kind),
    label:
      kind === `issue`
        ? ISSUE_FACE_LABEL
        : kind === `run`
          ? runFaceLabel(multipleRuns)
          : kind === `changes`
            ? CHANGES_FACE_LABEL
            : RESULTS_FACE_LABEL,
    onSelect: () => onFace(kind),
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
