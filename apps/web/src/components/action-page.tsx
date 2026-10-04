import { useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { useNavigate } from "@tanstack/react-router"
import { Ellipsis, Trash2 } from "lucide-react"
import {
  conceptIcon,
  getActionIcon,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  GlassSectionHeader,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  useIsMobile,
} from "@exp/ui"
import type { Team } from "@/db/schema"
import { actionCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { useSteerConfig } from "@/components/agent-session"
import {
  ActionPromptForm,
  type ActionRepoOption,
  type TeamAction,
} from "@/components/action-prompt-form"
import { ActionTriggersSection } from "@/components/action-triggers-section"
import { ActionRunsSection } from "@/components/action-runs-section"
import { DeleteActionDialog } from "@/components/team-actions-panel"
import { PinToggleButton } from "@/components/pin-toggle-button"
import {
  primeTabEnter,
  useFaceSwipe,
} from "@/components/mobile-face-tabs"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamPermissions } from "@/hooks/use-team-permissions"

// ONE action as a page (SLOP-2): its prompt, its triggers and its runs. A
// desktop viewport reads them as three SECTIONS of one scroller; a phone gets
// the same three as TABS (`?tab=`), like the Work screen's faces. The row is
// read from the synced shape, so the page settles on its own as Electric
// catches up.

const UiBackIcon = conceptIcon(`ui-back`)
const ActionRunIcon = conceptIcon(`action-run`)

export const ACTION_PAGE_TABS = [`prompt`, `triggers`, `runs`] as const
export type ActionPageTab = (typeof ACTION_PAGE_TABS)[number]

export function parseActionPageTab(value: unknown): ActionPageTab | undefined {
  return ACTION_PAGE_TABS.find((tab) => tab === value)
}

export function ActionPage({
  team,
  teamSlug,
  actionId,
  tab,
  onTabChange,
}: {
  team: Team
  teamSlug: string
  actionId: string
  /** Phone only — the controlled tab, i.e. the route's `?tab=`. */
  tab: ActionPageTab
  onTabChange: (tab: ActionPageTab) => void
}) {
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  // EXP-1190: the phone's tabs page with a swipe, like the Work faces.
  const swipe = useFaceSwipe(ACTION_PAGE_TABS, tab, onTabChange)
  const { data: session } = useSession()
  const { isMember, isOwner } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)
  const openComposer = useOpenComposer()

  const { data: actionRows } = useLiveQuery(
    (query) =>
      query.from({ a: actionCollection }).where(({ a }) => eq(a.id, actionId)),
    [actionId]
  )
  const action = useMemo<TeamAction | null>(() => {
    const row = actionRows?.[0]
    return row ? { ...row, builtin: false as const } : null
  }, [actionRows])

  // The trigger rows name their device and the trigger form picks one.
  const remote = useRemoteStart({
    enabled: steerEnabled,
    currentUserId: session?.user?.id,
    teamId: team.id,
  })

  // Repo names for the form's repository select.
  const [repos, setRepos] = useState<ActionRepoOption[]>([])
  useEffect(() => {
    if (!isMember) return
    let active = true
    trpc.repositories.list
      .query({ teamId: team.id })
      .then(
        (rows) =>
          active &&
          setRepos(rows.map((r) => ({ id: r.id, fullName: r.fullName })))
      )
      .catch(() => {})
    return () => {
      active = false
    }
  }, [team.id, isMember])

  const [deleteOpen, setDeleteOpen] = useState(false)

  const backToList = () =>
    void navigate({ to: `/t/$teamSlug/actions`, params: { teamSlug } })

  if (!isMember) return null
  if (actionRows === undefined) {
    return <div className="p-6 text-sm text-muted-foreground">Loading…</div>
  }
  if (!action) {
    return (
      <div className="flex flex-col items-start gap-3 p-6 text-sm text-muted-foreground">
        This action no longer exists.
        <Button variant="outline" size="sm" onClick={backToList}>
          Back to actions
        </Button>
      </div>
    )
  }

  const ActionIcon = getActionIcon(action)
  const prompt = (
    <ActionPromptForm repos={repos} action={action} readOnly={!isOwner} />
  )
  const triggersSection = (header: boolean) => (
    <ActionTriggersSection
      action={action}
      devices={remote.devices ?? []}
      isOwner={isOwner}
      steerEnabled={steerEnabled}
      showHeader={header}
    />
  )
  const runsSection = (header: boolean) => (
    <ActionRunsSection
      actionId={action.id}
      teamId={team.id}
      showHeader={header}
    />
  )

  return (
    <>
      <header className="flex items-center gap-2 pb-4">
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={backToList}
          aria-label="Back to actions"
        >
          <UiBackIcon />
        </Button>
        <ActionIcon className="size-4 shrink-0 text-foreground/70" />
        <h1 className="min-w-0 flex-1 truncate text-base font-medium">
          {action.name}
        </h1>
        <PinToggleButton teamId={action.teamId} kind="action" targetId={action.id} />
        {isOwner && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Action menu for ${action.name}`}
              >
                <Ellipsis />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem
                variant="destructive"
                onClick={() => setDeleteOpen(true)}
              >
                <Trash2 className="h-4 w-4" />
                Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        )}
        {steerEnabled && (
          <Button
            variant="glass"
            size="icon"
            // EXP-825: the composer with this action as the subject chip.
            onClick={() => openComposer({ actionId: action.id })}
            aria-label="Run"
            title="Run"
          >
            <ActionRunIcon />
          </Button>
        )}
      </header>

      {isMobile ? (
        <div className="flex-1" {...swipe}>
          <Tabs
            value={tab}
            onValueChange={(value) => {
              primeTabEnter(ACTION_PAGE_TABS, tab, value as ActionPageTab)
              onTabChange(value as ActionPageTab)
            }}
          >
            <TabsList className="w-full">
              <TabsTrigger value="prompt" className="flex-1">
                Prompt
              </TabsTrigger>
              <TabsTrigger value="triggers" className="flex-1">
                Triggers
              </TabsTrigger>
              <TabsTrigger value="runs" className="flex-1">
                Runs
              </TabsTrigger>
            </TabsList>
            <TabsContent value="prompt" data-face-body="">
              {prompt}
            </TabsContent>
            <TabsContent value="triggers" data-face-body="">
              {triggersSection(false)}
            </TabsContent>
            <TabsContent value="runs" data-face-body="">
              {runsSection(false)}
            </TabsContent>
          </Tabs>
        </div>
      ) : (
        <div className="space-y-6">
          <div>
            <GlassSectionHeader label="Prompt" />
            <div className="pt-2">{prompt}</div>
          </div>
          {triggersSection(true)}
          {runsSection(true)}
        </div>
      )}

      <DeleteActionDialog
        action={deleteOpen ? action : null}
        onClose={() => setDeleteOpen(false)}
        onDeleted={backToList}
      />
    </>
  )
}
