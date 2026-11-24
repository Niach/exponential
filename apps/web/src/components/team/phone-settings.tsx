import { useState, type ReactNode } from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import settingsNav from "@exp/domain-contract/fixtures/settings-nav.json"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import {
  SETTINGS_NAV_ROWS,
  SETTINGS_NAV_TEST_IDS,
  settingsNavRow,
  settingsNavVisible,
  type SettingsNavRow,
} from "@exp/domain-contract/settings-nav"
import { GlassGroup, UserAvatar, conceptIcon } from "@exp/ui"
import gettingStartedSpec from "@exp/domain-contract/fixtures/getting-started.json"
import { AssistantMark } from "@/components/team/assistant-mark"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { MobilePageHeader } from "@/components/team/mobile-page-header"
import { ChangelogSheet } from "@/components/whats-new"
import { openFeedbackWidget } from "@/components/feedback-widget-provider"
import { useFeedbackWidgetAvailable } from "@/components/feedback-button"
import {
  GettingStartedDialog,
  useGettingStartedEntryVisible,
} from "@/components/getting-started/getting-started-button"
import { GETTING_STARTED_TITLE } from "@/components/getting-started/getting-started-copy"
import { useAgents } from "@/hooks/use-session-tree"
import { useSession } from "@/hooks/use-session"
import { useSignOut } from "@/hooks/use-sign-out"
import { useTeamBoards, useTeamBySlug } from "@/hooks/use-team-data"
import { isAdminUser } from "@/lib/auth/app-user"
import { personalSettingsPath, type PersonalSettingsPage } from "@/lib/app-paths"
import { cn } from "@/lib/utils"
import {
  flattenSettingsNav,
  SETTINGS_NAV,
  type SettingsNavItem,
  type SettingsSectionPath,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

// #EXP-2109 (settings-nav.json `nav.phone`, ×3 with iOS and Android): the
// phones' two settings lists. THE SETTINGS TAB (`PhoneSettingsRoot`): the
// viewer's card, the personal pages, Devices, then the rest of the old Team
// page. A TEAM'S SETTINGS (`PhoneTeamSettings`, from the team page's `…`): its
// rows in `nav.phone.team` order, each opening its own page.

const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
const AdminIcon = conceptIcon(`nav-admin`)
const ChangelogIcon = conceptIcon(`nav-changelog`)
const AboutIcon = conceptIcon(`settings-about`)
const DevicesIcon = conceptIcon(`nav-devices`)
const GettingStartedIcon = conceptIcon(`nav-getting-started`)
const ReportBugIcon = conceptIcon(`nav-report-bug`)
const SignOutIcon = conceptIcon(`nav-sign-out`)

/** One phone settings row (a glass group's). */
export const PHONE_ROW_CLASS = `flex min-h-[52px] w-full items-center gap-3 px-4 py-3 text-left text-[15px] text-foreground outline-none transition-colors duration-fast active:bg-glass-active/50 focus-visible:bg-glass-active/50`

type TeamRowPath =
  | SettingsSectionPath
  | `/t/$teamSlug/settings/agents`
  | `/t/$teamSlug/settings/boards`

/** Where a settings-nav.json row opens: its own settings page. */
export function teamRowPath(id: string): TeamRowPath {
  return `/t/$teamSlug/settings/${id}` as TeamRowPath
}

/** S-3 (settings-nav.json `nav`): a group's web pages that are no `rows`
 *  row (Team: Storage, Import; Boards: Archived boards; the Advanced group
 *  is gone), drawn after that group's rows on the Team page. */
export function teamExtraPages(group: string): SettingsNavItem[] {
  return flattenSettingsNav(SETTINGS_NAV.find((entry) => entry.id === group)?.items ?? []).filter(
    (item) => settingsNavRow(item.id) === undefined
  )
}

/** One row of a team's phone settings page: a fixture row, or a web page after it. */
export type TeamPageRow = { kind: `row`; row: SettingsNavRow } | { kind: `page`; item: SettingsNavItem }

/** A team's phone settings rows (settings-nav.json `nav.phone.team`, the same
 *  list every phone draws), then the web-only rows, then the web
 *  pages that are no `rows` row, each filtered for the viewer. */
export function teamPageRows(
  visibleRow: (row: SettingsNavRow) => boolean,
  visibleItem: (item: SettingsNavItem) => boolean
): TeamPageRow[] {
  const phone = settingsNav.nav.phone.team.map((id) => SETTINGS_NAV_ROWS.find((row) => row.id === id)).filter(
    (row): row is SettingsNavRow => Boolean(row)
  )
  const groups: string[] = []
  for (const row of SETTINGS_NAV_ROWS) if (!groups.includes(row.group)) groups.push(row.group)
  return [
    ...[...phone, ...SETTINGS_NAV_ROWS.filter((row) => !row.phone && row.id !== `devices`)]
      .filter(visibleRow)
      .map((row): TeamPageRow => ({ kind: `row`, row })),
    // Personal pages are the Settings tab's.
    ...groups.filter((group) => group !== `personal`).flatMap((group) =>
      teamExtraPages(group)
        .filter(visibleItem)
        .map((item): TeamPageRow => ({ kind: `page`, item }))
    ),
  ]
}

export function PhoneRowLink({
  to,
  teamSlug,
  icon,
  label,
  count,
}: {
  to: TeamRowPath
  teamSlug: string
  icon: ReactNode
  label: string
  count?: number
}) {
  return (
    <Link to={to} params={{ teamSlug }} className={PHONE_ROW_CLASS} data-testid={SETTINGS_NAV_TEST_IDS.row}>
      <span className="flex size-6 shrink-0 items-center justify-center text-foreground/80">{icon}</span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {count !== undefined && count > 0 && (
        <span className="shrink-0 text-sm tabular-nums text-muted-foreground">{count}</span>
      )}
      <ChevronRightIcon className="size-4 shrink-0 text-foreground/50" />
    </Link>
  )
}

export function PhoneRowButton({
  icon,
  label,
  onClick,
  destructive = false,
  testId,
}: {
  icon: ReactNode
  label: string
  onClick: () => void
  destructive?: boolean
  testId?: string
}) {
  return (
    <button
      type="button"
      className={cn(PHONE_ROW_CLASS, destructive && `text-destructive`)}
      onClick={onClick}
      data-testid={testId}
    >
      <span className="flex size-6 shrink-0 items-center justify-center">{icon}</span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
    </button>
  )
}

/** A team's settings list on a phone. */
export function PhoneTeamSettings({ teamSlug }: { teamSlug: string }) {
  const { session, team, members, permissions, config } = useSettingsPage(teamSlug)
  const boards = useTeamBoards(team?.id)
  const { agents } = useAgents(team?.id, session?.user?.id)
  const navContext = { isCloud: Boolean(config?.isCloud) }
  const viewer = { isOwner: permissions.isOwner, isCloud: navContext.isCloud }
  const rows = teamPageRows(
    (row) => settingsNavVisible(row, viewer),
    (item) => item.visible(permissions, navContext)
  )
  const countOf = (row: SettingsNavRow): number | undefined =>
    row.count === `members`
      ? members.length
      : row.count === `boards`
        ? boards.length
        : row.count === `agents`
          ? agents.length
          : undefined
  return (
    <GlassGroup data-testid="team-settings">
      {rows.map((entry) => {
        if (entry.kind === `page`) {
          const Icon = entry.item.icon
          return (
            <PhoneRowLink key={entry.item.to} to={entry.item.to} teamSlug={teamSlug} icon={<Icon className="size-5" />} label={entry.item.label} />
          )
        }
        const Icon = conceptIcon(entry.row.icon as Parameters<typeof conceptIcon>[0])
        return (
          <PhoneRowLink
            key={entry.row.id}
            to={teamRowPath(entry.row.id)}
            teamSlug={teamSlug}
            icon={<Icon className="size-5" />}
            label={entry.row.label}
            count={countOf(entry.row)}
          />
        )
      })}
    </GlassGroup>
  )
}

const PERSONAL_ITEMS = SETTINGS_NAV.find((group) => group.id === `personal`)?.items ?? []

function TeamlessRow({ href, icon, label, testId }: { href: string; icon: ReactNode; label: string; testId?: string }) {
  const navigate = useNavigate()
  return (
    <a
      href={href}
      className={PHONE_ROW_CLASS}
      data-testid={testId ?? SETTINGS_NAV_TEST_IDS.row}
      onClick={(event) => {
        if (event.metaKey || event.ctrlKey || event.shiftKey) return
        event.preventDefault()
        void navigate({ href })
      }}
    >
      <span className="flex size-6 shrink-0 items-center justify-center text-foreground/80">{icon}</span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
      <ChevronRightIcon className="size-4 shrink-0 text-foreground/50" />
    </a>
  )
}

/** The phone's Settings tab. `teamSlug` = the default team (Getting started). */
export function PhoneSettingsRoot({ teamSlug }: { teamSlug: string }) {
  const navigate = useNavigate()
  const { data: session } = useSession()
  const user = session?.user
  const team = useTeamBySlug(teamSlug)
  const handleSignOut = useSignOut()
  const feedbackAvailable = useFeedbackWidgetAvailable()
  const [whatsNewOpen, setWhatsNewOpen] = useState(false)
  // #EXP-1632: the phone's Getting started entry, gone once every step is done.
  const gettingStartedVisible = useGettingStartedEntryVisible(team)
  const [gettingStartedOpen, setGettingStartedOpen] = useState(false)
  const personal = settingsNav.nav.phone.settings.flatMap((id) => {
    const item = PERSONAL_ITEMS.find((entry) => entry.id === id)
    return item && id !== `account` ? [item] : []
  })
  return (
    <div className="h-full overflow-y-auto" data-testid="phone-settings">
      <div
        className={`mx-auto flex w-full max-w-xl flex-col gap-4 px-4 pt-[max(1rem,env(safe-area-inset-top))] ${PAGE_END_CLEARANCE}`}
      >
        <MobilePageHeader title={CHATS_COPY.headers.settings} />
        <GlassGroup data-testid="settings-account">
          <a
            href={personalSettingsPath(`account`)}
            className={PHONE_ROW_CLASS}
            onClick={(event) => {
              event.preventDefault()
              void navigate({ href: personalSettingsPath(`account`) })
            }}
          >
            <UserAvatar user={user} size={40} />
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate">{user?.name || user?.email}</span>
              {user?.email && <span className="truncate text-xs text-muted-foreground">{user.email}</span>}
            </span>
            <ChevronRightIcon className="size-4 shrink-0 text-foreground/50" />
          </a>
        </GlassGroup>
        <GlassGroup>
          {personal.map((item) => {
            const Icon = item.icon
            return (
              <TeamlessRow
                key={item.id}
                href={personalSettingsPath(item.id as PersonalSettingsPage)}
                // #EXP-1987: the Assistant's row wears its brand mark.
                icon={item.id === `assistant` ? <AssistantMark className="size-5" /> : <Icon className="size-5" />}
                label={item.label}
              />
            )
          })}
        </GlassGroup>
        <GlassGroup>
          <TeamlessRow href="/devices" icon={<DevicesIcon className="size-5" />} label={CHATS_COPY.headers.devices} />
        </GlassGroup>
        <GlassGroup>
          {gettingStartedVisible && (
            <PhoneRowButton
              icon={<GettingStartedIcon className="size-5" />}
              label={GETTING_STARTED_TITLE}
              onClick={() => setGettingStartedOpen(true)}
              testId={gettingStartedSpec.testIds.entry}
            />
          )}
          {isAdminUser(user) && (
            <PhoneRowButton icon={<AdminIcon className="size-5" />} label="Admin" onClick={() => void navigate({ to: `/admin` })} />
          )}
          <PhoneRowButton icon={<ChangelogIcon className="size-5" />} label="What's new" onClick={() => setWhatsNewOpen(true)} />
          <PhoneRowButton icon={<AboutIcon className="size-5" />} label="About" onClick={() => void navigate({ to: `/about` })} />
          {feedbackAvailable && (
            <PhoneRowButton icon={<ReportBugIcon className="size-5" />} label="Report bug" onClick={() => openFeedbackWidget()} />
          )}
          <PhoneRowButton icon={<SignOutIcon className="size-5" />} label="Sign out" onClick={handleSignOut} destructive />
        </GlassGroup>
      </div>
      <ChangelogSheet open={whatsNewOpen} onOpenChange={setWhatsNewOpen} />
      {team && (
        <GettingStartedDialog team={team} teamSlug={teamSlug} open={gettingStartedOpen} onOpenChange={setGettingStartedOpen} />
      )}
    </div>
  )
}
