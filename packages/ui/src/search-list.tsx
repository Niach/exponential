import * as React from "react"
import { Command as CommandPrimitive, useCommandState } from "cmdk"
import { Popover as PopoverPrimitive } from "radix-ui"
import spec from "@exp/domain-contract/fixtures/search-list.json"

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
  SelectionCheck,
  resolveSelection,
  type ComboboxSelection,
} from "./combobox-core"
import { ListEmpty } from "./empty-state"
import { conceptIcon } from "./icons.generated"
import { MENU_SURFACE_CLASS } from "./menu-surface"
import { PickerItemBody, pickerItemKeywords, type PickerItem } from "./picker/picker-item"
import { Sheet, SheetContent, SheetTitle } from "./sheet"
import { SkeletonRows } from "./skeleton"
import type { TypeaheadAnchor } from "./typeahead"
import { useKeyboardInset } from "./use-keyboard-inset"
import { useIsMobile } from "./use-mobile"

// SLOP-28 (EXP-1259 D4/D5) — THE search list, ×4
// (`packages/domain-contract/fixtures/search-list.json`).
//
// Every place that searches a list and picks from it is this ONE component:
// the `#` issue typeahead in every editor and composer (`SearchListPopover`
// at the caret, a bottom sheet on a phone), the composer "+" menu's
// Implement issue › and Run action › submenus, the issue pickers and the
// global issue search. Anatomy: an INPUT row (40px, the nav-search glyph,
// the field, the muted result COUNT once something is typed) over the
// hairline, then the Picker ROWS (`PickerItemBody`, no new row type) at ONE
// height (32px on a pointer, 44 on a phone), at most eight visible, the rest
// scrolling under a 24px bottom fade.
//
// cmdk owns the keys: the top row is preselected, ↑/↓ move (no animation), a
// hover selects, Enter picks — and Tab picks too (cmdk leaves Tab alone, so
// the root forwards it to the selected row). The selection language is the
// picker's arithmetic (`resolveSelection`); a picked row wears the trailing
// check on BOTH arms here (the fixture: the Implement submenu's picks are
// pinned first with checkmarks), never the multi wash, which in a keyboard
// list reads as a second cursor.

export const SEARCH_LIST_SPEC = spec

const SearchGlyph = conceptIcon(`nav-search`)

/** The one height a row takes, per density. */
const ROW_HEIGHT = { pointer: spec.row.height, phone: spec.row.heightPhone } as const
/** Literal classes, so Tailwind sees them (32px / 44px). */
const ROW_CLASS = { pointer: `h-8`, phone: `h-11` } as const

export type SearchListDensity = keyof typeof ROW_HEIGHT

/** What the `#` typeahead opens with: where the `#` sits and the token typed
 *  after it, or null when the caret is not in a `#` token. A `#` triggers
 *  after whitespace or `(` at once, and at the START of a line (the text's
 *  or after a newline) only once a token character follows, so `# Title`
 *  and `## H` stay markdown headings. The token is `[A-Za-z0-9-]*` (keeps an
 *  identifier's dashes); whitespace or punctuation ends it. Replayed against
 *  `search-list.json` `typeaheadPrefill` on every client. */
export interface TypeaheadPrefill {
  /** Offset of the `#`. */
  start: number
  /** The text between the `#` and the caret: the input's prefill. */
  query: string
}

/** Group 1 = a line-start `#` with its (non-empty) token; group 2 = a `#`
 *  after inline whitespace or `(`. */
const HASH_AT_CARET = /(?:^|\n)#([A-Za-z0-9-]+)$|(?:[^\S\n]|\()#([A-Za-z0-9-]*)$/

export function typeaheadPrefill(text: string, caret: number): TypeaheadPrefill | null {
  const match = HASH_AT_CARET.exec(text.slice(0, caret))
  if (match === null) return null
  const query = match[1] ?? match[2] ?? ``
  return { start: caret - query.length - 1, query }
}

/** cmdk's own pick event — Enter dispatches it on the selected row. */
const CMDK_SELECT_EVENT = `cmdk-item-select`

interface SearchListBase<T extends string> {
  items: readonly PickerItem<T>[]
  /** Controlled search text (an external engine: `useIssueSearchResults`);
   *  pass `shouldFilter={false}` with it so the order renders verbatim. */
  query?: string
  onQueryChange?: (query: string) => void
  /** `false` = the caller ranked the rows. Defaults to cmdk's own filter. */
  shouldFilter?: boolean
  placeholder?: string
  /** What an empty list reads as (the shared list-size line). */
  emptyText?: React.ReactNode
  loading?: boolean
  /** Under the rows (the Implement submenu's button). */
  footer?: React.ReactNode
  /** In the input row before the glyph (the phone search page's back). */
  leading?: React.ReactNode
  /** Replaces the row BODY; the selection check stays the list's. */
  renderItem?: (item: PickerItem<T>, state: { selected: boolean }) => React.ReactNode
  /** Extra classes on every row (a `renderItem` body taller than one row,
   *  the account rows' inline usage bars on touch). */
  rowClassName?: string
  /** The muted result count in the input row once a query is typed. */
  showCount?: boolean
  /** At most this many rows visible before the list scrolls; `null` = the
   *  host caps the height (a full-screen page). */
  maxVisibleRows?: number | null
  /** Pointer or phone geometry; defaults to the viewport's. */
  density?: SearchListDensity
  autoFocus?: boolean
  inputRef?: React.Ref<HTMLInputElement>
  /** Backspace in an EMPTY input (the `#` typeahead closes and deletes its
   *  `#`). */
  onBackspaceEmpty?: () => void
  /** The input's test id (the composer submenus name theirs). */
  inputTestId?: string
  className?: string
  "data-testid"?: string
}

export type SearchListProps<T extends string = string> = SearchListBase<T> &
  (
    | {
        mode: `single`
        /** The picked value; null while none is (a typeahead, a search). */
        value: T | null
        onChange: (value: T) => void
      }
    | {
        mode: `multi`
        value: readonly T[]
        onChange: (value: T[]) => void
        max?: number
      }
  )

/** The input row's trailing count: cmdk's filtered count, so it agrees with
 *  the rows on both filter arms. */
function ResultCount() {
  const count = useCommandState((state) => state.filtered.count)
  return (
    <span
      data-testid={spec.testIds.count}
      className="shrink-0 text-xs text-muted-foreground tabular-nums"
    >
      {count}
    </span>
  )
}

export function SearchList<T extends string = string>(props: SearchListProps<T>) {
  const {
    items,
    query: queryProp,
    onQueryChange,
    shouldFilter,
    placeholder,
    emptyText = `No results`,
    loading = false,
    footer,
    leading,
    renderItem,
    rowClassName,
    showCount = true,
    maxVisibleRows = spec.maxVisibleRows,
    density: densityProp,
    autoFocus,
    inputRef,
    onBackspaceEmpty,
    inputTestId = spec.testIds.input,
    className,
  } = props
  const testId = props[`data-testid`] ?? spec.testIds.list
  const isMobile = useIsMobile()
  const density: SearchListDensity = densityProp ?? (isMobile ? `phone` : `pointer`)

  const [ownQuery, setOwnQuery] = React.useState(``)
  const query = queryProp ?? ownQuery
  const setQuery = (next: string) => {
    if (queryProp === undefined) setOwnQuery(next)
    onQueryChange?.(next)
  }

  const selection = resolveSelection<T>(
    (props.mode === `multi`
      ? { multiple: true, value: props.value, onChange: props.onChange, max: props.max }
      : {
          multiple: false,
          value: props.value,
          onChange: (next: T | null) => {
            if (next !== null) props.onChange(next)
          },
        }) as ComboboxSelection<T>
  )

  // The bottom fade shows only while rows sit below the fold.
  const listRef = React.useRef<HTMLDivElement | null>(null)
  const [moreBelow, setMoreBelow] = React.useState(false)
  const measure = React.useCallback(() => {
    const list = listRef.current
    if (!list) return
    setMoreBelow(list.scrollTop + list.clientHeight < list.scrollHeight - 1)
  }, [])
  React.useLayoutEffect(measure, [measure, items, loading])

  // A new query preselects the TOP row (cmdk re-selects the first match);
  // a list left scrolled down by the previous, longer result would keep that
  // row out of view. Every query change starts the list at the top.
  React.useLayoutEffect(() => {
    const list = listRef.current
    if (!list) return
    list.scrollTop = 0
    measure()
  }, [query, measure])

  const rowHeight = ROW_HEIGHT[density]
  const listStyle: React.CSSProperties = {
    ...(maxVisibleRows === null
      ? null
      : {
          maxHeight: `calc(${maxVisibleRows * rowHeight}px + 2 * var(--menu-surface-padding))`,
        }),
    ...(moreBelow
      ? {
          maskImage: `linear-gradient(to bottom, black calc(100% - ${spec.fadeHeight}px), transparent)`,
          WebkitMaskImage: `linear-gradient(to bottom, black calc(100% - ${spec.fadeHeight}px), transparent)`,
        }
      : null),
  }

  return (
    <Command
      data-slot="search-list"
      data-testid={testId}
      data-density={density}
      shouldFilter={shouldFilter}
      className={cn(`min-h-0 flex-1 rounded-none`, className)}
      onKeyDown={(event) => {
        if (event.nativeEvent.isComposing) return
        if (event.key === `Backspace` && query === `` && onBackspaceEmpty) {
          event.preventDefault()
          onBackspaceEmpty()
          return
        }
        // cmdk picks on Enter only; Tab is the typeahead's second accept key.
        if (event.key === `Tab` && !event.shiftKey) {
          const selected = event.currentTarget.querySelector(
            `[cmdk-item=""][aria-selected="true"]`
          )
          if (selected === null) return
          event.preventDefault()
          selected.dispatchEvent(new Event(CMDK_SELECT_EVENT))
        }
      }}
    >
      {density === `phone` ? (
        <CommandInput
          ref={inputRef}
          leading={leading}
          variant="field"
          autoFocus={autoFocus}
          placeholder={placeholder ?? `Search`}
          value={query}
          onValueChange={setQuery}
          data-testid={inputTestId}
        />
      ) : (
        <div
          data-slot="search-list-input"
          className="flex shrink-0 items-center gap-2 border-b border-glass-stroke px-3"
          style={{ height: spec.input.height }}
        >
          {leading}
          <SearchGlyph aria-hidden className="size-4 shrink-0 text-muted-foreground" />
          <CommandPrimitive.Input
            ref={inputRef}
            data-slot="command-input"
            data-testid={inputTestId}
            autoFocus={autoFocus}
            placeholder={placeholder ?? `Search`}
            value={query}
            onValueChange={setQuery}
            className="h-full min-w-0 flex-1 bg-transparent text-sm outline-hidden placeholder:text-muted-foreground"
          />
          {showCount && query.trim() !== `` && !loading ? <ResultCount /> : null}
        </div>
      )}
      <CommandList
        ref={listRef}
        onScroll={measure}
        className="max-h-none min-h-0 flex-1"
        style={listStyle}
      >
        {loading ? (
          // D8: the placeholder in THIS list's row geometry (32/44px).
          <SkeletonRows rowHeight={rowHeight} />
        ) : (
          <>
            <CommandEmpty className="">
              <ListEmpty>{emptyText}</ListEmpty>
            </CommandEmpty>
            <CommandGroup>
              {items.map((item) => {
                const state = selection.stateOf(item)
                const selected = state === `selected`
                return (
                  <CommandItem
                    key={item.value}
                    value={item.value}
                    keywords={pickerItemKeywords(item)}
                    disabled={selection.isDisabled(item)}
                    aria-pressed={selection.multiple ? selected : undefined}
                    data-testid={spec.testIds.row}
                    data-picked={selected ? `true` : undefined}
                    className={cn(`min-h-0 py-0`, ROW_CLASS[density], rowClassName)}
                    onSelect={() => selection.pick(item)}
                  >
                    {renderItem ? (
                      renderItem(item, { selected })
                    ) : (
                      <PickerItemBody item={{ ...item, description: undefined }} />
                    )}
                    <SelectionCheck state={state} />
                  </CommandItem>
                )
              })}
            </CommandGroup>
          </>
        )}
      </CommandList>
      {footer !== undefined && footer !== null ? (
        <div data-slot="search-list-footer" className="shrink-0 border-t border-glass-stroke">
          {footer}
        </div>
      ) : null}
    </Command>
  )
}

/** A caret rect as a Radix virtual anchor (zero width at the `#`). */
function virtualAnchor(anchor: TypeaheadAnchor) {
  return {
    current: {
      getBoundingClientRect: () =>
        ({
          x: anchor.left,
          y: anchor.top,
          left: anchor.left,
          top: anchor.top,
          right: anchor.left,
          bottom: anchor.bottom,
          width: 0,
          height: anchor.bottom - anchor.top,
          toJSON: () => ({}),
        }) as DOMRect,
    },
  }
}

/** THE `#` typeahead's surface: a popover `popoverWidth` wide hanging off
 *  the caret on a pointer (Radix positions and flips it), a bottom sheet
 *  riding the keyboard on a phone. Either way the list's input takes the
 *  focus on open; closing hands it back through `onReturnFocus` (the editor),
 *  never to a trigger — there is none — unless the close was a click
 *  somewhere else, which keeps the focus it went to. Stamped
 *  `data-editor-autocomplete`, so a dialog host treats a click in it as
 *  inside and an open one as "a menu is up" when routing Escape. */
export function SearchListPopover({
  open,
  onOpenChange,
  anchor,
  title,
  onReturnFocus,
  children,
  "data-testid": testId,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The caret rect in viewport coordinates (pointer only). */
  anchor: TypeaheadAnchor | null
  /** The phone sheet's accessible title. */
  title: string
  /** Focus back to the host editor once the surface closed. */
  onReturnFocus?: () => void
  children: React.ReactNode
  "data-testid"?: string
}) {
  const isMobile = useIsMobile()
  const { inset } = useKeyboardInset()
  const contentRef = React.useRef<HTMLDivElement | null>(null)
  const interactedOutside = React.useRef(false)
  const markOutside = () => {
    interactedOutside.current = true
  }
  const focusInput = (event: Event) => {
    interactedOutside.current = false
    event.preventDefault()
    contentRef.current
      ?.querySelector<HTMLInputElement>(`[data-slot=command-input]`)
      ?.focus()
  }
  const returnFocus = (event: Event) => {
    event.preventDefault()
    if (!interactedOutside.current) onReturnFocus?.()
    interactedOutside.current = false
  }

  if (isMobile) {
    return (
      <Sheet open={open} onOpenChange={onOpenChange}>
        <SheetContent
          ref={contentRef}
          side="bottom"
          data-testid={testId}
          data-editor-autocomplete=""
          onOpenAutoFocus={focusInput}
          onCloseAutoFocus={returnFocus}
          onInteractOutside={markOutside}
          style={inset > 0 ? { bottom: inset } : undefined}
          className="flex max-h-[60dvh] flex-col gap-0 p-0 pb-[env(safe-area-inset-bottom)]"
        >
          <SheetTitle className="sr-only">{title}</SheetTitle>
          {children}
        </SheetContent>
      </Sheet>
    )
  }

  return (
    <PopoverPrimitive.Root open={open && anchor !== null} onOpenChange={onOpenChange}>
      {anchor !== null && <PopoverPrimitive.Anchor virtualRef={virtualAnchor(anchor)} />}
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Content
          ref={contentRef}
          side="bottom"
          align="start"
          sideOffset={4}
          collisionPadding={8}
          data-testid={testId}
          data-editor-autocomplete=""
          onOpenAutoFocus={focusInput}
          onCloseAutoFocus={returnFocus}
          onInteractOutside={markOutside}
          className={cn(
            MENU_SURFACE_CLASS,
            `pointer-events-auto flex max-h-(--radix-popover-content-available-height) max-w-[calc(100vw-16px)] origin-(--radix-popover-content-transform-origin) flex-col overflow-hidden p-0 outline-hidden`
          )}
          style={{ width: spec.popoverWidth }}
        >
          {children}
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  )
}
