import { useState } from "react"
import { deleteTeamPrompt, promptActions } from "@/lib/prompts"
import { createFileRoute } from "@tanstack/react-router"
import { TeamGeneralSection } from "@/components/team/general-section"
import {
  Button,
  GlassRow,
  GlassSectionHeader,
  Input,
  Label,
  Prompt,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/general`)({
  head: () => ({
    meta: [{ title: pageTitle(`General`, `Settings`) }],
  }),
  component: SettingsGeneral,
})

function SettingsGeneral() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)

  const [showDeleteTeam, setShowDeleteTeam] = useState(false)
  const [deleteConfirmation, setDeleteConfirmation] = useState(``)
  const [deletingTeam, setDeletingTeam] = useState(false)
  const [deleteError, setDeleteError] = useState(``)

  const handleDeleteTeam = async () => {
    if (!team || deleteConfirmation !== team.name) return
    setDeletingTeam(true)
    setDeleteError(``)
    try {
      await trpc.teams.delete.mutate({ teamId: team.id })
      // Deleting a team rotates every shape's scope — hard-navigate so all
      // Electric collections restart cleanly. Deleting your LAST team is
      // allowed (EXP-188): the root redirect then lands on /onboarding.
      window.location.assign(`/`)
    } catch (err) {
      // The server refuses a team with a live subscription (REV2-55) — that
      // message tells the owner to cancel in Billing first, so it must be
      // shown rather than swallowed.
      setDeleteError(
        err instanceof Error && err.message
          ? err.message
          : `Couldn't delete this team`
      )
      setDeletingTeam(false)
    }
  }

  const deleteCopy = deleteTeamPrompt(team?.name ?? ``)

  return (
    <SettingsSectionGuard
      resolved={resolved}
      allowed={permissions.canManageTeam}
    >
      <div className="space-y-6">
        {team && <TeamGeneralSection team={team} />}

        {team && (
          <>
            <div>
              <GlassSectionHeader
                label="Danger zone"
                className="[&>span]:text-destructive"
              />
              <GlassRow className="justify-between gap-3">
                <span className="min-w-0 text-sm text-muted-foreground">
                  Permanently delete this team and all its data.
                </span>
                <Button
                  variant="destructive"
                  size="xs"
                  className="shrink-0"
                  onClick={() => setShowDeleteTeam(true)}
                >
                  Delete team
                </Button>
              </GlassRow>
            </div>

            <Prompt
              open={showDeleteTeam}
              onOpenChange={(open) => {
                if (!open) {
                  setShowDeleteTeam(false)
                  setDeleteConfirmation(``)
                  setDeleteError(``)
                }
              }}
              busy={deletingTeam}
              title={deleteCopy.title}
              body={deleteCopy.body}
              actions={promptActions(deleteCopy, {
                delete: {
                  busy: deletingTeam,
                  disabled: deleteConfirmation !== team.name,
                  onSelect: handleDeleteTeam,
                },
              })}
            >
              <div className="space-y-2">
                <Label htmlFor="delete-confirm">
                  <span>
                    Type <span className="font-semibold">{team.name}</span> to
                    confirm
                  </span>
                </Label>
                <Input
                  id="delete-confirm"
                  data-prompt-autofocus
                  value={deleteConfirmation}
                  onChange={(e) => setDeleteConfirmation(e.target.value)}
                  placeholder={team.name}
                />
                {deleteError && (
                  <p className="text-sm text-destructive">{deleteError}</p>
                )}
              </div>
            </Prompt>
          </>
        )}
      </div>
    </SettingsSectionGuard>
  )
}
