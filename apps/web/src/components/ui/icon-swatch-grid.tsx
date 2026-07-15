import type { ProjectIcon } from "@exp/db-schema/domain"
import { PROJECT_ICON_OPTIONS } from "@/lib/project-types"

interface IconSwatchGridProps {
  value: ProjectIcon
  onChange: (icon: ProjectIcon) => void
  // Tints the selected glyph with the project color for a live preview.
  color?: string
}

// Sibling of ColorSwatchGrid: the curated project icon set as a swatch row.
export function IconSwatchGrid({ value, onChange, color }: IconSwatchGridProps) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {PROJECT_ICON_OPTIONS.map(({ name, icon: Icon }) => (
        <button
          key={name}
          type="button"
          aria-label={name}
          className={`flex h-7 w-7 items-center justify-center rounded-md border transition-all hover:scale-110 ${
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
