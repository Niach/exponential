import { useEffect, useMemo, useState, type ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import {
  AgentAvatar,
  Button,
  GlassSectionHeader,
  ListRow,
  Menu,
  SETTINGS_LIST_CLASS,
  SkeletonRows,
  TeamAvatar,
  conceptIcon,
  type MenuEntry,
} from "@exp/ui"
import type { IconConcept } from "@exp/icons"
import { isAssistant } from "@exp/domain-contract/agent-settings"
import { TEMPORARY_PICK } from "@exp/domain-contract/composer-agent-row"
import { SHARE_TARGETS, shareTargets } from "@exp/domain-contract/share-targets"
import type { Team } from "@/db/schema"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { agentCollection } from "@/lib/collections"
import { newDraftNavigation } from "@/lib/issue-draft-page"
import { readLastVisited } from "@/lib/last-visited"
import { shareImages, type PendingShare } from "@/lib/share/pending-share"

// EXP-1635 (share-targets.json): step 1 of a share into the app, the page the
// manifest's `share_target` lands on. A preview line, New issue + New chat,
// then the agents in the agent row's order. A pick only navigates: step 2 is
// today's New issue page or THE composer on the Agent page, each reading the
// pending share (`lib/share/pending-share.ts`). Nothing is sent from here.

const { copy, icons, ids, testIds } = SHARE_TARGETS

const NewIssueIcon = conceptIcon(icons.newIssue as IconConcept)
const NewChatIcon = conceptIcon(icons.newChat as IconConcept)
const ChevronIcon = conceptIcon(`ui-chevron-right`)
const TeamSwitcherIcon = conceptIcon(`nav-team-switcher`)

const MAX_PREVIEW_THUMBS = 4

interface ShareAgent {
  id: string
  name: string
  icon: string | null
  color: string | null
  sortOrder: number
  role: string | null
  ownerId: string
}

function shareRowTestId(id: string): string {
  return testIds.row.replace(`{id}`, id)
}

/** The team a share starts on: this device's last one, else the default. */
export function shareTeam(
  teams: readonly Team[],
  lastSlug: string | undefined,
  defaultSlug: string
): Team | undefined {
  return (
    teams.find((team) => team.slug === lastSlug) ??
    teams.find((team) => team.slug === defaultSlug) ??
    teams[0]
  )
}

export function ShareTargetsPage({
  share,
  defaultTeamSlug,
  me,
}: {
  share: PendingShare
  defaultTeamSlug: string
  me: string
}) {
  const { myTeams } = useTeamMemberships(me)
  const [pickedTeamId, setPickedTeamId] = useState<string | null>(null)
  const team =
    myTeams.find((row) => row.id === pickedTeamId) ??
    shareTeam(myTeams, readLastVisited()?.teamSlug, defaultTeamSlug)

  return (
    <div className="h-full overflow-y-auto">
      <div
        data-testid={testIds.page}
        className="mx-auto flex w-full max-w-2xl flex-col gap-4 px-4 pb-6 pt-[max(1.5rem,env(safe-area-inset-top))]"
      >
        <h1 className="text-lg font-semibold">{copy.title}</h1>
        {myTeams.length > 1 && team && (
          <ShareTeamSelector teams={myTeams} team={team} onPick={setPickedTeamId} />
        )}
        <SharePreview share={share} />
        {team ? <ShareRows key={team.id} team={team} me={me} /> : <SkeletonRows />}
      </div>
    </div>
  )
}

function ShareTeamSelector({
  teams,
  team,
  onPick,
}: {
  teams: readonly Team[]
  team: Team
  onPick: (teamId: string) => void
}) {
  const entries: MenuEntry[] = teams.map((row) => ({
    id: row.id,
    kind: `item`,
    label: row.name,
    icon: <TeamAvatar iconUrl={row.iconUrl} size={20} />,
    checked: row.id === team.id,
    onSelect: () => onPick(row.id),
  }))
  return (
    <Menu
      title="Team"
      entries={entries}
      trigger={
        <Button variant="ghost" size="sm" className="-ml-2 self-start" data-testid="share-targets-team">
          <TeamAvatar iconUrl={team.iconUrl} size={20} />
          {team.name}
          <TeamSwitcherIcon className="size-4 text-muted-foreground" />
        </Button>
      }
    />
  )
}

/** The shared images' thumbnails + their count; no line without images. */
function SharePreview({ share }: { share: PendingShare }) {
  const images = useMemo(() => shareImages(share), [share])
  const [urls, setUrls] = useState<string[]>([])
  useEffect(() => {
    const created = images.slice(0, MAX_PREVIEW_THUMBS).map((file) => URL.createObjectURL(file))
    setUrls(created)
    return () => {
      for (const url of created) URL.revokeObjectURL(url)
    }
  }, [images])
  if (images.length === 0) return null
  return (
    <div data-testid={testIds.preview} className="flex items-center gap-2">
      {urls.map((url) => (
        <img key={url} src={url} alt="" className="size-10 rounded-md object-cover" />
      ))}
      <span className="text-sm text-muted-foreground">
        {images.length === 1
          ? copy.imagesOne
          : copy.imagesMany.replace(`{n}`, String(images.length))}
      </span>
    </div>
  )
}

function ShareRows({ team, me }: { team: Team; me: string }) {
  const navigate = useNavigate()
  // The viewer's Assistant exists before the list leads with it.
  useEnsureAssistant(team.id)
  const { data: agentRows } = useLiveQuery(
    (query) =>
      query.from({ agents: agentCollection }).where(({ agents }) => inArray(agents.teamId, [team.id])),
    [team.id]
  )
  const { agents } = shareTargets({
    agents: (agentRows ?? []) as ShareAgent[],
    me,
  })
  const openChat = (pick: string) =>
    void navigate({
      to: `/t/$teamSlug/agent`,
      params: { teamSlug: team.slug },
      search: { share: pick },
    })

  return (
    <>
      <div className={SETTINGS_LIST_CLASS}>
        <ShareRow
          id={ids.newIssue}
          lead={<NewIssueIcon className="size-5 text-muted-foreground" />}
          title={copy.newIssue}
          onPick={() => void navigate(newDraftNavigation({ teamSlug: team.slug, share: true }))}
        />
        <ShareRow
          id={ids.newChat}
          lead={<NewChatIcon className="size-5 text-muted-foreground" />}
          title={copy.newChat}
          subtitle={copy.newChatSubtitle}
          onPick={() => openChat(TEMPORARY_PICK)}
        />
      </div>
      {agents.length > 0 && (
        <div>
          <GlassSectionHeader label={copy.agents} />
          <div className={SETTINGS_LIST_CLASS}>
            {agents.map((agent) => (
              <ShareRow
                key={agent.id}
                id={agent.id}
                lead={
                  <AgentAvatar
                    agent={{
                      icon: agent.icon ?? ``,
                      color: agent.color ?? ``,
                      assistant: isAssistant(agent),
                    }}
                    className="size-4"
                  />
                }
                title={agent.name}
                onPick={() => openChat(agent.id)}
              />
            ))}
          </div>
        </div>
      )}
    </>
  )
}

function ShareRow({
  id,
  lead,
  title,
  subtitle,
  onPick,
}: {
  id: string
  lead: ReactNode
  title: string
  subtitle?: string
  onPick: () => void
}) {
  return (
    <ListRow interactive onClick={onPick} data-testid={shareRowTestId(id)}>
      <span
        data-part="leading"
        className="flex size-8 shrink-0 items-center justify-center rounded-full bg-muted"
      >
        {lead}
      </span>
      <span data-part="body" className="flex min-w-0 flex-1 flex-col">
        <span data-part="title" className="truncate text-sm">
          {title}
        </span>
        {subtitle && (
          <span data-part="subtitle" className="truncate text-xs text-muted-foreground">
            {subtitle}
          </span>
        )}
      </span>
      <ChevronIcon data-part="chevron" className="size-4 shrink-0 text-muted-foreground" />
    </ListRow>
  )
}
