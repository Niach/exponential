import type { EditorView } from "@tiptap/pm/view"

// EXP-198: mobile on-screen keyboards OVERLAY the layout viewport instead of
// resizing it (the `interactive-widget=resizes-visual` default on Android
// Chrome; always on iOS Safari), so `100dvh`/`h-full` layouts keep their full
// height and the bottom of every scroll region sits behind the keyboard.
// ProseMirror's built-in scrollIntoView works in layout-viewport coordinates
// and therefore believes a caret under the keyboard is perfectly visible —
// typing past the fold leaves the user writing blind. The helpers here track
// the VISUAL viewport (which the keyboard does shrink) and scroll the caret
// back above the keyboard, temporarily extending a scroller with bottom
// padding when the content itself ends behind the keyboard and there is no
// scroll room left.

/** Keyboard heights start well above this; URL-bar show/hide resizes the
 *  layout viewport itself and stays near 0. */
const KEYBOARD_MIN_OCCLUSION = 80
/** Clearance under the caret so the line being written (and a hint of the
 *  next one) stays visible above the keyboard. */
const CARET_BOTTOM_MARGIN = 56
const CARET_TOP_MARGIN = 12

/** Pixels of the layout viewport hidden behind the on-screen keyboard. */
export function keyboardOcclusion(
  layoutViewportHeight: number,
  visualOffsetTop: number,
  visualHeight: number
) {
  return Math.max(0, layoutViewportHeight - (visualOffsetTop + visualHeight))
}

/** How far scrollTop must change (positive = scroll down) so the caret sits
 *  inside the visible band with margin clearance. Never scrolls the caret's
 *  top edge out of view while revealing its bottom. */
export function caretScrollDelta({
  caretTop,
  caretBottom,
  visibleTop,
  visibleBottom,
  topMargin = CARET_TOP_MARGIN,
  bottomMargin = CARET_BOTTOM_MARGIN,
}: {
  caretTop: number
  caretBottom: number
  visibleTop: number
  visibleBottom: number
  topMargin?: number
  bottomMargin?: number
}) {
  const overflowBelow = caretBottom - (visibleBottom - bottomMargin)
  if (overflowBelow > 0) {
    return Math.min(overflowBelow, Math.max(0, caretTop - visibleTop))
  }
  const overflowAbove = visibleTop + topMargin - caretTop
  if (overflowAbove > 0) return -overflowAbove
  return 0
}

interface AppliedClearance {
  priorInline: string
  basePx: number
  clearancePx: number
}

// A reveal may spread clearance across a scroll chain (a capped inner region
// cascading to the document), so track one record per padded element.
const applied = new Map<HTMLElement, AppliedClearance>()

/** Restore any keyboard bottom padding added by revealCaretAboveKeyboard. */
export function releaseKeyboardClearance() {
  for (const [el, record] of applied) {
    el.style.paddingBottom = record.priorInline
  }
  applied.clear()
}

/** overflow-y scroll regions from the caret outward, ending at the document
 *  scroller. Which of them actually clips is probed at scroll time — a
 *  `100dvh` flex chain often leaves an `overflow-y-auto` region growing with
 *  its content (never clipping) while the document does the real scrolling. */
function scrollCandidates(start: HTMLElement): HTMLElement[] {
  const out: HTMLElement[] = []
  let node = start.parentElement
  while (node) {
    const overflowY = getComputedStyle(node).overflowY
    if (overflowY === `auto` || overflowY === `scroll`) out.push(node)
    node = node.parentElement
  }
  const docEl = (document.scrollingElement ??
    document.documentElement) as HTMLElement
  if (!out.includes(docEl)) out.push(docEl)
  return out
}

/** Change scrollTop by up to `delta` (browser-clamped); returns the applied
 *  amount. */
function scrollElementBy(el: HTMLElement, delta: number) {
  const before = el.scrollTop
  el.scrollTop = before + delta
  return el.scrollTop - before
}

/** Try to create `neededPx` of extra scroll room below the content by
 *  growing bottom padding. Returns false — reverting the attempt — when the
 *  element does not actually clip (padding then grows the element instead of
 *  its overflow, e.g. an `overflow-y-auto` region whose flex chain never
 *  resolved a definite height). */
function tryAddClearance(
  scroller: HTMLElement,
  neededPx: number,
  maxClearancePx: number
) {
  // Padding on <html> is unreliable for extending the document scroll area —
  // pad <body> when the document itself is the scroller.
  const padTarget =
    scroller === document.scrollingElement ||
    scroller === document.documentElement
      ? document.body
      : scroller
  const record = applied.get(padTarget)
  const currentPx = record?.clearancePx ?? 0
  // The caret can never sit more than one keyboard below the visible band,
  // so clearance beyond occlusion + margin means something else is wrong —
  // stop growing.
  const targetPx = Math.min(currentPx + neededPx, maxClearancePx)
  if (targetPx <= currentPx) return false
  const priorInline = record?.priorInline ?? padTarget.style.paddingBottom
  const basePx =
    record?.basePx ??
    (Number.parseFloat(getComputedStyle(padTarget).paddingBottom) || 0)
  const overflowBefore = scroller.scrollHeight - scroller.clientHeight
  padTarget.style.paddingBottom = `${basePx + targetPx}px`
  if (scroller.scrollHeight - scroller.clientHeight <= overflowBefore) {
    padTarget.style.paddingBottom = record
      ? `${basePx + currentPx}px`
      : priorInline
    return false
  }
  applied.set(padTarget, { priorInline, basePx, clearancePx: targetPx })
  return true
}

/** While the keyboard occludes the layout viewport, scroll the caret's
 *  scroll chain so the caret stays visible above the keyboard. */
export function revealCaretAboveKeyboard(view: EditorView) {
  const vv = window.visualViewport
  if (!vv) return
  const occlusion = keyboardOcclusion(
    window.innerHeight,
    vv.offsetTop,
    vv.height
  )
  if (occlusion < KEYBOARD_MIN_OCCLUSION) {
    releaseKeyboardClearance()
    return
  }
  let caret: { top: number; bottom: number }
  try {
    // Client (layout-viewport) coordinates — the same space visualViewport's
    // offsetTop/height describe the visible band in.
    caret = view.coordsAtPos(view.state.selection.head)
  } catch {
    return
  }
  let remaining = caretScrollDelta({
    caretTop: caret.top,
    caretBottom: caret.bottom,
    visibleTop: vv.offsetTop,
    visibleBottom: vv.offsetTop + vv.height,
  })
  if (remaining === 0) return
  const maxClearancePx = occlusion + CARET_BOTTOM_MARGIN
  for (const scroller of scrollCandidates(view.dom)) {
    remaining -= scrollElementBy(scroller, remaining)
    if (Math.abs(remaining) < 1) return
    if (remaining > 0 && tryAddClearance(scroller, remaining, maxClearancePx)) {
      remaining -= scrollElementBy(scroller, remaining)
      if (Math.abs(remaining) < 1) return
    }
  }
}
