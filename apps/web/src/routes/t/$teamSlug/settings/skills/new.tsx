import { createFileRoute } from "@tanstack/react-router"
import { NEW_SKILL_LABEL, SkillSettingsPage } from "@/components/team/skills-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/skills/new`)({
  head: () => ({ meta: [{ title: pageTitle(NEW_SKILL_LABEL, `Settings`) }] }),
  component: SettingsNewSkill,
})

// #EXP-1659: "New skill" = the skill page in CREATE mode. Owner-only.
function SettingsNewSkill() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isOwner}>
      {team && <SkillSettingsPage team={team} skill={null} isOwner />}
    </SettingsSectionGuard>
  )
}
