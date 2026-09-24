import type { LucideIcon } from "lucide-react"
import type { ReactNode } from "react"

import { cn } from "../cn"
import { Combobox } from "../combobox"
import type { PickerOption } from "../picker-option"

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
// The two surfaces are `Combobox`'s (EXP-941): `MobilePopover` already is
// the popover-or-sheet pair, cmdk already owns the filter and the keys, and
// the sheet already closes on a drag down (EXP-687). What EXP-1021 adds is
// the SELECTION LANGUAGE this issue asked for — a multi pick reads as the
// row's own highlight, never a leading circle — so the picker that links a
// relation and the picker that batches issues finally look like one thing.
// That is `selectionStyle="highlight"`, and the primitive is the only
// caller that passes it: the surfaces EXP-1021 does not own (the composer,
// device settings, the workflow screens) keep the circles until the
// integration node swaps them onto this API.

export interface PickerItem<T extends string = string> {
  /** The stable identity of the row; also what search matches on. */
  value: T
  /** What the row reads as. A string also seeds the search keywords. */
  label: ReactNode
  /** A leading glyph — always a concept icon, never a raw lucide import. */
  icon?: LucideIcon
  /** A colour for the glyph (a board's hex, a label's dot, a status tone).
   *  A hex (or any CSS colour) tints the glyph; a Tailwind text class is
   *  applied as-is. With no `icon` the colour draws as the row's DOT — that
   *  is what makes a label row a coloured dot and a board row a tinted
   *  glyph without the caller choosing a shape. */
  color?: string
  /** A muted second line or trailing note (an email, a branch age). */
  description?: ReactNode
  /** Rendered, never pickable. */
  disabled?: boolean
  /** Extra search terms when `label` is not a string (an identifier, an
   *  email). */
  keywords?: string[]
  /** Multi mode only: what THIS row reads as when membership in `value` is
   *  not the whole story — a bulk edit over rows that disagree marks a
   *  label on all of them `true`, on some `"indeterminate"`. */
  checked?: boolean | `indeterminate`
}

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
   *  the anchor and wires the open state; the caller styles it. */
  trigger: ReactNode
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

/** A row's glyph tint: a CSS colour tints, a Tailwind class is a class. */
function isColorClass(color: string) {
  return !color.startsWith(`#`) && !color.includes(`(`)
}

/** THE row body: the dot or the tinted glyph, the label, the muted second
 *  line. Plain — a picker row is never a card, on any surface. Exported for
 *  the MENU arm alone (`pickerMenuRows`). */
export function PickerItemBody<T extends string>({ item }: { item: PickerItem<T> }) {
  const Glyph = item.icon
  return (
    <>
      {Glyph ? (
        <Glyph
          aria-hidden
          className={cn(
            `size-4 shrink-0`,
            item.color && isColorClass(item.color) ? item.color : undefined
          )}
          style={
            item.color && !isColorClass(item.color)
              ? { color: item.color }
              : undefined
          }
        />
      ) : item.color !== undefined ? (
        <span
          aria-hidden
          data-slot="picker-dot"
          className="size-2.5 shrink-0 rounded-full"
          style={{ backgroundColor: item.color }}
        />
      ) : null}
      <span className="flex min-w-0 flex-1 flex-col gap-0.5 py-0.5 text-left">
        <span className="min-w-0 truncate text-sm">{item.label}</span>
        {item.description !== undefined && item.description !== null ? (
          <span
            data-slot="picker-description"
            className="min-w-0 text-xs text-muted-foreground"
          >
            {item.description}
          </span>
        ) : null}
      </span>
    </>
  )
}

/**
 * THE picker: a popover at the trigger on a pointer device, a bottom sheet
 * of plain rows on a phone, `data-slot="picker"` either way (what every
 * typed picker is asserted through).
 */
export function Picker<T extends string = string>(props: PickerProps<T>) {
  const {
    items,
    trigger,
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

  const byValue = new Map(items.map((item) => [item.value, item]))
  const options = pickerItemOptions(items)

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
    <span
      data-slot="picker"
      data-picker-mode={props.mode}
      data-picker-search={search ? `true` : undefined}
      className="contents"
    >
      <Combobox
        {...selection}
        options={options}
        // The primitive's own selection language: highlight, never circles.
        selectionStyle="highlight"
        searchable={search}
        placeholder={searchPlaceholder}
        emptyText={emptyText}
        mobileTitle={mobileTitle ?? `Options`}
        renderTrigger={() => trigger}
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
    </span>
  )
}

/** A picker row as the `PickerOption` every `Combobox` arm speaks. The row's
 *  colour is NOT part of it: the body is drawn by `PickerItemBody`, so the
 *  dot-or-tinted-glyph decision stays in exactly one place. */
function pickerItemOptions<T extends string>(
  items: readonly PickerItem<T>[]
): PickerOption<T>[] {
  return items.map((item) => ({
    value: item.value,
    label: item.label,
    keywords: pickerItemKeywords(item),
    disabled: item.disabled,
    checked: item.checked,
  }))
}

/**
 * The picker's rows as the MENU arm speaks them (`ComboboxMenuItems`,
 * EXP-957): spread the result into it.
 *
 * A Radix submenu is a shell the primitive does not own — a popover cannot
 * nest inside one without stacking a second portal and focus trap — but
 * EXP-1021 still wants ONE set of rows, so the typed pickers' items are
 * bridged here instead of being rebuilt by hand at every context menu. Only
 * the SURFACE differs: the menu keeps its own selection glyph (EXP-957), the
 * row body is the primitive's.
 */
export function pickerMenuRows<T extends string>(items: readonly PickerItem<T>[]) {
  const byValue = new Map(items.map((item) => [item.value, item]))
  return {
    options: pickerItemOptions(items),
    renderOption: (option: PickerOption<T>) => {
      const item = byValue.get(option.value)
      return item ? <PickerItemBody item={item} /> : null
    },
  }
}

/** The keywords a row matches on: the explicit ones, else a string label. */
export function pickerItemKeywords(item: PickerItem<string>): string[] {
  if (item.keywords && item.keywords.length > 0) return item.keywords
  return typeof item.label === `string` ? [item.label] : []
}
