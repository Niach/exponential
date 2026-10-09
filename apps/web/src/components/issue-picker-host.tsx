import { useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import {
  Dialog,
  DialogContent,
  DialogTitle,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  PickerList,
  Sheet,
  SheetContent,
  SheetTitle,
  issuePickerItems,
  useIsMobile,
  type PickerItem,
} from "@exp/ui"
import { toStatusPickerStatus } from "@/components/issue-properties/status-dropdown"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import {
  useIssueSearchResults,
  type IssueSearchServerHit,
} from "@/hooks/use-issue-search-results"
import type { IssueSearchRow } from "@/lib/issue-search"

// UI cleanup batch — ONE issue picker host for every searchable issue list:
// the relation linker and the duplicate picker (`IssuePickerDialog`), the
// team search (`IssueSearchSheet`) and the composer's issue batch
// (`launch-dialog/issue-picker`) were three copies of the same engine + body
// in three shells. The ENGINE is the shared one (EXP-892,
// `useIssueSearchResults`: local ranking now, the server's full-text pass
// spliced in behind; cmdk's own filter OFF so the ranking renders verbatim),
// the BODY is the shared picker list (EXP-1021: the status glyph in its
// colour, then `IDENT Title`, ×4), single (a pick) or multi (pinned picks
// first, a toggle per row). Only the SHELL is a prop:
//
//   dialog   a centred dialog md+, the full sheet on a phone (opened from a
//            menu row: there is no trigger to hang a popover off)
//   page     the search: a centred dialog md+, a full-screen page on a phone
//            with a back arrow in the field's row
//   popover  anchored at a trigger md+, a bottom sheet on a phone
//
// `IssuePickerBody` is the body alone, for a host that already is a surface
// (the composer's "+" menu submenu).

export interface IssuePickerRow extends IssueSearchRow {
  id: string
  identifier: string
  title: string
}

interface IssuePickerBodyBase<R extends IssuePickerRow> {
  /** The team `issues.search` runs against; undefined = local only. */
  teamId: string | undefined
  /** The locally synced pool to rank. */
  rows: readonly R[]
  /** A server hit as a row (`useIssueSearchResults`), or null to drop it. */
  resolveHit: (hit: IssueSearchServerHit) => R | null
  limit?: number
  exclude?: readonly string[]
  /** false = never call the server. */
  server?: boolean
  /** What an empty query lists: the newest rows, or nothing. */
  emptyQuery?: `recent` | `none`
  searchPlaceholder?: string
  /** The empty line, given the query. */
  emptyText?: (query: string) => ReactNode
  /** Replaces the row BODY (the selection language stays the primitive's). */
  renderItem?: (row: R, item: PickerItem) => ReactNode
  /** Resolve each row's status glyph into its item (default). `false` for a
   *  `renderItem` that draws its own — resolving one per hit per keystroke
   *  would be thrown away. */
  statusGlyphs?: boolean
  /** Rendered in the field's row before the search glyph (a back arrow). */
  leading?: ReactNode
  inputVariant?: `inline` | `field`
  className?: string
  listClassName?: string
}

export type IssuePickerBodyProps<R extends IssuePickerRow> =
  IssuePickerBodyBase<R> &
    (
      | {
          mode: `single`
          onPick: (row: R) => void
        }
      | {
          mode: `multi`
          /** The picked rows, pinned first. */
          checked: readonly R[]
          onToggle: (id: string) => void
        }
    )

const NO_ROWS: never[] = []

/** The engine + the shared list, with no surface around it. */
export function IssuePickerBody<R extends IssuePickerRow>(
  props: IssuePickerBodyProps<R> & {
    query: string
    onQueryChange: (query: string) => void
  }
) {
  const {
    teamId,
    rows,
    resolveHit,
    limit,
    exclude,
    server,
    emptyQuery,
    query,
    onQueryChange,
    searchPlaceholder = `Search issues…`,
    emptyText,
    renderItem,
    statusGlyphs = true,
    leading,
    inputVariant,
    className,
    listClassName,
  } = props
  const { resolve } = useTeamStatusesContext()
  const checked = props.mode === `multi` ? props.checked : (NO_ROWS as R[])
  const checkedIds = useMemo(
    () => new Set(checked.map((row) => row.id)),
    [checked]
  )
  // Multi: the picks pin first, so the engine ranks only the rest.
  const pool = useMemo(
    () =>
      checkedIds.size === 0 ? rows : rows.filter((row) => !checkedIds.has(row.id)),
    [rows, checkedIds]
  )
  const { results } = useIssueSearchResults<R>({
    teamId,
    query,
    rows: pool,
    limit,
    exclude,
    resolveHit,
    server,
    emptyQuery,
  })
  const ordered = useMemo(() => [...checked, ...results], [checked, results])
  const byId = useMemo(() => new Map(ordered.map((row) => [row.id, row])), [ordered])
  const items = useMemo(
    () =>
      issuePickerItems(
        ordered.map((row) => {
          if (!statusGlyphs) {
            return { id: row.id, identifier: row.identifier, title: row.title }
          }
          // The issue's own status row, resolved against the team's synced
          // statuses exactly as every list row resolves it.
          const status = toStatusPickerStatus(resolve(row as never))
          return {
            id: row.id,
            identifier: row.identifier,
            title: row.title,
            icon: status.icon,
            color: status.colorHex ?? undefined,
          }
        })
      ),
    [ordered, statusGlyphs, resolve]
  )
  const shared = {
    items,
    search: true,
    // The engine already ranked and capped the rows (EXP-892).
    shouldFilter: false,
    query,
    onQueryChange,
    searchPlaceholder,
    emptyText: emptyText?.(query),
    leading,
    inputVariant,
    className,
    listClassName,
    renderItem: renderItem
      ? (item: PickerItem) => {
          const row = byId.get(item.value)
          return row ? renderItem(row, item) : null
        }
      : undefined,
  }

  // The picker marker every typed picker is asserted through.
  const marker = (list: ReactNode) => (
    <div
      data-slot="picker"
      data-picker-mode={props.mode}
      data-picker-search="true"
      className="contents"
    >
      {list}
    </div>
  )

  if (props.mode === `multi`) {
    const values = checked.map((row) => row.id)
    return marker(
      <PickerList
        mode="multi"
        value={values}
        onChange={(next) => {
          const changed =
            next.find((id) => !checkedIds.has(id)) ??
            values.find((id) => !next.includes(id))
          if (changed) props.onToggle(changed)
        }}
        {...shared}
      />
    )
  }
  return marker(
    <PickerList
      mode="single"
      // Picking LINKS or NAVIGATES: nothing is ever the picked row.
      value={null}
      onChange={(id) => {
        const row = byId.get(id)
        if (row) props.onPick(row)
      }}
      {...shared}
    />
  )
}

export type IssuePickerHostProps<R extends IssuePickerRow> =
  IssuePickerBodyProps<R> & {
    shell: `dialog` | `page` | `popover`
    open: boolean
    onOpenChange: (open: boolean) => void
    /** The surface's title (sr-only in the dialog shells, the phone sheet's
     *  headline in the popover shell). */
    title: string
    /** The popover shell's trigger (ONE element, wrapped `asChild`). */
    trigger?: ReactNode
    /** A single pick closes the surface (default); `false` keeps it. */
    closeOnPick?: boolean
    /** Extra classes on the shell's content. */
    contentClassName?: string
    "data-testid"?: string
  }

/** The phone page shell's back arrow, handed the close. */
export type IssuePickerBackButton = (close: () => void) => ReactNode

export function IssuePickerHost<R extends IssuePickerRow>(
  props: IssuePickerHostProps<R> & { backButton?: IssuePickerBackButton }
) {
  const {
    shell,
    open,
    onOpenChange,
    title,
    trigger,
    closeOnPick = true,
    contentClassName,
    backButton,
  } = props
  const testId = props[`data-testid`]
  const isMobile = useIsMobile()
  const [query, setQuery] = useState(``)
  const handleOpenChange = (next: boolean) => {
    onOpenChange(next)
    if (!next) setQuery(``)
  }
  const bodyProps: IssuePickerBodyProps<R> =
    props.mode === `single`
      ? {
          ...props,
          onPick: (row: R) => {
            if (closeOnPick) handleOpenChange(false)
            props.onPick(row)
          },
        }
      : props
  // Pickers off the shell never call the server.
  const engine = { ...bodyProps, server: (props.server ?? true) && open }

  // cmdk's input takes no `autoFocus`, and the dialog shells' own open-focus
  // lands on their first tabbable child; the field is focused explicitly.
  const shellRef = useRef<HTMLDivElement | null>(null)
  useEffect(() => {
    if (!open || shell === `popover`) return
    const frame = requestAnimationFrame(() => {
      shellRef.current
        ?.querySelector<HTMLInputElement>(`[data-slot=command-input]`)
        ?.focus()
    })
    return () => cancelAnimationFrame(frame)
  }, [open, shell])

  if (shell === `popover`) {
    return (
      <MobilePopover open={open} onOpenChange={handleOpenChange}>
        {trigger && <MobilePopoverTrigger asChild>{trigger}</MobilePopoverTrigger>}
        <MobilePopoverContent
          align="start"
          collisionPadding={12}
          mobileTitle={title}
          data-testid={testId}
          className={`flex max-h-(--radix-popover-content-available-height) w-[22rem] flex-col overflow-hidden p-0 ${contentClassName ?? ``}`}
        >
          <IssuePickerBody {...engine} query={query} onQueryChange={setQuery} />
        </MobilePopoverContent>
      </MobilePopover>
    )
  }

  if (!open) return null

  if (shell === `page` && isMobile) {
    return (
      <Sheet open={open} onOpenChange={handleOpenChange}>
        {/* Page-like, not a sheet: it covers the whole screen, so it takes
            the New-issue page's chrome — no grabber, no radius, a leading
            back arrow in the field's row (EXP-687/971). */}
        <SheetContent
          ref={shellRef}
          side="bottom"
          showGrabber={false}
          data-testid={testId}
          className="top-0 flex h-[100dvh] max-h-none flex-col gap-0 rounded-none p-0"
        >
          <SheetTitle className="sr-only">{title}</SheetTitle>
          <IssuePickerBody
            {...engine}
            query={query}
            onQueryChange={setQuery}
            leading={backButton?.(() => handleOpenChange(false))}
            inputVariant="field"
            className={`${props.className ?? ``} **:data-[slot=command-input-wrapper]:border-border/50 **:data-[slot=command-input]:rounded-full`}
          />
        </SheetContent>
      </Sheet>
    )
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        ref={shellRef}
        mobile={shell === `dialog` ? `sheet-full` : `sheet`}
        showCloseButton={false}
        data-testid={testId}
        className={`flex flex-col gap-0 overflow-hidden p-0 sm:p-0 max-sm:px-0 sm:top-[15%] sm:max-h-[60vh] sm:max-w-lg sm:translate-y-0 ${contentClassName ?? ``}`}
      >
        <DialogTitle className="sr-only">{title}</DialogTitle>
        <IssuePickerBody {...engine} query={query} onQueryChange={setQuery} />
      </DialogContent>
    </Dialog>
  )
}
