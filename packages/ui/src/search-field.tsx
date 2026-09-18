import * as React from "react"

import { Button } from "./button"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { Input } from "./input"

// EXP-941 — the ONE "filter this list" field on the web.
//
// Eight of them existed at five different heights, most of them a bare `Input`
// re-dressed by hand and none of them with the leading glyph or the trailing
// clear the natives have drawn for years (iOS `GlassSheetSearchField`,
// Android `GlassSheet.kt`'s search row). This is their web twin: the glyph
// sits INSIDE the field, the clear button appears only once there is something
// to clear, and clearing puts the caret back in the field so typing continues.
//
// It is an `Input`, not a new box: every chrome decision (glass fill, hairline,
// focus stroke) still comes from there.
const SearchGlyph = conceptIcon(`nav-search`)
const ClearGlyph = conceptIcon(`ui-clear`)

/** The two rungs: `md` is the stock 36px field, `sm` the 28px one dense
 *  columns use (the Reviews file filter, a sidebar filter). Literal strings —
 *  the package scans its own sources, so a size map must not be built at
 *  runtime. */
const SIZE_CLASS = {
  md: `h-9 pl-8`,
  sm: `h-7 pl-7 text-xs`,
} as const

const GLYPH_CLASS = {
  md: `left-2.5 size-4`,
  sm: `left-2 size-3.5`,
} as const

type SearchFieldProps = Omit<
  React.ComponentProps<typeof Input>,
  `value` | `onChange` | `size`
> & {
  value: string
  onValueChange: (value: string) => void
  size?: keyof typeof SIZE_CLASS
  /** The trailing clear button. On by default. */
  clearable?: boolean
  /** The clear button's accessible name. */
  clearLabel?: string
}

const SearchField = React.forwardRef<HTMLInputElement, SearchFieldProps>(
  (
    {
      value,
      onValueChange,
      size = `md`,
      clearable = true,
      clearLabel,
      className,
      ...props
    },
    ref
  ) => {
    const innerRef = React.useRef<HTMLInputElement | null>(null)
    const showClear = clearable && value !== ``

    return (
      <div data-slot="search-field" className="relative w-full">
        <SearchGlyph
          aria-hidden
          className={cn(
            `pointer-events-none absolute top-1/2 -translate-y-1/2 text-muted-foreground`,
            GLYPH_CLASS[size]
          )}
        />
        <Input
          ref={(node) => {
            innerRef.current = node
            if (typeof ref === `function`) {
              ref(node)
            } else if (ref) {
              ref.current = node
            }
          }}
          // The native decoration is a second, differently-drawn clear button
          // in WebKit; ours is the only one.
          type="search"
          value={value}
          onChange={(event) => onValueChange(event.target.value)}
          className={cn(
            SIZE_CLASS[size],
            `[&::-webkit-search-cancel-button]:hidden`,
            showClear && `pr-8`,
            className
          )}
          {...props}
        />
        {showClear && (
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            data-slot="search-field-clear"
            aria-label={clearLabel ?? `Clear search`}
            className="absolute top-1/2 right-1.5 -translate-y-1/2 text-muted-foreground"
            onClick={() => {
              onValueChange(``)
              innerRef.current?.focus()
            }}
          >
            <ClearGlyph aria-hidden />
          </Button>
        )}
      </div>
    )
  }
)

SearchField.displayName = `SearchField`

export { SearchField }
export type { SearchFieldProps }
