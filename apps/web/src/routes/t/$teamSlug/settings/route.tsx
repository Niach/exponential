import {
  createFileRoute,
  Outlet,
  redirect,
  useNavigate,
  useRouterState,
} from "@tanstack/react-router"
import { Separator, useIsMobile } from "@exp/ui"
import { SETTINGS_NAV_TEST_IDS } from "@exp/domain-contract/settings-nav"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import { NEW_PROMPT_LABEL } from "@/components/team/prompts-section"
import { useTeamBoards, useTeamBySlug } from "@/hooks/use-team-data"
import { useAgents } from "@/hooks/use-session-tree"
import { useApps } from "@/hooks/use-apps"
import { usePrompts } from "@/hooks/use-prompts"
import { useSession } from "@/hooks/use-session"
import {
  NEW_AGENT_LABEL,
  NEW_APP_LABEL,
  SETTINGS_LIST_SECTIONS,
  settingsRouteOf,
  settingsSectionLabel,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings`)({
  head: () => ({ meta: [{ title: pageTitle(`Settings`) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: SettingsLayout,
})

function SettingsLayout() {
  const isMobile = useIsMobile()
  return isMobile ? <PhoneSettingsLayout /> : <WideSettingsLayout />
}

// EXP-456: on md+ the settings nav lives in the app sidebar (the main nav
// slides out, the settings nav slides in — see TeamSidebar/SettingsSidebar),
// so the page is the title over the section.
function WideSettingsLayout() {
  const { teamSlug } = Route.useParams()
  const { team } = useSettingsPage(teamSlug)
  return (
    <div className={`mx-auto w-full max-w-4xl space-y-6 p-4 sm:p-6 ${TAB_BAR_CLEARANCE}`}>
      <div>
        <h1 className="text-2xl font-bold">Settings</h1>
        <p className="text-sm text-muted-foreground">
          Manage {team?.name ?? `your team`} and your account
        </p>
      </div>
      <Separator />
      <div className="min-w-0 flex-1">
        <Outlet />
      </div>
    </div>
  )
}

// #EXP-1381 (settings-nav.json): on a phone every settings page is its OWN
// page, opened from a Team-page row: the phone detail header (back + the
// section's label) over only that section — never the h1 + the strip of
// every category. A list's sub-page (one board, agent, app, prompt) goes
// back to its list and wears the row's name.
function PhoneSettingsLayout() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const { section, sub } = settingsRouteOf(pathname)
  const isListSub =
    section !== null &&
    sub !== null &&
    sub !== `archived` &&
    (SETTINGS_LIST_SECTIONS as readonly string[]).includes(section)
  const label = section ? settingsSectionLabel(section, sub) : null

  const back = () => {
    if (isListSub && section) {
      void navigate({ to: `/t/$teamSlug/settings/${section}` as `/t/$teamSlug/settings/boards`, params: { teamSlug } })
      return
    }
    void navigate({ to: `/t/$teamSlug/team`, params: { teamSlug } })
  }

  return (
    <div className={`flex w-full flex-col ${TAB_BAR_CLEARANCE}`} data-testid={SETTINGS_NAV_TEST_IDS.page}>
      <MobileDetailHeader
        title={
          isListSub && section && sub ? (
            <SubPageTitle teamSlug={teamSlug} section={section} sub={sub} fallback={label ?? ``} />
          ) : (
            label ?? `Settings`
          )
        }
        onBack={back}
      />
      <div className="min-w-0 px-4 pb-4 pt-2">
        <Outlet />
      </div>
    </div>
  )
}

/** The row's own name on a list's sub-page; each list mounts only its own data. */
function SubPageTitle({
  teamSlug,
  section,
  sub,
  fallback,
}: {
  teamSlug: string
  section: string
  sub: string
  fallback: string
}) {
  if (sub === `new`) {
    if (section === `agents`) return <>{NEW_AGENT_LABEL}</>
    if (section === `apps`) return <>{NEW_APP_LABEL}</>
    if (section === `prompts`) return <>{NEW_PROMPT_LABEL}</>
  }
  switch (section) {
    case `boards`:
      return <BoardName teamSlug={teamSlug} id={sub} fallback={fallback} />
    case `agents`:
      return <AgentName teamSlug={teamSlug} id={sub} fallback={fallback} />
    case `apps`:
      return <AppName teamSlug={teamSlug} id={sub} fallback={fallback} />
    case `prompts`:
      return <PromptName teamSlug={teamSlug} id={sub} fallback={fallback} />
    default:
      return <>{fallback}</>
  }
}

type NameProps = { teamSlug: string; id: string; fallback: string }

function BoardName({ teamSlug, id, fallback }: NameProps) {
  const team = useTeamBySlug(teamSlug)
  const boards = useTeamBoards(team?.id)
  return <>{boards.find((row) => row.id === id)?.name ?? fallback}</>
}

function AgentName({ teamSlug, id, fallback }: NameProps) {
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const { agents } = useAgents(team?.id, authSession?.user?.id)
  return <>{agents.find((row) => row.agentId === id)?.name ?? fallback}</>
}

function AppName({ teamSlug, id, fallback }: NameProps) {
  const team = useTeamBySlug(teamSlug)
  const { apps } = useApps(team?.id)
  return <>{apps.find((row) => row.id === id)?.name ?? fallback}</>
}

function PromptName({ teamSlug, id, fallback }: NameProps) {
  const team = useTeamBySlug(teamSlug)
  const { prompts } = usePrompts(team?.id)
  return <>{prompts.find((row) => row.id === id)?.name || fallback}</>
}
