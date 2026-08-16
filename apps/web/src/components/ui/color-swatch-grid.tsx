import { LABEL_COLORS } from "@/lib/label-colors"

interface ColorSwatchGridProps {
  colors?: string[]
  value: string
  onChange: (color: string) => void
}

// Selection is drawn INSIDE the swatch's own box — a 28px cell with a 2px
// border around a 20px dot — never as a `ring-offset` halo. A ring paints
// past the border box, and the nearest scroll container clips it: inside a
// `DialogBody` (`overflow-y-auto`, no padding of its own) that sliced the
// left half off the first swatch (EXP-524). Same 28px cell as
// IconSwatchGrid, so the two grids line up column for column.
export function ColorSwatchGrid({
  colors = LABEL_COLORS,
  value,
  onChange,
}: ColorSwatchGridProps) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {colors.map((c) => (
        <button
          key={c}
          type="button"
          aria-label={c}
          aria-pressed={value === c}
          title={c}
          className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full border-2 transition-all hover:scale-110 ${
            value === c ? `border-foreground` : `border-transparent`
          }`}
          onClick={() => onChange(c)}
        >
          <span
            className="h-5 w-5 rounded-full"
            style={{ backgroundColor: c }}
          />
        </button>
      ))}
    </div>
  )
}
