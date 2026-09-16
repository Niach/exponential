import { createFileRoute } from "@tanstack/react-router"
import { TeamStorageSection } from "@/components/team/storage-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/storage`)({
  head: () => ({
    meta: [{ title: pageTitle(`Storage`, `Settings`) }],
  }),
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
