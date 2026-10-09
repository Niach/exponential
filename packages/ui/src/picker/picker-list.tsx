import type { ReactNode } from "react"

import { ComboboxList } from "../combobox"
import { PickerItemBody, type PickerItem } from "./picker-item"

// EXP-1021 — the picker BODY with no surface around it.
//
// `Picker` owns the popover-or-sheet; some hosts are already a surface and
// cannot nest a second one. The relations linker is the shape this exists
// for: a centred command dialog on a pointer device, a full sheet on a phone
// (it is opened from a MENU ITEM, so there is no trigger to hang a popover
// off). Its rows still have to be the primitive's rows — that is the whole
// point of EXP-1021 — so the host keeps its shell and renders this inside it.
//
// The same relationship the internal `ComboboxList` has to `Combobox`
// (EXP-941): a multi pick reads as the row's own highlight, never a glyph.

interface PickerListBase<T extends string> {
  items: readonly PickerItem<T>[]
  /** A filter field at the top of the list. */
  search?: boolean
  searchPlaceholder?: string
  emptyText?: ReactNode
  /** Controlled search text for an external ranking engine
   *  (`useIssueSearchResults`, EXP-892); pass `shouldFilter={false}` with it. */
  query?: string
  onQueryChange?: (query: string) => void
  shouldFilter?: boolean
  loading?: boolean
  error?: ReactNode
  footer?: ReactNode
  /** Rendered inside the filter field's row, before the search glyph. */
  leading?: ReactNode
  /** `field` draws the `SearchField` box around the filter input — for a
   *  page-like host; a popover stays `inline`. */
  inputVariant?: `inline` | `field`
  className?: string
  /** Extra classes on the scrolling list itself, when the host caps it. */
  listClassName?: string
  /** Replaces the row BODY; the selection language stays the primitive's. */
  renderItem?: (item: PickerItem<T>, state: { selected: boolean }) => ReactNode
}

export type PickerListProps<T extends string = string> = PickerListBase<T> &
  (
    | {
        mode: `single`
        value: T | null
        onChange: (value: T) => void
        /** The primitive's none row (the assignee picker's `Unassigned`),
         *  reported through `onNone`: no sentinel value in `items`. */
        noneLabel?: string
        onNone?: () => void
      }
    | {
        mode: `multi`
        value: readonly T[]
        onChange: (value: T[]) => void
        max?: number
      }
  )

export function PickerList<T extends string = string>(
  props: PickerListProps<T>
) {
  const {
    items,
    search = false,
    searchPlaceholder,
    emptyText,
    query,
    onQueryChange,
    shouldFilter,
    loading,
    error,
    footer,
    leading,
    inputVariant,
    className,
    listClassName,
    renderItem,
  } = props

  const selection =
    props.mode === `multi`
      ? ({
          multiple: true as const,
          value: props.value,
          onChange: props.onChange,
          max: props.max,
        } as const)
      : ({
          multiple: false as const,
          value: props.value,
          noneLabel: props.noneLabel,
          onChange: (next: T | null) => {
            if (next === null) props.onNone?.()
            else props.onChange(next)
          },
        } as const)

  return (
    <ComboboxList
      {...selection}
      options={items}
      searchable={search}
      placeholder={searchPlaceholder}
      emptyText={emptyText}
      query={query}
      onQueryChange={onQueryChange}
      shouldFilter={shouldFilter}
      loading={loading}
      error={error}
      footer={footer}
      leading={leading}
      inputVariant={inputVariant}
      className={className}
      listClassName={listClassName}
      renderOption={(item, state) =>
        renderItem ? renderItem(item, state) : <PickerItemBody item={item} />
      }
    />
  )
}
