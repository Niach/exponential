import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useSkills } from "@/hooks/use-skills"
import { SkillSettingsPage, SKILLS_TITLE } from "@/components/team/skills-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle, usePageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/skills/$skillId`)({
  head: () => ({ meta: [{ title: pageTitle(SKILLS_TITLE, `Settings`) }] }),
  component: SettingsSkill,
})

// #EXP-1659: one skill's page, fed the LIVE synced row (the body comes from
// `skills.get`); a skill deleted out from under it lands on the list.
// Members read it; the owner-only controls are hidden inside.
function SettingsSkill() {
  const { teamSlug, skillId } = Route.useParams()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const { skills, isReady } = useSkills(team?.id)
  const skill = skills.find((row) => row.id === skillId) ?? null
  usePageTitle(skill?.name ? pageTitle(skill.name, `Settings`) : undefined)
  const navigate = useNavigate()

  useEffect(() => {
    if (!isReady || !team || skill) return
    void navigate({ to: `/t/$teamSlug/settings/skills`, params: { teamSlug }, replace: true })
  }, [isReady, navigate, skill, team, teamSlug])

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isMember}>
      {team && skill && (
        <SkillSettingsPage team={team} skill={skill} isOwner={permissions.isOwner} />
      )}
    </SettingsSectionGuard>
  )
}
