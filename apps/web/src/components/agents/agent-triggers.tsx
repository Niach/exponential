import { useCallback, useEffect, useState } from "react"
import type { AgentTrigger, TriggerWhen } from "@exp/db-schema/domain"
import {
  Button,
  conceptIcon,
  GlassGroup,
  GlassSectionHeader,
  ListRow,
  Menu,
  Picker,
  Pill,
  Prompt,
  Switch,
} from "@exp/ui"
import {
  AGENT_SETTINGS_COPY,
  ASSISTANT_SCHEDULES,
  checkInOf,
  listedTriggers,
  triggerSummary,
  withCheckIn,
  type AssistantScheduleId,
} from "@exp/domain-contract/agent-settings"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { deleteTriggerPrompt, promptActions } from "@/lib/prompts"
import { TriggerDialog } from "@/components/trigger-dialog"

// #EXP-1846 (agent-settings.json `page` › `triggers`, `checkIn`): THE
// trigger list of an agent page, ONE piece the Assistant, every team agent
// and an app's Events tab (#EXP-1829) mount: the Triggers header with its
// `+ Add` pill, then one group: the Check in row (THE single Picker over the
// presets, `checkInOf` / `withCheckIn`) and a row per `listedTriggers`
// (sentence · Switch · ⋯ Edit/Delete; the row opens the editor). Every write
// is the WHOLE array, each trigger kept as stored (its `preset` included).

const COPY = AGENT_SETTINGS_COPY
const UiAddIcon = conceptIcon(`ui-add`)
const UiMoreIcon = conceptIcon(`ui-more`)
const UiEditIcon = conceptIcon(`ui-edit`)
const UiDeleteIcon = conceptIcon(`ui-delete`)
const DELETE_TRIGGER = deleteTriggerPrompt()
const CHECK_IN_ITEMS = ASSISTANT_SCHEDULES.map((preset) => ({ value: preset.id, label: preset.label }))

/** Tolerant read: the synced `triggers` jsonb may hold a shape a newer
 *  client wrote; only the two known kinds render. */
export function readTriggers(value: unknown): AgentTrigger[] {
  if (!Array.isArray(value)) return []
  return value.filter(
    (trigger): trigger is AgentTrigger =>
      typeof trigger === `object` &&
      trigger !== null &&
      typeof (trigger as { id?: unknown }).id === `string` &&
      ((trigger as { kind?: unknown }).kind === `schedule` ||
        (trigger as { kind?: unknown }).kind === `event`)
  )
}

/** A trigger as stored: the when-part plus its id and switch. */
function withTriggerMeta(when: TriggerWhen, id: string, enabled: boolean): AgentTrigger {
  return { ...when, id, enabled }
}

const localZone = () => {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || null
  } catch {
    return null
  }
}

export interface AgentTriggersProps {
  /** The agent's triggers as the page holds them (optimistic). */
  triggers: readonly AgentTrigger[]
  /** Members read: a disabled Check in, no `+ Add`, no ⋯. */
  readOnly: boolean
  /** Persist the whole array; a rejection keeps an open editor/confirm open
   *  (the caller toasts and rolls back). */
  onWrite: (next: AgentTrigger[]) => Promise<void>
}

/** The Triggers section (controlled): header + `+ Add`, Check in, the rows,
 *  the trigger editor and the delete confirm. */
export function AgentTriggers({ triggers, readOnly, onWrite }: AgentTriggersProps) {
  const [dialog, setDialog] = useState<{ open: boolean; editing: AgentTrigger | null }>({
    open: false,
    editing: null,
  })
  const [deleting, setDeleting] = useState<AgentTrigger | null>(null)
  const quietly = (promise: Promise<void>) => void promise.catch(() => {})

  const pickCheckIn = (id: AssistantScheduleId | null) => {
    if (!id || id === checkInOf(triggers)) return
    // The preset trigger keeps its id (and firing history); a new one mints.
    const next = withCheckIn(triggers, id, localZone()).map(
      (trigger) => ({ ...trigger, id: trigger.id ?? crypto.randomUUID() }) as AgentTrigger
    )
    quietly(onWrite(next))
  }

  const saveTrigger = async (when: TriggerWhen) => {
    const editing = dialog.editing
    const next = editing
      ? triggers.map((existing) =>
          existing.id === editing.id ? withTriggerMeta(when, existing.id, existing.enabled) : existing
        )
      : [...triggers, withTriggerMeta(when, crypto.randomUUID(), true)]
    await onWrite(next)
  }

  return (
    <div data-testid="agent-triggers">
      <GlassSectionHeader
        label={COPY.triggers}
        trailing={
          readOnly ? undefined : (
            <Pill
              mode="action"
              onClick={() => setDialog({ open: true, editing: null })}
              data-testid="agent-add-trigger"
            >
              <UiAddIcon className="size-3" />
              {COPY.addTrigger}
            </Pill>
          )
        }
      />
      <GlassGroup>
        <Picker
          mode="single"
          triggerVariant="row"
          width="md"
          mobileTitle={COPY.checkIn}
          triggerLabel={COPY.checkIn}
          value={checkInOf(triggers)}
          items={CHECK_IN_ITEMS}
          onChange={pickCheckIn}
          disabled={readOnly}
          data-testid="agent-check-in"
        />
        {listedTriggers(triggers).map((trigger) => (
          <AgentTriggerRow
            key={trigger.id}
            trigger={trigger}
            readOnly={readOnly}
            onToggle={(enabled) =>
              quietly(
                onWrite(
                  triggers.map((existing) => (existing.id === trigger.id ? { ...existing, enabled } : existing))
                )
              )
            }
            onEdit={() => setDialog({ open: true, editing: trigger })}
            onDelete={() => setDeleting(trigger)}
          />
        ))}
      </GlassGroup>

      {!readOnly && (
        <TriggerDialog
          open={dialog.open}
          onOpenChange={(open) =>
            setDialog((current) => (open ? { ...current, open } : { open: false, editing: null }))
          }
          trigger={dialog.editing}
          onSave={saveTrigger}
        />
      )}

      <Prompt
        open={deleting !== null}
        onOpenChange={(next) => {
          if (!next) setDeleting(null)
        }}
        title={DELETE_TRIGGER.title}
        body={DELETE_TRIGGER.body}
        actions={promptActions(DELETE_TRIGGER, {
          delete: {
            onSelect: async () => {
              const target = deleting
              if (!target) return
              try {
                await onWrite(triggers.filter((existing) => existing.id !== target.id))
                setDeleting(null)
              } catch {
                // Toast already shown; the confirm stays open for a retry.
              }
            },
          },
        })}
      />
    </div>
  )
}

/** `AgentTriggers` over a synced `agents` row alone (an app's Events tab):
 *  holds the optimistic array and writes `agents.update {triggers}`. */
export function AgentTriggersFor({
  agent,
  readOnly,
}: {
  agent: { id: string; triggers: unknown }
  readOnly: boolean
}) {
  const [triggers, setTriggers] = useState<AgentTrigger[]>(() => readTriggers(agent.triggers))
  useEffect(() => setTriggers(readTriggers(agent.triggers)), [agent.id, agent.triggers])
  const write = useCallback(
    async (next: AgentTrigger[]): Promise<void> => {
      const previous = triggers
      setTriggers(next)
      try {
        await trpc.agents.update.mutate({ id: agent.id, triggers: next }, SKIP_ERROR_TOAST)
      } catch (err) {
        setTriggers(previous)
        failedAction(`save`, `the agent`, err, { retry: () => void write(next).catch(() => {}) })
        throw err
      }
    },
    [agent.id, triggers]
  )
  return <AgentTriggers triggers={triggers} readOnly={readOnly} onWrite={write} />
}

/** One Triggers row: the trigger's sentence, its switch, and (editors) a ⋯
 *  with Edit / Delete; the row itself opens the editor. */
function AgentTriggerRow({
  trigger,
  readOnly,
  onToggle,
  onEdit,
  onDelete,
}: {
  trigger: AgentTrigger
  readOnly: boolean
  onToggle: (enabled: boolean) => void
  onEdit: () => void
  onDelete: () => void
}) {
  const sentence = triggerSummary({ ...trigger, enabled: true })
  return (
    <ListRow
      interactive={!readOnly}
      onClick={readOnly ? undefined : onEdit}
      aria-label={readOnly ? undefined : sentence}
      className="px-4"
      data-testid={`agent-trigger-${trigger.id}`}
    >
      <span className={`min-w-0 flex-1 truncate text-sm ${trigger.enabled ? `` : `text-muted-foreground`}`}>
        {sentence}
      </span>
      <div
        className="flex shrink-0 items-center gap-1"
        // The switch and the menu are their own targets.
        onClick={(e) => e.stopPropagation()}
      >
        <Switch
          size="compact"
          checked={trigger.enabled}
          disabled={readOnly}
          onCheckedChange={onToggle}
          aria-label={`Enabled: ${sentence}`}
        />
        {!readOnly && (
          <Menu
            align="end"
            aria-label={`${sentence} actions`}
            title={sentence}
            entries={[
              { kind: `item`, id: `edit`, label: `Edit`, icon: UiEditIcon, onSelect: onEdit },
              {
                kind: `item`,
                id: `delete`,
                label: `Delete`,
                icon: UiDeleteIcon,
                destructive: true,
                onSelect: onDelete,
              },
            ]}
            trigger={
              <Button variant="ghost" size="icon-sm" aria-label={`Trigger menu for ${sentence}`}>
                <UiMoreIcon />
              </Button>
            }
          />
        )}
      </div>
    </ListRow>
  )
}
