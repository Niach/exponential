import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import {
  FIND_IN_CHAT_COPY,
  FIND_IN_CHAT_DEBOUNCE_MS,
  FIND_IN_CHAT_TEST_IDS,
  findInChatKey,
  findInChatQuery,
  findInChatShortcut,
} from "@exp/domain-contract/find-in-chat"
import { Button, conceptIcon, MessageFoldOpenContext, MobileDetailHeader, SearchField, type MenuEntry } from "@exp/ui"
import { cn } from "@/lib/utils"

// #EDE-129 (fixtures/find-in-chat.json): find in chat ×web. The chat screen
// owns the bar (`useFindInChat`: open, the field, the steps it asks for, the
// counter and status line the transcript reports back); the transcript
// (`AgentSessionView`) counts its rows, merges the older hits and lands on
// the current stop; rows read `FindPaintContext` + `FindRowKeyContext` for
// genUI's `highlight` (`useFindRowHighlight`).

const SearchIcon = conceptIcon(`nav-search`)
const UpIcon = conceptIcon(`ui-chevron-up`)
const DownIcon = conceptIcon(`ui-chevron-down`)
const CloseIcon = conceptIcon(`ui-close`)

export type FindStepDir = `previous` | `next`

export interface FindReport {
  counter: string
  status: string | null
}

export interface FindInChat {
  open: boolean
  /** The field's text. */
  raw: string
  /** The query the transcript searches (collapsed, debounced; `` = none). */
  query: string
  /** The last step the bar asked for (`n` bumps per press). */
  step: { dir: FindStepDir; n: number } | null
  report: FindReport
  openBar: () => void
  close: () => void
  setRaw: (raw: string) => void
  go: (dir: FindStepDir) => void
  setReport: (report: FindReport) => void
  /** Bumps when the field should take the focus (open, Ctrl/Cmd+F again). */
  focusNonce: number
}

/** The chat screen's find, for the ROOT's transcript under it. */
export const FindInChatContext = createContext<FindInChat | null>(null)

/** The last query per chat, for the session (memory only). */
const LAST_QUERY = new Map<string, string>()
const NO_REPORT: FindReport = { counter: ``, status: null }

export function useFindInChat(chatKey: string): FindInChat {
  const [open, setOpen] = useState(false)
  const [raw, setRawState] = useState(``)
  const [query, setQuery] = useState(``)
  const [step, setStep] = useState<{ dir: FindStepDir; n: number } | null>(null)
  const [report, setReportState] = useState<FindReport>(NO_REPORT)
  const [focusNonce, setFocusNonce] = useState(0)
  useEffect(() => {
    setOpen(false)
    setRawState(``)
    setQuery(``)
  }, [chatKey])
  useEffect(() => {
    if (!open) return
    const next = findInChatQuery(raw)
    if (next === ``) {
      setQuery(``)
      return
    }
    const timer = window.setTimeout(() => setQuery(next), FIND_IN_CHAT_DEBOUNCE_MS)
    return () => window.clearTimeout(timer)
  }, [raw, open])
  const openBar = useCallback(() => {
    setOpen((was) => {
      if (!was) setRawState(LAST_QUERY.get(chatKey) ?? ``)
      return true
    })
    setFocusNonce((n) => n + 1)
  }, [chatKey])
  const close = useCallback(() => {
    setOpen(false)
    setQuery(``)
    setStep(null)
    setReportState(NO_REPORT)
  }, [])
  const setRaw = useCallback(
    (value: string) => {
      LAST_QUERY.set(chatKey, value)
      setRawState(value)
    },
    [chatKey]
  )
  const go = useCallback((dir: FindStepDir) => setStep((prev) => ({ dir, n: (prev?.n ?? 0) + 1 })), [])
  const setReport = useCallback((next: FindReport) => {
    setReportState((prev) => (prev.counter === next.counter && prev.status === next.status ? prev : next))
  }, [])
  return { open, raw, query: open ? query : ``, step, report, openBar, close, setRaw, go, setReport, focusNonce }
}

/** Ctrl/Cmd+F inside an open chat = find in chat (before the app's global
 *  search, which skips a handled press); Ctrl/Cmd+K stays global. */
export function useFindInChatShortcut(find: FindInChat, enabled: boolean) {
  const openBar = find.openBar
  useEffect(() => {
    if (!enabled) return
    const onKey = (event: KeyboardEvent) => {
      const action = findInChatKey({ key: event.key, mod: event.metaKey || event.ctrlKey, shift: event.shiftKey, inChat: true, inField: false })
      if (action !== `find`) return
      event.preventDefault()
      openBar()
    }
    window.addEventListener(`keydown`, onKey, true)
    return () => window.removeEventListener(`keydown`, onKey, true)
  }, [enabled, openBar])
}

/** Apple platforms show ⌘F, every other Ctrl F. */
function applePlatform(): boolean {
  if (typeof navigator === `undefined`) return false
  return /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent)
}

/** The `…` menu's entry (`findInChatMenu`: first, its own group). */
export function findInChatMenuEntries(entries: readonly MenuEntry[], onFind: (() => void) | undefined, phone: boolean): MenuEntry[] {
  if (!onFind) return [...entries]
  const item: MenuEntry = {
    kind: `item`,
    id: `search`,
    label: FIND_IN_CHAT_COPY.menu,
    icon: SearchIcon,
    ...(phone ? {} : { shortcut: findInChatShortcut(applePlatform()) }),
    "data-testid": `session-screen-menu-search`,
    onSelect: onFind,
  }
  return entries.length > 0 ? [item, { kind: `separator`, id: `sep-search` }, ...entries] : [item]
}

function useFieldFocus(find: FindInChat) {
  const ref = useRef<HTMLInputElement>(null)
  useEffect(() => {
    if (!find.open) return
    const el = ref.current
    if (!el) return
    el.focus()
    el.select()
  }, [find.open, find.focusNonce])
  return ref
}

function onFieldKey(find: FindInChat) {
  return (event: React.KeyboardEvent<HTMLInputElement>) => {
    const action = findInChatKey({ key: event.key, mod: event.metaKey || event.ctrlKey, shift: event.shiftKey, inChat: true, inField: true })
    if (action === `previous` || action === `next`) {
      event.preventDefault()
      find.go(action)
    } else if (action === `close`) {
      event.preventDefault()
      find.close()
    } else if (action === `find`) {
      event.preventDefault()
      event.currentTarget.select()
    }
  }
}

function StepButtons({ find }: { find: FindInChat }) {
  return (
    <>
      <Button variant="ghost" size="icon-sm" className="shrink-0 text-muted-foreground" aria-label={FIND_IN_CHAT_COPY.previous} title={FIND_IN_CHAT_COPY.previous} data-testid={FIND_IN_CHAT_TEST_IDS.previous} onClick={() => find.go(`previous`)}>
        <UpIcon className="size-4" />
      </Button>
      <Button variant="ghost" size="icon-sm" className="shrink-0 text-muted-foreground" aria-label={FIND_IN_CHAT_COPY.next} title={FIND_IN_CHAT_COPY.next} data-testid={FIND_IN_CHAT_TEST_IDS.next} onClick={() => find.go(`next`)}>
        <DownIcon className="size-4" />
      </Button>
    </>
  )
}

function Counter({ text }: { text: string }) {
  return (
    <span className="shrink-0 text-sm text-muted-foreground tabular-nums" data-testid={FIND_IN_CHAT_TEST_IDS.counter} aria-live="polite">
      {text}
    </span>
  )
}

/** The status line, the transcript's first line under the bar. */
function StatusLine({ text, className }: { text: string | null; className?: string }) {
  if (!text) return null
  return (
    <p className={cn(`shrink-0 truncate text-xs text-muted-foreground`, className)} data-testid={FIND_IN_CHAT_TEST_IDS.status}>
      {text}
    </p>
  )
}

/** WIDE: the band under the chat header across the chat column. */
export function FindInChatBar({ find }: { find: FindInChat }) {
  const ref = useFieldFocus(find)
  if (!find.open) return null
  return (
    <div className="shrink-0" data-testid={FIND_IN_CHAT_TEST_IDS.bar}>
      <div className="flex items-center gap-2 border-b border-border bg-muted/30 px-6 py-2">
        <div className="w-full max-w-sm min-w-0">
          <SearchField
            ref={ref}
            value={find.raw}
            onValueChange={find.setRaw}
            placeholder={FIND_IN_CHAT_COPY.placeholder}
            aria-label={FIND_IN_CHAT_COPY.placeholder}
            data-testid={FIND_IN_CHAT_TEST_IDS.input}
            onKeyDown={onFieldKey(find)}
          />
        </div>
        <Counter text={find.report.counter} />
        <StepButtons find={find} />
        <Button variant="ghost" size="icon-sm" className="shrink-0 text-muted-foreground" aria-label={FIND_IN_CHAT_COPY.close} title={FIND_IN_CHAT_COPY.close} data-testid={FIND_IN_CHAT_TEST_IDS.close} onClick={find.close}>
          <CloseIcon className="size-4" />
        </Button>
      </div>
      <StatusLine text={find.report.status} className="px-6 pt-2" />
    </div>
  )
}

/** COMPACT: the bar REPLACES the chat header while open (back closes). */
export function FindInChatHeader({ find }: { find: FindInChat }) {
  const ref = useFieldFocus(find)
  return (
    <div className="shrink-0" data-testid={FIND_IN_CHAT_TEST_IDS.bar}>
      <MobileDetailHeader
        onBack={find.close}
        backLabel={FIND_IN_CHAT_COPY.close}
        title={
          <SearchField
            ref={ref}
            value={find.raw}
            onValueChange={find.setRaw}
            placeholder={FIND_IN_CHAT_COPY.placeholder}
            aria-label={FIND_IN_CHAT_COPY.placeholder}
            data-testid={FIND_IN_CHAT_TEST_IDS.input}
            onKeyDown={onFieldKey(find)}
          />
        }
        menu={
          <span className="flex shrink-0 items-center gap-1">
            <Counter text={find.report.counter} />
            <StepButtons find={find} />
          </span>
        }
      />
      <StatusLine text={find.report.status} className="px-3 pt-2" />
    </div>
  )
}

// ── Painting ────────────────────────────────────────────────────────────────

/** What the transcript's rows paint: the query, the current stop. */
export interface FindPaint {
  sessionId: string
  query: string
  current: { key: string; local: number } | null
  /** Folded groups find opened (they stay open). */
  openGroups: ReadonlySet<string>
  /** Tool rows whose output matches → its match count (the hint). */
  outputHits: ReadonlyMap<string, number>
}

export const FindPaintContext = createContext<FindPaint | null>(null)
/** The find key of the transcript row being drawn (null = not counted). */
export const FindRowKeyContext = createContext<string | null>(null)

/** The `highlight` for the row being drawn (undefined = none). */
export function useFindRowHighlight(): { query: string; current?: number } | undefined {
  const paint = useContext(FindPaintContext)
  const key = useContext(FindRowKeyContext)
  return useMemo(() => {
    if (!paint || !key || !paint.query) return undefined
    return paint.current && paint.current.key === key ? { query: paint.query, current: paint.current.local } : { query: paint.query }
  }, [paint, key])
}

/** A row's find wrapper: its key for the marks, `data-find-key` for the
 *  scroll (`display: contents`, the row's own box stays the measured one). */
export function FindRow({ findKey, children }: { findKey: string | null; children: ReactNode }) {
  const paint = useContext(FindPaintContext)
  if (!findKey) return <>{children}</>
  // A bubble's fold opens when the current match is in it.
  const opens = paint?.current?.key === findKey
  return (
    <FindRowKeyContext.Provider value={findKey}>
      <MessageFoldOpenContext.Provider value={opens}>
        <div className="contents" data-find-key={findKey}>
          {children}
        </div>
      </MessageFoldOpenContext.Provider>
    </FindRowKeyContext.Provider>
  )
}
