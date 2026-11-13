import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react"
import { useComposerDraft } from "@/hooks/use-composer-draft"
import type { ComposerDraftTarget } from "@exp/domain-contract/composer-draft"
import { sendSessionMessage } from "@/lib/enet/send-sealed"
import { sendThroughOutbox } from "@/hooks/use-session-outbox"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { SESSIONS_SEND_TEXT_MAX } from "@exp/db-schema/domain"
import { composerTextAfterSend } from "@exp/domain-contract/composer-send"
import {
  AttachmentFileTile,
  AttachmentThumb,
  Composer,
  ComposerFrame,
  ComposerReplies,
  ComposerSubmit,
  ContextRing,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  InlinePickerRow,
  Prompt,
  Spinner,
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
import {
  ComposerOptionsMenu,
  type ComposerMcpBinding,
  type ComposerRunnerBinding,
  type ComposerTeamsBinding,
} from "@/components/launch-dialog/composer-plus-menu"
import { useMcpServers } from "@/hooks/use-mcp-servers"
import { TeamPickChips } from "@/components/launch-dialog/team-pick"
import { useSession } from "@/hooks/use-session"
import { useAgentRow } from "@/hooks/use-agent-row"
import { useTeamById, useTeamMemberships } from "@/hooks/use-team-data"
import { useSessionAccountSwitch } from "@/components/session-account-switch"
import type { ComposerMenuSurface } from "@/components/launch-dialog/composer-menu"
import {
  SlashCommandMenu,
  useSlashCommandMenu,
} from "@/components/steer-command-menu"
import {
  modelChoices,
  modelLabel,
} from "@/components/launch-dialog/launch-options-pane"
import { useComposerReplies } from "@/hooks/use-composer-replies"
import { issueRefInsertionText } from "@/components/issue-editor/formatting-rail"
import { useSessionRow } from "@/hooks/use-session-tree"
import { useSteerCommand } from "@/hooks/use-steer-command"
import { usePrompts } from "@/hooks/use-prompts"
import { sessionModel } from "@/lib/agent-feed"
import { contextPercent, severity } from "@/lib/agent-usage"
import { defaultModelFor } from "@/lib/coding-launch-prefs"
import {
  acquireSteerSession,
  mergeHeldIntoDraft,
  type SteerDraftSnapshot,
  type SteerSessionSnapshot,
  type SteerSessionStore,
} from "@/lib/steer-session-store"
import {
  parseSteerCommand,
  runAgentId,
  steerCommandConfirmCopy,
  steerCommandsFor,
  matchSlashDraft,
  type SteerCommand,
} from "@/lib/steer-commands"
import { uploadTeamSessionImageFile } from "@/lib/storage/issue-image-upload"
import { deviceCollection, messageCollection, sessionCollection } from "@/lib/collections"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import type { SessionRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { onStopRetraction, stopSessionTurn } from "@/lib/session-stop"
import { cn } from "@/lib/utils"

// EXP-1293: THE reply box ×4 = the home's composer box (the
// Composer primitive), never a new box. It posts ONE message to a session
// (`sessions.send({sessionId})`): a chat root (an ended chat continues
// server-side in a new child), an agent's channel, or an opened session.
// Stop = `sessions.stop` while the session works.
//
// EXP-1334 (owner: ONE composer, the old session layout): the start, the
// chat and the session view render the SAME frame (`ComposerFrame`): the
// card = the field (the `/` menu over it) and ONE tool row. N-12 (owner
// 2026-10-22, the iOS row ×4): the row = the "+" = THE options menu
// (composer-menu.json rows minus the subject rows: Effort ›, Ultracode,
// MCP servers ›, Computer use, Add file — changed mid-session through
// `sessions.configure`; the "+" is the only tool, no inline edit tools) …
// the context ring (its usage popover on click) beside Send ⇄ Stop. Nothing
// under the card. S-3 (composer-menu.json surface `session` | `agent`): the
// "+" = Photo or file · Issue › (a pick inserts `#IDENT`) · Model › (THE
// model rows; a pick = `sessions.configure {model}`, shown at once until
// the relay's `config_state` confirms) while the agent switches models;
// #EXP-1535 Account › (THE AccountPicker rows; a pick = the usage wall's
// session account switch, `useSessionAccountSwitch`) while the session's
// device is online and the viewer steers it; no Device pin (a running
// session never moves device here; #FEED-204: an app editor's composer is the
// exception, its Device › is apps.json EDITOR DEVICE), no model word in the row. An agent chat with no conversation yet (`sessionId` null) sends
// `sessions.send({agentId})` through the same composer. The live
// reads (model in force, usage, commands) are the session's relay state
// (`acquireSteerSession`), the synced row the fallback.

/** How long a model pick reads as picked without the relay confirming. */
const MODEL_PICK_CONFIRM_MS = 20_000

/** The model a session runs on before its relay says: the row's own pick,
 *  else the device's default, else the defaults. */
function fallbackModel(
  row: SessionRow | null,
  deviceAgents: Record<string, { model?: string }> | undefined
): string {
  const agent = row?.cli ?? `claude`
  if (row?.model) return row.model
  const defaults = deviceAgents?.[agent]
  return defaults?.model || defaultModelFor(agent)
}

/** No session yet (an agent chat's first message): a store that never
 *  connects and holds nothing. */
const IDLE_SNAPSHOT = { config: null, usage: null, contextLayout: null } as unknown as SteerSessionSnapshot
const IDLE_DRAFT: SteerDraftSnapshot = { text: ``, images: [] }
const IDLE_STORE = {
  subscribe: () => () => undefined,
  getSnapshot: () => IDLE_SNAPSHOT,
  getDraftSnapshot: () => IDLE_DRAFT,
  takeDraftText: () => ``,
  noteSessionStatus: () => undefined,
  connect: () => undefined,
} as unknown as SteerSessionStore

/** The models a live session can switch to mid-run (composer-menu.json
 *  `model`: always for claude and codex). */
const MODEL_SWITCHING_AGENTS = new Set([`claude`, `codex`])

/** The session's live relay reads + the synced row, for the bottom row. */
function useComposerSession(sessionId: string | null) {
  const { row } = useSessionRow(sessionId)
  const live = row !== null && row.status !== `ended`
  const store = useMemo(() => (sessionId ? acquireSteerSession(sessionId) : IDLE_STORE), [sessionId])
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot)
  // The chat and the agent channel render no feed of their own: the
  // composer is what keeps the root's relay state (model, usage) current.
  useEffect(() => {
    if (row) store.noteSessionStatus(row.status, { busy: row.busy, deviceId: row.deviceId })
  }, [store, row?.status, row?.busy, row?.deviceId])
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
    | { launchDefaults?: { agents?: Record<string, { model?: string }> } | null }
    | undefined
  return {
    store,
    row,
    config: snapshot.config,
    usage: snapshot.usage,
    contextLayout: snapshot.contextLayout,
    fallback: fallbackModel(row, device?.launchDefaults?.agents ?? undefined),
  }
}

/** #EXP-1715: the Teams pick of a composer talking to the viewer's
 *  Assistant (its conversation `row`, or `agentId` before it exists);
 *  null anywhere else. */
function useAssistantTeamsPick(
  row: { id: string; parentId: string | null; agentId: string | null; scopeTeamIds?: string[] | null } | null,
  agentId: string | undefined
): ComposerTeamsBinding | null {
  const { data: auth } = useSession()
  const me = auth?.user?.id
  const { agent } = useAgentRow(row ? (row.parentId === null ? row.agentId : null) : (agentId ?? null))
  const { myTeams } = useTeamMemberships(me)
  const assistant = agent !== null && agent.role === `assistant` && agent.ownerId === me
  const stored = row?.scopeTeamIds ?? null
  const [picked, setPicked] = useState<string[] | null>(stored)
  // A pick made elsewhere (another client) re-seeds it.
  const storedKey = stored?.join(`,`) ?? null
  useEffect(() => setPicked(stored), [storedKey])
  const teams = useMemo(
    () => myTeams.map((team) => ({ id: team.id, name: team.name, iconUrl: team.iconUrl ?? null })),
    [myTeams]
  )
  return assistant ? { teams, picked, onChange: setPicked } : null
}

/** Two MCP server picks name the same servers (order aside). */
function sameMcpPick(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((id) => b.includes(id))
}

/** #EXP-1793 (composer-menu.json `mcpPick`): an agent chat's MCP servers ›.
 *  Seed = the conversation's pick (`sessions.mcpPicks`), none yet = the
 *  agent's `mcpServerIds`; a pick that differs from the seed rides the next
 *  send as `mcpServerIds` (`sendPick`; an empty pick clears), after which it
 *  is the seed (`sent`). null off an agent chat. */
function useAgentMcpPick({
  enabled,
  teamId,
  sessionId,
  agentId,
}: {
  enabled: boolean
  teamId: string
  sessionId: string | null
  agentId: string | null
}): {
  binding: ComposerMcpBinding
  sendPick: string[] | undefined
  sent: (pick: string[]) => void
} | null {
  const { agent } = useAgentRow(enabled ? agentId : null)
  const { servers } = useMcpServers(teamId, enabled)
  const teamSlug = useTeamById(enabled ? teamId : undefined)?.slug ?? null
  const [sessionSeed, setSessionSeed] = useState<string[] | null>(null)
  const [picked, setPicked] = useState<string[] | null>(null)
  useEffect(() => {
    setSessionSeed(null)
    setPicked(null)
    if (!enabled || !sessionId) return
    let active = true
    trpc.sessions.mcpPicks
      .query({ sessionId }, SKIP_ERROR_TOAST)
      .then((result) => {
        if (active) setSessionSeed(result.mcpServerIds)
      })
      // Unreadable (not the viewer's to steer): the agent's own stand in.
      .catch(() => undefined)
    return () => {
      active = false
    }
  }, [enabled, sessionId])
  const seed = sessionSeed ?? agent?.mcpServerIds ?? []
  if (!enabled) return null
  const list = servers ?? []
  // Ids naming a deleted server drop out of the count and the send.
  const current = (picked ?? seed).filter((id) => list.some((server) => server.id === id))
  const changed = picked !== null && !sameMcpPick(picked, seed)
  return {
    binding: {
      servers: list,
      selectedIds: current,
      onToggle: (id) =>
        setPicked(current.includes(id) ? current.filter((other) => other !== id) : [...current, id]),
      connectHref: (serverId) =>
        teamSlug
          ? `/t/${encodeURIComponent(teamSlug)}/settings/mcp?connect=${encodeURIComponent(serverId)}`
          : null,
    },
    sendPick: changed ? current : undefined,
    sent: (pick) => {
      setSessionSeed(pick)
      setPicked(null)
    },
  }
}

function teamsPickChips(pick: ComposerTeamsBinding | null): boolean {
  if (!pick || pick.picked === null) return false
  const count = pick.teams.filter((team) => pick.picked!.includes(team.id)).length
  return count > 0 && count < pick.teams.length
}

/** #EXP-1729: an app editor's Account › and Model › before its root runs,
 *  and what they add to the next send. */
export interface EditorHeldRunner {
  account?: ComposerRunnerBinding[`account`]
  model?: ComposerRunnerBinding[`model`]
  send: { cli?: string; account?: string; model?: string }
}

interface PendingFile {
  key: string
  file: File
  previewUrl: string | null
}

/** EXP-1211: the session's quick replies on their own (a host that draws
 *  them outside the composer card, e.g. over the phones' capsule). */
export function SessionComposerReplies({ sessionId }: { sessionId: string }) {
  const quick = useComposerReplies(sessionId)
  if (!quick.replies) return null
  return (
    <ComposerReplies
      replies={quick.replies}
      pending={quick.pending}
      onSend={(messageId, itemId) => quick.send(messageId, itemId)}
      onDismiss={quick.dismiss}
    />
  )
}

export function SessionComposer({
  teamId,
  browseTeamId,
  sessionId,
  agentId,
  appId,
  deviceId,
  deviceMenu,
  moveDevice = false,
  heldRunner,
  disabled = false,
  surface = `session`,
  onSent,
  placeholder,
  users = [],
  busy = false,
  canStop = false,
  testId = `session-composer`,
  className,
  opaque = false,
  hideReplies = false,
  autoFocus = false,
}: {
  teamId: string
  /** #EXP-1714: the team the `/` prompts come from when it is not the
   *  conversation's (the global Assistant's lives in the personal team). */
  browseTeamId?: string
  /** The session messaged; null = an agent chat with no conversation yet
   *  (the send opens it: `sessions.send({agentId})`). */
  sessionId: string | null
  /** The agent a first message goes to (`sessionId` null). */
  agentId?: string
  /** VAPP-110: or the app whose editor session a first message opens
   *  (`sessions.send({appId, deviceId})`). */
  appId?: string
  /** The device a fresh editor session runs on (an app send). */
  deviceId?: string
  /** #FEED-204: an app editor's Device › row (apps.json EDITOR DEVICE). */
  deviceMenu?: ComposerRunnerBinding[`device`]
  /** #FEED-204: the ended root's next send goes to `deviceId`: it goes as
   *  `sessions.send({appId, deviceId})`, which revives the root there. */
  moveDevice?: boolean
  /** #EXP-1729 (apps.json `editorHeader`): an app editor's Account › and
   *  Model › while its root is not live (none yet, or ENDED): THE launch
   *  composer's rows, and the picks ride the next send (`send`). A live
   *  root reads its own and a pick is the session's switch, as anywhere. */
  heldRunner?: EditorHeldRunner
  /** Nothing can be sent (an app editor with no device). */
  disabled?: boolean
  /** composer-menu.json: a chat / a session, or an agent chat. */
  surface?: Exclude<ComposerMenuSurface, `launch`>
  /** The conversation a first message opened. */
  onSent?: (sessionId: string) => void
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
  /** EXP-1211: the host draws `SessionComposerReplies` itself (the phones'
   *  FloatingComposer keeps them over its collapsed capsule). */
  hideReplies?: boolean
  /** VAPP-176: a new app draft's editor opens with the field focused. */
  autoFocus?: boolean
}) {
  // #FEED-219 (composer-draft.json): the field IS the target's synced draft
  // (a session's, an agent's before its conversation, an app editor's before
  // its first session), so a tab switch, a reload or another device keeps it.
  const draftTarget: ComposerDraftTarget | null = sessionId
    ? { kind: `session`, id: sessionId }
    : appId
      ? { kind: `app`, id: appId }
      : agentId
        ? { kind: `agent`, id: agentId }
        : null
  const { text, textRef, setText } = useComposerDraft(draftTarget, teamId)
  const [files, setFiles] = useState<PendingFile[]>([])
  const [sending, setSending] = useState(false)
  /** A context-discarding command (`/clear`) waiting on its confirmation. */
  const [confirming, setConfirming] = useState<SteerCommand | null>(null)
  const [usageOpen, setUsageOpen] = useState(false)
  const fieldRef = useRef<MentionTextareaHandle>(null)
  const toolsRef = useRef<ComposerInlineToolsHandle>(null)

  const { store, row, config, usage, contextLayout, fallback } = useComposerSession(sessionId)
  const accountSwitch = useSessionAccountSwitch(row)
  // EXP-1211: the agent's quick replies, above the card.
  const quick = useComposerReplies(sessionId ?? ``)
  // EXP-1342: a Stop (here or the header's) hands the unread messages' text
  // back through the session store's draft: it lands in THIS field, after a
  // draft in progress, and the store's copy is cleared. What was merged is
  // remembered (the text handed out, the field before and after it) so a
  // REFUSED Stop can take exactly that back out: the device still holds
  // the queue, and the text must not stay here too (it would go twice).
  const restored = useSyncExternalStore(store.subscribe, store.getDraftSnapshot).text
  const heldRef = useRef<{
    held: string
    before: string
    after: string
  } | null>(null)
  useEffect(() => {
    if (!restored) return
    const held = store.takeDraftText()
    if (!held) return
    const before = textRef.current
    const after = mergeHeldIntoDraft(before, [held])
    heldRef.current = { held, before, after }
    setText(after)
  }, [store, restored, setText, textRef])
  useEffect(
    () =>
      onStopRetraction(sessionId ?? ``, (held) => {
        const last = heldRef.current
        if (!last || last.held !== held) return false
        heldRef.current = null
        // Typed over since: the person's edit stays.
        if (textRef.current !== last.after) return false
        setText(last.before)
        return true
      }),
    [sessionId]
  )
  const agent = row?.cli ?? `claude`
  const ended = row === null || row.status === `ended`
  // The `/` menu (NEW-2): the contract builtins for this agent, then the
  // team's prompts; the CLI's own injected commands are never offered.
  const catalogAgent = runAgentId(row?.cli)
  const commands = useMemo(() => steerCommandsFor(catalogAgent), [catalogAgent])
  const { prompts } = usePrompts(browseTeamId ?? teamId)
  // #EXP-1715 (composer-menu.json `teamsPick`): the viewer's Assistant's
  // Teams pick, seeded from its conversation root (null = every team).
  const teamsPick = useAssistantTeamsPick(row, agentId)
  // #EXP-1793: an agent chat's MCP servers pick (its conversation's).
  const mcpPick = useAgentMcpPick({
    enabled: surface === `agent`,
    teamId,
    sessionId,
    agentId: row ? (row.parentId === null ? row.agentId : null) : (agentId ?? null),
  })
  // A picked prompt's body (`prompts.get`, the only body path) REPLACES the
  // `/name` draft the menu was open on, to be edited and sent.
  const pickPrompt = (prompt: { id: string; name: string }) => {
    trpc.prompts.get
      .query({ id: prompt.id })
      .then(({ prompt: full }) => {
        if (matchSlashDraft(textRef.current) === null) return
        setText(full.body)
      })
      .catch((error) => failedAction(`load`, prompt.name, error))
  }
  // #EXP-1434: Prompts › in the "+" — the same body, inserted at the caret.
  const insertPrompt = (prompt: { id: string; name: string }) => {
    trpc.prompts.get
      .query({ id: prompt.id })
      .then(({ prompt: full }) => fieldRef.current?.insertText(full.body))
      .catch((error) => failedAction(`load`, prompt.name, error))
  }
  const menu = useSlashCommandMenu({
    text,
    commands,
    prompts,
    onAccept: (next: string) => setText(next),
    onPickPrompt: pickPrompt,
  })
  // NEW-2: `/effort <level>` = `sessions.configure`, never a message.
  const dispatch = useSteerCommand({ sessionId: sessionId ?? ``, agent: catalogAgent })

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
    if (sending || empty || disabled) return
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
    // EXP-1537 (composer-send.json): the field stays editable while the
    // send is in flight; a success clears only what went out.
    const sentText = text
    const sentFiles = files
    const sentMcpPick = mcpPick?.sendPick
    const clearSent = () => {
      const after = composerTextAfterSend(sentText, textRef.current)
      setText(after, { now: true })
    }
    setSending(true)
    try {
      // A steer command the web runs itself (`/effort`): the draft clears
      // once it is applied; a failed one toasted and keeps the draft.
      const dispatched = sessionId ? await dispatch(sentText) : null
      if (dispatched !== null) {
        if (dispatched === `sent`) clearSent()
        return
      }
      // #FEED-237 (message-delivery.json): a message into a session is an
      // outbox bubble from the press: the field, its synced draft and the
      // picked files clear now; the bubble carries the clock, then the
      // device's checks, or the red mark with Retry.
      if (sessionId && !moveDevice && !(ended && heldRunner)) {
        const extra = {
          ...(teamsPick ? { scopeTeamIds: teamsPick.picked === null ? null : [...teamsPick.picked] } : {}),
          ...(sentMcpPick ? { mcpServerIds: sentMcpPick } : {}),
        }
        sendThroughOutbox({
          teamId,
          sessionId,
          text: sentText,
          extra,
          // FILES: the entry keeps them (IndexedDB) and uploads them itself.
          files: sentFiles.map((entry) => entry.file),
        })
        for (const entry of sentFiles) if (entry.previewUrl) URL.revokeObjectURL(entry.previewUrl)
        if (sentMcpPick) mcpPick?.sent(sentMcpPick)
        setText(``, { now: true })
        heldRef.current = null
        const gone = new Set(sentFiles.map((entry) => entry.key))
        setFiles((current) => current.filter((entry) => !gone.has(entry.key)))
        return
      }
      const attachmentIds: string[] = []
      for (const entry of sentFiles) {
        const uploaded = await uploadTeamSessionImageFile(teamId, entry.file)
        attachmentIds.push(uploaded.id)
      }
      const result = await sendSessionMessage({
        teamId,
        ...(sessionId && !moveDevice ? { sessionId } : appId ? { appId, deviceId } : { agentId }),
        text: sentText.trim() || undefined,
        attachmentIds: attachmentIds.length > 0 ? attachmentIds : undefined,
        ...(ended && heldRunner ? heldRunner.send : {}),
        ...(teamsPick ? { scopeTeamIds: teamsPick.picked === null ? null : [...teamsPick.picked] } : {}),
        ...(sentMcpPick ? { mcpServerIds: sentMcpPick } : {}),
      })
      if (sentMcpPick) mcpPick?.sent(sentMcpPick)
      for (const entry of sentFiles) if (entry.previewUrl) URL.revokeObjectURL(entry.previewUrl)
      clearSent()
      // Sent: nothing of a Stop's text is left here to take back.
      heldRef.current = null
      const gone = new Set(sentFiles.map((entry) => entry.key))
      setFiles((current) => current.filter((entry) => !gone.has(entry.key)))
      // A message to a live session writes only a `messages` row: free the
      // composer now, then wait (bounded) for that row to sync.
      setSending(false)
      if (!sessionId) {
        // The first message opened the conversation: its row, then the chat.
        await sessionCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
        onSent?.(result.sessionId)
        return
      }
      await messageCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
    } catch {
      toast.error(`Couldn't send the message`)
    } finally {
      setSending(false)
    }
  }

  const stop = async () => {
    if (!sessionId) return
    try {
      // Chats STOP: the composer's stop = the TURN interrupt; an
      // undelivered one toasts inside `stopSessionTurn`.
      await stopSessionTurn(sessionId)
    } catch {
      toast.error(CHATS_COPY.stopTurn.failed)
    }
  }

  const pickModel = async (value: string) => {
    // One pick at a time: a pick in flight (until the relay confirms it, a
    // failure or the confirm window) latches the next. EXP-1401: the pick
    // is the row's (`sessions.configure`), never a `/model` turn: a walled
    // session takes no turn (the server lifts a model wall or refuses).
    if (!sessionId || value === model || ended || pickedModel !== null) return
    setPickedModel(value)
    try {
      await trpc.sessions.configure.mutate({ sessionId, model: value }, SKIP_ERROR_TOAST)
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
      replies={
        !hideReplies &&
        quick.replies && (
          <ComposerReplies
            replies={quick.replies}
            pending={quick.pending}
            onSend={(messageId, itemId) => quick.send(messageId, itemId)}
            onDismiss={quick.dismiss}
          />
        )
      }
    >
      {/* N-14: the one-row card is a ROW host: inside a dialog or a sheet an
          opened pin (Device, Model) drops its rows under the whole row, as
          wide as it; the collapsed pins never move. */}
      <InlinePickerRow slotClassName="pt-1.5 text-sm text-foreground">
      <Composer
        opaque={opaque}
        // #EXP-1762: the session screen's reply box is THE flat one-line
        // card: `+` left, the context ring + send right, growing upward.
        flat
        strip={
          files.length > 0 || teamsPickChips(teamsPick) ? (
            <div className="flex flex-wrap items-center gap-2 px-3 pt-3">
              {teamsPick && <TeamPickChips {...teamsPick} disabled={sending} />}
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
              surface={surface}
              agent={agent}
              runner={{
                ...(deviceMenu ? { device: deviceMenu } : {}),
                // #EXP-1729: an app editor's root not running yet = the held picks.
                ...(ended && heldRunner
                  ? {
                      ...(heldRunner.account ? { account: heldRunner.account } : {}),
                      ...(heldRunner.model ? { model: heldRunner.model } : {}),
                    }
                  : {}),
                ...(row &&
                !ended &&
                // #EXP-1729: an app editor's composer always carries it (the
                // switch's own rules disable the rows it refuses).
                (appId !== undefined || (accountSwitch.canSteer && accountSwitch.deviceOnline)) &&
                accountSwitch.options.length > 0
                  ? {
                      account: {
                        value: accountSwitch.value,
                        options: accountSwitch.options,
                        onChange: (key: string) => void accountSwitch.switchTo(key),
                      },
                    }
                  : {}),
                ...(row && !(ended && heldRunner) && MODEL_SWITCHING_AGENTS.has(agent)
                  ? {
                      model: {
                        value: model,
                        models: modelOptions,
                        onChange: (value: string) => void pickModel(value),
                      },
                    }
                  : {}),
              }}
              issues={{
                kind: `insert`,
                onPick: (identifier) =>
                  fieldRef.current?.insertText(
                    `${issueRefInsertionText(fieldRef.current.charBeforeCaret())}${identifier} `
                  ),
              }}
              prompts={{ prompts, onPick: insertPrompt }}
              teams={teamsPick ?? undefined}
              mcp={mcpPick?.binding}
              onAttach={(choice) => toolsRef.current?.attach(choice)}
              // #EXP-1729: an app editor's ENDED root revives on the next send.
              rowsDisabled={row !== null && ended && appId === undefined}
              highlighted={files.length > 0}
            />
            <ComposerInlineTools ref={toolsRef} onFiles={addFiles} />
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
              disabled={!showStop && (empty || sending || disabled)}
              onClick={showStop ? stop : () => void send()}
            >
              {sending ? <Spinner size={16} label="Sending" /> : undefined}
            </ComposerSubmit>
          </>
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
            onValueChange={(next) => setText(next)}
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
            autoFocus={autoFocus}
            className="max-h-60 min-h-0 resize-none border-0 bg-transparent px-1.5 py-2.5 shadow-none focus-visible:ring-0"
            maxLength={SESSIONS_SEND_TEXT_MAX}
          />
          {menu.open && (
            <SlashCommandMenu
              candidates={menu.candidates}
              active={menu.active}
              onSelect={menu.accept}
              onHover={menu.setActive}
            />
          )}
        </div>
      </Composer>
      </InlinePickerRow>
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
