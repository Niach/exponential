import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react"
import {
  dock,
  dockAutoOpen,
  DOCK_AUTO_OPEN_GRACE_MS,
  DOCK_COPY,
  DOCK_ICONS,
  DOCK_SHEET,
  DOCK_STACK,
  DOCK_TEST_IDS,
  type DockRow,
  type DockTint,
} from "@exp/domain-contract/dock"
import { stackLayout } from "@exp/domain-contract/stack"
import type { IconConcept } from "@exp/icons"
import { Button } from "./button"
import { cn } from "./cn"
import { GlassCard } from "./glass-card"
import { conceptIcon } from "./icons.generated"
import { Sheet, SheetContent, SheetDescription, SheetTitle } from "./sheet"
import { useMediaQuery } from "./use-media-query"
import { useIsMobile } from "./use-mobile"

// N-12: THE Dock ×web (fixtures/dock.json ×4; desktop `dock.rs`, iOS
// `Dock.swift`, Android `Dock.kt`): the floating band over the transcript's
// bottom edge in the composer column: the DockCards (a waiting plan or
// question, THE Queue, the queued messages, a status banner) piled like the
// toasts (`stackLayout`, anchored at the bottom: the front card last, the
// older ones peeking out above it, `visible` at most; hover or a tap fans
// them `gap` apart), then THE composer, which never leaves. A tap on a card
// opens its sheet (`DockSheet`: a pinned header, ONE scrolling body
// (`DockSheetBody`), a pinned footer (`DockSheetFooter`), at most
// `sheet.maxHeightFraction` of the viewport). The band measures itself
// (`onHeight`): the host pads the transcript by it and lifts the jump button
// over it.

const OpenIcon = conceptIcon(DOCK_ICONS.open as IconConcept)
const CloseIcon = conceptIcon(DOCK_ICONS.close as IconConcept)

const TINT_CLASS: Record<DockTint, string> = {
  plan: `text-primary`,
  question: `text-yellow-400`,
  muted: `text-muted-foreground`,
  warning: `text-amber-400`,
}

/** The band's column: the work column's measure, the phone's gutter. */
export const DOCK_COLUMN_CLASS = `mx-auto w-full max-w-4xl px-3 md:px-5`

/** The phone's floating work bar sits under the band: the band clears it
 *  (`MOBILE_WORK_BAR_CLEARANCE`, restated so the barrel stays a line). */
const PHONE_CLEARANCE_CLASS = `pb-[calc(5.5rem+env(safe-area-inset-bottom))]`

/** THE floating band. `rows` = `dockRows(sources)`, top to bottom. */
export function Dock({
  rows,
  openId,
  onOpenChange,
  renderSheet,
  renderAction,
  composer,
  onHeight,
  phoneClearance = false,
  initialExpanded = false,
  className,
}: {
  rows: readonly DockRow[]
  /** The open sheet's card id (controlled). */
  openId: string | null
  onOpenChange: (id: string | null) => void
  /** The sheet of an expandable card: a `DockSheetBody` and an optional
   *  `DockSheetFooter` (null = the card does not open). */
  renderSheet: (row: DockRow, close: () => void) => ReactNode
  /** A banner's action button (its `action` word), else nothing. */
  renderAction?: (row: DockRow) => ReactNode
  /** THE composer, under the cards (absent on a phone whose capsule floats
   *  in the work bar). */
  composer?: ReactNode
  /** The band's measured height in px (the transcript's bottom padding). */
  onHeight?: (height: number) => void
  /** The phone's floating work bar sits under the band: clear it. */
  phoneClearance?: boolean
  /** The pile fanned on mount (the styleguide's fanned state). */
  initialExpanded?: boolean
  className?: string
}) {
  const bandRef = useRef<HTMLDivElement | null>(null)
  useLayoutEffect(() => {
    const node = bandRef.current
    if (!node || !onHeight) return
    const measure = () => onHeight(node.offsetHeight)
    measure()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(measure)
    observer.observe(node)
    return () => observer.disconnect()
  }, [onHeight])
  const [expanded, setExpanded] = useState(initialExpanded)
  const view = useMemo(() => dock({ rows, expanded, openId }), [rows, expanded, openId])
  const openRow = view.openId === null ? null : (rows.find((row) => row.id === view.openId) ?? null)
  const close = useCallback(() => onOpenChange(null), [onOpenChange])
  const sheet = openRow ? renderSheet(openRow, close) : null
  if (view.mode === `none` && !composer) return null
  return (
    <>
      <div
        ref={bandRef}
        data-testid={DOCK_TEST_IDS.dock}
        data-mode={view.mode}
        className={cn(`pointer-events-none absolute inset-x-0 bottom-0 z-20`, className)}
      >
        <div
          aria-hidden
          className="pointer-events-none absolute inset-x-0 -top-10 bottom-0"
          style={{ background: `linear-gradient(to top, var(--background) 72%, transparent)` }}
        />
        <div
          className={cn(
            `pointer-events-auto relative flex flex-col gap-1.5`,
            DOCK_COLUMN_CLASS,
            phoneClearance ? PHONE_CLEARANCE_CLASS : `pb-3 md:pb-4`
          )}
        >
          {view.mode !== `none` && (
            <DockPile
              rows={view.rows}
              expanded={view.mode === `fanned`}
              single={view.mode === `single`}
              onExpandedChange={setExpanded}
              onOpen={(row) => (row.expandable ? onOpenChange(row.id) : undefined)}
              renderAction={renderAction}
            />
          )}
          {composer}
        </div>
      </div>
      {openRow && sheet && (
        <DockSheet row={openRow} open onOpenChange={(next) => (next ? undefined : close())}>
          {sheet}
        </DockSheet>
      )}
    </>
  )
}

/** The pile: every card absolutely placed by `stackLayout` (measured
 *  heights), the front one last; collapsed = the older cards peek out above
 *  it, their words hidden; a hover-capable pointer fans it on hover, a
 *  touch on it fans it (the next touch on a card opens the card). */
export function DockPile({
  rows,
  expanded,
  single = false,
  onExpandedChange,
  onOpen,
  renderAction,
}: {
  rows: readonly DockRow[]
  expanded: boolean
  single?: boolean
  onExpandedChange: (expanded: boolean) => void
  onOpen: (row: DockRow) => void
  renderAction?: (row: DockRow) => ReactNode
}) {
  const hover = useMediaQuery(`(hover: hover)`)
  const [heights, setHeights] = useState<Record<string, number>>({})
  const observers = useRef(new Map<string, ResizeObserver>())
  const measure = useCallback((id: string, node: HTMLDivElement | null) => {
    observers.current.get(id)?.disconnect()
    observers.current.delete(id)
    if (!node) return
    const update = () =>
      setHeights((current) =>
        current[id] === node.offsetHeight ? current : { ...current, [id]: node.offsetHeight }
      )
    update()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(update)
    observer.observe(node)
    observers.current.set(id, observer)
  }, [])
  useEffect(() => {
    const live = observers.current
    return () => live.forEach((observer) => observer.disconnect())
  }, [])
  const fanned = expanded || single
  const layout = stackLayout(
    rows.map((row) => heights[row.id] ?? 0),
    { expanded: fanned, anchoredBottom: true },
    DOCK_STACK
  )
  const frontId = rows.at(-1)?.id ?? null
  return (
    <div
      data-testid={DOCK_TEST_IDS.pile}
      data-expanded={fanned}
      className="relative transition-[height] duration-slow ease-standard motion-reduce:transition-none"
      style={{ height: layout.height }}
      onMouseEnter={hover ? () => onExpandedChange(true) : undefined}
      onMouseLeave={hover ? () => onExpandedChange(false) : undefined}
      onFocus={() => onExpandedChange(true)}
      onBlur={(event) => {
        const next = event.relatedTarget as Node | null
        if (!next || !event.currentTarget.contains(next)) onExpandedChange(false)
      }}
    >
      {rows.map((row, index) => {
        const item = layout.items[index]!
        const front = row.id === frontId
        const style: CSSProperties = {
          top: item.offset,
          transform: `scale(${item.scale})`,
          opacity: item.visible ? 1 : 0,
        }
        return (
          <div
            key={row.id}
            ref={(node) => measure(row.id, node)}
            data-front={front}
            className={cn(
              `absolute inset-x-0 origin-top transition-[top,transform,opacity] duration-slow ease-standard motion-reduce:transition-none`,
              !item.visible && `pointer-events-none`
            )}
            style={style}
          >
            <DockCard
              row={row}
              hidden={!fanned && !front}
              action={renderAction?.(row)}
              onOpen={() => {
                // A touch fans the pile first; the next touch opens.
                if (!hover && !fanned) {
                  onExpandedChange(true)
                  return
                }
                onOpen(row)
              }}
            />
          </div>
        )
      })}
    </div>
  )
}

/** A DockCard collapsed: the AskCard chrome (the kind icon, the tinted
 *  label, the muted caption, the trailing meta) over the one title line and
 *  the open chevron (an expandable card) or the banner's action. `hidden`
 *  = a card behind the front one while piled (its chrome peeks, its words
 *  do not). */
export function DockCard({
  row,
  onOpen,
  action,
  hidden = false,
  className,
}: {
  row: DockRow
  onOpen?: () => void
  action?: ReactNode
  hidden?: boolean
  className?: string
}) {
  const Icon = conceptIcon(row.icon as IconConcept)
  const pressable = row.expandable && onOpen !== undefined
  return (
    <GlassCard
      role={pressable ? `button` : undefined}
      tabIndex={pressable && !hidden ? 0 : undefined}
      aria-label={pressable ? `${row.label}: ${row.title}` : undefined}
      aria-hidden={hidden || undefined}
      data-testid={DOCK_TEST_IDS.card}
      data-kind={row.kind}
      data-id={row.id}
      className={cn(
        `border-glass-stroke-strong bg-glass-card-opaque p-3 outline-none`,
        pressable &&
          `cursor-pointer transition-colors duration-fast hover:bg-glass-active focus-visible:ring-[3px] focus-visible:ring-ring/50`,
        className
      )}
      onClick={pressable ? onOpen : undefined}
      onKeyDown={
        pressable
          ? (event) => {
              if (event.key === `Enter` || event.key === ` `) {
                event.preventDefault()
                onOpen?.()
              }
            }
          : undefined
      }
    >
      <div className={cn(`flex items-start gap-2`, hidden && `opacity-0`)}>
        <Icon className={cn(`mt-0.5 size-3.5 shrink-0`, TINT_CLASS[row.tint])} />
        <div className="min-w-0 flex-1">
          <div className="mb-1 flex items-center gap-2">
            <span className={cn(`truncate text-xs font-medium`, TINT_CLASS[row.tint])}>{row.label}</span>
            {row.caption && <span className="min-w-0 truncate text-xs text-muted-foreground">{row.caption}</span>}
            {row.meta && <span className="ml-auto shrink-0 text-xs text-muted-foreground">{row.meta}</span>}
          </div>
          <div className="flex min-w-0 items-center gap-2">
            <span className="min-w-0 flex-1 truncate text-sm text-foreground/90">{row.title}</span>
            {row.expandable ? (
              <OpenIcon className="size-3.5 shrink-0 text-muted-foreground" />
            ) : (
              action && <span className="shrink-0">{action}</span>
            )}
          </div>
        </div>
      </div>
    </GlassCard>
  )
}

const SheetMetaContext = createContext<((meta: ReactNode) => void) | null>(null)

/** The sheet header's trailing meta (the plan's "Question 2 of 3"), set
 *  by the content that knows it. */
export function useDockSheetMeta(meta: ReactNode) {
  const set = useContext(SheetMetaContext)
  useEffect(() => {
    set?.(meta)
    return () => set?.(null)
  }, [set, meta])
}

/** The sheet's ONE scrolling region. */
export function DockSheetBody({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      data-testid={DOCK_TEST_IDS.sheetBody}
      className={cn(`min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pb-3 text-sm`, className)}
    >
      {children}
    </div>
  )
}

/** The sheet's pinned footer: the primary actions. */
export function DockSheetFooter({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      data-testid={DOCK_TEST_IDS.sheetFooter}
      className={cn(
        `flex shrink-0 flex-col gap-2 border-t border-glass-stroke-card px-4 py-3 pb-[max(0.75rem,env(safe-area-inset-bottom))]`,
        className
      )}
    >
      {children}
    </div>
  )
}

/** The sheet's face: the pinned header (the card's icon, label, caption,
 *  the content's meta, the close glyph) over the content (`DockSheetBody`
 *  then `DockSheetFooter`). The live sheet and the styleguide's static one
 *  share it. */
export function DockSheetChrome({
  row,
  onClose,
  title,
  children,
}: {
  row: Pick<DockRow, `icon` | `tint` | `label` | `caption`>
  onClose?: () => void
  /** The accessible title node (the live sheet's Radix title). */
  title?: ReactNode
  children: ReactNode
}) {
  const [meta, setMeta] = useState<ReactNode>(null)
  const Icon = conceptIcon(row.icon as IconConcept)
  return (
    <SheetMetaContext.Provider value={setMeta}>
      <div className="flex shrink-0 items-center gap-2 px-4 pt-3 pb-2">
        <Icon className={cn(`size-3.5 shrink-0`, TINT_CLASS[row.tint])} />
        {title ?? <span className={cn(`text-xs font-medium`, TINT_CLASS[row.tint])}>{row.label}</span>}
        {row.caption && <span className="min-w-0 truncate text-xs text-muted-foreground">{row.caption}</span>}
        <span className="ml-auto flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
          {meta}
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={DOCK_COPY.close}
            title={DOCK_COPY.close}
            data-testid={DOCK_TEST_IDS.close}
            onClick={onClose}
          >
            <CloseIcon />
          </Button>
        </span>
      </div>
      {children}
    </SheetMetaContext.Provider>
  )
}

/** THE sheet a DockCard opens into: at most `sheet.maxHeightFraction` of
 *  the viewport, fitting its content below that; the composer column on
 *  md+, edge to edge (with the grabber and drag to dismiss) on a phone. */
export function DockSheet({
  row,
  open,
  onOpenChange,
  children,
}: {
  row: Pick<DockRow, `id` | `icon` | `tint` | `label` | `caption`>
  open: boolean
  onOpenChange: (open: boolean) => void
  children: ReactNode
}) {
  const isMobile = useIsMobile()
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side="bottom"
        showGrabber={isMobile}
        data-testid={DOCK_TEST_IDS.sheet}
        data-id={row.id}
        className={cn(
          `gap-0 p-0`,
          `md:inset-x-auto md:left-1/2 md:w-full md:max-w-4xl md:-translate-x-1/2 md:border-x`
        )}
        style={{
          maxHeight: `${DOCK_SHEET.maxHeightFraction * 100}dvh`,
          borderTopLeftRadius: DOCK_SHEET.radius,
          borderTopRightRadius: DOCK_SHEET.radius,
        }}
      >
        <DockSheetChrome
          row={row}
          onClose={() => onOpenChange(false)}
          title={
            <SheetTitle className={cn(`text-xs font-medium max-sm:text-xs max-sm:leading-normal`, TINT_CLASS[row.tint])}>
              {row.label}
            </SheetTitle>
          }
        >
          <SheetDescription className="sr-only">{row.caption ?? row.label}</SheetDescription>
          {children}
        </DockSheetChrome>
      </SheetContent>
    </Sheet>
  )
}

/** The styleguide's sheet at rest (no portal): the same chrome in a box
 *  wearing the sheet's face, `height` tall. */
export function DockSheetSpecimenFrame({
  row,
  height = 360,
  children,
}: {
  row: Pick<DockRow, `icon` | `tint` | `label` | `caption`>
  height?: number
  children: ReactNode
}) {
  return (
    <div
      className="flex flex-col overflow-hidden border border-glass-stroke-card bg-glass-bottom"
      style={{ height, borderTopLeftRadius: DOCK_SHEET.radius, borderTopRightRadius: DOCK_SHEET.radius }}
    >
      <DockSheetChrome row={row}>{children}</DockSheetChrome>
    </div>
  )
}

/** AUTO-OPEN (dock.json): the plan or question that just arrived opens its
 *  sheet when the view is in front (`enabled` and the document visible).
 *  Until the host's data has loaded (`ready`: the relay live, the thread's
 *  rows in) and for `autoOpenGraceMs` after that, every card shown only
 *  seeds the memory, so what already waited never opens itself; every id
 *  seen is remembered so a card never re-opens. */
export function useDockAutoOpen(
  rows: readonly DockRow[],
  enabled: boolean,
  onOpen: (id: string) => void,
  ready = true
) {
  const seen = useRef<Set<string>>(new Set())
  const readyAt = useRef<number | null>(null)
  useEffect(() => {
    const ids = rows.map((row) => row.id)
    const now = Date.now()
    if (!ready) {
      readyAt.current = null
      ids.forEach((id) => seen.current.add(id))
      return
    }
    if (readyAt.current === null) readyAt.current = now
    if (now - readyAt.current < DOCK_AUTO_OPEN_GRACE_MS) {
      ids.forEach((id) => seen.current.add(id))
      return
    }
    const next = dockAutoOpen({ previous: [...seen.current], rows })
    ids.forEach((id) => seen.current.add(id))
    if (next === null || !enabled) return
    if (typeof document !== `undefined` && document.visibilityState === `hidden`) return
    onOpen(next)
  }, [rows, enabled, ready, onOpen])
}
