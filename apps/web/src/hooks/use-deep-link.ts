import { createContext, useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react"
import { WAITING_CARDS_CONSTANTS } from "@exp/domain-contract/waiting-cards"
import {
  centreOffset,
  deepLinkTick,
  takeDeepLink,
  type DeepLinkEffects,
  type DeepLinkProbe,
} from "@/lib/message-deep-link"

// waiting-cards.json DEEP LINKS ×web, the React half (the pure half:
// `lib/deep-link.ts`). THE pending deep link lives with the screen that
// took it (`useDeepLinkHost`: the chat screen, a session's own screen) and
// reaches the session view that holds the card through `DeepLinkContext`;
// that view drives it (`useDeepLinkDriver`) against its scroller, holds its
// follow-scroll while it is pending and drops it on the reader's own scroll.

/** A link handed to the session view that holds its card. */
export interface DeepLinkTarget {
  /** The card's message id (`data-card-id`). */
  id: string
  /** The session whose view draws it. */
  sessionId: string
  takenAt: number
  nonce: number
}

export type DeepLinkOutcome = `ring` | `missing` | `dropped`

export interface DeepLinkHost {
  link: DeepLinkTarget | null
  settle: (link: DeepLinkTarget, outcome: DeepLinkOutcome) => void
}

export const DeepLinkContext = createContext<DeepLinkHost | null>(null)

let nonces = 0

/** The screen's pending link + THE highlight ring (`highlightMs`). */
export function useDeepLinkHost(): DeepLinkHost & {
  /** Take a link to card `id` drawn by `sessionId`'s view. */
  take: (id: string, sessionId: string, takenAt?: number) => void
  /** Drop whatever is pending. */
  drop: () => void
  /** The ringed card's id (`CardHighlightContext`). */
  highlight: string | null
} {
  const [link, setLink] = useState<DeepLinkTarget | null>(null)
  const [ringed, setRinged] = useState<{ id: string; nonce: number } | null>(null)
  const take = useCallback((id: string, sessionId: string, takenAt = Date.now()) => {
    setRinged(null)
    setLink({ id, sessionId, takenAt, nonce: ++nonces })
  }, [])
  const drop = useCallback(() => setLink(null), [])
  const settle = useCallback((settled: DeepLinkTarget, outcome: DeepLinkOutcome) => {
    setLink((current) => (current?.nonce === settled.nonce ? null : current))
    if (outcome === `ring`) setRinged({ id: settled.id, nonce: settled.nonce })
  }, [])
  // A link no view drives (its session never mounted) gives up too.
  useEffect(() => {
    if (!link) return
    const left = link.takenAt + WAITING_CARDS_CONSTANTS.deepLinkMs + 1_000 - Date.now()
    const timer = setTimeout(() => setLink((current) => (current?.nonce === link.nonce ? null : current)), Math.max(0, left))
    return () => clearTimeout(timer)
  }, [link])
  useEffect(() => {
    if (!ringed) return
    const timer = setTimeout(() => setRinged(null), WAITING_CARDS_CONSTANTS.highlightMs)
    return () => clearTimeout(timer)
  }, [ringed])
  return useMemo(
    () => ({ link, settle, take, drop, highlight: ringed?.id ?? null }),
    [link, settle, take, drop, ringed]
  )
}

/** Drive ONE pending link: every frame while `link` is set, probe the view
 *  and run `deepLinkStep`'s effect, until it rings or goes missing.
 *  `probe` / `effects` are read fresh each frame. */
export function useDeepLinkDriver(
  link: { key: string | number; takenAt: number } | null,
  probe: () => DeepLinkProbe,
  effects: DeepLinkEffects
): void {
  const probeRef = useRef(probe)
  probeRef.current = probe
  const effectsRef = useRef(effects)
  effectsRef.current = effects
  const key = link?.key ?? null
  const takenAt = link?.takenAt ?? 0
  useEffect(() => {
    if (key === null) return
    let cursor = takeDeepLink(takenAt)
    let raf = 0
    let done = false
    const frame = () => {
      if (done) return
      const result = deepLinkTick(cursor, probeRef.current(), Date.now(), {
        page: () => effectsRef.current.page(),
        scroll: () => effectsRef.current.scroll(),
        ring: () => effectsRef.current.ring(),
        missing: () => effectsRef.current.missing(),
      })
      cursor = result.cursor
      if (result.settled) done = true
      else raf = requestAnimationFrame(frame)
    }
    raf = requestAnimationFrame(frame)
    return () => {
      done = true
      cancelAnimationFrame(raf)
    }
  }, [key, takenAt])
}

/** The card's element inside `scroller` (`data-card-id`), if mounted. */
export function cardElement(scroller: HTMLElement | null, id: string): HTMLElement | null {
  if (!scroller) return null
  const escaped = typeof CSS !== `undefined` && CSS.escape ? CSS.escape(id) : id.replace(/"/g, `\\"`)
  return scroller.querySelector<HTMLElement>(`[data-card-id="${escaped}"]`)
}

/** Measure `el` against `scroller`: laid out (a real, nonzero height) and
 *  centred (`centreOffset`). */
export function measureCard(
  scroller: HTMLElement,
  el: HTMLElement
): { laidOut: boolean; centred: boolean; delta: number } {
  const rect = el.getBoundingClientRect()
  const view = scroller.getBoundingClientRect()
  const { delta, centred } = centreOffset(
    { top: rect.top, height: rect.height },
    { top: view.top, height: scroller.clientHeight, scrollTop: scroller.scrollTop, scrollHeight: scroller.scrollHeight }
  )
  return { laidOut: rect.height > 0, centred, delta }
}

/** The reader's OWN scroll on `scroller` (wheel, trackpad, a touch pan, a
 *  press on its scrollbar, the scroll keys) calls `onDrop`. */
export function useReaderScrollDrops(
  scrollerRef: RefObject<HTMLElement | null>,
  active: boolean,
  onDrop: () => void
): void {
  const dropRef = useRef(onDrop)
  dropRef.current = onDrop
  useEffect(() => {
    const scroller = scrollerRef.current
    if (!scroller || !active) return
    const drop = () => dropRef.current()
    // A press on the element itself (not a row inside it) = its scrollbar.
    const press = (event: PointerEvent) => {
      if (event.target === scroller) drop()
    }
    const key = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      const typing = target?.isContentEditable || target?.tagName === `INPUT` || target?.tagName === `TEXTAREA`
      if (SCROLL_KEYS.has(event.key) && !typing) drop()
    }
    scroller.addEventListener(`wheel`, drop, { passive: true })
    scroller.addEventListener(`touchmove`, drop, { passive: true })
    scroller.addEventListener(`pointerdown`, press)
    scroller.addEventListener(`keydown`, key)
    return () => {
      scroller.removeEventListener(`wheel`, drop)
      scroller.removeEventListener(`touchmove`, drop)
      scroller.removeEventListener(`pointerdown`, press)
      scroller.removeEventListener(`keydown`, key)
    }
  }, [scrollerRef, active])
}

const SCROLL_KEYS = new Set([`PageUp`, `PageDown`, `Home`, `End`, `ArrowUp`, `ArrowDown`, ` `])
