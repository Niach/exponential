import { useMemo, useState, type ReactNode } from "react"
import { Link, createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { APPS_COPY } from "@exp/domain-contract/apps"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import {
  GlassGroup,
  TeamAvatar,
  UserAvatar,
  blockingLabel,
  conceptIcon,
  whileBlocking,
} from "@exp/ui"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { CreateTeamDialog } from "@/components/create-team-dialog"
import { ChangelogSheet } from "@/components/whats-new"
import { openFeedbackWidget } from "@/components/feedback-widget-provider"
import { useFeedbackWidgetAvailable } from "@/components/feedback-button"
import { useAgents } from "@/hooks/use-session-tree"
import { useSignOut } from "@/hooks/use-sign-out"
import { useTeamBoards, useTeamMemberships } from "@/hooks/use-team-data"
import { isAdminUser } from "@/lib/auth/app-user"
import { boardCollection, issueCollection, notificationCollection } from "@/lib/collections"
import { pageTitle } from "@/lib/page-title"
import { cn } from "@/lib/utils"
import {
  SETTINGS_NAV,
  type SettingsNavItem,
  type SettingsSectionPath,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

// Thread 58cb32df (#EXP-1373, chats.json `shell`, ×3 with iOS and Android):
// the phone's TEAM tab. The active team's avatar (84) + name + meta, a
// glass group of the viewer's teams (the active one checked, a team with an
// unread notification an amber dot; a tap SWITCHES = opens that team's Team
// tab — the only place a phone switches team), then the settings rows the
// sidebar's settings nav holds (same order, same destinations, the boards /
// agents / apps groups as one row each with their count), then the account
// row (the viewer's avatar) and the rest of the old avatar menu. md+ has the
// sidebar for all of it; this route still renders there (a plain column).

export const Route = createFileRoute(`/t/$teamSlug/team`)({
  head: () => ({ meta: [{ title: pageTitle(CHATS_COPY.tabs.team) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({ to: `/auth/login`, search: { redirect: location.href } })
    }
  },
  component: TeamTabPage,
})

const CheckIcon = conceptIcon(`ui-check`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
const AddIcon = conceptIcon(`ui-add`)
const BoardsIcon = conceptIcon(`settings-boards`)
const AgentsIcon = conceptIcon(`settings-agents`)
const AppsIcon = conceptIcon(`nav-apps`)
const AdminIcon = conceptIcon(`nav-admin`)
const ChangelogIcon = conceptIcon(`nav-changelog`)
const AboutIcon = conceptIcon(`settings-about`)
const ReportBugIcon = conceptIcon(`nav-report-bug`)
const SignOutIcon = conceptIcon(`nav-sign-out`)

const ROW = `flex min-h-[52px] w-full items-center gap-3 px-4 py-3 text-left text-[15px] text-foreground outline-none transition-colors duration-fast active:bg-glass-active/50 focus-visible:bg-glass-active/50`

/** `copy.team.meta`: `{members} members · {boards} boards`. */
export function teamMeta(members: number, boards: number): string {
  return CHATS_COPY.team.meta
    .replace(`{members}`, String(members))
    .replace(`{boards}`, String(boards))
}

/** The team ids holding an unread notification: an issue-less row names
 *  its team, an issue row resolves through its issue's board. */
function useUnreadTeamIds(): Set<string> {
  const { data: notifications } = useLiveQuery((query) => query.from({ n: notificationCollection }))
  const unread = (notifications ?? []).filter((n) => !n.readAt)
  const issueIds = [...new Set(unread.map((n) => n.issueId).filter((id): id is string => Boolean(id)))].sort()
  const { data: issues } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, issueIds))
            .select(({ issues }) => ({ id: issues.id, boardId: issues.boardId }))
        : undefined,
    [issueIds.join(`,`)]
  )
  const { data: boards } = useLiveQuery((query) => query.from({ boards: boardCollection }))
  return useMemo(() => {
    const boardTeam = new Map((boards ?? []).map((board) => [board.id, board.teamId]))
    const issueTeam = new Map(
      (issues ?? []).map((issue) => [issue.id, boardTeam.get(issue.boardId) ?? null])
    )
    const ids = new Set<string>()
    for (const n of unread) {
      const teamId = n.teamId ?? (n.issueId ? issueTeam.get(n.issueId) : null)
      if (teamId) ids.add(teamId)
    }
    return ids
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [unread.map((n) => n.id).join(`,`), issues, boards])
}

function RowLink({
  to,
  teamSlug,
  icon,
  label,
  count,
}: {
  to: SettingsSectionPath | `/t/$teamSlug/settings/agents` | `/t/$teamSlug/settings/boards` | `/t/$teamSlug/apps`
  teamSlug: string
  icon: ReactNode
  label: string
  count?: number
}) {
  return (
    <Link to={to} params={{ teamSlug }} className={ROW} data-testid="team-settings-row">
      <span className="flex size-6 shrink-0 items-center justify-center text-foreground/80">{icon}</span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {count !== undefined && count > 0 && (
        <span className="shrink-0 text-sm tabular-nums text-muted-foreground">{count}</span>
      )}
      <ChevronRightIcon className="size-4 shrink-0 text-foreground/50" />
    </Link>
  )
}

function RowButton({
  icon,
  label,
  onClick,
  destructive = false,
}: {
  icon: ReactNode
  label: string
  onClick: () => void
  destructive?: boolean
}) {
  return (
    <button
      type="button"
      className={cn(ROW, destructive && `text-destructive`)}
      onClick={onClick}
    >
      <span className="flex size-6 shrink-0 items-center justify-center">{icon}</span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
    </button>
  )
}

function TeamTabPage() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const { session, team, members, permissions, config } = useSettingsPage(teamSlug)
  const user = session?.user
  const boards = useTeamBoards(team?.id)
  const { agents } = useAgents(team?.id, user?.id)
  const { myTeams } = useTeamMemberships(user?.id)
  const unreadTeams = useUnreadTeamIds()
  const handleSignOut = useSignOut()
  const feedbackAvailable = useFeedbackWidgetAvailable()
  const [whatsNewOpen, setWhatsNewOpen] = useState(false)
  const [createTeamOpen, setCreateTeamOpen] = useState(false)

  const navContext = { isCloud: Boolean(config?.isCloud) }
  const visible = (item: SettingsNavItem) => item.visible(permissions, navContext)
  const group = (name: string) =>
    (SETTINGS_NAV.find((entry) => entry.group === name)?.items ?? []).filter(visible)
  const iconOf = (item: SettingsNavItem) => {
    const Icon = item.icon
    return <Icon className="size-5" />
  }
  const itemRow = (item: SettingsNavItem, count?: number) => (
    <RowLink
      key={item.to}
      to={item.to}
      teamSlug={teamSlug}
      icon={iconOf(item)}
      label={item.label}
      count={count}
    />
  )
  const personal = group(`Personal`)
  const account = personal.find((item) => item.to === `/t/$teamSlug/settings/account`)

  const switchTeam = (slug: string, name: string) => {
    if (slug === teamSlug) return
    void whileBlocking(blockingLabel(`switchTeam`, name), () =>
      navigate({ to: `/t/$teamSlug/team`, params: { teamSlug: slug } })
    )
  }

  const teamName = team?.name ?? teamSlug

  return (
    <div className="h-full overflow-y-auto" data-testid="team-tab">
      <div
        className={`mx-auto flex w-full max-w-xl flex-col gap-4 px-4 pt-[max(1.5rem,env(safe-area-inset-top))] ${TAB_BAR_CLEARANCE}`}
      >
        <div className="flex flex-col items-center gap-1.5 pb-2 text-center">
          <TeamAvatar name={teamName} size={84} />
          <h1 className="mt-2 text-2xl font-bold">{teamName}</h1>
          <p className="text-sm text-muted-foreground">{teamMeta(members.length, boards.length)}</p>
        </div>

        <GlassGroup aria-label={CHATS_COPY.team.switch} data-testid="team-switch">
          {myTeams.map((row) => (
            <button
              key={row.id}
              type="button"
              className={ROW}
              aria-current={row.slug === teamSlug ? `true` : undefined}
              onClick={() => switchTeam(row.slug, row.name)}
            >
              <TeamAvatar name={row.name} size={24} />
              <span className="min-w-0 flex-1 truncate">{row.name}</span>
              {row.slug === teamSlug ? (
                <CheckIcon className="size-5 shrink-0" />
              ) : unreadTeams.has(row.id) ? (
                <span aria-label="Unread" className="size-2.5 shrink-0 rounded-full bg-amber-400" />
              ) : null}
            </button>
          ))}
          <RowButton
            icon={<AddIcon className="size-5 text-muted-foreground" />}
            label="New team"
            onClick={() => setCreateTeamOpen(true)}
          />
        </GlassGroup>

        <GlassGroup data-testid="team-settings">
          {group(`Team`).map((item) =>
            itemRow(item, item.to === `/t/$teamSlug/settings/members` ? members.length : undefined)
          )}
          {permissions.isOwner && (
            <RowLink
              to="/t/$teamSlug/settings/boards"
              teamSlug={teamSlug}
              icon={<BoardsIcon className="size-5" />}
              label="Boards"
              count={boards.length}
            />
          )}
          <RowLink
            to="/t/$teamSlug/settings/agents"
            teamSlug={teamSlug}
            icon={<AgentsIcon className="size-5" />}
            label="Agents"
            count={agents.length}
          />
          {group(`Agents`).map((item) => itemRow(item))}
          {/* apps.json: the phones' way into the Apps LIST (it left the
              Chats header, which carries search + the inbox now). */}
          <RowLink
            to="/t/$teamSlug/apps"
            teamSlug={teamSlug}
            icon={<AppsIcon className="size-5" />}
            label={APPS_COPY.title}
          />
          {group(`Features`).map((item) => itemRow(item))}
        </GlassGroup>

        <GlassGroup data-testid="team-account">
          {account && (
            <Link to={account.to} params={{ teamSlug }} className={ROW}>
              <UserAvatar user={user} size={32} />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate">{user?.name || account.label}</span>
                {user?.email && (
                  <span className="truncate text-xs text-muted-foreground">{user.email}</span>
                )}
              </span>
              <ChevronRightIcon className="size-4 shrink-0 text-foreground/50" />
            </Link>
          )}
          {personal.filter((item) => item !== account).map((item) => itemRow(item))}
          {group(`Advanced`).map((item) => itemRow(item))}
        </GlassGroup>

        <GlassGroup>
          {isAdminUser(user) && (
            <RowButton icon={<AdminIcon className="size-5" />} label="Admin" onClick={() => void navigate({ to: `/admin` })} />
          )}
          <RowButton icon={<ChangelogIcon className="size-5" />} label="What's new" onClick={() => setWhatsNewOpen(true)} />
          <RowButton icon={<AboutIcon className="size-5" />} label="About" onClick={() => void navigate({ to: `/about` })} />
          {feedbackAvailable && (
            <RowButton icon={<ReportBugIcon className="size-5" />} label="Report bug" onClick={() => openFeedbackWidget()} />
          )}
          <RowButton icon={<SignOutIcon className="size-5" />} label="Sign out" onClick={handleSignOut} destructive />
        </GlassGroup>
      </div>
      <ChangelogSheet open={whatsNewOpen} onOpenChange={setWhatsNewOpen} />
      <CreateTeamDialog open={createTeamOpen} onOpenChange={setCreateTeamOpen} />
    </div>
  )
}
