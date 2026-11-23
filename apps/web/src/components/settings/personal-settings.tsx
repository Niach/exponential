// #EXP-2027 (settings-nav.json `desktopNavs.personal`): the Personal pages'
// bodies, ONE copy for the team-less md+ routes (`/settings/<page>`) and the
// phones' team-scoped ones (`/t/$teamSlug/settings/<page>`). Each route keeps
// its own loader + search; the body is the same.
import { useMemo } from "react"
import { ASSISTANT_COPY } from "@exp/domain-contract/agent-settings"
import { EmptyState, SkeletonRows, conceptIcon } from "@exp/ui"
import type { Team } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { AccountOverview } from "@/components/account/account-overview"
import { DeleteAccountSection } from "@/components/account/delete-account-section"
import { PasskeysSection } from "@/components/account/passkeys-section"
import { PrivacySection } from "@/components/account/privacy-section"
import {
  SignInMethodsSection,
  type LinkReturn,
} from "@/components/account/sign-in-methods-section"
import { EmailNotificationsCard } from "@/components/account/email-notifications-card"
import { NotificationSourcesCard } from "@/components/account/notification-sources-card"
import { ApiKeysSection } from "@/components/account/api-keys-section"
import { AgentSettingsPage } from "@/components/team/agents-section"
import { useAgentRow } from "@/hooks/use-agent-row"
import { useSession } from "@/hooks/use-session"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"
import { useTeamById } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"

// ── Account ────────────────────────────────────────────────────────────────

export type AccountLinkSearch = {
  linked?: string
  link_error?: string
  error?: string
}

const nonEmpty = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined
// The router JSON-parses search values, so a `?link_error=1` marker arrives
// as the number 1: any present, non-empty value counts.
const present = (value: unknown): string | undefined =>
  value === undefined || value === null || value === `` || value === false
    ? undefined
    : String(value)

/** EXP-1126: where a provider link round-trip lands. `?linked=<id>` on
 *  success; on failure Better Auth appends `&error=<reason>` to our
 *  `?link_error=1` errorCallbackURL. */
export function parseAccountSearch(search: Record<string, unknown>): AccountLinkSearch {
  return {
    linked: nonEmpty(search.linked),
    link_error: present(search.link_error),
    error: nonEmpty(search.error),
  }
}

export async function loadAccountSettings() {
  const [timezone, signInMethods] = await Promise.all([
    trpc.users.timezone.query(),
    trpc.users.signInMethods.query(),
  ])
  return { timezone: timezone.timezone, signInMethods }
}

type AccountSettingsData = Awaited<ReturnType<typeof loadAccountSettings>>

// Personal section (EXP-238): identity, sign-in methods (EXP-1126: the
// changeable primary email, Google/Apple/OIDC linking, passkeys), timezone,
// diagnostics (#LOGS-2, this device only) and account deletion.
export function AccountSettingsBody({
  data,
  search,
  accountPath,
  onLinkReturnConsumed,
  onChanged,
}: {
  data: AccountSettingsData
  search: AccountLinkSearch
  /** The page's own path: the provider link round trip comes back to it. */
  accountPath: string
  onLinkReturnConsumed: () => void
  /** EXP-1209: both bands gate the last way in on the ONE `waysIn`; a
   *  change in either reloads it. */
  onChanged: () => void
}) {
  const { timezone, signInMethods } = data
  const linkReturn: LinkReturn = {
    linked: search.linked,
    linkError: search.link_error ? (search.error ?? `unable_to_link_account`) : undefined,
  }
  return (
    <div className="space-y-6">
      <AccountOverview initialTimezone={timezone} />
      <SignInMethodsSection
        initialMethods={signInMethods}
        accountPath={accountPath}
        linkReturn={linkReturn}
        onLinkReturnConsumed={onLinkReturnConsumed}
        onChanged={onChanged}
      />
      <PasskeysSection
        passkeyEnabled={signInMethods.passkeyEnabled}
        waysIn={signInMethods.waysIn}
        onChanged={onChanged}
      />
      <PrivacySection />
      <DeleteAccountSection />
    </div>
  )
}

// ── Assistant ──────────────────────────────────────────────────────────────

const DevicesIcon = conceptIcon(`nav-devices`)

// #EXP-1987 (settings-nav.json `assistant`, agent-settings.json `assistant`): the
// viewer's OWN Assistant's page. It resolves the Assistant through
// `agents.ensureAssistant` WITHOUT a team (created on first use on the
// member's newest own desktop) and renders the agent page's Assistant variant
// over the LIVE synced row; with no device yet it reads `copy.noDevice`.
// `team` = the phones' current team; absent (the team-less page) = the team
// the Assistant lives in (#EXP-1714: the member's personal team).
export function AssistantSettingsBody({ team, isOwner }: { team?: Team | null; isOwner?: boolean }) {
  const { data: auth } = useSession()
  const me = auth?.user?.id ?? null
  const assistantId = useEnsureAssistant(null)
  const { agent: row } = useAgentRow(assistantId ?? null)
  const agent = row && row.role === `assistant` && row.ownerId === me ? row : null
  const home = useTeamById(team === undefined ? agent?.teamId : null)
  const homePermissions = useTeamPermissions(team === undefined ? home : null)
  // The Assistant lives in its member's personal team, which the `teams`
  // shape never carries (no row to wait for): the team-less page then stands
  // on the agent's own team id. An Assistant's page reads nothing else of
  // its team (its devices are the member's own; skills and MCP servers are
  // the per-team sections).
  const pageTeam = useMemo(
    () => (team === undefined ? (home ?? (agent ? ({ id: agent.teamId, slug: ``, name: `` } as Team) : null)) : team),
    [team, home, agent]
  )
  const pageIsOwner = team === undefined ? homePermissions.isOwner : Boolean(isOwner)

  if (assistantId === null) {
    return <EmptyState icon={DevicesIcon} title={ASSISTANT_COPY.noDevice} size="list" />
  }
  if (pageTeam && agent) {
    return <AgentSettingsPage key={agent.id} team={pageTeam} agent={agent} isOwner={pageIsOwner} />
  }
  return <SkeletonRows className="py-1" />
}

// ── Notifications ──────────────────────────────────────────────────────────

export async function loadNotificationSettings() {
  const emailPrefs = await trpc.notifications.emailPrefs.query()
  return { emailPrefs }
}

// Personal section (EXP-238): the email digest preferences. #EXP-1815: the
// session-source switches sit above the email card. Per user, no team.
export function NotificationSettingsBody({
  data,
}: {
  data: Awaited<ReturnType<typeof loadNotificationSettings>>
}) {
  return (
    <div className="space-y-6">
      <NotificationSourcesCard initial={data.emailPrefs.sourcePrefs} />
      <EmailNotificationsCard emailPrefs={data.emailPrefs} />
    </div>
  )
}

// ── Security ───────────────────────────────────────────────────────────────

export async function loadSecuritySettings() {
  const { keys } = await trpc.users.listPersonalApiKeys.query()
  return { keys }
}

// Personal section (EXP-862): what acts AS you, the self-service expu_ API
// keys. What signs you in is Settings › Account › Sign-in methods (EXP-1126).
export function SecuritySettingsBody({
  data,
}: {
  data: Awaited<ReturnType<typeof loadSecuritySettings>>
}) {
  return (
    <div className="space-y-6">
      <ApiKeysSection initialKeys={data.keys} />
    </div>
  )
}
