import { useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { TeamGeneralSection } from "@/components/team/general-section"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/general`)({
  component: SettingsGeneral,
})

function SettingsGeneral() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const { session, team, permissions, solo, resolved } =
    useSettingsPage(teamSlug)

  const [showDeleteTeam, setShowDeleteTeam] = useState(false)
  const [deleteConfirmation, setDeleteConfirmation] = useState(``)
  const [deletingTeam, setDeletingTeam] = useState(false)

  const { myTeams } = useTeamMemberships(session?.user?.id)
  // Deleting your LAST personal team is server-refused (EXP-82) — the
  // bootstrap feedback team (slug `feedback`) never counts as one.
  // Empty-while-loading biases to disabled, the safe default.
  const isOnlyTeam =
    myTeams.filter((w) => w.slug !== `feedback`).length <= 1

  const handleDeleteTeam = async () => {
    if (!team || deleteConfirmation !== team.name) return
    setDeletingTeam(true)
    try {
      await trpc.teams.delete.mutate({ teamId: team.id })
      void navigate({ to: `/` })
    } catch {
      setDeletingTeam(false)
    }
  }

  return (
    <SettingsSectionGuard
      resolved={resolved}
      allowed={permissions.canManageTeam}
    >
      <div className="space-y-6">
        {team && (
          <TeamGeneralSection team={team} solo={solo} />
        )}

        {team && !solo && (
          <>
            <Card className="border-destructive/50">
              <CardHeader>
                <CardTitle className="text-base text-destructive">
                  Danger Zone
                </CardTitle>
                <CardDescription>
                  Permanently delete this team and all its data.
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-2">
                <Button
                  variant="destructive"
                  disabled={isOnlyTeam}
                  onClick={() => setShowDeleteTeam(true)}
                >
                  Delete team
                </Button>
                {isOnlyTeam && (
                  <p className="text-sm text-muted-foreground">
                    This is your only team, so it can't be deleted.
                  </p>
                )}
              </CardContent>
            </Card>

            <Dialog
              open={showDeleteTeam}
              onOpenChange={(open) => {
                if (!open) {
                  setShowDeleteTeam(false)
                  setDeleteConfirmation(``)
                }
              }}
            >
              <DialogContent>
                <DialogHeader>
                  <DialogTitle>Delete team</DialogTitle>
                  <DialogDescription>
                    This will permanently delete{` `}
                    <span className="font-semibold text-foreground">
                      {team.name}
                    </span>
                    {` `}
                    and all its boards, issues, and data. This cannot be
                    undone.
                  </DialogDescription>
                </DialogHeader>
                <div className="space-y-2 py-2">
                  <Label htmlFor="delete-confirm">
                    Type{` `}
                    <span className="font-semibold">{team.name}</span>
                    {` `}to confirm
                  </Label>
                  <Input
                    id="delete-confirm"
                    value={deleteConfirmation}
                    onChange={(e) => setDeleteConfirmation(e.target.value)}
                    placeholder={team.name}
                  />
                </div>
                <DialogFooter>
                  <Button
                    variant="outline"
                    onClick={() => {
                      setShowDeleteTeam(false)
                      setDeleteConfirmation(``)
                    }}
                    disabled={deletingTeam}
                  >
                    Cancel
                  </Button>
                  <Button
                    variant="destructive"
                    onClick={handleDeleteTeam}
                    disabled={
                      deleteConfirmation !== team.name ||
                      deletingTeam
                    }
                  >
                    {deletingTeam ? `Deleting...` : `Delete team`}
                  </Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
          </>
        )}
      </div>
    </SettingsSectionGuard>
  )
}
