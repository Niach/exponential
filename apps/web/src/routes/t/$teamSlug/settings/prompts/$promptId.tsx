import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { usePrompts } from "@/hooks/use-prompts"
import { PromptSettingsPage } from "@/components/team/prompts-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle, usePageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/prompts/$promptId`)({
  head: () => ({ meta: [{ title: pageTitle(`Prompts`, `Settings`) }] }),
  component: SettingsPrompt,
})

// NEW-2: one prompt's settings PAGE, fed the LIVE synced row (the body comes
// from `prompts.get`), so every write reflects via sync and a prompt deleted
// out from under the page lands on the list. Members read it; the owner-only
// controls are hidden inside.
function SettingsPrompt() {
  const { teamSlug, promptId } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const { prompts, isReady } = usePrompts(team?.id)
  const prompt = prompts.find((row) => row.id === promptId) ?? null
  usePageTitle(prompt?.name ? pageTitle(prompt.name, `Settings`) : undefined)
  const navigate = useNavigate()

  useEffect(() => {
    if (!isReady || !team || prompt) return
    void navigate({
      to: `/t/$teamSlug/settings/prompts`,
      params: { teamSlug },
      replace: true,
    })
  }, [isReady, navigate, prompt, team, teamSlug])

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {team && prompt && (
        <PromptSettingsPage team={team} prompt={prompt} isOwner={permissions.isOwner} />
      )}
    </SettingsSectionGuard>
  )
}
