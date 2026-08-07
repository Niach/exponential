import { MessageSquarePlus } from "lucide-react"
import { paletteFor, pickForeground } from "@exp/widget/theme"
import type { ThemeMode } from "@exp/widget/theme"
import { cn } from "@/lib/utils"

// The floating-button preview pill shared by the widget settings dialog and
// the getting-started widget card (EXP-141 — extracted unchanged from
// team/widget-section.tsx). Accent defaults and foreground contrast come
// straight from the widget package so the preview can never drift from what
// embedders actually see.

export const DEFAULT_ACCENT = paletteFor(`dark`).defaultAccent

export const previewForeground = pickForeground

export function WidgetLauncherPreview({
  accentColor,
  label,
  theme = `dark`,
  className,
}: {
  accentColor?: string
  label?: string
  theme?: ThemeMode
  className?: string
}) {
  const accent = accentColor || paletteFor(theme).defaultAccent
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
