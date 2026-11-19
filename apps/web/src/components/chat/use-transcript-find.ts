import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react"
import {
  FIND_IN_CHAT_HITS_MAX,
  findEventKey,
  findInChatCounter,
  findInChatLand,
  findInChatStatus,
  findInChatStep,
  findInChatStops,
  type FindOlderHit,
  type FindRow as FindRowSpec,
  type FindSearchPhase,
  type FindStop,
} from "@exp/domain-contract/find-in-chat"
import type { ChatHit } from "@exp/domain-contract/chat-store"
import { globalChatStart } from "@exp/domain-contract/search-list"
import { HIGHLIGHT_NAMES } from "@exponential_app/eui/web"
import { chatStoreCall } from "@/lib/chat-store/client"
import type { SteerSearchResult } from "@exp/domain-contract/steer-enet"
import type { FindInChat, FindPaint } from "@/components/chat/find-in-chat"

// #EDE-129 (fixtures/find-in-chat.json): the transcript's half of find in
// chat ×web: counts the loaded feed (`rows`, built by the view over its
// WHOLE feed, not the painted window), merges the store's and the device's
// older hits, keeps the current stop while answers arrive, lands on it
// (reveal it in the window, open its group or fold, page an older one in,
// centre, ring) and reports the counter and status line to the bar.

/** A loaded row with what landing needs: the feed item to reveal (#EXP-1975:
 *  null = a bubble drawn outside the feed: the synced tail, the own tail,
 *  the sent inputs; always mounted, found by its `data-find-key`). */
export interface TranscriptFindRow extends FindRowSpec {
  itemId: number | null
  /** Its output's matches (the hint), 0 = none. */
  outputCount: number
}

export interface TranscriptFindInput {
  find: FindInChat | null
  compact: boolean
  sessionId: string
  rootId: string
  /** The loaded rows for `query` (top to bottom). */
  rowsFor: (query: string) => TranscriptFindRow[]
  /** The lowest loaded seq (undefined = nothing loaded). */
  lowestSeq: number | undefined
  /** The loaded stretch starts at the session's first event. */
  fromStart: boolean
  deviceOnline: boolean | null
  deviceName: string
  search: (query: string, beforeSeq?: number) => Promise<SteerSearchResult | null>
  /** Make the painted window hold this feed item. */
  reveal: (itemId: number) => void
  /** Page older rows in (the Load earlier path). */
  pageOlder: () => void
  canPageOlder: boolean
  scrollRef: { current: HTMLElement | null }
}

const RING_MS = 1600
const sameStop = (a: FindStop, b: FindStop) => a.key === b.key && a.local === b.local && a.output === b.output

/** The row's element: its find wrapper is `display: contents`. */
function rowElement(scroller: HTMLElement, key: string): HTMLElement | null {
  const wrapper = scroller.querySelector<HTMLElement>(`[data-find-key="${CSS.escape(key)}"]`)
  return (wrapper?.firstElementChild as HTMLElement | null) ?? null
}

/** The painted current match's box inside `row`, else null. */
function currentMarkRect(row: HTMLElement): DOMRect | null {
  const registry = (globalThis as { CSS?: { highlights?: Map<string, Iterable<Range>> } }).CSS?.highlights
  const current = registry?.get(HIGHLIGHT_NAMES.current)
  if (!current) return null
  for (const range of current) {
    if (row.contains(range.commonAncestorContainer)) {
      const rect = range.getBoundingClientRect()
      if (rect.height > 0) return rect
    }
  }
  return null
}

function ring(el: HTMLElement) {
  const style = el.style
  style.outline = `2px solid var(--ring)`
  style.outlineOffset = `2px`
  window.setTimeout(() => {
    style.outline = ``
    style.outlineOffset = ``
  }, RING_MS)
}

export function useTranscriptFind(input: TranscriptFindInput): { paint: FindPaint | null; holding: boolean } {
  const { find, sessionId, rootId } = input
  const query = find?.query ?? ``
  const rows = useMemo(() => (query ? input.rowsFor(query) : []), [query, input.rowsFor]) // eslint-disable-line react-hooks/exhaustive-deps

  // ── Older hits (store, then device) ──────────────────────────────────────
  const [older, setOlder] = useState<{ query: string; store: FindOlderHit[]; device: FindOlderHit[]; storePhase: FindSearchPhase; devicePhase: FindSearchPhase }>({
    query: ``,
    store: [],
    device: [],
    storePhase: `none`,
    devicePhase: `none`,
  })
  const lowestRef = useRef(input.lowestSeq)
  lowestRef.current = input.lowestSeq
  const askRef = useRef({ fromStart: input.fromStart, online: input.deviceOnline, search: input.search })
  askRef.current = { fromStart: input.fromStart, online: input.deviceOnline, search: input.search }
  useEffect(() => {
    if (!query) {
      setOlder({ query: ``, store: [], device: [], storePhase: `none`, devicePhase: `none` })
      return
    }
    let live = true
    const { fromStart, online, search } = askRef.current
    const askDevice = !fromStart && online === true
    setOlder({ query, store: [], device: [], storePhase: `searching`, devicePhase: fromStart ? `none` : askDevice ? `searching` : online === false ? `offline` : `none` })
    void chatStoreCall<{ hits: ChatHit[] }>(`searchChat`, rootId, query, FIND_IN_CHAT_HITS_MAX)
      .catch(() => undefined)
      .then((result) => {
        if (!live) return
        const hits = (result?.hits ?? [])
          .filter((hit) => hit.sessionId === sessionId && typeof hit.seq === `number`)
          .map((hit) => ({ key: findEventKey(sessionId, hit.seq!), output: hit.kind === `tool_output` }))
        setOlder((prev) => (prev.query === query ? { ...prev, store: hits, storePhase: `done` } : prev))
      })
    if (askDevice) {
      void search(query, lowestRef.current).then((result) => {
        if (!live) return
        const hits = (result?.hits ?? []).map((hit) => ({ key: findEventKey(sessionId, hit.seq), output: hit.kind === `tool_output` }))
        setOlder((prev) => (prev.query === query ? { ...prev, device: hits, devicePhase: `done` } : prev))
      })
    }
    return () => {
      live = false
    }
  }, [query, rootId, sessionId])

  // An older hit is one BELOW the loaded stretch (only event keys carry a
  // seq; #EXP-1975: a message key is never an older event).
  const lowest = input.lowestSeq
  const olderHits = useMemo(() => {
    if (older.query !== query) return []
    const below = (hit: FindOlderHit) => {
      if (!hit.key.startsWith(`e:`)) return false
      const seq = Number(hit.key.slice(hit.key.lastIndexOf(`:`) + 1))
      return Number.isFinite(seq) && (lowest === undefined || seq < lowest)
    }
    return [...older.store, ...older.device].filter(below)
  }, [older, query, lowest])
  const stops = useMemo(() => findInChatStops(rows, olderHits), [rows, olderHits])

  // ── The current stop (kept by identity while answers arrive) ─────────────
  const [current, setCurrent] = useState<FindStop | null>(null)
  const [landing, setLanding] = useState<{ stop: FindStop; n: number } | null>(null)
  useEffect(() => {
    setCurrent(null)
    setLanding(null)
  }, [query])
  let index = current ? stops.findIndex((stop) => sameStop(stop, current)) : -1
  if (index < 0 && stops.length > 0) index = stops.length - 1
  const shown = index >= 0 ? stops[index]! : null
  // A new query starts at the newest stop and lands there; #EDE-99: a
  // global search pick starts at ITS hit's stop (search-list.json
  // `globalChatStart`), waiting for the store's answer before it falls back.
  const target = find?.target ?? null
  const dropTarget = find?.dropTarget
  const storeDone = older.query === query && older.storePhase !== `searching`
  useEffect(() => {
    if (current !== null || !shown) return
    let start = shown
    if (target && target.query === query) {
      const found = stops.findIndex((stop) => stop.key === target.key && stop.output === target.output)
      if (found < 0 && !storeDone) return
      start = stops[globalChatStart(stops, target) ?? stops.length - 1]!
      dropTarget?.()
    }
    setCurrent(start)
    setLanding((prev) => ({ stop: start, n: (prev?.n ?? 0) + 1 }))
  }, [current, shown, target, query, stops, storeDone, dropTarget])
  const step = find?.step
  const stepRef = useRef(step?.n ?? 0)
  useEffect(() => {
    if (!step || step.n === stepRef.current) return
    stepRef.current = step.n
    const next = findInChatStep(index < 0 ? null : index, stops.length, step.dir)
    if (next === null) return
    const stop = stops[next]!
    setCurrent(stop)
    setLanding((prev) => ({ stop, n: (prev?.n ?? 0) + 1 }))
  }, [step, index, stops])

  // ── Landing ──────────────────────────────────────────────────────────────
  const [openGroups, setOpenGroups] = useState<ReadonlySet<string>>(() => new Set())
  const [paging, setPaging] = useState<FindStop | null>(null)
  const rowsByKey = useMemo(() => new Map(rows.map((row) => [row.key, row])), [rows])
  const landRef = useRef({ rowsByKey, reveal: input.reveal, scrollRef: input.scrollRef })
  landRef.current = { rowsByKey, reveal: input.reveal, scrollRef: input.scrollRef }
  const { pageOlder, canPageOlder } = input
  // Only a NEW landing moves the reader (a streamed row never re-scrolls).
  useEffect(() => {
    if (!landing) return
    const { rowsByKey: byKey, reveal, scrollRef } = landRef.current
    const live = byKey.get(landing.stop.key)
    const stop = live ? { ...landing.stop, loaded: true, group: live.group ?? null } : landing.stop
    const land = findInChatLand(stop)
    if (land.pageIn) {
      setPaging(landing.stop)
      return
    }
    setPaging(null)
    if (land.openGroup) {
      const group = land.openGroup
      setOpenGroups((prev) => (prev.has(group) ? prev : new Set([...prev, group])))
    }
    if (live && live.itemId !== null) reveal(live.itemId)
    // Centre once the row (and its marks) painted: a revealed window or an
    // opened group mounts on the next frames.
    let tries = 0
    let frame = 0
    const settle = () => {
      const scroller = scrollRef.current
      const el = scroller ? rowElement(scroller, stop.key) : null
      if (!scroller || !el) {
        if (tries++ < 30) frame = requestAnimationFrame(settle)
        return
      }
      const box = (!land.ring && currentMarkRect(el)) || el.getBoundingClientRect()
      const view = scroller.getBoundingClientRect()
      scroller.scrollTop += box.top + box.height / 2 - (view.top + view.height / 2)
      if (land.ring || landing.stop.loaded === false) ring(el)
    }
    frame = requestAnimationFrame(() => requestAnimationFrame(settle))
    return () => cancelAnimationFrame(frame)
  }, [landing])
  // An older stop pages its row in, then lands on it.
  useLayoutEffect(() => {
    if (!paging) return
    if (rowsByKey.has(paging.key)) {
      setLanding((prev) => ({ stop: paging, n: (prev?.n ?? 0) + 1 }))
      setPaging(null)
      return
    }
    if (canPageOlder) pageOlder()
    else setPaging(null)
  }, [paging, rowsByKey, canPageOlder, pageOlder])

  // ── The bar's counter and status line ────────────────────────────────────
  const setReport = find?.setReport
  const done = older.query === query && older.storePhase !== `searching` && older.devicePhase !== `searching`
  const counter = query ? findInChatCounter({ current: index < 0 ? null : index, total: stops.length, done, compact: input.compact }) : ``
  const status = query && older.query === query ? findInChatStatus({ store: older.storePhase, device: older.devicePhase, deviceName: input.deviceName }) : null
  useEffect(() => {
    setReport?.({ counter, status })
  }, [setReport, counter, status])

  const outputHits = useMemo(() => new Map(rows.filter((row) => row.outputCount > 0).map((row) => [row.key, row.outputCount])), [rows])
  const paint = useMemo<FindPaint | null>(() => {
    if (!query) return null
    const mark = shown && shown.loaded && !shown.output ? { key: shown.key, local: shown.local } : null
    return { sessionId, query, current: mark, openGroups, outputHits }
  }, [query, shown, sessionId, openGroups, outputHits])
  return { paint, holding: Boolean(query) && stops.length > 0 }
}
