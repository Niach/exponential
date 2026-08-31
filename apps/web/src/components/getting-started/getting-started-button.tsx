import { Sparkles } from "lucide-react"
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import { useGettingStartedSheet } from "@/components/getting-started/getting-started-sheet"
import { useGettingStartedProgressContext } from "@/hooks/use-getting-started-progress"

// Sidebar-footer re-entry point for the "Getting started" cards (EXP-88):
// the inline block on the empty board disappears once issues exist, so this
// keeps the setup guidance reachable. EXP-697: ALWAYS rendered (a completed
// checklist used to hide it) — the sheet doubles as the way back to the
// suggestions and install snippets, so the entry stays. Only the initial
// signal load hides it, to avoid a flash of state.
export function GettingStartedButton() {
  const { loading } = useGettingStartedProgressContext()
  const sheet = useGettingStartedSheet()

  if (loading) return null

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={() => sheet.open(`first-steps`)}
        aria-label="Getting started"
        className="text-muted-foreground"
      >
        <Sparkles className="size-4" />
        <span>Getting started</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}
