import { useMemo, useState } from "react"
import { Search } from "lucide-react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { BOARD_ICON_OPTIONS } from "@/lib/board-icons"
import { Input } from "@/components/ui/input"

interface IconSwatchGridProps {
  value: BoardIcon
  onChange: (icon: BoardIcon) => void
  // Tints the selected glyph with the board color for a live preview.
  color?: string
}

// Sibling of ColorSwatchGrid: the curated icon set as a swatch grid.
// EXP-273 grew the set from 16 to 60, so it gained a filter and a bounded
// scroll area — a flat wrap of 60 swatches pushed the dialog's action row off
// screen. The filter matches the stored Lucide name, which is also what the
// name reads as ("git-branch" finds the branch glyph).
export function IconSwatchGrid({ value, onChange, color }: IconSwatchGridProps) {
  const [query, setQuery] = useState(``)

  const options = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return BOARD_ICON_OPTIONS
    return BOARD_ICON_OPTIONS.filter(({ name }) => name.includes(q))
  }, [query])

  return (
    <div className="space-y-2">
      <div className="relative">
        <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search icons"
          aria-label="Search icons"
          className="h-8 pl-7 text-sm"
        />
      </div>
      {options.length === 0 ? (
        <p className="py-3 text-center text-xs text-muted-foreground">
          No icon matches “{query.trim()}”
        </p>
      ) : (
        <div className="flex max-h-44 flex-wrap gap-1.5 overflow-y-auto">
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
      )}
    </div>
  )
}
