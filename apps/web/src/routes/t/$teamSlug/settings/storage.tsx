import { createFileRoute } from "@tanstack/react-router"
import { TeamStorageSection } from "@/components/team/storage-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/storage`)({
  component: SettingsStorage,
})

function SettingsStorage() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isOwner}>
      {team && <TeamStorageSection team={team} teamSlug={teamSlug} />}
    </SettingsSectionGuard>
  )
}
