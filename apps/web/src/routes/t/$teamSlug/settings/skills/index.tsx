import { createFileRoute } from "@tanstack/react-router"
import { useSkills } from "@/hooks/use-skills"
import { SkillsListBand, SKILLS_TITLE } from "@/components/team/skills-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/skills/`)({
  head: () => ({ meta: [{ title: pageTitle(SKILLS_TITLE, `Settings`) }] }),
  component: SettingsSkillsIndex,
})

// #EXP-1659: the team's skills as one band; a row opens the skill's page.
// Member-readable; "New skill" and the row menus are owner-only.
function SettingsSkillsIndex() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const { skills, isReady } = useSkills(team?.id)
  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {team && (
        <SkillsListBand team={team} isOwner={permissions.isOwner} skills={skills} isReady={isReady} />
      )}
    </SettingsSectionGuard>
  )
}
