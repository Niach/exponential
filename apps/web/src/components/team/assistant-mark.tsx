import { ExponentialLogo } from "@exp/ui"
import { cn } from "@/lib/utils"

// S-3 (agent-settings.json `assistant`): the Assistant's avatar in a
// settings row = the Exponential brand mark (the composer home headline's
// logo) in the foreground colour on bg muted, never its stored icon/colour.
// Sized like the BoardGlyph it stands beside (size-4).
export function AssistantMark({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      data-slot="assistant-mark"
      className={cn(`flex size-4 shrink-0 items-center justify-center rounded-full bg-muted text-foreground`, className)}
    >
      <ExponentialLogo variant="light" size={12} className="size-3/4" />
    </span>
  )
}
