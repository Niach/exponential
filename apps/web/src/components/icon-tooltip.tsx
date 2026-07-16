import type { ReactNode } from "react"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"

// Hover label for the icon-only buttons in the issue header. Icon-only controls
// carry no visible text, so the tooltip IS their label — without one, a bare
// chain/bell glyph gives the user nothing to read (EXP-140).
//
// Prefer this over the native `title` attribute: `title` waits ~1-2s before it
// appears, renders unstyled, and never fires on a disabled button.
export function IconTooltip({
  label,
  shortcut,
  children,
}: {
  label: string
  // Keyboard equivalent, rendered muted after the label (e.g. `J`).
  shortcut?: string
  children: ReactNode
}) {
  return (
    <Tooltip>
      {/* The span keeps the trigger hoverable while the button inside is
          disabled — a disabled button fires no pointer events of its own, so
          binding the trigger straight to it would drop the tooltip exactly when
          the user most wants to know why the control is dead. */}
      <TooltipTrigger asChild>
        <span className="inline-flex">{children}</span>
      </TooltipTrigger>
      <TooltipContent>
        {label}
        {shortcut && (
          <span className="ml-1.5 font-mono text-background/60">
            {shortcut}
          </span>
        )}
      </TooltipContent>
    </Tooltip>
  )
}
