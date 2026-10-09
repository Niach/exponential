import type { LucideIcon } from "lucide-react"
import type { ComponentType, ReactNode, SVGProps } from "react"

import { cn } from "../cn"

// EXP-1021 + the UI cleanup batch — THE one row shape every picker speaks.
//
// `PickerOption` (EXP-941) and `PickerItem` (EXP-1021) grew side by side with
// the same fields and two colour models; they are one type now. `value` +
// `label` are the only required fields, everything else is a slot a row MAY
// draw. It is presentational: no records, no async state — a caller maps its
// own domain rows into these once, at the call site (or through a typed
// picker's `…PickerItems` mapper).

/** What a row may lead with: a registry concept glyph, or one of the
 *  generated BRAND marks (`@exp/ui` `brandIcon()`, the MCP catalog). Both
 *  take SVG props and size through `className`; a brand mark keeps its own
 *  fills, so a row `color` tints a glyph and leaves a mark alone. */
export type PickerGlyph = LucideIcon | ComponentType<SVGProps<SVGSVGElement>>

export interface PickerItem<T extends string = string> {
  /** The stable identity of the row; also what search matches on. */
  value: T
  /** What the row reads as. A string also seeds the search keywords. */
  label: ReactNode
  /** A leading glyph — a concept icon or a generated brand mark, never a
   *  raw lucide import or a hand-drawn path. */
  icon?: PickerGlyph
  /** A colour for the glyph (a board's hex, a label's dot, a status tone).
   *  A hex (or any CSS colour) tints the glyph; a Tailwind text class is
   *  applied as-is. With no `icon` the colour draws as the row's DOT — that
   *  is what makes a label row a coloured dot and a board row a tinted
   *  glyph without the caller choosing a shape. */
  color?: string
  /** A muted second line (an email, a path). */
  description?: ReactNode
  /** A muted TRAILING note on the same line (`default`, a branch's age, a
   *  login's health). */
  hint?: ReactNode
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

/** @deprecated The two option shapes merged: use `PickerItem`. Kept as an
 *  alias while `apps/web/src/lib/domain.ts` still names it. */
export type PickerOption<T extends string = string> = PickerItem<T>

/** A row's glyph tint: a CSS colour tints, a Tailwind class is a class. */
function isColorClass(color: string) {
  return !color.startsWith(`#`) && !color.includes(`(`)
}

/** THE row body: the dot or the tinted glyph, the label, the muted second
 *  line, the trailing hint. Plain — a picker row is never a card, on any
 *  surface (popover, sheet, or a menu's submenu). */
export function PickerItemBody<T extends string>({
  item,
}: {
  item: PickerItem<T>
}) {
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
      {item.hint !== undefined && item.hint !== null ? (
        <span
          data-slot="picker-hint"
          className="ml-2 shrink-0 text-xs text-muted-foreground"
        >
          {item.hint}
        </span>
      ) : null}
    </>
  )
}

/** The keywords a row matches on: the explicit ones, else a string label. */
export function pickerItemKeywords(item: PickerItem<string>): string[] {
  if (item.keywords && item.keywords.length > 0) return item.keywords
  return typeof item.label === `string` ? [item.label] : []
}
