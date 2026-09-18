import type { LucideIcon } from "lucide-react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { BOARD_ICON_OPTIONS } from "./board-icons"

// One pickable set: the board icons, or the device icons (EXP-924).
export type IconOptions<T extends string> = ReadonlyArray<{
  name: T
  icon: LucideIcon
}>

interface IconSwatchGridProps<T extends string> {
  // Empty = nothing selected (IconPicker with `allowsNone`).
  value: T | ``
  onChange: (icon: T) => void
  // Tints the selected glyph with the board color for a live preview.
  color?: string
  // The set to offer; the board set unless a surface names another.
  options?: IconOptions<T>
}

// Sibling of ColorSwatchGrid: the curated icon set as a swatch grid. Since
// EXP-575 it only renders inside `IconPicker`'s popover — forms show the slim
// trigger, never this grid inline. The search filter it once had was dropped
// in EXP-390: 60 glyphs scan faster than they search, on every platform.
//
// EXP-771 shape rule: icon cells are rounded SQUARES (`rounded-md`, 10px),
// like the picker trigger that opens them; only COLOR swatches are circles.
export function IconSwatchGrid<T extends string = BoardIcon>({
  value,
  onChange,
  color,
  options = BOARD_ICON_OPTIONS as unknown as IconOptions<T>,
}: IconSwatchGridProps<T>) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {options.map(({ name, icon: Icon }) => (
        <button
          key={name}
          type="button"
          aria-label={name}
          aria-pressed={value === name}
          title={name}
          className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-md border transition-all hover:scale-110 ${
            value === name
              ? `border-foreground bg-accent`
              : `border-border text-muted-foreground`
          }`}
          onClick={() => onChange(name)}
        >
          <Icon
            className="h-4 w-4"
            style={value === name && color ? { color } : undefined}
          />
        </button>
      ))}
    </div>
  )
}
