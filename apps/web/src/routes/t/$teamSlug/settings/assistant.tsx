import { createFileRoute } from "@tanstack/react-router"
import { ASSISTANT_COPY } from "@exp/domain-contract/agent-settings"
import { AssistantSettingsBody } from "@/components/settings/personal-settings"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027: phones keep the team-scoped page; md+ forwards this URL to
// `/settings/assistant` (the same body, components/settings).
export const Route = createFileRoute(`/t/$teamSlug/settings/assistant`)({
  head: () => ({
    meta: [{ title: pageTitle(ASSISTANT_COPY.title, `Settings`) }],
  }),
  component: SettingsAssistant,
})

function SettingsAssistant() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      <AssistantSettingsBody team={team} isOwner={permissions.isOwner} />
    </SettingsSectionGuard>
  )
}
