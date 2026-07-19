import { Link, useParams } from "@tanstack/react-router"
import { Plus } from "lucide-react"
import type { Board } from "@/db/schema"
import { Button } from "@/components/ui/button"
import { SidebarTrigger } from "@/components/ui/sidebar"

interface TeamMobileTopbarProps {
  teamSlug: string
  boards: Board[]
  teamId?: string
}

// Mobile-only chrome: hamburger (opens the sidebar drawer) + team/board
// context + New issue. Search, My Issues, Inbox and the user menu all live in
// the drawer, so the topbar deliberately does NOT duplicate them. The whole
// header is `md:hidden` — on desktop the persistent sidebar covers everything,
// so rendering nothing here avoids an empty 48px strip.
export function TeamMobileTopbar({
  teamSlug,
  boards,
}: TeamMobileTopbarProps) {
  const params = useParams({ strict: false }) as { boardSlug?: string }
  const activeBoard = params.boardSlug
    ? boards.find((p) => p.slug === params.boardSlug)
    : undefined

  return (
    <header className="flex items-center gap-2 border-b px-3 h-12 md:hidden">
      <SidebarTrigger />
      {activeBoard && (
        <div className="flex items-center gap-1.5 text-sm font-medium truncate min-w-0">
          <span
            className="h-2.5 w-2.5 rounded-full shrink-0"
            style={{ backgroundColor: activeBoard.color }}
          />
          <span className="truncate">{activeBoard.name}</span>
        </div>
      )}
      {activeBoard && (
        <Button
          asChild
          size="icon"
          variant="ghost"
          className="ml-auto size-9 text-muted-foreground"
        >
          <Link
            to="/t/$teamSlug/boards/$boardSlug"
            params={{ teamSlug, boardSlug: activeBoard.slug }}
            search={{ new: 1 }}
            aria-label="New issue"
          >
            <Plus className="size-5" />
          </Link>
        </Button>
      )}
    </header>
  )
}
