import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react"
import {
  dock,
  dockAutoOpen,
  DOCK_AUTO_OPEN_GRACE_MS,
  DOCK_COPY,
  DOCK_ICONS,
  DOCK_SHEET,
  DOCK_TEST_IDS,
  type DockRow,
  type DockTint,
} from "@exp/domain-contract/dock"
import type { IconConcept } from "@exp/icons"
import { Button } from "./button"
import { cn } from "./cn"
import {
  DOCK_CARD_CLASSES,
  DockCard as PrimitiveDockCard,
  DockPile as PrimitiveDockPile,
} from "@exponential_app/ui-react/primitives/dock-card"
import { conceptIcon } from "./icons.generated"
import { DialogPresentationContext } from "@exponential_app/ui-react/primitives/dialog"
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

const CloseIcon = conceptIcon(DOCK_ICONS.close as IconConcept)

const TINT_CLASS: Record<DockTint, string> = DOCK_CARD_CLASSES.tint

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
  const [expanded, setExpanded] = useState(initialExpanded)
  const view = useMemo(() => dock({ rows, expanded, openId }), [rows, expanded, openId])
  const openRow = view.openId === null ? null : (rows.find((row) => row.id === view.openId) ?? null)
  const close = useCallback(() => onOpenChange(null), [onOpenChange])
  // Release train 2026-10-23 (N-12 F2): the open card vanished (the run
  // ended, the ask was answered): forget its id, or a later row with the
  // same fixed id (`tasks`, `queued`, `banner:*`) reopens the sheet unasked.
  useEffect(() => {
    if (openId !== null && view.openId === null) onOpenChange(null)
  }, [openId, view.openId, onOpenChange])
  // Release train 2026-10-23 (N-12 F1): a phone's band stays mounted with
  // no cards (it carries the work bar's clearance, the floating capsule
  // sits over it) and is re-measured whenever it mounts or resizes, so the
  // transcript's last rows never run under the capsule or a later card.
  const present = view.mode !== `none` || !!composer || phoneClearance
  useLayoutEffect(() => {
    if (!onHeight) return
    const node = bandRef.current
    if (!node) {
      onHeight(0)
      return
    }
    const measure = () => onHeight(node.offsetHeight)
    measure()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(measure)
    observer.observe(node)
    return () => observer.disconnect()
  }, [onHeight, present])
  const sheet = openRow ? renderSheet(openRow, close) : null
  if (!present) return null
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
            // An empty phone band is clearance only: taps reach the transcript.
            `relative flex flex-col gap-1.5`,
            view.mode === `none` && !composer ? `pointer-events-none` : `pointer-events-auto`,
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

/** The pile: THE Dock pile primitive (`DockPile`, @exponential_app/ui-react
 *  primitives: the toast's stack geometry with the Dock's numbers, anchored
 *  at the bottom), the front card last; collapsed = the older cards peek
 *  out above it, their words hidden; a hover-capable pointer fans it on
 *  hover, a touch on it fans it (the next touch on a card opens the card). */
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
  const fanned = expanded || single
  const frontId = rows.at(-1)?.id ?? null
  return (
    <PrimitiveDockPile
      data-testid={DOCK_TEST_IDS.pile}
      expanded={fanned}
      onExpandedChange={hover || !fanned ? onExpandedChange : (next) => (next ? undefined : onExpandedChange(false))}
      items={rows.map((row) => ({
        id: row.id,
        node: (
          <DockCard
            row={row}
            hidden={!fanned && row.id !== frontId}
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
        ),
      }))}
    />
  )
}

/** A DockCard collapsed = THE DockCard primitive (@exponential_app/ui-react
 *  primitives `DockCard`, its Solid twin paints the same): the kind glyph,
 *  the tinted label, the muted caption, the trailing meta, the one title
 *  line and the open chevron (an expandable card) or the banner's action.
 *  `hidden` = a card behind the front one while piled (its chrome peeks,
 *  its words do not). */
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
  return (
    <PrimitiveDockCard
      tint={row.tint}
      icon={conceptIcon(row.icon as IconConcept)}
      label={row.label}
      caption={row.caption ?? undefined}
      meta={row.meta ?? undefined}
      title={row.title}
      onOpen={row.expandable ? (onOpen ?? noop) : undefined}
      openLabel={row.expandable ? `${row.label}: ${row.title}` : undefined}
      trailing={row.expandable ? undefined : action}
      hidden={hidden}
      data-testid={DOCK_TEST_IDS.card}
      data-kind={row.kind}
      data-id={row.id}
      className={className}
    />
  )
}

const noop = () => {}

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
          // N-12: ONE gray: the sheet wears the card's fill, not the phone
          // sheet's darker ground.
          `gap-0 bg-glass-card-opaque p-0`,
          `md:inset-x-auto md:left-1/2 md:w-full md:max-w-4xl md:-translate-x-1/2 md:border-x`
        )}
        style={{
          maxHeight: `${DOCK_SHEET.maxHeightFraction * 100}dvh`,
          borderTopLeftRadius: DOCK_SHEET.radius,
          borderTopRightRadius: DOCK_SHEET.radius,
        }}
      >
        {/* N-6/N-14: the Dock sheet is a SHEET host: every picker inside it
            (the suggestions' Start on… / Start with…) takes the inline
            presentation. */}
        <DialogPresentationContext.Provider value="sheet">
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
        </DialogPresentationContext.Provider>
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
      className="flex flex-col overflow-hidden border border-glass-stroke-card bg-glass-card-opaque"
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
