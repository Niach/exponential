import { useRef, useState, type CSSProperties } from "react"
import spec from "@exp/domain-contract/fixtures/getting-started.json"
import {
  Dialog,
  DialogBody,
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
  const visible = useGettingStartedEntryVisible(team)
  const [open, setOpen] = useState(false)

  if (!visible || !team) return null

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
      <GettingStartedDialog team={team} teamSlug={teamSlug} open={open} onOpenChange={setOpen} />
    </SidebarMenuItem>
  )
}

/** Whether the Getting started entry shows (sidebar, phone Team page): not
 *  while the signals load, not once every step is done. */
export function useGettingStartedEntryVisible(team: Team | null | undefined): boolean {
  const { loading, complete } = useGettingStartedProgressContext()
  return !loading && !complete && Boolean(team)
}

// The block in its dialog (the phone width turns DialogContent into a bottom
// sheet): the sidebar entry and the phone Team page's row (#EXP-1632) open
// the SAME one.
export function GettingStartedDialog({
  team,
  teamSlug,
  open,
  onOpenChange,
}: {
  team: Team
  teamSlug: string
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  // A row that navigates (Start a session, the settings pages) closes the
  // dialog; its focus then belongs to the new page, not back on this entry.
  const leaving = useRef(false)
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* No corner close: the Docs button holds that corner; Escape and
          a click outside close it. The dialog IS the block's box:
          `maxWidth` wide, scrolling vertically only. */}
      <DialogContent
        className="gap-0 p-0 pb-[env(safe-area-inset-bottom)] sm:max-w-(--getting-started-max) sm:pb-0"
        style={{ [`--getting-started-max`]: `${spec.maxWidth}px` } as CSSProperties}
        showCloseButton={false}
        aria-describedby={undefined}
        onCloseAutoFocus={(event) => {
          if (leaving.current) event.preventDefault()
          leaving.current = false
        }}
      >
        <DialogTitle className="sr-only">{GETTING_STARTED_TITLE}</DialogTitle>
        <DialogBody className="overflow-x-hidden">
          <GettingStartedBlock
            team={team}
            teamSlug={teamSlug}
            framed={false}
            onLeave={() => {
              leaving.current = true
              onOpenChange(false)
            }}
          />
        </DialogBody>
      </DialogContent>
    </Dialog>
  )
}
