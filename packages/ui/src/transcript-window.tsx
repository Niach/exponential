import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ComponentProps,
  type ReactNode,
  type RefObject,
} from "react"
import {
  CHATS_COPY,
  CHATS_TEST_IDS,
  CHATS_TRANSCRIPT,
  transcriptWaitingIndexes,
  transcriptWindowStart,
} from "@exp/domain-contract/chats"
import { Button } from "./button"
import { cn } from "./cn"

// #EXP-1380 (chats.json WINDOW + PERF ×4; review of #1218): a chat paints
// its NEWEST `transcript.window` rows, reaching up to the oldest WAITING
// plan / question card; `Load earlier` (the run view's ghost button) moves
// the start up by `transcript.windowStep` in ONE press keeping the reader's
// place; the first paint's start is PINNED (by row id), so arrivals only
// append. The start = `transcriptWindowStart` (`windowCases`). Rows render
// once per change of their own data (`StableRow`).

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
  /** The rows to paint (the window, from its start to the newest row). */
  rows: readonly T[]
  /** Older rows exist above the window (painted or still unloaded). */
  canLoadEarlier: boolean
  /** One `windowStep` earlier; the reader's distance from the bottom is kept. */
  loadEarlier: () => void
}

export interface TranscriptWindowOptions {
  /** The chat's identity: a change = a reopen (the newest window again). */
  resetKey?: string | null
  /** The rows are complete enough to pin the start (the messages loaded). */
  ready?: boolean
  /** Older MESSAGES exist beyond the loaded ones (an on-demand subset). */
  more?: boolean
  /** How many messages are loaded (a page that added none still moves it). */
  loaded?: number
  /** Load one page of older messages. */
  loadMore?: () => void
}

interface Pin {
  key: string | null
  /** The first paint's start row (null = not painted yet). */
  id: string | null
  /** Load earlier presses since. */
  steps: number
}

/** The window over a chat's rows. `scrollRef` = the transcript's scroller
 *  (its position is kept across a Load earlier). */
export function useTranscriptWindow<T extends { id: string }>(
  rows: readonly T[],
  scrollRef: RefObject<HTMLElement | null>,
  options: TranscriptWindowOptions = {}
): TranscriptWindow<T> {
  const { resetKey = null, ready = true, more = false, loaded = 0, loadMore } = options
  const [stored, setPin] = useState<Pin>({ key: resetKey, id: null, steps: 0 })
  const pin: Pin = stored.key === resetKey ? stored : { key: resetKey, id: null, steps: 0 }
  const waitingIndexes = useMemo(
    () => transcriptWaitingIndexes(rows as unknown as readonly { type: string }[]),
    [rows]
  )
  const pinnedAt = pin.id === null ? -1 : rows.findIndex((row) => row.id === pin.id)
  const start = transcriptWindowStart({
    rowCount: rows.length,
    waitingIndexes,
    pinnedStart: pinnedAt === -1 ? null : pinnedAt,
    loadedSteps: pin.steps,
  })
  // The loaded rows fill no window yet while older messages exist (a busy
  // tree's pages are mostly its children's messages, which paint no row):
  // page them in before pinning.
  const filling = pin.id === null && more && rows.length < CHATS_TRANSCRIPT.window
  // Pin the first paint's start: an arrival never re-slides the window.
  const firstStartId = rows[start]?.id ?? null
  useLayoutEffect(() => {
    if (ready && !filling && pin.id === null && firstStartId !== null) {
      setPin({ key: resetKey, id: firstStartId, steps: pin.steps })
    }
  }, [ready, filling, pin.id, pin.steps, firstStartId, resetKey])

  const anchorRef = useRef<number | null>(null)
  const holdPlace = useCallback(() => {
    const el = scrollRef.current
    anchorRef.current = el ? el.scrollHeight - el.scrollTop : null
  }, [scrollRef])

  // ONE press = one step of ROWS: while the loaded rows fall short of the
  // window's start and older messages exist, page them in (`loaded` moves
  // even when a page adds no row, so the next page follows).
  const pressed = pin.steps > 0
  const short = filling || (pressed && start === 0 && more)
  const loadMoreRef = useRef(loadMore)
  loadMoreRef.current = loadMore
  useEffect(() => {
    if (!short) return
    if (pressed) holdPlace()
    loadMoreRef.current?.()
  }, [short, pressed, loaded, holdPlace])

  const loadEarlier = useCallback(() => {
    holdPlace()
    setPin((prev) => {
      const base = prev.key === resetKey ? prev : { key: resetKey, id: null, steps: 0 }
      return { ...base, steps: base.steps + 1 }
    })
  }, [holdPlace, resetKey])

  const firstId = rows[start]?.id ?? null
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
  }, [firstId, scrollRef])

  const visible = useMemo(() => (start === 0 ? rows : rows.slice(start)), [rows, start])
  return { rows: visible, canLoadEarlier: start > 0 || more, loadEarlier }
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
