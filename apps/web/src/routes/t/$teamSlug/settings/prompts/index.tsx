import { createFileRoute } from "@tanstack/react-router"
import { usePrompts } from "@/hooks/use-prompts"
import { PromptsListBand } from "@/components/team/prompts-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/prompts/`)({
  head: () => ({ meta: [{ title: pageTitle(`Prompts`, `Settings`) }] }),
  component: SettingsPromptsIndex,
})

// NEW-2: the team's prompts as one band; a row opens the prompt's page.
// Member-readable; "New prompt" is owner-only.
function SettingsPromptsIndex() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const { prompts, isReady } = usePrompts(team?.id)
  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {team && isReady && (
        <PromptsListBand team={team} isOwner={permissions.isOwner} prompts={prompts} />
      )}
    </SettingsSectionGuard>
  )
}
