import { useEffect, useState } from "react"
import { useRouterState } from "@tanstack/react-router"
import { Sparkles } from "lucide-react"
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"
import { GettingStartedCards } from "@/components/getting-started/getting-started-cards"
import type { Workspace } from "@/db/schema"

// Sidebar-footer re-entry point for the "Getting started" cards (EXP-88):
// the inline block on the empty board disappears once issues exist (or is
// dismissed), so this keeps the setup guidance reachable.
export function GettingStartedButton({
  workspaceSlug,
  workspace,
}: {
  workspaceSlug: string
  workspace: Workspace | null | undefined
}) {
  const [open, setOpen] = useState(false)
  const permissions = useWorkspacePermissions(workspace)

  // Close the sheet when a card's link navigates (e.g. "Create a widget" →
  // workspace settings) — otherwise it would keep covering the new page.
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  useEffect(() => {
    setOpen(false)
  }, [pathname])

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={() => setOpen(true)}
        aria-label="Getting started"
        className="text-muted-foreground"
      >
        <Sparkles className="size-4" />
        <span>Getting started</span>
      </SidebarMenuButton>
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetContent side="right" className="overflow-y-auto sm:max-w-xl">
          <SheetHeader>
            <SheetTitle>Getting started</SheetTitle>
            <SheetDescription>
              Set up the coding loop, collect feedback from your site, and
              connect your tools.
            </SheetDescription>
          </SheetHeader>
          <div className="px-4 pb-6">
            <GettingStartedCards
              workspaceSlug={workspaceSlug}
              canManageWidgets={permissions.canManageWidgets}
              layout="stack"
            />
          </div>
        </SheetContent>
      </Sheet>
    </SidebarMenuItem>
  )
}
