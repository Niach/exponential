import * as React from "react"

import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import type { PickerItem } from "./picker/picker-item"

// EXP-957 — what every Combobox ARM shares: the selection model and the glyph
// that draws it.
//
// `ComboboxList` (cmdk rows in a popover or sheet) and `PickerMenuRows` (rows
// inside a `Menu` submenu) are different shells over the SAME vocabulary.
// Keeping the arithmetic and the glyph here, and NOT exporting either from the
// barrel, is what stops a third shell — or an app row — from inventing another
// "this is picked" language. ONE language since the UI cleanup batch (the
// circle pair EXP-957 drew on the multi arm is gone everywhere):
//
//   single  the picked row wears a trailing `ui-check`; nothing else moves
//   multi   the row's own highlight (`data-picked`), never a glyph; a row
//           picked on only SOME of the things being edited reads `mixed`

const CheckGlyph = conceptIcon(`ui-check`)

interface ComboboxSingleSelection<TValue extends string> {
  multiple?: false
  value: TValue | null
  onChange: (value: TValue | null) => void
  /** A first row that reports `null` ("Unassign", "None"). */
  noneLabel?: string
  /** The value is not ONE value — a bulk edit over rows that disagree. No
   *  row is marked, not even the `noneLabel` row, which `value: null` on its
   *  own would mark. */
  indeterminate?: boolean
}

interface ComboboxMultiSelection<TValue extends string> {
  multiple: true
  value: readonly TValue[]
  onChange: (value: TValue[]) => void
  /** At the cap the unselected rows go disabled; the picked ones still toggle
   *  off, or the user would be stuck. */
  max?: number
}

type ComboboxSelection<TValue extends string> =
  | ComboboxSingleSelection<TValue>
  | ComboboxMultiSelection<TValue>

/** What one row reads as. `indeterminate` exists on the multi arm only. */
type SelectionState = `selected` | `unselected` | `indeterminate`

function isMultiple<TValue extends string>(
  selection: ComboboxSelection<TValue>
): selection is ComboboxMultiSelection<TValue> {
  return selection.multiple === true
}

function selectedValuesOf<TValue extends string>(
  selection: ComboboxSelection<TValue>
): readonly TValue[] {
  if (isMultiple(selection)) {
    return selection.value
  }
  return selection.value === null ? [] : [selection.value]
}

interface ResolvedSelection<TValue extends string> {
  multiple: boolean
  /** The values the selection holds — zero or one on the single arm. */
  selected: readonly TValue[]
  noneLabel: string | undefined
  /** The `noneLabel` row wears the check. */
  noneMarked: boolean
  stateOf: (option: PickerItem<TValue>) => SelectionState
  isDisabled: (option: PickerItem<TValue>) => boolean
  /** A pick. Single reports the value; multi reports the NEXT array — a
   *  `selected` row leaves it, any other row joins it. */
  pick: (option: PickerItem<TValue>) => void
  pickNone: () => void
}

/**
 * The one place a row's state and a pick's outcome are decided.
 *
 * On the multi arm a row's `checked` WINS over membership in `value`: a bulk
 * edit marks a label that sits on every issue `true`, on some of them
 * `"indeterminate"`, and leaves the rest to the array. The arithmetic still
 * runs on `value`, so a caller keeps the two in agreement (`checked: true`
 * rows ARE in `value`) and reads a toggle off the array `onChange` hands back.
 */
function resolveSelection<TValue extends string>(
  props: ComboboxSelection<TValue>
): ResolvedSelection<TValue> {
  const multiple = isMultiple(props)
  const selected = selectedValuesOf(props)
  const selectedSet = new Set<string>(selected)

  const stateOf = (option: PickerItem<TValue>): SelectionState => {
    if (multiple) {
      if (option.checked === `indeterminate`) {
        return `indeterminate`
      }
      if (option.checked !== undefined) {
        return option.checked ? `selected` : `unselected`
      }
    }
    return selectedSet.has(option.value) ? `selected` : `unselected`
  }

  const atCap =
    isMultiple(props) && props.max !== undefined && selected.length >= props.max

  const isDisabled = (option: PickerItem<TValue>) =>
    option.disabled === true || (atCap && stateOf(option) !== `selected`)

  const pick = (option: PickerItem<TValue>) => {
    if (isMultiple(props)) {
      const rest = props.value.filter((entry) => entry !== option.value)
      props.onChange(
        stateOf(option) === `selected` ? rest : [...rest, option.value]
      )
      return
    }
    props.onChange(option.value)
  }

  const single = isMultiple(props) ? undefined : props
  const pickNone = () => single?.onChange(null)

  return {
    multiple,
    selected,
    noneLabel: single?.noneLabel,
    noneMarked:
      single !== undefined &&
      selected.length === 0 &&
      single.indeterminate !== true,
    stateOf,
    isDisabled,
    pick,
    pickNone,
  }
}

/**
 * THE single-pick glyph: a trailing `ui-check` on the picked row, muted like
 * every other secondary glyph in a row. `data-selected-glyph` names what was
 * drawn so a test (and the styleguide gate) can count idioms instead of
 * reading SVG paths. A multi pick draws NO glyph: the row's highlight is it.
 */
function SelectionCheck({
  state,
  className,
}: {
  state: SelectionState
  className?: string
}) {
  if (state !== `selected`) {
    return null
  }
  return (
    <CheckGlyph
      aria-hidden
      data-selected-glyph="check"
      className={cn(`ml-auto size-3.5 shrink-0 text-muted-foreground`, className)}
    />
  )
}

/** The multi arm's highlight, shared by every shell. Every row carries a
 *  transparent border so picking never shifts it; a picked row takes the
 *  wash plus the active stroke — the stroke is what separates "picked" from
 *  "the cursor is here" — and adjacent picks lose their shared edge, so three
 *  picked rows read as ONE block, not three stacked pills. */
const MULTI_ROW_CLASS = `border border-transparent`
const MULTI_PICKED_CLASS = `bg-glass-active font-medium text-foreground border-glass-stroke-active [&:has(+[data-picked=true])]:rounded-b-none [&:has(+[data-picked=true])]:border-b-transparent [[data-picked=true]+&]:rounded-t-none [[data-picked=true]+&]:border-t-transparent`
const MULTI_MIXED_CLASS = `bg-glass-active`

/** The `data-picked` stamp: `true` on a picked row (both arms), `mixed` on a
 *  partly-picked multi row, absent otherwise. */
function pickedAttr(
  multiple: boolean,
  state: SelectionState
): `true` | `mixed` | undefined {
  if (state === `selected`) return `true`
  if (multiple && state === `indeterminate`) return `mixed`
  return undefined
}

/** The row classes the selection state adds (multi only). */
function pickedRowClass(multiple: boolean, state: SelectionState) {
  if (!multiple) return undefined
  return cn(
    MULTI_ROW_CLASS,
    state === `selected` && MULTI_PICKED_CLASS,
    state === `indeterminate` && MULTI_MIXED_CLASS
  )
}

/** The row-body renderer both arms accept. The selection glyph stays the
 *  primitive's, so a custom row can never invent a seventh "this is picked"
 *  language. */
type RenderComboboxOption<TValue extends string> = (
  option: PickerItem<TValue>,
  state: { selected: boolean }
) => React.ReactNode

export {
  SelectionCheck,
  pickedAttr,
  pickedRowClass,
  isMultiple,
  resolveSelection,
  selectedValuesOf,
}
export type {
  ComboboxMultiSelection,
  ComboboxSelection,
  ComboboxSingleSelection,
  RenderComboboxOption,
  SelectionState,
}
