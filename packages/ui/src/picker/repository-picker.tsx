import { useMemo, type ReactNode } from "react"

import { cn } from "../cn"
import { conceptIcon } from "../icons.generated"
import { Picker, type PickerItem, type PickerSurfaceProps } from "./picker"
import { PickerList } from "./picker-list"
import type { PickerTriggerVariant } from "./picker-trigger"

// The UI cleanup batch folded `repository-picker.tsx` in here: ONE
// `RepositoryPicker` (the popover/sheet over the team's repositories — the
// composer's Repository word, the board form, a trigger's repo row) and the
// bare `RepositoryPickerList` body below.
//
// SLOP-7: the repository picker's BODY — a `PickerList` of `owner/name` rows,
// each with the GitHub glyph, a private lock and an optional trailing tag
// ("matches board", "used by Website"). Two hosts share it: the readiness
// checklist's "Choose repository" (the team's registry, EXP-1121) and the
// Add-repository / guided-page picker (the live push-able listing off
// GitHub). It is a list and nothing else: the host owns the search state
// (or lets the list filter), the footer (Add from GitHub…, Install on
// another account), the empty copy and what a pick does.

const GithubGlyph = conceptIcon(`ui-github`)
const PrivateGlyph = conceptIcon(`ui-private`)

export interface RepositoryPickerRow {
  /** The pick value (a registry id, or the full name for a live row). */
  id: string
  fullName: string
  private?: boolean
  /** A trailing note; `emphasis` paints it green ("matches board"). */
  tag?: string | null
  emphasis?: boolean
}

export interface RepositoryPickerListProps {
  rows: readonly RepositoryPickerRow[]
  onPick: (id: string) => void
  /** A filter field at the top. With `query`/`onQueryChange` the host
   * ranks (and passes `shouldFilter={false}`); without, the list filters. */
  search?: boolean
  searchPlaceholder?: string
  query?: string
  onQueryChange?: (query: string) => void
  shouldFilter?: boolean
  loading?: boolean
  emptyText?: ReactNode
  footer?: ReactNode
  /** The scrolling list's cap, when the host sets one. */
  listClassName?: string
  className?: string
}

export function repositoryPickerItems(
  rows: readonly RepositoryPickerRow[]
): PickerItem[] {
  return rows.map((row) => ({
    value: row.id,
    label: row.fullName,
    icon: GithubGlyph,
    keywords: [row.fullName],
  }))
}

/** The row body both arms draw: glyph, `owner/name`, the private lock, the
 *  trailing tag. */
function RepositoryRowBody({
  label,
  row,
}: {
  label: ReactNode
  row: RepositoryPickerRow | undefined
}) {
  return (
    <>
      <GithubGlyph aria-hidden className="size-4 shrink-0" />
      <span className="min-w-0 flex-1 truncate text-left text-sm">{label}</span>
      {row?.private && (
        <PrivateGlyph
          className="size-3.5 shrink-0 text-muted-foreground"
          aria-label="Private"
        />
      )}
      {row?.tag && (
        <span
          className={cn(
            `shrink-0 text-xs`,
            row.emphasis ? `text-emerald-400` : `text-muted-foreground`
          )}
        >
          {row.tag}
        </span>
      )}
    </>
  )
}

export interface RepositoryPickerProps extends PickerSurfaceProps {
  repositories: readonly RepositoryPickerRow[]
  value: string | null
  onChange: (id: string) => void
  /** A bespoke trigger; omitted = the primitive's own in `triggerVariant`. */
  trigger?: ReactNode
  triggerVariant?: PickerTriggerVariant
  triggerLabel?: string
  mobileTitle?: string
  /** A filter field; on by default once the list is long. */
  search?: boolean
  /** A first row that clears the pick ("No repository"). */
  noneLabel?: string
  onNone?: () => void
  emptyText?: ReactNode
  footer?: ReactNode
  disabled?: boolean
  className?: string
}

/** THE repository picker: one pick out of the team's repositories. */
export function RepositoryPicker({
  repositories,
  mobileTitle = `Repository`,
  search,
  emptyText = `No repositories`,
  ...props
}: RepositoryPickerProps) {
  const items = useMemo(() => repositoryPickerItems(repositories), [repositories])
  const byId = useMemo(
    () => new Map(repositories.map((row) => [row.id, row])),
    [repositories]
  )
  return (
    <Picker
      mode="single"
      items={items}
      mobileTitle={mobileTitle}
      search={search ?? repositories.length > 7}
      searchPlaceholder="Search repositories…"
      emptyText={emptyText}
      renderItem={(item) => (
        <RepositoryRowBody label={item.label} row={byId.get(item.value)} />
      )}
      {...props}
    />
  )
}

export function RepositoryPickerList({
  rows,
  onPick,
  search = true,
  searchPlaceholder,
  query,
  onQueryChange,
  shouldFilter,
  loading,
  emptyText,
  footer,
  listClassName,
  className,
}: RepositoryPickerListProps) {
  const items = useMemo(() => repositoryPickerItems(rows), [rows])
  const byId = useMemo(() => new Map(rows.map((row) => [row.id, row])), [rows])
  return (
    <div
      className={cn(
        `overflow-hidden rounded-md border border-glass-stroke-card bg-glass-card`,
        className
      )}
      data-testid="repository-picker"
    >
      <PickerList
        mode="single"
        value={null}
        onChange={onPick}
        items={items}
        search={search}
        searchPlaceholder={searchPlaceholder}
        query={query}
        onQueryChange={onQueryChange}
        shouldFilter={shouldFilter}
        loading={loading}
        emptyText={emptyText}
        listClassName={listClassName}
        footer={footer}
        renderItem={(item) => (
          <RepositoryRowBody label={item.label} row={byId.get(item.value)} />
        )}
      />
    </div>
  )
}
