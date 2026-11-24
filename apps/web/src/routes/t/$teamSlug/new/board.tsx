import { useEffect, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { BoardLimitUpgradeDialog, CreateBoardForm } from "@/components/create-board-dialog"
import { NewPageHeading } from "@/components/team/new-page-heading"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { boardCollection } from "@/lib/collections"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 (sidebar-tree.json TEAM MENU): New › Board opens this PAGE, never
// a dialog: the create-board form (`CreateBoardForm`, the dialog's body) under
// its heading; a created board opens at once. Owner-only like the dialog's
// entry points: anyone else lands on Home.
export const Route = createFileRoute(`/t/$teamSlug/new/board`)({
  head: () => ({ meta: [{ title: pageTitle(`New board`) }] }),
  component: NewBoardRoute,
})

function NewBoardRoute() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  const refused = resolved && !permissions.isOwner
  useEffect(() => {
    if (refused) void navigate({ to: `/home`, replace: true })
  }, [refused, navigate])
  if (!team || !resolved || refused) return null
  return (
    <div className={`mx-auto w-full max-w-4xl space-y-6 p-4 sm:p-6 ${PAGE_END_CLEARANCE}`}>
      <NewPageHeading teamSlug={teamSlug}>New board</NewPageHeading>
      <CreateBoardForm
        team={team}
        layout="page"
        onCreated={async ({ slug, txId }) => {
          await boardCollection.utils.awaitTxId(txId, 3000).catch(() => undefined)
          void navigate({
            to: `/t/$teamSlug/boards/$boardSlug`,
            params: { teamSlug: team.slug, boardSlug: slug },
            replace: true,
          })
        }}
        onPlanLimit={() => setUpgradeOpen(true)}
      />
      <BoardLimitUpgradeDialog open={upgradeOpen} onOpenChange={setUpgradeOpen} teamId={team.id} />
    </div>
  )
}
