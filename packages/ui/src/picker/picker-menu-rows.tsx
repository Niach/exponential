import type { ReactNode } from "react"

import { cn } from "../cn"
import {
  SelectionCheck,
  pickedAttr,
  pickedRowClass,
  resolveSelection,
} from "../combobox-core"
import { MenuRow } from "../menu"
import { PickerItemBody, type PickerItem } from "./picker-item"

// The picker's MENU arm (was `ComboboxMenuItems`, EXP-957): the typed
// pickers' rows as rows INSIDE a `Menu` submenu — the issue context menu's
// Status / Assignee / Labels …, the bulk bar, the composer's Effort.
//
// A menu is a different shell from the popover: no cmdk, no search field, the
// menu owns typeahead and focus, and its rows must be its own (`MenuRow`, so
// the same body is a Radix item in the dropdown and a touch row in the phone
// sheet). The selection language is the picker's, ONE on every surface: a
// single pick wears the trailing check, a multi pick the row's highlight —
// never the menu's radio dot or checkbox tick, never the old circle pair.
//
// Single closes on pick, as any menu row does. Multi stays open across
// toggles — a batch is several picks.

interface PickerMenuRowsBase<T extends string> {
  items: readonly PickerItem<T>[]
  /** Replaces the row BODY; the selection language stays the primitive's. */
  renderItem?: (item: PickerItem<T>, state: { selected: boolean }) => ReactNode
  /** A disabled row shown instead of an EMPTY list ("No labels yet"). */
  emptyText?: ReactNode
  /** Extra classes on every row. */
  className?: string
}

export type PickerMenuRowsProps<T extends string = string> =
  PickerMenuRowsBase<T> &
    (
      | {
          mode: `single`
          value: T | null
          onChange: (value: T) => void
          /** A FIRST row that clears the pick ("Unassigned"), reported
           *  through `onNone`. */
          noneLabel?: string
          onNone?: () => void
          /** The value is not ONE value (a bulk edit): no row is marked. */
          indeterminate?: boolean
        }
      | {
          mode: `multi`
          value: readonly T[]
          onChange: (value: T[]) => void
          max?: number
        }
    )

export function PickerMenuRows<T extends string = string>(
  props: PickerMenuRowsProps<T>
) {
  const { items, renderItem, emptyText, className } = props
  const selection = resolveSelection<T>(
    props.mode === `multi`
      ? {
          multiple: true,
          value: props.value,
          onChange: props.onChange,
          max: props.max,
        }
      : {
          multiple: false,
          value: props.value,
          noneLabel: props.noneLabel,
          indeterminate: props.indeterminate,
          onChange: (next) => {
            if (next === null) props.onNone?.()
            else props.onChange(next)
          },
        }
  )
  const { multiple, noneLabel, noneMarked } = selection
  const role = multiple ? `menuitemcheckbox` : `menuitemradio`

  return (
    <>
      {noneLabel !== undefined && (
        <MenuRow
          role="menuitemradio"
          aria-checked={noneMarked}
          className={className}
          data={{ "data-picker-none": `true` }}
          onSelect={() => selection.pickNone()}
        >
          <span className="min-w-0 flex-1 truncate">{noneLabel}</span>
          <SelectionCheck state={noneMarked ? `selected` : `unselected`} />
        </MenuRow>
      )}
      {items.length === 0 && emptyText !== undefined && emptyText !== null ? (
        <MenuRow disabled className={className} data={{ "data-picker-empty": `true` }}>
          {emptyText}
        </MenuRow>
      ) : null}
      {items.map((item) => {
        const state = selection.stateOf(item)
        const isSelected = state === `selected`
        return (
          <MenuRow
            key={item.value}
            role={role}
            aria-checked={state === `indeterminate` ? `mixed` : isSelected}
            keepOpen={multiple}
            disabled={selection.isDisabled(item)}
            className={cn(pickedRowClass(multiple, state), className)}
            // A test finds a row by identity on every arm, like cmdk's rows.
            data={{
              "data-value": item.value,
              "data-picked": pickedAttr(multiple, state),
            }}
            onSelect={() => selection.pick(item)}
          >
            {renderItem ? (
              renderItem(item, { selected: isSelected })
            ) : (
              <PickerItemBody item={item} />
            )}
            {!multiple && <SelectionCheck state={state} />}
          </MenuRow>
        )
      })}
    </>
  )
}
