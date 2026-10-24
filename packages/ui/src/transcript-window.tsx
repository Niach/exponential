import {
  memo,
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ComponentProps,
  type ReactNode,
  type RefObject,
} from "react"
import { CHATS_COPY, CHATS_TEST_IDS, CHATS_TRANSCRIPT } from "@exp/domain-contract/chats"
import { Button } from "./button"
import { cn } from "./cn"

// #EXP-1380 (chats.json WINDOW + PERF ×4): a chat paints its NEWEST
// `transcript.window` rows; `Load earlier` (the run view's ghost button)
// widens the window by `transcript.windowStep` keeping the reader's place,
// and a new arrival never shrinks it. Rows render once per change of their
// own data (`StableRow`).

/** THE `Load earlier` ghost button above a windowed transcript (the run
 *  view's and every chat's). */
export function LoadEarlierButton({ className, ...props }: Omit<ComponentProps<typeof Button>, `children`>) {
  return (
    <div className="flex justify-center pb-1">
      <Button
        variant="ghost"
        size="sm"
        data-testid={CHATS_TEST_IDS.loadEarlier}
        className={cn(`h-7 text-xs text-muted-foreground`, className)}
        {...props}
      >
        {CHATS_COPY.loadEarlier}
      </Button>
    </div>
  )
}

/** How long a Load earlier holds the reader's place through late layout. */
const ANCHOR_HOLD_MS = 2_000
const RELEASE_EVENTS = [`wheel`, `touchstart`, `keydown`, `pointerdown`] as const

export interface TranscriptWindow<T> {
  /** The rows to paint (the newest window, widened by Load earlier). */
  rows: readonly T[]
  /** Older rows exist above the window. */
  canLoadEarlier: boolean
  /** Widen by one step; the reader's distance from the bottom is kept. */
  loadEarlier: () => void
}

/** The window over a chat's rows. `scrollRef` = the transcript's scroller
 *  (its position is kept across a Load earlier). The window's OLDEST row is
 *  pinned by id at the first paint, so later arrivals only add rows. */
export function useTranscriptWindow<T extends { id: string }>(
  rows: readonly T[],
  scrollRef: RefObject<HTMLElement | null>,
  size: number = CHATS_TRANSCRIPT.window,
  step: number = CHATS_TRANSCRIPT.windowStep
): TranscriptWindow<T> {
  const [fromId, setFromId] = useState<string | null>(null)
  const start = useMemo(() => {
    const tail = Math.max(0, rows.length - size)
    if (fromId === null) return tail
    const at = rows.findIndex((row) => row.id === fromId)
    return at === -1 ? tail : Math.min(at, tail)
  }, [rows, fromId, size])
  // Pin the first paint's oldest row: a new arrival never shrinks the window.
  const startId = rows[start]?.id ?? null
  useLayoutEffect(() => {
    if (fromId === null && startId !== null) setFromId(startId)
  }, [fromId, startId])

  const anchorRef = useRef<number | null>(null)
  const loadEarlier = useCallback(() => {
    const el = scrollRef.current
    anchorRef.current = el ? el.scrollHeight - el.scrollTop : null
    const next = rows[Math.max(0, start - step)]
    if (next) setFromId(next.id)
  }, [rows, start, step, scrollRef])
  useLayoutEffect(() => {
    const anchor = anchorRef.current
    if (anchor === null) return
    anchorRef.current = null
    const el = scrollRef.current
    if (!el) return
    const keep = () => {
      el.scrollTop = el.scrollHeight - anchor
    }
    keep()
    // The widened rows finish laying out after this (deferred markdown,
    // decoded images): hold the reader's place through that growth until
    // they scroll themselves (or a beat passes).
    const content = el.firstElementChild
    if (!content || typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(keep)
    observer.observe(content)
    const release = () => {
      observer.disconnect()
      clearTimeout(timer)
      for (const type of RELEASE_EVENTS) el.removeEventListener(type, release)
    }
    const timer = setTimeout(release, ANCHOR_HOLD_MS)
    for (const type of RELEASE_EVENTS) el.addEventListener(type, release, { passive: true })
    return release
  }, [start, scrollRef])

  const visible = useMemo(() => (start === 0 ? rows : rows.slice(start)), [rows, start])
  return { rows: visible, canLoadEarlier: start > 0, loadEarlier }
}

/** Plain-data equality (rows are rebuilt from synced rows on every change;
 *  an unchanged row must compare equal). */
export function sameData(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true
  if (typeof a !== `object` || typeof b !== `object` || a === null || b === null) return false
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false
    for (let i = 0; i < a.length; i++) if (!sameData(a[i], b[i])) return false
    return true
  }
  if (Array.isArray(b)) return false
  const ak = Object.keys(a)
  const bk = Object.keys(b)
  if (ak.length !== bk.length) return false
  for (const key of ak) {
    if (!sameData((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key])) return false
  }
  return true
}

/** One transcript row that re-renders only when its row data or one of its
 *  `deps` (plain values: a caption, a pending flag) changes. `render` may be
 *  a fresh closure each time: the skipped renders keep the last output. */
export const StableRow = memo(
  function StableRow<T>({ row, render }: { row: T; deps: readonly unknown[]; render: (row: T) => ReactNode }) {
    return <>{render(row)}</>
  },
  (prev, next) =>
    sameData(prev.row, next.row) &&
    prev.deps.length === next.deps.length &&
    prev.deps.every((dep, i) => Object.is(dep, next.deps[i]))
) as <T>(props: { row: T; deps: readonly unknown[]; render: (row: T) => ReactNode }) => ReactNode
