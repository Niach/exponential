import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { SESSIONS_SEND_TEXT_MAX } from "@exp/db-schema/domain"
import {
  AttachmentFileTile,
  AttachmentThumb,
  Composer,
  ComposerFrame,
  ComposerOptionsRow,
  ComposerSubmit,
  ContextRing,
  Label,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  ModelPicker,
  Prompt,
  Spinner,
  Switch,
  toast,
  type ValueChoice,
} from "@exp/ui"
import type { User } from "@/db/schema"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import { ContextWindowBlock } from "@/components/context-window-block"
import {
  ComposerInlineTools,
  type ComposerInlineToolsHandle,
} from "@/components/composer-inline-tools"
import { ComposerOptionsMenu } from "@/components/launch-dialog/composer-plus-menu"
import { useSessionComposerOptions } from "@/components/session-composer-options"
import { SessionDevicePin } from "@/components/session-device-pin"
import {
  SlashCommandMenu,
  useSlashCommandMenu,
} from "@/components/steer-command-menu"
import {
  modelChoices,
  modelLabel,
} from "@/components/launch-dialog/launch-options-pane"
import { useSessionRow } from "@/hooks/use-session-tree"
import { useSteerCommand } from "@/hooks/use-steer-command"
import { planModeToggle, sessionModel } from "@/lib/agent-feed"
import { contextPercent, severity } from "@/lib/agent-usage"
import {
  defaultModelFor,
  defaultThreadModelFor,
} from "@/lib/coding-launch-prefs"
import { acquireSteerSession } from "@/lib/steer-session-store"
import {
  mergeAgentCommands,
  parseSteerCommand,
  steerAgentId,
  steerCommandConfirmCopy,
  steerCommandsFor,
  type SteerCommand,
} from "@/lib/steer-commands"
import { uploadTeamSessionImageFile } from "@/lib/storage/issue-image-upload"
import { deviceCollection, messageCollection } from "@/lib/collections"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import type { SessionRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"

// EXP-1293 Threads v2: THE reply box ×4 = the home's composer box (the
// Composer primitive), never a new box. It posts ONE message to a session
// (`sessions.send({sessionId})`): a thread root (an ended thread continues
// server-side in a new child), an agent's channel, or an opened session.
// Stop = `sessions.stop` while the session works.
//
// EXP-1334 (owner: ONE composer, the old session layout): the start, the
// thread and the session view render the SAME frame (`ComposerFrame`): the
// card = the field (the `/` menu over it), the "+" = THE options menu
// (composer-menu.json rows minus the subject rows: Effort ›, Ultracode,
// MCP servers ›, Computer use — changed mid-session through
// `sessions.configure`), the inline edit tools (image · files · # · emoji)
// and Send ⇄ Stop; UNDER it the options line: the Device pin (EXP-1252, a
// pick MOVES the session; `session-device-pin.tsx`) · the model pin (THE
// ModelPicker; a pick sends `/model <alias>` through `sessions.send`, shown
// at once until the relay's `config_state` confirms) · the Plan switch …
// the context ring (its usage popover on click). The live reads (model in
// force, usage, commands) are the session's relay state
// (`acquireSteerSession`), the synced row the fallback.

/** How long a model pick reads as picked without the relay confirming. */
const MODEL_PICK_CONFIRM_MS = 20_000

/** The model a session runs on before its relay says: the row's own pick,
 *  else (a dispatcher) the device's thread model, else the defaults. */
function fallbackModel(
  row: SessionRow | null,
  deviceAgents: Record<string, { model?: string; threadModel?: string }> | undefined
): string {
  const agent = row?.cli ?? `claude`
  if (row?.model) return row.model
  const defaults = deviceAgents?.[agent]
  if (row?.dispatch) return defaults?.threadModel || defaultThreadModelFor(agent)
  return defaults?.model || defaultModelFor(agent)
}

/** The session's live relay reads + the synced row, for the bottom row. */
function useComposerSession(sessionId: string) {
  const { row } = useSessionRow(sessionId)
  const live = row !== null && row.status !== `ended`
  const store = useMemo(() => acquireSteerSession(sessionId), [sessionId])
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot)
  // The thread and the agent channel render no feed of their own: the
  // composer is what keeps the root's relay state (model, usage) current.
  useEffect(() => {
    if (row) store.noteSessionStatus(row.status)
  }, [store, row?.status])
  useEffect(() => {
    if (live) store.connect()
  }, [store, live])
  const deviceId = row?.deviceId ?? null
  const { data: devices } = useLiveQuery(
    (query) =>
      deviceId
        ? query
            .from({ devices: deviceCollection })
            .where(({ devices }) => eq(devices.id, deviceId))
        : undefined,
    [deviceId]
  )
  const device = (devices ?? [])[0] as
    | { launchDefaults?: { agents?: Record<string, { model?: string; threadModel?: string }> } | null }
    | undefined
  return {
    row,
    config: snapshot.config,
    usage: snapshot.usage,
    contextLayout: snapshot.contextLayout,
    fallback: fallbackModel(row, device?.launchDefaults?.agents ?? undefined),
  }
}

interface PendingFile {
  key: string
  file: File
  previewUrl: string | null
}

export function SessionComposer({
  teamId,
  sessionId,
  placeholder,
  users = [],
  busy = false,
  canStop = false,
  testId,
  className,
  opaque = false,
}: {
  teamId: string
  sessionId: string
  placeholder: string
  users?: readonly User[]
  /** The session works right now: the submit offers Stop when `canStop`. */
  busy?: boolean
  canStop?: boolean
  testId?: string
  className?: string
  /** EXP-1316: the phones' floating card (`FloatingComposer`) floats over
   *  the conversation: the opaque Composer surface, never see-through. */
  opaque?: boolean
}) {
  const [text, setText] = useState(``)
  const [files, setFiles] = useState<PendingFile[]>([])
  const [sending, setSending] = useState(false)
  /** A context-discarding command (`/clear`) waiting on its confirmation. */
  const [confirming, setConfirming] = useState<SteerCommand | null>(null)
  const [usageOpen, setUsageOpen] = useState(false)
  const fieldRef = useRef<MentionTextareaHandle>(null)
  const toolsRef = useRef<ComposerInlineToolsHandle>(null)

  const { row, config, usage, contextLayout, fallback } = useComposerSession(sessionId)
  const agent = row?.cli ?? `claude`
  const options = useSessionComposerOptions({ teamId, sessionId, row, agent })
  // The `/` menu: the contract catalog for this agent, then the agent's own
  // advertised commands (`config_state.commands`).
  const catalogAgent = steerAgentId(row?.cli, config !== null)
  const commands = useMemo(
    () =>
      mergeAgentCommands(
        steerCommandsFor(catalogAgent),
        config?.commands ?? [],
        catalogAgent
      ),
    [catalogAgent, config?.commands]
  )
  const menu = useSlashCommandMenu({ text, commands, onAccept: setText })
  // EXP-1323: `/btw <question>` is an ASIDE child of this session
  // (`sessions.send({parentId, text, aside})`), never a message to it.
  const dispatch = useSteerCommand({ teamId, sessionId, agent: catalogAgent })

  // The model pin: the model in force (the relay's `config_state`), else the
  // row's / device's. A pick reads as picked AT ONCE (the CLI only confirms
  // once it ran the `/model` prompt — an idle session resumes first) until
  // the relay says so, a failure, or the confirm window runs out.
  const liveModel = sessionModel(config)
  const [pickedModel, setPickedModel] = useState<string | null>(null)
  useEffect(() => {
    if (pickedModel === null) return
    if (liveModel === pickedModel) {
      setPickedModel(null)
      return
    }
    const timer = setTimeout(() => setPickedModel(null), MODEL_PICK_CONFIRM_MS)
    return () => clearTimeout(timer)
  }, [pickedModel, liveModel])
  const model = pickedModel ?? liveModel ?? fallback
  // THE model choices, no Default row; a model this build has no value for
  // still reads as itself.
  const modelOptions = useMemo<ValueChoice[]>(() => {
    const choices = modelChoices(agent)
    return choices.some((option) => option.value === model)
      ? choices
      : [{ value: model, label: modelLabel(model) }, ...choices]
  }, [agent, model])
  // The Plan switch: a pick leads, then the run's live mode (never a
  // dispatcher's: its plan mode is a WORK pick its children get), then the
  // synced row.
  const livePlan = row?.dispatch ? null : planModeToggle(config)
  const planOn = options.planPending
    ? options.planMode
    : (livePlan?.active ?? options.planMode)
  const percent = contextPercent(usage)

  const addFiles = (picked: File[]) => {
    if (picked.length === 0) return
    setFiles((current) => [
      ...current,
      ...picked.map((file, index) => ({
        key: `${Date.now()}-${index}-${file.name}`,
        file,
        previewUrl: file.type.startsWith(`image/`) ? URL.createObjectURL(file) : null,
      })),
    ])
  }

  const removeFile = (key: string) =>
    setFiles((current) => {
      const gone = current.find((entry) => entry.key === key)
      if (gone?.previewUrl) URL.revokeObjectURL(gone.previewUrl)
      return current.filter((entry) => entry.key !== key)
    })

  const empty = text.trim().length === 0 && files.length === 0

  const send = async (confirmed = false) => {
    if (sending || empty) return
    // A slash command is the WHOLE message: no attachments around it, and a
    // context-discarding one asks first (the device runs what it receives).
    const command = parseSteerCommand(text, commands)
    if (command) {
      if (files.length > 0) {
        toast.error(`Remove the attachments to send a command`)
        return
      }
      if (command.command.confirm && !confirmed) {
        setConfirming(command.command)
        return
      }
    }
    setSending(true)
    try {
      // A steer command the web runs itself (`/btw`): the draft clears once
      // the aside is in; a failed one toasted and keeps the draft.
      const dispatched = await dispatch(text)
      if (dispatched !== null) {
        if (dispatched === `sent`) setText(``)
        return
      }
      const attachmentIds: string[] = []
      for (const entry of files) {
        const uploaded = await uploadTeamSessionImageFile(teamId, entry.file)
        attachmentIds.push(uploaded.id)
      }
      const result = await trpc.sessions.send.mutate({
        teamId,
        sessionId,
        text: text.trim() || undefined,
        attachmentIds: attachmentIds.length > 0 ? attachmentIds : undefined,
      })
      for (const entry of files) if (entry.previewUrl) URL.revokeObjectURL(entry.previewUrl)
      setText(``)
      setFiles([])
      // A message to a live session writes only a `messages` row: free the
      // composer now, then wait (bounded) for that row to sync.
      setSending(false)
      await messageCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
    } catch {
      toast.error(`Couldn't send the message`)
    } finally {
      setSending(false)
    }
  }

  const stop = async () => {
    try {
      await trpc.sessions.stop.mutate({ sessionId })
    } catch {
      toast.error(`Couldn't stop the session`)
    }
  }

  const pickModel = async (value: string) => {
    // One `/model` turn at a time: a pick in flight (until the relay
    // confirms it, a failure or the confirm window) latches the next.
    if (value === model || options.ended || pickedModel !== null) return
    setPickedModel(value)
    try {
      await trpc.sessions.send.mutate(
        { teamId, sessionId, text: `/model ${value}` },
        SKIP_ERROR_TOAST
      )
    } catch (error) {
      setPickedModel(null)
      failedAction(`switch`, `the model`, error)
    }
  }

  const showStop = canStop && busy && empty && !sending
  const confirmCopy = steerCommandConfirmCopy(confirming?.name ?? ``)

  return (
    <ComposerFrame
      data-testid={testId}
      className={cn(`w-full`, className)}
      options={
        row && (
          <ComposerOptionsRow data-testid="session-composer-options">
            {/* EXP-1252: THE launch line's Device pin — a pick moves the
                session to that device after its turn. */}
            <SessionDevicePin teamId={teamId} row={row} />
            <span data-testid="session-composer-model" data-model={model}>
              <ModelPicker
                value={model}
                models={modelOptions}
                onChange={(value) => void pickModel(value)}
                // Codex has no mid-run model prompt: its pin is a label ×4.
                disabled={options.ended || agent !== `claude`}
              />
            </span>
            {options.planSupported && (
              <Label className="cursor-pointer gap-1.5 font-normal">
                <span>Plan</span>
                <Switch
                  size="sm"
                  checked={planOn}
                  onCheckedChange={options.setPlanMode}
                  disabled={options.ended}
                  aria-label="Plan mode"
                  data-testid="session-composer-plan"
                />
              </Label>
            )}
            {percent !== null && (
              <span className="ml-auto flex items-center">
                <MobilePopover open={usageOpen} onOpenChange={setUsageOpen}>
                  <MobilePopoverTrigger asChild>
                    <ContextRing percent={percent} tone={severity(percent)} />
                  </MobilePopoverTrigger>
                  <MobilePopoverContent
                    align="end"
                    collisionPadding={8}
                    className="w-80 p-0"
                    aria-label="Usage"
                    mobileTitle="Usage"
                    data-testid="session-usage-popover"
                  >
                    <ContextWindowBlock
                      sessionUsage={usage}
                      contextLayout={contextLayout}
                      teamId={teamId}
                    />
                  </MobilePopoverContent>
                </MobilePopover>
              </span>
            )}
          </ComposerOptionsRow>
        )
      }
    >
      <Composer
        opaque={opaque}
        strip={
          files.length > 0 ? (
            <div className="flex flex-wrap gap-2 px-3 pt-3">
              {files.map((entry) =>
                entry.previewUrl ? (
                  <AttachmentThumb
                    key={entry.key}
                    src={entry.previewUrl}
                    alt={entry.file.name}
                    onRemove={() => removeFile(entry.key)}
                  />
                ) : (
                  <AttachmentFileTile
                    key={entry.key}
                    name={entry.file.name}
                    onRemove={() => removeFile(entry.key)}
                  />
                )
              )}
            </div>
          ) : undefined
        }
        tools={
          <>
            <ComposerOptionsMenu
              options={options.binding}
              onAttach={() => toolsRef.current?.attach()}
              rowsDisabled={options.ended}
              highlighted={files.length > 0}
            />
            <ComposerInlineTools
              ref={toolsRef}
              fieldRef={fieldRef}
              onFiles={addFiles}
              disabled={sending}
            />
          </>
        }
        submit={
          <ComposerSubmit
            stop={showStop}
            aria-label={showStop ? `Stop` : `Send`}
            disabled={!showStop && (empty || sending)}
            onClick={showStop ? stop : () => void send()}
          >
            {sending ? <Spinner size={16} label="Sending" /> : undefined}
          </ComposerSubmit>
        }
        onDrop={(event) => {
          if (event.dataTransfer.files.length === 0) return
          event.preventDefault()
          addFiles(Array.from(event.dataTransfer.files))
        }}
        onDragOver={(event) => {
          if (event.dataTransfer.types.includes(`Files`)) event.preventDefault()
        }}
      >
        <div className="relative">
          <MentionTextarea
            ref={fieldRef}
            value={text}
            onValueChange={setText}
            users={users as User[]}
            onKeyDown={(event) => {
              if (event.nativeEvent.isComposing) return
              // The `/` menu gets first refusal: Enter/Tab accept a command
              // and never send the half-typed draft.
              if (menu.handleKeyDown(event)) return
              if (event.key !== `Enter` || event.shiftKey) return
              event.preventDefault()
              void send()
            }}
            onPaste={(event) => {
              if (event.clipboardData.files.length === 0) return
              event.preventDefault()
              addFiles(Array.from(event.clipboardData.files))
            }}
            placeholder={placeholder}
            className="min-h-20 resize-none border-0 bg-transparent px-3 pt-3 shadow-none focus-visible:ring-0"
            maxLength={SESSIONS_SEND_TEXT_MAX}
          />
          {menu.open && (
            <SlashCommandMenu
              commands={menu.candidates}
              active={menu.active}
              onSelect={menu.accept}
              onHover={menu.setActive}
            />
          )}
        </div>
      </Composer>
      <Prompt
        open={confirming !== null}
        onOpenChange={(next) => {
          if (!next) setConfirming(null)
        }}
        title={confirmCopy.title}
        body={confirmCopy.body}
        actions={[
          { label: confirmCopy.cancel, role: `cancel` },
          {
            label: confirmCopy.confirm,
            role: `destructive`,
            onSelect: () => {
              setConfirming(null)
              void send(true)
            },
          },
        ]}
      />
    </ComposerFrame>
  )
}
