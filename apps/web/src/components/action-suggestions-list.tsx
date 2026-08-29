import { useEffect, useMemo, useState } from "react"
import type { BoardIcon } from "@exp/db-schema/domain"
import type { Team } from "@/db/schema"
import {
  ACTION_SUGGESTIONS,
  type ActionSuggestion,
} from "@/lib/action-suggestions"
import {
  BUILTIN_CREATE_ACTION_ID,
  BUILTIN_CREATE_ACTION_NAME,
} from "@/lib/builtin-actions"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import { useSteerConfig } from "@/components/agent-session"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import type { ActionRepoOption } from "@/components/action-editor-dialog"
import { CreateActionDialog } from "@/components/launch-dialog/create-action-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import { BOARD_ICON_COMPONENTS } from "@/lib/board-icons"

// EXP-686: the suggestion seeds left the Actions surface and became the
// "Suggested actions" tab of Getting started — the Actions and Automations
// pages only keep the lightbulb that opens it. Everything a seed needs to
// actually launch the builtin creator run (the steer devices, the team's
// repositories, the create dialog) lives here so the tab is self-contained.

// EXP-530: the suggestion glyph is a cross-client concept, never a raw glyph.
const ActionSuggestionIcon = conceptIcon(`action-suggestion`)
const ActionAutomationIcon = conceptIcon(`action-automation`)

// One suggestion seed as a row (EXP-530; rows since EXP-618 — native-app
// parity). "Use" opens the create-action dialog with the description/icon
// prefilled — the same owner+steer gate as the "New action" button, since it
// launches the same builtin creator run.
function SuggestionRow({
  suggestion,
  canUse,
  disabled,
  onUse,
}: {
  suggestion: ActionSuggestion
  canUse: boolean
  disabled: boolean
  onUse: () => void
}) {
  const RowIcon =
    BOARD_ICON_COMPONENTS[suggestion.icon as BoardIcon] ?? ActionSuggestionIcon
  return (
    <GlassRow>
      <RowIcon className="size-4 shrink-0 text-foreground/70" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate font-medium">{suggestion.title}</span>
          {/* EXP-583: a seed either just authors an action, or authors it and
              sets up the automation that runs it. */}
          <Badge variant="outline" className="shrink-0 gap-1 text-[0.625rem]">
            {suggestion.automation && (
              <ActionAutomationIcon className="h-3 w-3" />
            )}
            {suggestion.automation ? `Automation` : `Action`}
          </Badge>
        </div>
        <div className="line-clamp-3 text-xs text-muted-foreground">
          {suggestion.description}
        </div>
      </div>
      {canUse && (
        <Button
          variant="outline"
          size="sm"
          className="shrink-0"
          disabled={disabled}
          onClick={onUse}
        >
          Use
        </Button>
      )}
    </GlassRow>
  )
}

/** The seed list plus the create-action dialog it prefills — the Getting
 * started sheet's second tab (desktop web) and the mobile Actions page's
 * third tab. */
export function ActionSuggestionsPanel({ team }: { team: Team }) {
  const { data: session } = useSession()
  const { isMember, isOwner } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()

  const currentUserId = session?.user?.id
  const teamId = team.id
  // Steer tickets require team membership and a configured relay; the server
  // enforces both at mint time, this only decides whether the interactive
  // affordances render.
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)

  const remote = useRemoteStart({
    enabled: steerEnabled,
    currentUserId,
    teamId,
  })
  const runBusy = remote.starting || remote.sentTo !== null

  // The creator run writes into a repository when one is picked, so the
  // dialog needs the team's registry.
  const [repos, setRepos] = useState<ActionRepoOption[]>([])
  useEffect(() => {
    if (!isMember) return
    let active = true
    trpc.repositories.list
      .query({ teamId })
      .then(
        (rows) =>
          active &&
          setRepos(rows.map((r) => ({ id: r.id, fullName: r.fullName })))
      )
      .catch(() => {})
    return () => {
      active = false
    }
  }, [teamId, isMember])

  const [createOpen, setCreateOpen] = useState(false)
  const [prefill, setPrefill] = useState<ActionSuggestion | null>(null)
  const suggestions = useMemo(() => ACTION_SUGGESTIONS, [])

  if (!isMember) return null

  return (
    <>
      <GlassSectionHeader label="Suggestions" count={suggestions.length} />
      <div className="flex flex-col gap-2">
        {suggestions.map((suggestion) => (
          <SuggestionRow
            key={suggestion.id}
            suggestion={suggestion}
            canUse={steerEnabled && isOwner}
            disabled={runBusy}
            onUse={() => {
              setPrefill(suggestion)
              setCreateOpen(true)
            }}
          />
        ))}
      </div>

      <CreateActionDialog
        open={createOpen}
        onOpenChange={(next) => {
          if (!next) {
            setCreateOpen(false)
            // A later plain "New action" open must start blank again.
            setPrefill(null)
          }
        }}
        devices={remote.devices ?? []}
        starting={remote.starting}
        teamId={teamId}
        repos={repos}
        initialDescription={prefill?.description}
        initialIcon={prefill?.icon}
        automationPrefill={prefill?.automation}
        onCreate={(device, options, inputs) => {
          remote
            .runAction(
              device,
              {
                id: BUILTIN_CREATE_ACTION_ID,
                name: BUILTIN_CREATE_ACTION_NAME,
                teamId,
              },
              options,
              inputs
            )
            .then(() => setCreateOpen(false))
            .catch(() => {})
        }}
      />
    </>
  )
}
