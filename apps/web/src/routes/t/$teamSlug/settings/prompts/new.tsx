import { createFileRoute } from "@tanstack/react-router"
import { PromptSettingsPage } from "@/components/team/prompts-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/prompts/new`)({
  head: () => ({ meta: [{ title: pageTitle(`New prompt`, `Settings`) }] }),
  component: SettingsNewPrompt,
})

// NEW-2: "New prompt" = the prompt page in CREATE mode — every field local
// until "Create prompt", then the page becomes the new prompt's. Owner-only.
function SettingsNewPrompt() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isOwner}>
      {team && <PromptSettingsPage team={team} prompt={null} isOwner />}
    </SettingsSectionGuard>
  )
}
