import { Sparkles } from "lucide-react"
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import { useGettingStartedSheet } from "@/components/getting-started/getting-started-sheet"
import { useGettingStartedProgressContext } from "@/hooks/use-getting-started-progress"

// Sidebar-footer re-entry point for the "Getting started" cards (EXP-88):
// the inline block on the empty board disappears once issues exist, so this
// keeps the setup guidance reachable. EXP-548: rendered until every entry is
// done (no dismissal), and not at all while the signals still load — the
// desktop rail entry follows the exact same rule. EXP-686: the sheet itself
// lives at the team layout (`getting-started-sheet.tsx`), so the Actions and
// Automations lightbulbs can open it once this entry has hidden itself.
export function GettingStartedButton() {
  const { loading, complete } = useGettingStartedProgressContext()
  const sheet = useGettingStartedSheet()

  if (loading || complete) return null

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
