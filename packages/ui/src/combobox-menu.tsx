import * as React from "react"

import { cn } from "./cn"
import {
  ComboboxOptionBody,
  SelectionGlyph,
  resolveSelection,
  type ComboboxSelection,
  type RenderComboboxOption,
} from "./combobox-core"
import { ContextMenuItem } from "./context-menu"
import { DropdownMenuItem } from "./dropdown-menu"
import type { PickerOption } from "./picker-option"

// EXP-957 — the Combobox's MENU arm: the same option rows as items inside a
// Radix ContextMenu or DropdownMenu.
//
// A Radix menu is a different shell from the popover: no cmdk, no search
// field, the menu owns keyboard typeahead and focus, and its items must be
// its own (a `Combobox` nested in a submenu would stack a second portal and
// two focus traps). So this arm renders `PickerOption`s as the HOST's items,
// with the primitive's selection glyph instead of the menu's radio dot or
// checkbox tick — which is what retires those two "this is picked" idioms.
//
// `menu` names the host because Radix scopes an item to its menu package:
// a ContextMenu item inside a DropdownMenu content is undefined behaviour,
// and the two `@exp/ui` wrappers carry their host's row metrics, so the rows
// read like the plain items beside them.
//
// Single select closes on pick, as any menu item does. Multi stays open
// across toggles — a batch is several picks — by cancelling Radix's select.

type ComboboxMenuShell = `context` | `dropdown`

const MENU_ITEM = {
  context: ContextMenuItem,
  dropdown: DropdownMenuItem,
} as const

interface ComboboxMenuItemsBaseProps<TValue extends string> {
  /** The Radix shell the rows sit in. */
  menu: ComboboxMenuShell
  options: readonly PickerOption<TValue>[]
  /** The row BODY only; the selection glyph stays the primitive's. */
  renderOption?: RenderComboboxOption<TValue>
  /** A disabled row shown instead of an EMPTY list ("No labels yet").
   *  Nothing renders for an empty list when omitted. */
  emptyText?: React.ReactNode
  /** Extra classes on every row. */
  className?: string
}

type ComboboxMenuItemsProps<TValue extends string> =
  ComboboxMenuItemsBaseProps<TValue> & ComboboxSelection<TValue>

function ComboboxMenuItems<TValue extends string>(
  props: ComboboxMenuItemsProps<TValue>
) {
  const { menu, options, renderOption, emptyText, className } = props
  const Item = MENU_ITEM[menu]
  const selection = resolveSelection(props)
  const { multiple, noneLabel, noneMarked } = selection
  const role = multiple ? `menuitemcheckbox` : `menuitemradio`

  const onPick = (option: PickerOption<TValue>) => (event: Event) => {
    if (multiple) {
      event.preventDefault()
    }
    selection.pick(option)
  }

  return (
    <>
      {noneLabel !== undefined && (
        <Item
          role="menuitemradio"
          aria-checked={noneMarked}
          data-combobox-none="true"
          className={className}
          onSelect={() => selection.pickNone()}
        >
          <span className="min-w-0 flex-1 truncate">{noneLabel}</span>
          <SelectionGlyph
            arity="single"
            state={noneMarked ? `selected` : `unselected`}
          />
        </Item>
      )}
      {options.length === 0 && emptyText !== undefined && emptyText !== null ? (
        <Item disabled data-combobox-empty="true" className={className}>
          {emptyText}
        </Item>
      ) : null}
      {options.map((option) => {
        const state = selection.stateOf(option)
        const isSelected = state === `selected`
        return (
          <Item
            key={option.value}
            role={role}
            aria-checked={state === `indeterminate` ? `mixed` : isSelected}
            data-selected-state={state}
            // cmdk stamps the popover arm's rows with their value; the menu
            // arm does the same, so a test finds a row by identity on both.
            data-value={option.value}
            disabled={selection.isDisabled(option)}
            className={cn(className)}
            onSelect={onPick(option)}
          >
            {multiple && <SelectionGlyph arity="multi" state={state} />}
            {renderOption ? (
              renderOption(option, { selected: isSelected })
            ) : (
              <ComboboxOptionBody option={option} />
            )}
            {!multiple && <SelectionGlyph arity="single" state={state} />}
          </Item>
        )
      })}
    </>
  )
}

export { ComboboxMenuItems }
export type { ComboboxMenuItemsProps, ComboboxMenuShell }
