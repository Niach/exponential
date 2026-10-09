import type { ReactNode } from "react"

import { Combobox } from "../combobox"
import { PickerItemBody, type PickerItem } from "./picker-item"
import type { PickerTriggerVariant } from "./picker-trigger"

export * from "./picker-item"

// EXP-1029 contract, EXP-1021 implementation — THE picker primitive.
//
// One primitive per platform, typed pickers on top, the same names
// everywhere: web here, IDE `ui::picker`, iOS `ExpUI/Sources/Picker`
// (`GlassPicker`, since SwiftUI owns the bare name), Android
// `ui/components/picker`. Presentation belongs to the primitive, NEVER to
// the caller:
//
//   phone   → a bottom sheet with PLAIN rows (no cards inside the sheet);
//             multi-select marks rows by the highlight colour, no circles;
//             swipe down closes
//   pointer → a context menu / popover anchored at the trigger
//             (`MENU_SURFACE_CLASS`, the `Combobox` rows)
//
// `search` adds the filter field at the top of either surface. The trigger
// is whatever chip or button the caller wants opened — the primitive owns
// the surface, the caller owns the trigger.
//
// THE SELECTION LANGUAGE, one on all four clients (review r3 settled it,
// after iOS and Android had drifted to the wash for both arms):
//
//   single  a trailing `ui-check` on the picked row, and NO wash. This is
//           EXP-957's rule, which was matched to the natives; EXP-1021 only
//           ever set out to change the MULTI arm.
//   multi   the row's own highlight, never a glyph — the circle pair this
//           issue was opened about.
//
// The wash means "picked" only where there is more than one pick to see; a
// single pick wearing it reads like a multi pick. Both halves are pinned in
// picker.test.tsx, and the IDE, iOS and Android carry the same case.
//
// The surfaces are the internal `Combobox`'s (EXP-941, no longer exported):
// `MobilePopover` is the popover-or-sheet pair, cmdk owns the filter and the
// keys, the sheet closes on a drag down (EXP-687). `PickerList` is the same
// body for a host that already is a surface, `PickerMenuRows` the same rows
// inside a `Menu` submenu. The UI cleanup batch made this the ONLY public
// picker: the typed pickers (board, status, model, effort, repository,
// branch …) all sit on it.

export type PickerMode = `single` | `multi`

/** The four popover widths, as the literals Tailwind can scan. */
export type PickerWidth = `sm` | `md` | `lg` | `xl`

/** What a typed picker forwards to the SURFACE verbatim (EXP-1021 sweep):
 *  where the popover hangs, how wide it is, a controlled open state, and the
 *  trigger-less host. Every typed picker extends it, so a call site never has
 *  to drop back to `Combobox` just to place a popover. */
export interface PickerSurfaceProps {
  /** The filter field's placeholder (only read while `search`). */
  searchPlaceholder?: string
  /** Popover alignment against the trigger (pointer only). */
  align?: `start` | `end`
  /** The popover's width (pointer only; the sheet is the screen). */
  width?: PickerWidth
  /** Controlled open state; uncontrolled when absent. */
  open?: boolean
  onOpenChange?: (open: boolean) => void
  /** Controlled-open with NO trigger element: a host that opens the picker
   *  from its own menu item (the phone issue-detail `…` menu's "Move to
   *  board"). The `trigger` is then never rendered. */
  hideTrigger?: boolean
  "data-testid"?: string
}

interface PickerPropsBase<T extends string> extends PickerSurfaceProps {
  items: readonly PickerItem<T>[]
  /** The chip or button that opens the picker. The primitive renders it as
   *  the anchor and wires the open state; the caller styles it. A function
   *  gets the picked rows and their one-line summary. Omitted = the
   *  primitive's own `PickerTrigger` in `triggerVariant`. */
  trigger?:
    | ReactNode
    | ((state: {
        selected: PickerItem<T>[]
        summary: string
        open: boolean
      }) => ReactNode)
  /** The primitive's own trigger when no `trigger` is given: `pill` (the
   *  property chip), `field` (a full-width form row), `row` (the glass form
   *  ladder: `mobileTitle` leading, the pick trailing) or `inline` (one word
   *  of a muted sentence, which collapses to plain text while there is at
   *  most one item). Defaults to `pill`. */
  triggerVariant?: PickerTriggerVariant
  /** The trigger's text while nothing is picked; defaults to `mobileTitle`. */
  triggerLabel?: string
  /** A filter field at the top of the surface. */
  search?: boolean
  /** What an empty `items` (or an empty search) reads as. */
  emptyText?: ReactNode
  /** The sheet's title on phones (the popover has none). */
  mobileTitle?: string
  disabled?: boolean
  /** Extra classes on the SURFACE — the popover panel the rows sit in. NOT
   *  the trigger: the caller builds that element itself and styles it there.
   *  Applied after the width, so `w-auto` hugs a fixed-size `panel` (the icon
   *  grid). The phone sheet is the screen and ignores it. */
  className?: string
  /** Controlled search text — for an external ranking engine
   *  (`useIssueSearchResults`, EXP-892). With it, pass
   *  `shouldFilter={false}` so the caller's order is rendered verbatim. */
  query?: string
  onQueryChange?: (query: string) => void
  shouldFilter?: boolean
  /** A muted "Loading…" row instead of the rows. */
  loading?: boolean
  /** A muted row instead of the rows (a retry button belongs here). */
  error?: ReactNode
  /** Rendered under the rows — a "Create label" action row. */
  footer?: ReactNode
  /** REPLACES the search field and the rows with an inline body (the icon
   *  picker's swatch grid). The surface, its sheet and its trigger stay the
   *  primitive's. */
  panel?: ReactNode
  /** Replaces the row BODY. The selection language stays the primitive's,
   *  so a custom row can never invent a second "this is picked" idiom
   *  (the account rows' EXP-992 limit preview is the one caller). */
  renderItem?: (
    item: PickerItem<T>,
    state: { selected: boolean }
  ) => ReactNode
}

export type PickerProps<T extends string = string> = PickerPropsBase<T> &
  (
    | {
        mode: `single`
        /** The picked value, or null while none is. */
        value: T | null
        /** Fires on a pick and closes the surface. */
        onChange: (value: T) => void
        /** A FIRST row that clears the pick ("Unassigned", "None"): the
         *  `Combobox` none row, marked while `value` is null and reporting
         *  through `onNone` — so no sentinel value ever sits in `items`. An
         *  empty-string item is what cmdk reads as "no value" (never
         *  highlighted, never picked by Enter, dropped by the filter), which
         *  is why the sentinel route is closed here. Its own callback rather
         *  than a `null` through `onChange`: one signature per arm keeps a
         *  caller's inline `(value) =>` contextually typed. */
        noneLabel?: string
        /** Fires when the none row is picked; the surface closes. */
        onNone?: () => void
      }
    | {
        mode: `multi`
        value: readonly T[]
        /** Fires on every toggle with the whole new set; the surface stays
         *  open. */
        onChange: (value: T[]) => void
        /** At the cap the unpicked rows go disabled; picked ones still
         *  toggle off, or the user would be stuck. */
        max?: number
      }
  )

/**
 * THE picker: a popover at the trigger on a pointer device, a bottom sheet
 * of plain rows on a phone, `data-slot="picker"` either way (what every
 * typed picker is asserted through).
 */
export function Picker<T extends string = string>(props: PickerProps<T>) {
  const {
    items,
    trigger,
    triggerVariant,
    triggerLabel,
    search = false,
    searchPlaceholder,
    emptyText,
    mobileTitle,
    disabled = false,
    open,
    onOpenChange,
    align,
    width,
    hideTrigger,
    className,
    query,
    onQueryChange,
    shouldFilter,
    loading,
    error,
    footer,
    panel,
    renderItem,
  } = props
  const testId = props[`data-testid`]

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
    <span
      data-slot="picker"
      data-picker-mode={props.mode}
      data-picker-search={search ? `true` : undefined}
      className="contents"
    >
      <Combobox
        {...selection}
        options={items}
        searchable={search}
        placeholder={searchPlaceholder}
        emptyText={emptyText}
        mobileTitle={mobileTitle ?? `Options`}
        triggerVariant={triggerVariant}
        triggerLabel={triggerLabel}
        renderTrigger={
          trigger === undefined
            ? undefined
            : typeof trigger === `function`
              ? trigger
              : () => trigger
        }
        query={query}
        onQueryChange={onQueryChange}
        shouldFilter={shouldFilter}
        loading={loading}
        error={error}
        footer={footer}
        panel={panel}
        open={open}
        onOpenChange={onOpenChange}
        hideTrigger={hideTrigger}
        align={align}
        width={width}
        disabled={disabled}
        surfaceClassName={className}
        data-testid={testId}
        renderOption={(item, state) =>
          renderItem ? renderItem(item, state) : <PickerItemBody item={item} />
        }
      />
    </span>
  )
}
