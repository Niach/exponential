import * as React from "react"

import { Button } from "./button"
import { cn } from "./cn"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "./command"
import {
  ComboboxOptionBody,
  SelectionGlyph,
  isMultiple,
  resolveSelection,
  selectedValuesOf,
  type ComboboxSelection,
  type RenderComboboxOption,
} from "./combobox-core"
import { GLASS_SELECT_TRIGGER } from "./glass-rows"
import { conceptIcon } from "./icons.generated"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "./mobile-popover"
import type { PickerOption } from "./picker-option"
import { Pill } from "./pill"

// EXP-941 — the ONE searchable picker on the web.
//
// The `MobilePopover` + `Command` shell was copy-pasted about twelve times
// (assignee, board, branch, action, PR, MCP servers, automation filters, the
// issue batch picker …), each copy re-deciding four things the user can see:
//
//  * how a picked row LOOKS — a leading `Checkbox`, a trailing `Check`, an
//    invisible check reserving the gutter, or the `ui-selected`/`ui-unselected`
//    concept pair. This primitive fixes it, and matches both natives: single
//    select = a trailing `ui-check` on the picked row, multi select = the
//    LEADING circle pair on every row (iOS `AgentIssuePickerSheet.swift:86`,
//    Android `AgentIssuePickerSheet.kt:173`).
//  * what `value=` carries — half the copies smuggled a searchable string into
//    it (`` `${name} ${id}` ``) because two boards can share a name. Here the
//    value is the IDENTITY and `keywords` carries the search terms, so a
//    duplicate label is not a bug.
//  * the popover width — seven literals scattered over the app, now the
//    `width` enum.
//  * "nothing picked" — six different sentinel values (`__none__`,
//    `__unassign__`, `""`, `"none"` …). `noneLabel` renders a row that
//    reports `null`; no sentinel ever reaches a caller.
//
// Single select closes on pick, multi stays open (a batch is several picks).
// `ComboboxList` is the same body without the popover, for an inline host (a
// tab, a sheet that is already open) and for the styleguide island — a closed
// Radix portal renders nothing, so the gallery shows the trigger beside the
// bare list. `ComboboxMenuItems` (combobox-menu.tsx, EXP-957) is the third
// arm: the same rows as items INSIDE a Radix menu, for the right-click
// submenus and the bulk bar. The selection arithmetic and the glyph live in
// combobox-core.tsx, shared by every arm and exported by none.
//
// EXP-958 retired the last two closed single-selects that drew their own
// shells — `OptionDropdownMenu` (the status/priority menu: a Radix dropdown
// on desktop that marked NO row, a hand-rolled sheet on the phone) and
// `GlassPickerRow` (the settings row: a Radix Select on desktop, another
// hand-rolled sheet on the phone). Both are `searchable={false}` Comboboxes
// now, so a status pick wears the same trailing check as every other pick.
// Two trigger variants came with them: `row` IS the picker row of the glass
// form ladder (label leading, value trailing at 70%, chevron at 50% — the
// iOS `GlassPickerRow` / desktop `surface::glass_picker_row` twin), `inline`
// is a WORD inside a muted sentence (the composer's options line), which
// collapses to plain text when there is only one thing it could say.

const ChevronGlyph = conceptIcon(`ui-chevron-down`)
const ChevronRightGlyph = conceptIcon(`ui-chevron-right`)

/** The `row` trigger: the glass form ladder's picker row (glass-rows.tsx's
 *  `GLASS_PICKER_ROW` is the same shell on a stock `SelectTrigger`). */
const ROW_TRIGGER = `flex w-full items-center gap-3 px-4 py-3 text-left transition-colors duration-fast outline-none hover:bg-glass-active/50 focus-visible:ring-[3px] focus-visible:ring-inset focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50`

/** The `inline` trigger and its collapsed word share the row of the sentence
 *  they sit in — no chrome, the sentence's own colour, hover lifts it. */
const INLINE_WORD = `flex items-center gap-1`
const INLINE_TRIGGER = `${INLINE_WORD} outline-none hover:text-foreground focus-visible:text-foreground disabled:pointer-events-none disabled:opacity-50`

/** The four popover widths the app actually used, as literals — Tailwind
 *  scans these sources, so a width composed at runtime would not exist. */
const WIDTH_CLASS = {
  sm: `w-[14rem]`,
  md: `w-[16rem]`,
  lg: `w-[18rem]`,
  xl: `w-[22rem]`,
} as const

type ComboboxWidth = keyof typeof WIDTH_CLASS

/** What both arms of the union share — everything that is not selection. */
interface ComboboxListBaseProps<TValue extends string> {
  options: readonly PickerOption<TValue>[]
  /** The row BODY only. The selection glyph stays the primitive's, so a
   *  custom row can never invent a seventh "this is picked" language. */
  renderOption?: RenderComboboxOption<TValue>
  /** The filter field. On by default; pass `false` for a short fixed list. */
  searchable?: boolean
  /** The filter field's placeholder. */
  placeholder?: string
  /** Shown when the filter matches nothing. */
  emptyText?: React.ReactNode
  /** Controlled search text — for an external ranking engine. */
  query?: string
  onQueryChange?: (query: string) => void
  /** `false` hands ranking to the caller: the options are rendered in the
   *  order given, verbatim (`useIssueSearchResults` pins checked rows first). */
  shouldFilter?: boolean
  /** A muted "Loading…" row instead of the list. */
  loading?: boolean
  /** A muted row instead of the list (a retry button belongs here). */
  error?: React.ReactNode
  /** Rendered after the list — a "Create label" action row. */
  footer?: React.ReactNode
  /** REPLACES the search field and the list: an inline sub-view (the create
   *  form the label picker swaps in). */
  panel?: React.ReactNode
  className?: string
  /** Extra classes on the scrolling list itself — its max height, when the
   *  host (a full-screen search, a card) is the one that caps it. */
  listClassName?: string
}

type ComboboxListProps<TValue extends string> =
  ComboboxListBaseProps<TValue> & ComboboxSelection<TValue>

interface ComboboxShellProps<TValue extends string> {
  /** The sheet arm's headline. Required: every mobile sheet is titled
   *  (EXP-687), and a picker with no title reads as a mystery panel. The
   *  `row` variant leads its row with it; `inline` names its word for the
   *  assistive tree (`title` + `aria-label`). */
  mobileTitle: string
  /** `pill` = the property-chip trigger, `field` = a full-width form row,
   *  `row` = the glass form ladder's picker row (`mobileTitle` leading, the
   *  picked label trailing), `inline` = one word of a muted sentence, which
   *  collapses to plain text while there is at most one option. */
  triggerVariant?: `pill` | `field` | `row` | `inline`
  /** The trigger's text while nothing is picked. */
  triggerLabel?: string
  /** A bespoke trigger. Must be ONE element — it is wrapped `asChild`. */
  renderTrigger?: (state: {
    selected: PickerOption<TValue>[]
    summary: string
    open: boolean
  }) => React.ReactNode
  width?: ComboboxWidth
  /** Defaults to `start`; the `row` variant defaults to `end`, under the
   *  value it trails with. */
  align?: `start` | `end`
  open?: boolean
  onOpenChange?: (open: boolean) => void
  /** Controlled-open with no trigger element: a host that opens the picker
   *  from its own menu item (the issue row's "Move to board"). */
  hideTrigger?: boolean
  disabled?: boolean
  "data-testid"?: string
}

type ComboboxProps<TValue extends string> = ComboboxListProps<TValue> &
  ComboboxShellProps<TValue>

/** The picker BODY — search field, rows, footer — with no popover around it.
 *  `Combobox` renders this inside its content; an inline host renders it on
 *  its own. */
function ComboboxList<TValue extends string>(props: ComboboxListProps<TValue>) {
  const {
    options,
    renderOption,
    searchable = true,
    placeholder,
    emptyText = `No results`,
    query,
    onQueryChange,
    shouldFilter,
    loading = false,
    error,
    footer,
    panel,
    className,
    listClassName,
  } = props

  if (panel !== undefined && panel !== null) {
    return (
      <div
        data-slot="combobox-panel"
        className={cn(`min-h-0 flex-1 overflow-y-auto`, className)}
      >
        {panel}
      </div>
    )
  }

  const selection = resolveSelection(props)
  const { multiple, noneLabel, noneMarked } = selection

  return (
    <Command
      data-slot="combobox-list"
      shouldFilter={shouldFilter}
      // The sheet arm drops `className` on the content, so the height chain
      // has to start here or a capped popover never lets the list shrink.
      className={cn(`min-h-0 flex-1`, className)}
    >
      {searchable && (
        <CommandInput
          placeholder={placeholder ?? `Search…`}
          value={query}
          onValueChange={onQueryChange}
        />
      )}
      <CommandList className={cn(`min-h-0 flex-1`, listClassName)}>
        {loading ? (
          <div className="flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground">
            Loading…
          </div>
        ) : null}
        {!loading && error ? (
          <div
            data-slot="combobox-error"
            className="px-3 py-2 text-sm text-muted-foreground"
          >
            {error}
          </div>
        ) : null}
        {!loading && !error ? (
          <>
            <CommandEmpty>{emptyText}</CommandEmpty>
            <CommandGroup>
              {noneLabel !== undefined && (
                // No `value`: cmdk derives it from the row's own text, so the
                // sentinel values this retires never come back as strings.
                <CommandItem
                  data-combobox-none="true"
                  keywords={[noneLabel]}
                  className="flex items-center gap-2.5"
                  onSelect={() => selection.pickNone()}
                >
                  <span className="min-w-0 flex-1 truncate text-sm">
                    {noneLabel}
                  </span>
                  <SelectionGlyph
                    arity="single"
                    state={noneMarked ? `selected` : `unselected`}
                  />
                </CommandItem>
              )}
              {options.map((option) => {
                const state = selection.stateOf(option)
                const isSelected = state === `selected`
                return (
                  <CommandItem
                    key={option.value}
                    value={option.value}
                    keywords={
                      option.keywords ??
                      (typeof option.label === `string` ? [option.label] : [])
                    }
                    disabled={selection.isDisabled(option)}
                    aria-pressed={
                      multiple
                        ? state === `indeterminate`
                          ? `mixed`
                          : isSelected
                        : undefined
                    }
                    className={cn(
                      `flex items-center gap-2.5`,
                      multiple && isSelected && `bg-glass-active`
                    )}
                    onSelect={() => selection.pick(option)}
                  >
                    {multiple && <SelectionGlyph arity="multi" state={state} />}
                    {renderOption ? (
                      renderOption(option, { selected: isSelected })
                    ) : (
                      <ComboboxOptionBody option={option} />
                    )}
                    {!multiple && <SelectionGlyph arity="single" state={state} />}
                  </CommandItem>
                )
              })}
            </CommandGroup>
          </>
        ) : null}
      </CommandList>
      {footer !== undefined && footer !== null ? (
        <div
          data-slot="combobox-footer"
          className="shrink-0 border-t border-glass-stroke"
        >
          {footer}
        </div>
      ) : null}
    </Command>
  )
}

/** `None` / `a` / `a, b` / `a, b +3` — the one summary every trigger shows. */
function summarize(labels: string[], fallback: string) {
  if (labels.length === 0) {
    return fallback
  }
  if (labels.length <= 2) {
    return labels.join(`, `)
  }
  return `${labels.slice(0, 2).join(`, `)} +${labels.length - 2}`
}

function Combobox<TValue extends string>(props: ComboboxProps<TValue>) {
  const {
    mobileTitle,
    triggerVariant = `pill`,
    triggerLabel,
    renderTrigger,
    width = `md`,
    align = triggerVariant === `row` ? `end` : `start`,
    open: openProp,
    onOpenChange,
    hideTrigger = false,
    disabled = false,
    className,
    options,
  } = props
  const testId = props[`data-testid`]

  const [uncontrolledOpen, setUncontrolledOpen] = React.useState(false)
  const open = disabled ? false : (openProp ?? uncontrolledOpen)
  const setOpen = (next: boolean) => {
    if (disabled) {
      return
    }
    if (openProp === undefined) {
      setUncontrolledOpen(next)
    }
    onOpenChange?.(next)
  }

  const selectedValues = selectedValuesOf(props)
  const selectedSet = new Set<string>(selectedValues)
  const selectedOptions = options.filter((option) =>
    selectedSet.has(option.value)
  )
  const summary = summarize(
    selectedOptions.map((option) =>
      typeof option.label === `string` ? option.label : option.value
    ),
    triggerLabel ?? mobileTitle
  )
  const nothingPicked = selectedValues.length === 0

  // Single select is one pick and done; a multi select is a batch, so it stays
  // open across toggles.
  const listProps: ComboboxListProps<TValue> = isMultiple(props)
    ? { ...props, onChange: props.onChange }
    : {
        ...props,
        onChange: (value: TValue | null) => {
          props.onChange(value)
          setOpen(false)
        },
      }

  // The word an `inline` picker is, and the value a `row` trails with: the
  // picked option's own label NODE (an action's icon flows inline with its
  // name), not the string summary — these two arms are single-select rows
  // of a form, never a multi summary.
  const picked = selectedOptions[0]
  const PickedGlyph = picked?.icon

  if (triggerVariant === `inline` && options.length <= 1) {
    // One option is not a choice: the sentence just says it. Nothing picked
    // and one option still reads as that option — there is nothing else.
    const word = picked ?? options[0]
    const WordGlyph = word?.icon
    return (
      <span
        data-slot="combobox-inline-word"
        data-testid={testId}
        className={cn(INLINE_WORD, className)}
        title={mobileTitle}
      >
        {WordGlyph && <WordGlyph aria-hidden className="size-3.5 shrink-0" />}
        {word ? word.label : (triggerLabel ?? mobileTitle)}
      </span>
    )
  }

  const trigger = renderTrigger ? (
    renderTrigger({ selected: selectedOptions, summary, open })
  ) : triggerVariant === `inline` ? (
    <button
      type="button"
      data-slot="combobox-inline-trigger"
      disabled={disabled}
      className={cn(INLINE_TRIGGER, className)}
      title={mobileTitle}
      aria-label={mobileTitle}
    >
      {PickedGlyph && <PickedGlyph aria-hidden className="size-3.5 shrink-0" />}
      {picked ? picked.label : (triggerLabel ?? mobileTitle)}
      <ChevronGlyph aria-hidden className="size-3 shrink-0" />
    </button>
  ) : triggerVariant === `row` ? (
    <button
      type="button"
      data-slot="glass-picker-row"
      disabled={disabled}
      className={cn(ROW_TRIGGER, className)}
    >
      <span className="shrink-0 text-sm text-foreground">{mobileTitle}</span>
      <span className="ml-auto min-w-0 truncate text-sm text-foreground/70 [&_svg]:inline">
        {picked ? picked.label : triggerLabel}
      </span>
      <ChevronRightGlyph
        aria-hidden
        className="size-3.5 shrink-0 text-foreground/50"
      />
    </button>
  ) : triggerVariant === `field` ? (
    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={disabled}
      className={cn(
        `h-8 w-full justify-between font-normal`,
        GLASS_SELECT_TRIGGER,
        nothingPicked && `text-muted-foreground`,
        className
      )}
    >
      <span className="min-w-0 truncate">{summary}</span>
      <ChevronGlyph aria-hidden className="size-3.5 shrink-0 opacity-50" />
    </Button>
  ) : (
    <Pill mode="action" disabled={disabled} className={cn(`max-w-full`, className)}>
      <span className="min-w-0 truncate">{summary}</span>
      <ChevronGlyph aria-hidden className="shrink-0 opacity-50" />
    </Pill>
  )

  return (
    <MobilePopover open={open} onOpenChange={setOpen}>
      {!hideTrigger && (
        <MobilePopoverTrigger asChild disabled={disabled}>
          {trigger}
        </MobilePopoverTrigger>
      )}
      <MobilePopoverContent
        align={align}
        collisionPadding={12}
        mobileTitle={mobileTitle}
        data-testid={testId}
        className={cn(
          `flex max-h-(--radix-popover-content-available-height) flex-col overflow-hidden p-0`,
          WIDTH_CLASS[width]
        )}
      >
        <ComboboxList {...listProps} className={undefined} />
      </MobilePopoverContent>
    </MobilePopover>
  )
}

export { Combobox, ComboboxList, WIDTH_CLASS as COMBOBOX_WIDTH_CLASS }
export type {
  ComboboxListProps,
  ComboboxProps,
  ComboboxSelection,
  ComboboxWidth,
}
