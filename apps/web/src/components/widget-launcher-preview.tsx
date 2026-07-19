import { MessageSquarePlus } from "lucide-react"
import { cn } from "@/lib/utils"

// The floating-button preview pill shared by the widget settings dialog and
// the getting-started widget card (EXP-141 — extracted unchanged from
// team/widget-section.tsx).

// Mirrors the widget bundle's theme.defaultAccent / pickForeground so the
// launcher preview matches what embedders actually see.
export const DEFAULT_ACCENT = `#e5e5e5`

export function previewForeground(color: string): string {
  const match = /^#([0-9a-f]{6})$/i.exec(color.trim())
  if (!match) return `#171717`
  const value = Number.parseInt(match[1], 16)
  const luminance =
    0.2126 * ((value >> 16) & 0xff) +
    0.7152 * ((value >> 8) & 0xff) +
    0.0722 * (value & 0xff)
  return luminance > 140 ? `#171717` : `#fafafa`
}

export function WidgetLauncherPreview({
  accentColor,
  label,
  className,
}: {
  accentColor?: string
  label?: string
  className?: string
}) {
  const accent = accentColor || DEFAULT_ACCENT
  return (
    <span
      className={cn(
        `inline-flex items-center gap-2 rounded-full px-4 py-2 text-[13px] font-semibold shadow`,
        className
      )}
      style={{
        backgroundColor: accent,
        color: previewForeground(accent),
      }}
    >
      <MessageSquarePlus className="h-4 w-4" />
      {label || `Feedback`}
    </span>
  )
}
