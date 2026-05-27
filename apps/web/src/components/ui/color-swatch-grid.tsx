import { LABEL_COLORS } from "@/lib/label-colors"

interface ColorSwatchGridProps {
  colors?: string[]
  value: string
  onChange: (color: string) => void
}

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
          className={`h-5 w-5 rounded-full transition-all hover:scale-110 ${
            value === c
              ? `ring-2 ring-offset-2 ring-offset-background ring-foreground`
              : ``
          }`}
          style={{ backgroundColor: c }}
          onClick={() => onChange(c)}
        />
      ))}
    </div>
  )
}
