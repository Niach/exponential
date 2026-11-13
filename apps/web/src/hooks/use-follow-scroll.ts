import { useCallback, useEffect, useRef, useState, type RefObject } from "react"
import { CHATS_TRANSCRIPT } from "@exp/domain-contract/chats"

// EXP-1316 (1n): THE bottom-anchored feed's scroll, ×web — the session view's
// transcript and a chat's Conversation (a chat is basically a session):
// it opens at the newest row, stays pinned there while rows land unless the
// reader scrolled up, and `jumpToBottom` (THE `JumpToBottomButton`) returns.
// EXP-440: rows finish laying out AFTER the pin effect (deferred markdown,
// decoded images), so every growth of the content column re-pins too.

/** Within this many px of the tail counts as "at the bottom"
 *  (chats.json `transcript.followSlack`, #EXP-1383). */
export const FOLLOW_SCROLL_SLACK = CHATS_TRANSCRIPT.followSlack

export interface FollowScroll {
  /** The scroller. */
  scrollRef: RefObject<HTMLDivElement | null>
  /** The content column inside it (its growth re-pins). */
  contentRef: (node: HTMLDivElement | null) => void
  /** Pinned to the newest row (the jump button hides). */
  atBottom: boolean
  /** Scrolled off the top (the top edge fade). */
  scrolled: boolean
  onScroll: () => void
  /** Back to the newest row, pinned again. */
  jumpToBottom: () => void
  /** waiting-cards.json DEEP LINKS: while a deep link is pending the feed
   *  never follows its newest row (no pin on arrival or growth); release
   *  leaves the reader where the link put them. */
  hold: (on: boolean) => void
}

/** `deps` = what makes rows land (the pin effect re-runs on each change). */
export function useFollowScroll(deps: readonly unknown[]): FollowScroll {
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const [atBottom, setAtBottom] = useState(true)
  const [scrolled, setScrolled] = useState(false)

  // #EXP-1383: the re-pin below reads this ref, written on the scroll event
  // itself: a reader who scrolled up is never yanked back by a row that
  // grows before React commits the state (a busy main thread).
  const atBottomRef = useRef(true)
  // #EXP-1407: a pin's scroll event can land after the content grew again
  // (a Dock band measuring in, rows streaming); only the reader scrolling UP
  // leaves the tail, never a scroll that moved down or stayed.
  const lastTopRef = useRef(0)
  const scrolledRef = useRef(false)
  // DEEP LINKS: a pending link holds the feed off its tail.
  const heldRef = useRef(false)
  // A mouse press in the feed (the reader drag-selecting text) never has the
  // rows moved under it: no pin while the button is down.
  const pressedRef = useRef(false)
  const hold = useCallback((on: boolean) => {
    if (heldRef.current === on) return
    heldRef.current = on
    if (!on) return
    if (atBottomRef.current) setAtBottom(false)
    atBottomRef.current = false
  }, [])
  const onScroll = useCallback(() => {
    const el = scrollRef.current
    if (!el) return
    const up = el.scrollTop < lastTopRef.current
    lastTopRef.current = el.scrollTop
    if (heldRef.current) {
      const scrolledNow = el.scrollTop > 0
      if (scrolledNow !== scrolledRef.current) {
        scrolledRef.current = scrolledNow
        setScrolled(scrolledNow)
      }
      return
    }
    const atTail = el.scrollHeight - el.scrollTop - el.clientHeight < FOLLOW_SCROLL_SLACK
    const bottom = atTail || (atBottomRef.current && !up)
    // Still following but short of the tail: the content outgrew the pin.
    if (bottom && !atTail && !pressedRef.current) {
      el.scrollTop = el.scrollHeight
      lastTopRef.current = el.scrollTop
    }
    // Set state only on a CHANGE: a same-value set still re-rendered the
    // whole transcript on every scroll event (62 commits per 60 wheels).
    if (bottom !== atBottomRef.current) setAtBottom(bottom)
    atBottomRef.current = bottom
    const scrolledNow = el.scrollTop > 0
    if (scrolledNow !== scrolledRef.current) {
      scrolledRef.current = scrolledNow
      setScrolled(scrolledNow)
    }
  }, [])

  const jumpToBottom = useCallback(() => {
    const el = scrollRef.current
    if (el) {
      if (typeof el.scrollTo === `function`) el.scrollTo({ top: el.scrollHeight })
      else el.scrollTop = el.scrollHeight
      lastTopRef.current = el.scrollTop
    }
    heldRef.current = false
    atBottomRef.current = true
    setAtBottom(true)
  }, [])

  const observerRef = useRef<ResizeObserver | null>(null)
  const unpressRef = useRef<(() => void) | null>(null)
  const contentRef = useCallback((node: HTMLDivElement | null) => {
    observerRef.current?.disconnect()
    observerRef.current = null
    unpressRef.current?.()
    unpressRef.current = null
    pressedRef.current = false
    if (!node) return
    const pressScroller = scrollRef.current ?? node.parentElement
    if (pressScroller) {
      // A press on the scroller itself is its scrollbar: that one scrolls.
      const press = (event: PointerEvent) => {
        if (event.pointerType === `mouse` && event.button === 0 && event.target !== pressScroller) pressedRef.current = true
      }
      const release = () => {
        pressedRef.current = false
      }
      pressScroller.addEventListener(`pointerdown`, press, { passive: true })
      document.addEventListener(`pointerup`, release, { passive: true })
      document.addEventListener(`pointercancel`, release, { passive: true })
      unpressRef.current = () => {
        pressScroller.removeEventListener(`pointerdown`, press)
        document.removeEventListener(`pointerup`, release)
        document.removeEventListener(`pointercancel`, release)
      }
    }
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(() => {
      if (!atBottomRef.current || heldRef.current || pressedRef.current) return
      const el = scrollRef.current
      if (el) {
        el.scrollTop = el.scrollHeight
        lastTopRef.current = el.scrollTop
      }
    })
    // The border box: a column's bottom padding grows it too.
    observer.observe(node, { box: `border-box` })
    // #EXP-1782: the scroller ends at the Dock band's top edge, so a band
    // that grows (a card lands, the composer grows a line) SHRINKS the
    // scroller: re-pin then too, so the last row stays fully visible just
    // above the composer.
    const scroller = scrollRef.current ?? node.parentElement
    if (scroller) observer.observe(scroller)
    observerRef.current = observer
  }, [])
  useEffect(
    () => () => {
      observerRef.current?.disconnect()
      unpressRef.current?.()
    },
    []
  )

  useEffect(() => {
    if (!atBottom || heldRef.current || pressedRef.current) return
    const el = scrollRef.current
    if (el) {
      el.scrollTop = el.scrollHeight
      lastTopRef.current = el.scrollTop
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [atBottom, ...deps])

  return { scrollRef, contentRef, atBottom, scrolled, onScroll, jumpToBottom, hold }
}
