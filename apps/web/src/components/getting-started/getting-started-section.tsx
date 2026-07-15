import { useState } from "react"
import { X } from "lucide-react"
import { useSession } from "@/hooks/use-session"
import { hasDismissedGettingStarted } from "@/lib/auth/app-user"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  GettingStartedCards,
  type GettingStartedCardsProps,
} from "@/components/getting-started/getting-started-cards"

// The dismissible "Getting started" block under the project board's
// "No issues yet" empty state (EXP-88). Dismissal is one-way and per-user
// (users.gettingStartedDismissedAt, surfaced read-only on the session); the
// sidebar's Getting started entry stays as the re-entry point.
export function GettingStartedSection({
  workspaceSlug,
  projectIsPublic,
  canManageWidgets,
}: Omit<GettingStartedCardsProps, `layout`>) {
  const { data: session, isPending } = useSession()
  const [dismissed, setDismissed] = useState(false)

  // Render nothing until the session resolves — a flash of cards that then
  // vanish for a user who already dismissed them is worse than a beat of
  // empty space.
  if (isPending || dismissed || hasDismissedGettingStarted(session?.user)) {
    return null
  }

  const dismiss = () => {
    // Hide immediately; the server flag keeps it hidden across reloads
    // (fire-and-forget, same contract as users.dismissDesktopAppCard).
    setDismissed(true)
    void trpc.users.dismissGettingStarted.mutate()
  }

  return (
    <div className="mx-auto w-full max-w-4xl px-6 pb-12">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-sm font-medium text-muted-foreground">
          Getting started
        </h3>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0 text-muted-foreground"
          aria-label="Dismiss getting started"
          onClick={dismiss}
        >
          <X className="size-4" />
        </Button>
      </div>
      <GettingStartedCards
        workspaceSlug={workspaceSlug}
        projectIsPublic={projectIsPublic}
        canManageWidgets={canManageWidgets}
        layout="grid"
      />
    </div>
  )
}
