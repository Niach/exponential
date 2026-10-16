import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { SESSIONS_SEND_TEXT_MAX } from "@exp/db-schema/domain"
import {
  AttachmentFileTile,
  AttachmentThumb,
  Composer,
  ComposerSubmit,
  ComposerTool,
  ContextRing,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  ModelPicker,
  Prompt,
  Spinner,
  conceptIcon,
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
  SlashCommandMenu,
  useSlashCommandMenu,
} from "@/components/steer-command-menu"
import {
  modelChoices,
  modelLabel,
} from "@/components/launch-dialog/launch-options-pane"
import { useSessionRow } from "@/hooks/use-session-tree"
import { planModeChipLabel, sessionModel } from "@/lib/agent-feed"
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

const AddIcon = conceptIcon(`ui-add`)

// EXP-1293 Threads v2: THE reply box ×4 = the home's composer box (the
// Composer primitive), never a new box. It posts ONE message to a session
// (`sessions.send({sessionId})`): a thread root (an ended thread continues
// server-side in a new child), an agent's channel, or an opened session.
// Stop = `sessions.stop` while the session works.
//
// Dispatch (owner 2026-10-15, session-thread.json DISPATCH): the SAME
// controls on every host, one component: the bottom row = "+" · the model
// pin (THE ModelPicker over the session's agent models; a pick sends
// `/model <alias>` through `sessions.send`) · the plan read-out (a plan-mode
// run that is NOT a dispatcher) … the context ring (its usage popover on
// click) · Send ⇄ Stop; the field offers the `/` menu (steerCommands ∪ the
// agent's own). The live reads (model in force, usage, commands) are the
// session's relay state (`acquireSteerSession`), the synced row the fallback.

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
  const fileInputRef = useRef<HTMLInputElement>(null)

  const { row, config, usage, contextLayout, fallback } = useComposerSession(sessionId)
  const agent = row?.cli ?? `claude`
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
  // The model pin: the model in force (the relay's `config_state`), else the
  // row's / device's. THE model choices, no Default row; a model this build
  // has no value for still reads as itself.
  const model = sessionModel(config) ?? fallback
  const modelOptions = useMemo<ValueChoice[]>(() => {
    const options = modelChoices(agent)
    return options.some((option) => option.value === model)
      ? options
      : [{ value: model, label: modelLabel(model) }, ...options]
  }, [agent, model])
  // The plan read-out: a plan-mode run, never a dispatcher (plan mode is a
  // WORK pick its children get).
  const planLabel = row?.dispatch
    ? null
    : (planModeChipLabel(config) ?? (config === null && row?.planMode ? `Plan` : null))
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
    if (value === model || row?.status === `ended`) return
    try {
      await trpc.sessions.send.mutate(
        { teamId, sessionId, text: `/model ${value}` },
        SKIP_ERROR_TOAST
      )
    } catch (error) {
      failedAction(`switch`, `the model`, error)
    }
  }

  const showStop = canStop && busy && empty && !sending
  const confirmCopy = steerCommandConfirmCopy(confirming?.name ?? ``)

  return (
    <Composer
      opaque={opaque}
      data-testid={testId}
      className={cn(`w-full`, className)}
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
          <input
            ref={fileInputRef}
            type="file"
            multiple
            hidden
            onChange={(event) => {
              addFiles(Array.from(event.target.files ?? []))
              event.target.value = ``
            }}
          />
          <ComposerTool aria-label="Add file" onClick={() => fileInputRef.current?.click()}>
            <AddIcon className="size-4" />
          </ComposerTool>
          {row && (
            <span className="ml-1 flex items-center text-xs text-muted-foreground">
              <ModelPicker
                value={model}
                models={modelOptions}
                onChange={(value) => void pickModel(value)}
                // Codex has no mid-run model prompt: its pin is a label ×4.
                disabled={row.status === `ended` || agent !== `claude`}
                data-testid="session-composer-model"
              />
            </span>
          )}
          {planLabel && (
            <span
              className="ml-2 text-xs text-sky-400"
              data-testid="session-composer-plan"
            >
              {planLabel}
            </span>
          )}
        </>
      }
      submit={
        <>
          {percent !== null && (
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
          )}
          <ComposerSubmit
            stop={showStop}
            aria-label={showStop ? `Stop` : `Send`}
            disabled={!showStop && (empty || sending)}
            onClick={showStop ? stop : () => void send()}
          >
            {sending ? <Spinner size={16} label="Sending" /> : undefined}
          </ComposerSubmit>
        </>
      }
    >
      <div className="relative">
      <MentionTextarea
        ref={fieldRef}
        value={text}
        onValueChange={setText}
        users={users as User[]}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return
          // The `/` menu gets first refusal: Enter/Tab accept a command and
          // never send the half-typed draft.
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
    </Composer>
  )
}
