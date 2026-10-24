import { useCallback, useEffect, useRef, useState, type RefObject } from "react"
import { CHATS_TRANSCRIPT } from "@exp/domain-contract/chats"

// EXP-1316 (1n): THE bottom-anchored feed's scroll, ×web — the session view's
// transcript and a thread's Conversation (a thread is basically a session):
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
  const onScroll = useCallback(() => {
    const el = scrollRef.current
    if (!el) return
    const bottom = el.scrollHeight - el.scrollTop - el.clientHeight < FOLLOW_SCROLL_SLACK
    atBottomRef.current = bottom
    setAtBottom(bottom)
    setScrolled(el.scrollTop > 0)
  }, [])

  const jumpToBottom = useCallback(() => {
    const el = scrollRef.current
    if (el) {
      if (typeof el.scrollTo === `function`) el.scrollTo({ top: el.scrollHeight })
      else el.scrollTop = el.scrollHeight
    }
    atBottomRef.current = true
    setAtBottom(true)
  }, [])

  const observerRef = useRef<ResizeObserver | null>(null)
  const contentRef = useCallback((node: HTMLDivElement | null) => {
    observerRef.current?.disconnect()
    observerRef.current = null
    if (!node || typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(() => {
      if (!atBottomRef.current) return
      const el = scrollRef.current
      if (el) el.scrollTop = el.scrollHeight
    })
    observer.observe(node)
    observerRef.current = observer
  }, [])
  useEffect(() => () => observerRef.current?.disconnect(), [])

  useEffect(() => {
    if (!atBottom) return
    const el = scrollRef.current
    if (el) el.scrollTop = el.scrollHeight
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [atBottom, ...deps])

  return { scrollRef, contentRef, atBottom, scrolled, onScroll, jumpToBottom }
}
