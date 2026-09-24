import type { ReactNode } from "react"

import { ComboboxList } from "../combobox"
import { PickerItemBody, pickerItemKeywords, type PickerItem } from "./picker"

// EXP-1021 — the picker BODY with no surface around it.
//
// `Picker` owns the popover-or-sheet; some hosts are already a surface and
// cannot nest a second one. The relations linker is the shape this exists
// for: a centred command dialog on a pointer device, a full sheet on a phone
// (it is opened from a MENU ITEM, so there is no trigger to hang a popover
// off). Its rows still have to be the primitive's rows — that is the whole
// point of EXP-1021 — so the host keeps its shell and renders this inside it.
//
// The same relationship `ComboboxList` has to `Combobox` (EXP-941), one level
// up: the selection language here is the picker's (a multi pick reads as the
// row's own highlight), never the circles.

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
    | { mode: `single`; value: T | null; onChange: (value: T) => void }
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

  const byValue = new Map(items.map((item) => [item.value, item]))
  const options = items.map((item) => ({
    value: item.value,
    label: item.label,
    keywords: pickerItemKeywords(item),
    disabled: item.disabled,
    checked: item.checked,
  }))

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
          onChange: (next: T | null) => {
            if (next !== null) props.onChange(next)
          },
        } as const)

  return (
    <ComboboxList
      {...selection}
      options={options}
      // The primitive's own selection language: highlight, never circles.
      selectionStyle="highlight"
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
      renderOption={(option, state) => {
        const item = byValue.get(option.value)
        if (!item) return null
        return renderItem ? (
          renderItem(item, state)
        ) : (
          <PickerItemBody item={item} />
        )
      }}
    />
  )
}
