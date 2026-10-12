import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  GettingStartedStrip,
  toast,
} from "@exp/ui"
import { openGithubConnect, POPUP_BLOCKED_MESSAGE } from "@/lib/github-connect"
import { useGettingStartedProgressContext } from "@/hooks/use-getting-started-progress"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { GETTING_STARTED_STEP_TITLES } from "@/components/getting-started/getting-started-copy"
import type { GettingStartedTarget } from "@/components/getting-started/getting-started-model"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { AddDeviceDialog } from "@/components/add-device-dialog"
import { InviteMemberForm } from "@/components/team/invite-member-form"
import type { Team } from "@/db/schema"

// EXP-1270 (EXP-1259 D6, `getting-started.json`): Getting started = ONE
// block ×4. Its face is `@exp/ui` `GettingStartedStrip` (the header + one
// row of square cards, the styleguide draws the same one); this wires it to
// the team's signals (`useGettingStartedProgressContext` → the pure
// `deriveGettingStarted`) and runs a card's target. The model already turned
// a device-needing step without a device into `add-device`.
//
// Mounted on an EMPTY board (under "No issues yet"), on the Agent home while
// the team has no board, and inside the sidebar entry's centred dialog
// (`getting-started-button.tsx`). It hides while the signals load and once
// every step is done.

/** The Agent page's composer field takes the focus once it is mounted (a
 *  navigation away from a board lands a frame or two later). */
function focusComposerField(framesLeft = 30) {
  requestAnimationFrame(() => {
    const field = document.getElementById(`agent-composer-field`)
    if (field) field.focus()
    else if (framesLeft > 0) focusComposerField(framesLeft - 1)
  })
}

export interface GettingStartedSectionProps {
  team: Team
  teamSlug: string
}

/** The block where a page shows it: nothing while loading or once done. */
export function GettingStartedSection(props: GettingStartedSectionProps) {
  const { loading, complete } = useGettingStartedProgressContext()
  if (loading || complete) return null
  return (
    <div className="mx-auto w-full max-w-4xl px-6 pb-12">
      <GettingStartedBlock {...props} />
    </div>
  )
}

/** The header + the strip, with the dialogs its targets open. `onLeave`
 *  runs before a target that navigates away (the sidebar dialog closes). */
export function GettingStartedBlock({
  team,
  teamSlug,
  onLeave,
}: GettingStartedSectionProps & { onLeave?: () => void }) {
  const { entries, done, total, progress } = useGettingStartedProgressContext()
  const navigate = useNavigate()
  const openComposer = useOpenComposer()
  const [dialog, setDialog] = useState<
    `create-board` | `add-device` | `invite` | null
  >(null)

  const run = (target: GettingStartedTarget) => {
    switch (target) {
      case `create-board`:
      case `add-device`:
      case `invite`:
        setDialog(target)
        return
      // The device-setup block has no tabs on the web: both cards (the
      // desktop app and the server one-liner) are always there.
      case `add-server`:
        setDialog(`add-device`)
        return
      case `github`:
        // SLOP-7: the ONE guided page, as a popup over this page (the
        // progress hook re-probes on focus).
        if (!openGithubConnect({ teamId: team.id })) {
          toast.error(POPUP_BLOCKED_MESSAGE)
        }
        return
      case `agent`:
        onLeave?.()
        openComposer({}, { origin: null })
        focusComposerField()
        return
      case `mcp`:
        onLeave?.()
        void navigate({
          to: `/t/$teamSlug/settings/mcp-servers`,
          params: { teamSlug },
        })
        return
      case `widget`:
        onLeave?.()
        void navigate({
          to: `/t/$teamSlug/settings/widget`,
          params: { teamSlug },
        })
        return
    }
  }

  const origin =
    typeof window === `undefined` ? `https://app.exponential.at` : window.location.origin

  return (
    <>
      <GettingStartedStrip
        entries={entries}
        done={done}
        total={total}
        progress={progress}
        onSelect={(key) => {
          const entry = entries.find((row) => row.key === key)
          if (entry) run(entry.target)
        }}
      />
      <CreateBoardDialog
        open={dialog === `create-board`}
        onOpenChange={(open) => setDialog(open ? `create-board` : null)}
        team={team}
      />
      <AddDeviceDialog
        open={dialog === `add-device`}
        onOpenChange={(open) => setDialog(open ? `add-device` : null)}
        origin={origin}
      />
      <InviteDialog
        open={dialog === `invite`}
        onOpenChange={(open) => setDialog(open ? `invite` : null)}
        teamId={team.id}
      />
    </>
  )
}

/** The invite target: the members section's form in a dialog. */
function InviteDialog({
  open,
  onOpenChange,
  teamId,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md" aria-describedby={undefined}>
        <DialogHeader>
          <DialogTitle>{GETTING_STARTED_STEP_TITLES.invite}</DialogTitle>
        </DialogHeader>
        <InviteMemberForm teamId={teamId} layout="stack" autoFocus="email" />
      </DialogContent>
    </Dialog>
  )
}
