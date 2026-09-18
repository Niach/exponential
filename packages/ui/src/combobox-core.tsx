import * as React from "react"

import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import type { PickerOption } from "./picker-option"

// EXP-957 — what every Combobox ARM shares: the selection model and the glyph
// that draws it.
//
// `ComboboxList` (cmdk rows in a popover) and `ComboboxMenuItems` (rows inside
// a Radix ContextMenu or DropdownMenu) are different shells over the SAME
// vocabulary. Keeping the arithmetic and the glyph here, and NOT exporting
// either from the barrel, is what stops a third shell — or an app row — from
// inventing another "this is picked" language. Two arities, one glyph each:
//
//   single  the picked row wears a trailing `ui-check`; nothing else moves
//   multi   EVERY row leads with the circle pair (`ui-selected` /
//           `ui-unselected`), and a row that is picked on only SOME of the
//           things being edited wears `ui-indeterminate` (circle-minus)

const CheckGlyph = conceptIcon(`ui-check`)
const SelectedGlyph = conceptIcon(`ui-selected`)
const UnselectedGlyph = conceptIcon(`ui-unselected`)
const IndeterminateGlyph = conceptIcon(`ui-indeterminate`)

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
  stateOf: (option: PickerOption<TValue>) => SelectionState
  isDisabled: (option: PickerOption<TValue>) => boolean
  /** A pick. Single reports the value; multi reports the NEXT array — a
   *  `selected` row leaves it, any other row joins it. */
  pick: (option: PickerOption<TValue>) => void
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

  const stateOf = (option: PickerOption<TValue>): SelectionState => {
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

  const isDisabled = (option: PickerOption<TValue>) =>
    option.disabled === true || (atCap && stateOf(option) !== `selected`)

  const pick = (option: PickerOption<TValue>) => {
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
 * THE selection glyph. `data-selected-glyph` names what was drawn so a test
 * (and the styleguide gate) can count idioms instead of reading SVG paths.
 *
 * Single draws only when picked — a trailing check, muted like every other
 * secondary glyph in a row. Multi always draws: the circle pair reads as a
 * toggle only when every row has one.
 */
function SelectionGlyph({
  arity,
  state,
  className,
}: {
  arity: `single` | `multi`
  state: SelectionState
  className?: string
}) {
  if (arity === `single`) {
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
  const Glyph =
    state === `selected`
      ? SelectedGlyph
      : state === `indeterminate`
        ? IndeterminateGlyph
        : UnselectedGlyph
  return (
    <Glyph
      aria-hidden
      data-selected-glyph={state}
      className={cn(
        `size-4 shrink-0`,
        state === `unselected` ? `text-muted-foreground` : `text-foreground`,
        className
      )}
    />
  )
}

/** The row body every option gets unless `renderOption` replaces it. */
function ComboboxOptionBody<TValue extends string>({
  option,
}: {
  option: PickerOption<TValue>
}) {
  const Glyph = option.icon
  return (
    <>
      {option.dot !== undefined && (
        <span
          aria-hidden
          className="size-2.5 shrink-0 rounded-full"
          style={{ backgroundColor: option.dot }}
        />
      )}
      {Glyph ? (
        <Glyph
          aria-hidden
          className={cn(`size-4 shrink-0`, option.color)}
          style={option.colorHex ? { color: option.colorHex } : undefined}
        />
      ) : null}
      <span className="min-w-0 flex-1 truncate text-sm">{option.label}</span>
      {option.hint !== undefined && (
        <span className="ml-2 shrink-0 text-xs text-muted-foreground">
          {option.hint}
        </span>
      )}
    </>
  )
}

/** The row-body renderer both arms accept. The selection glyph stays the
 *  primitive's, so a custom row can never invent a seventh "this is picked"
 *  language. */
type RenderComboboxOption<TValue extends string> = (
  option: PickerOption<TValue>,
  state: { selected: boolean }
) => React.ReactNode

export {
  ComboboxOptionBody,
  SelectionGlyph,
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
