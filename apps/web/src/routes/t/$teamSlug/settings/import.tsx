import { createFileRoute } from "@tanstack/react-router"
import { TeamImportSection } from "@/components/team/import-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/import`)({
  head: () => ({
    meta: [{ title: pageTitle(`Import`, `Settings`) }],
  }),
  component: SettingsImport,
})

function SettingsImport() {
  const { teamSlug } = Route.useParams()
  const { session, team, permissions, resolved } = useSettingsPage(teamSlug)

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isOwner}>
      {team && session?.user && (
        <TeamImportSection team={team} teamSlug={teamSlug} userId={session.user.id} />
      )}
    </SettingsSectionGuard>
  )
}
