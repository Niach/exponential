import { useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import {
  Dialog,
  DialogContent,
  DialogTitle,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  SearchList,
  Sheet,
  SheetContent,
  SheetTitle,
  issueSearchItems,
  useIsMobile,
  type SearchListDensity,
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
// the BODY is THE search list (SLOP-28, `@exp/ui` `SearchList`: the status
// glyph in its colour, the mono identifier, the title, ×4; at the default
// limit, no cap), single (a pick) or multi (pinned picks first with their
// check, a toggle per row). Only the SHELL is a prop:
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
  /** Rendered in the input row before the search glyph (a back arrow). */
  leading?: ReactNode
  /** At most this many rows visible (the search list's eight by default);
   *  `null` = the shell caps the height (a full-screen page). */
  maxVisibleRows?: number | null
  /** Under the rows (the Implement submenu's button). */
  footer?: ReactNode
  autoFocus?: boolean
  /** Pointer or phone geometry; defaults to the viewport's. */
  density?: SearchListDensity
  /** Backspace in the empty input (the `#` typeahead deletes its `#`). */
  onBackspaceEmpty?: () => void
  inputTestId?: string
  className?: string
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

/** The engine + THE search list (SLOP-28), with no surface around it. */
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
    searchPlaceholder = `Search issues`,
    emptyText,
    leading,
    maxVisibleRows,
    footer,
    autoFocus,
    density,
    onBackspaceEmpty,
    inputTestId,
    className,
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
  // The search list's issue rows: the issue's own status row (resolved
  // against the team's synced statuses exactly as every list row resolves
  // it), the identifier in its mono column, the title.
  const items = useMemo(
    () =>
      issueSearchItems(
        ordered.map((row) => {
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
    [ordered, resolve]
  )
  const shared = {
    items,
    // The engine already ranked and capped the rows (EXP-892).
    shouldFilter: false,
    query,
    onQueryChange,
    placeholder: searchPlaceholder,
    emptyText: emptyText?.(query),
    leading,
    maxVisibleRows,
    footer,
    autoFocus,
    density,
    onBackspaceEmpty,
    inputTestId,
    className,
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
      <SearchList
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
    <SearchList
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
  const engine = {
    ...bodyProps,
    server: (props.server ?? true) && open,
    density: props.density ?? (isMobile ? (`phone` as const) : (`pointer` as const)),
  }

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
            maxVisibleRows={null}
            className={`${props.className ?? ``} **:data-[slot=command-input]:rounded-full`}
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
        {/* A phone's sheet is the height: the list fills it. */}
        <IssuePickerBody
          {...engine}
          query={query}
          onQueryChange={setQuery}
          maxVisibleRows={isMobile ? null : undefined}
        />
      </DialogContent>
    </Dialog>
  )
}
