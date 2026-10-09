import { useMemo } from "react"
import type { BoardIcon } from "@exp/db-schema/domain"
import type { Team } from "@/db/schema"
import {
  ACTION_SUGGESTIONS,
  type ActionSuggestion,
} from "@/lib/action-suggestions"
import { BUILTIN_CREATE_ACTION_ID } from "@/lib/builtin-actions"
import { formatTriggerBlock } from "@/lib/action-triggers"
import {
  conceptIcon,
  GlassSectionHeader,
  ListRow,
  SETTINGS_LIST_CLASS,
  BOARD_ICON_COMPONENTS,
} from "@exp/ui"
import { triggerDevices } from "@/components/trigger-fields"
import { TriggerGlyph } from "@/components/action-triggers-section"
import { useSteerConfig } from "@/components/agent-session"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { defaultDeviceId } from "@/lib/steer-devices"

// EXP-686: the suggestion seeds left the Actions surface and became the
// "Suggested actions" tab of Getting started — the Actions page only keeps
// the lightbulb that opens it. EXP-825: a row is a navigation to the Agent
// page composer with the Create action builtin picked, the description as
// the draft text and the icon as its input; a seed with a trigger appends
// the machine-readable block the creator agent passes to
// `exponential_actions_update` (SLOP-2) — that block needs a runner device,
// so the devices are still read here for it.

// EXP-530: the suggestion glyph is a cross-client concept, never a raw glyph.
const ActionSuggestionIcon = conceptIcon(`action-suggestion`)

// One suggestion seed as a row (EXP-530; rows since EXP-618 — native-app
// parity). EXP-694: the trailing "Use" button is gone on every client — the
// WHOLE row is the affordance, opening the composer with the description/icon
// prefilled. Same owner+steer gate as the "New action" button, since it
// launches the same builtin creator run; without it the row is inert (no
// visual button, nothing to press).
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
  const clickable = canUse && !disabled
  return (
    <ListRow
      interactive={clickable}
      onClick={clickable ? onUse : undefined}
      className={canUse && disabled ? `opacity-60` : undefined}
    >
      <RowIcon className="size-4 shrink-0 text-foreground/70" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate font-medium">{suggestion.title}</span>
          {/* SLOP-2: a seed that also sets up a trigger wears its glyph, like
              an action row does. */}
          {suggestion.trigger && (
            <TriggerGlyph
              kind={suggestion.trigger.kind}
              className="size-3 shrink-0 text-muted-foreground"
            />
          )}
        </div>
        <div className="line-clamp-3 text-xs text-muted-foreground">
          {suggestion.description}
        </div>
      </div>
    </ListRow>
  )
}

/** The seed list — the Getting started sheet's second tab (desktop web) and
 * the mobile Actions page's third tab. */
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
  const openComposer = useOpenComposer()
  // The trigger's runner — trigger-capable machines, online or not (a
  // schedule catches up on reconnect): the caller's default one, else the
  // first. Without one the block is simply left off, as the old dialog did.
  const triggerDeviceId = useMemo(() => {
    const candidates = triggerDevices(remote.devices ?? [])
    return defaultDeviceId(candidates) ?? candidates[0]?.deviceId ?? null
  }, [remote.devices])
  const suggestions = useMemo(() => ACTION_SUGGESTIONS, [])

  if (!isMember) return null

  const use = (suggestion: ActionSuggestion) => {
    const block =
      suggestion.trigger && triggerDeviceId
        ? formatTriggerBlock({
            trigger: suggestion.trigger,
            deviceId: triggerDeviceId,
          })
        : ``
    openComposer({
      actionId: BUILTIN_CREATE_ACTION_ID,
      text: `${suggestion.description}${block}`,
      icon: suggestion.icon,
    })
  }

  return (
    <>
      <GlassSectionHeader label="Suggestions" />
      {/* ×4: ONE "Suggestions" band over flat hairline-divided rows (no
          per-row fill, no chevron). */}
      <div className={SETTINGS_LIST_CLASS}>
        {suggestions.map((suggestion) => (
          <SuggestionRow
            key={suggestion.id}
            suggestion={suggestion}
            canUse={steerEnabled && isOwner}
            disabled={false}
            onUse={() => use(suggestion)}
          />
        ))}
      </div>
    </>
  )
}
