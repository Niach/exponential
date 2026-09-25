import { useEffect, useRef, type RefObject } from "react"
import {
  findIssueMenuElement,
  issueMenuElementAt,
  type IssueMenuHit,
} from "./attr"

// EXP-1074 — the gestures that open THE issue context menu, as document
// listeners: a right-click (or the keyboard's context-menu key) on an opted-in
// element, and a touch long-press on one — Radix's `ContextMenuTrigger` rules
// (700 ms, cancelled by a slide of more than a few px), minus the trigger.

export interface IssueMenuAnchor {
  x: number
  y: number
  /** 0×0 at the pointer; the element's box for a keyboard-invoked menu. */
  width: number
  height: number
}

export interface IssueMenuTarget {
  issueId: string
  from?: string
  /** The element the gesture landed on; focus returns here on close. */
  origin: HTMLElement | null
  anchor: IssueMenuAnchor
}

/** Radix's `ContextMenuTrigger` long-press. */
const LONG_PRESS_MS = 700
const LONG_PRESS_SLOP_PX = 10
/** How long after a `contextmenu` a `click` on the same element is the
 *  gesture's tail rather than a new click. */
const CONTEXT_CLICK_TAIL_MS = 100

function isEditable(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    target.closest(
      `input, textarea, [contenteditable=""], [contenteditable="true"]`
    ) !== null
  )
}

function isTouchOrPen(event: PointerEvent): boolean {
  return event.pointerType === `touch` || event.pointerType === `pen`
}

/** `<html>`/`<body>` as the target means a modal layer owns the pointer. */
function isRootTarget(target: EventTarget | null): boolean {
  return target === document.documentElement || target === document.body
}

function targetOf(
  hit: IssueMenuHit,
  point: { x: number; y: number }
): IssueMenuTarget {
  const rect = hit.element.getBoundingClientRect()
  const atPointer =
    point.x >= rect.left &&
    point.x <= rect.right &&
    point.y >= rect.top &&
    point.y <= rect.bottom
  return {
    issueId: hit.issueId,
    ...(hit.from ? { from: hit.from } : {}),
    origin: hit.element,
    anchor: atPointer
      ? { x: point.x, y: point.y, width: 0, height: 0 }
      : { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
  }
}

export function useIssueMenuGestures(
  onOpen: (target: IssueMenuTarget) => void,
  openRef: RefObject<boolean>
): void {
  const onOpenRef = useRef(onOpen)
  onOpenRef.current = onOpen

  useEffect(() => {
    let timer: number | null = null
    let press: { x: number; y: number; hit: IssueMenuHit } | null = null
    const clearPress = () => {
      if (timer !== null) {
        window.clearTimeout(timer)
        timer = null
      }
      press = null
    }

    // A macOS ctrl+click is a right-click, and Firefox follows the
    // `contextmenu` with a `click` on the same element — which would open the
    // row under the menu that just appeared. The tail is swallowed HERE, at
    // the document in the capture phase, for one click within the tail
    // window; a row-side `ctrlKey` check would make ctrl+click a dead click
    // on Windows and Linux, where it is an ordinary modifier.
    let tail: { element: HTMLElement; onClick: (event: MouseEvent) => void; timer: number } | null = null
    const clearTail = () => {
      if (!tail) return
      document.removeEventListener(`click`, tail.onClick, { capture: true })
      window.clearTimeout(tail.timer)
      tail = null
    }
    const swallowClickTail = (element: HTMLElement) => {
      clearTail()
      const onClick = (click: MouseEvent) => {
        if (!(click.target instanceof Node) || !element.contains(click.target)) {
          return
        }
        click.stopPropagation()
        click.preventDefault()
        clearTail()
      }
      tail = {
        element,
        onClick,
        timer: window.setTimeout(clearTail, CONTEXT_CLICK_TAIL_MS),
      }
      document.addEventListener(`click`, onClick, { capture: true })
    }

    const openFromContextMenu = (hit: IssueMenuHit, point: { x: number; y: number }) => {
      onOpenRef.current(targetOf(hit, point))
      swallowClickTail(hit.element)
    }

    const onContextMenu = (event: MouseEvent) => {
      clearPress()
      if (event.defaultPrevented) return
      const point = { x: event.clientX, y: event.clientY }
      const layered = openRef.current || isRootTarget(event.target)
      if (layered) {
        // Ours is up (or still fading out): never the browser's menu on top
        // of it, and never the item under the release. A right-click on
        // another row moves the menu there — the rows are found by geometry,
        // since the browser hit-tests to <html> while the body is inert.
        event.preventDefault()
        if (event.target instanceof Element && event.target.closest(`[role="menu"]`)) {
          return
        }
        const hit =
          findIssueMenuElement(event.target) ?? issueMenuElementAt(point.x, point.y)
        if (hit) openFromContextMenu(hit, point)
        return
      }
      if (isEditable(event.target)) return
      const hit = findIssueMenuElement(event.target)
      if (!hit) return
      event.preventDefault()
      openFromContextMenu(hit, point)
    }

    const onPointerDown = (event: PointerEvent) => {
      if (!isTouchOrPen(event)) return
      const hit = findIssueMenuElement(event.target)
      if (!hit) return
      clearPress()
      press = { x: event.clientX, y: event.clientY, hit }
      timer = window.setTimeout(() => {
        const current = press
        clearPress()
        if (current) {
          onOpenRef.current(
            targetOf(current.hit, { x: current.x, y: current.y })
          )
        }
      }, LONG_PRESS_MS)
    }

    const onPointerMove = (event: PointerEvent) => {
      if (!press || !isTouchOrPen(event)) return
      if (
        Math.hypot(event.clientX - press.x, event.clientY - press.y) >
        LONG_PRESS_SLOP_PX
      ) {
        clearPress()
      }
    }

    const onPointerEnd = () => clearPress()

    document.addEventListener(`contextmenu`, onContextMenu)
    document.addEventListener(`pointerdown`, onPointerDown, { passive: true })
    document.addEventListener(`pointermove`, onPointerMove, { passive: true })
    document.addEventListener(`pointerup`, onPointerEnd, { passive: true })
    document.addEventListener(`pointercancel`, onPointerEnd, { passive: true })
    return () => {
      clearPress()
      clearTail()
      document.removeEventListener(`contextmenu`, onContextMenu)
      document.removeEventListener(`pointerdown`, onPointerDown)
      document.removeEventListener(`pointermove`, onPointerMove)
      document.removeEventListener(`pointerup`, onPointerEnd)
      document.removeEventListener(`pointercancel`, onPointerEnd)
    }
  }, [openRef])
}
