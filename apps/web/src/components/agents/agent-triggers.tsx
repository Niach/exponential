import { useCallback, useEffect, useState } from "react"
import { agentTriggerKindValues, type AgentTrigger, type TriggerWhen } from "@exp/db-schema/domain"
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
  triggerDetail,
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
// #EXP-1850: every kind is listed (record/webhook rows have no editor, a
// `run` trigger reads what it runs on a second line) and every write names
// all kinds (`triggerKinds`).

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
  // Every stored object with an id is KEPT, whatever its kind (an app
  // agent's record/webhook triggers, #EXP-1848): every write is the whole
  // array, so dropping one here would delete it. Rows list the kinds this
  // client reads (`isListedKind`).
  return value.filter(
    (trigger): trigger is AgentTrigger =>
      typeof trigger === `object` && trigger !== null && typeof (trigger as { id?: unknown }).id === `string`
  )
}

/** #EXP-1850: every kind is listed; a kind newer than this client is kept,
 *  never listed. */
function isListedKind(trigger: { kind?: unknown }): boolean {
  return (agentTriggerKindValues as readonly unknown[]).includes(trigger.kind)
}

/** #EXP-1850: the editor edits schedules and events; record and webhook
 *  rows have none (⋯ = Delete). */
function isEditableKind(trigger: { kind?: unknown }): boolean {
  return trigger.kind === `schedule` || trigger.kind === `event`
}

/** The kinds every write of this client names (agents.update replaces only
 *  those, agent-settings.json #EXP-1850). */
export const WRITTEN_TRIGGER_KINDS = [...agentTriggerKindValues]

/** An edited trigger: the new when-part over the stored one, every other key
 *  (prompt, run) kept; a schedule's Check in tag goes with its old cron. */
function withTriggerMeta(when: TriggerWhen, existing: AgentTrigger | null): AgentTrigger {
  if (!existing) return { ...when, id: crypto.randomUUID(), enabled: true } as AgentTrigger
  const { cron: _cron, tz: _tz, name: _name, preset: _preset, ...rest } = existing as AgentTrigger & {
    cron?: string
    tz?: string
    name?: string
    preset?: string
  }
  return { ...rest, ...when, id: existing.id, enabled: existing.enabled } as AgentTrigger
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
      ? triggers.map((existing) => (existing.id === editing.id ? withTriggerMeta(when, existing) : existing))
      : [...triggers, withTriggerMeta(when, null)]
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
        {listedTriggers(triggers).filter(isListedKind).map((trigger) => (
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
            onEdit={isEditableKind(trigger) ? () => setDialog({ open: true, editing: trigger }) : null}
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
        await trpc.agents.update.mutate({ id: agent.id, triggers: next, triggerKinds: WRITTEN_TRIGGER_KINDS }, SKIP_ERROR_TOAST)
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
  /** Null = no editor for this kind (#EXP-1850: record, webhook). */
  onEdit: (() => void) | null
  onDelete: () => void
}) {
  const sentence = triggerSummary({ ...trigger, enabled: true })
  const detail = triggerDetail(trigger)
  const editable = !readOnly && onEdit !== null
  return (
    <ListRow
      interactive={editable}
      onClick={editable ? onEdit! : undefined}
      aria-label={editable ? sentence : undefined}
      // In a GlassGroup the hairline between rows is this row's border: a
      // rounded row bends it at both ends (#EXP-1846 pictures).
      className="rounded-none px-4"
      data-testid={`agent-trigger-${trigger.id}`}
    >
      <span className={`min-w-0 flex-1 ${trigger.enabled ? `` : `text-muted-foreground`}`}>
        <span className="block truncate text-sm">{sentence}</span>
        {detail && <span className="block truncate text-xs text-muted-foreground">{detail}</span>}
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
              ...(onEdit ? [{ kind: `item` as const, id: `edit`, label: `Edit`, icon: UiEditIcon, onSelect: onEdit }] : []),
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
