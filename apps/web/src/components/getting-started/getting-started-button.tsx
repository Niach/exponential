import { useRef, useState } from "react"
import spec from "@exp/domain-contract/fixtures/getting-started.json"
import {
  Dialog,
  DialogContent,
  DialogTitle,
  SidebarMenuButton,
  SidebarMenuItem,
  conceptIcon,
} from "@exp/ui"
import { useGettingStartedProgressContext } from "@/hooks/use-getting-started-progress"
import { GETTING_STARTED_TITLE } from "@/components/getting-started/getting-started-copy"
import { GettingStartedBlock } from "@/components/getting-started/getting-started-section"
import type { Team } from "@/db/schema"

// EXP-317: a cross-client CONCEPT, not a raw lucide import — the natives'
// Getting started entry uses the same glyph.
const GettingStartedIcon = conceptIcon(`nav-getting-started`)

// The sidebar-footer entry (EXP-88; EXP-1270 / getting-started.json): it
// opens the SAME block the empty board and the board-less Agent home show,
// in a centred dialog (the right sheet is gone). Like the block it hides
// while the signals load and once every step is done.
export function GettingStartedButton({
  team,
  teamSlug,
}: {
  team: Team | null | undefined
  teamSlug: string
}) {
  const { loading, complete } = useGettingStartedProgressContext()
  const [open, setOpen] = useState(false)
  // A card that navigates (Start a run, the settings pages) closes the
  // dialog; its focus then belongs to the new page, not back on this entry.
  const leaving = useRef(false)

  if (loading || complete || !team) return null

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={() => setOpen(true)}
        aria-label={GETTING_STARTED_TITLE}
        data-testid={spec.testIds.entry}
        className="text-muted-foreground"
      >
        <GettingStartedIcon className="size-4" />
        <span>{GETTING_STARTED_TITLE}</span>
      </SidebarMenuButton>
      <Dialog open={open} onOpenChange={setOpen}>
        {/* No corner close: the Docs button holds that corner; Escape and
            a click outside close it. */}
        <DialogContent
          className="sm:max-w-[60rem]"
          showCloseButton={false}
          aria-describedby={undefined}
          onCloseAutoFocus={(event) => {
            if (leaving.current) event.preventDefault()
            leaving.current = false
          }}
        >
          <DialogTitle className="sr-only">{GETTING_STARTED_TITLE}</DialogTitle>
          <GettingStartedBlock
            team={team}
            teamSlug={teamSlug}
            onLeave={() => {
              leaving.current = true
              setOpen(false)
            }}
          />
        </DialogContent>
      </Dialog>
    </SidebarMenuItem>
  )
}
