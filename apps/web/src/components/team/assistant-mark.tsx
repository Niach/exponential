import { ExponentialLogo } from "@exp/ui"
import { cn } from "@/lib/utils"

// S-3 (agent-settings.json `assistant`): the Assistant's avatar in a row =
// the Exponential brand mark (the composer home headline's logo) in the
// foreground colour, filling the glyph's own box (no disc, no padding), never
// its stored icon/colour. Sized like the BoardGlyph it stands beside (size-4).
export function AssistantMark({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      data-slot="assistant-mark"
      className={cn(`flex size-4 shrink-0 items-center justify-center text-foreground`, className)}
    >
      <ExponentialLogo variant="light" size={16} className="size-full" />
    </span>
  )
}
