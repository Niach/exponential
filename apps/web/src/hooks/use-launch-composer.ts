import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { useNavigate } from "@tanstack/react-router"
import { actionErrorTitle, toast } from "@exp/ui"
import {
  SESSIONS_SEND_MAX_ISSUES,
  type sessionsSendInputSchema,
} from "@exp/db-schema/domain"
import type { z } from "zod"
import type { Board, Issue, SyncedPrompt } from "@/db/schema"
import { useMcpServers } from "@/hooks/use-mcp-servers"
import { useTeamBoards, useTeamById } from "@/hooks/use-team-data"
import { useTeamRepos } from "@/hooks/use-team-repos"
import { usePrompts } from "@/hooks/use-prompts"
import { useAgents } from "@/hooks/use-session-tree"
import type { AgentRow } from "@/lib/session-tree"
import { issueCollection, sessionCollection } from "@/lib/collections"
import {
  chatRepoOptions,
  defaultChatRepoId,
  type ChatRepoOption,
} from "@/lib/chat-repo"
import { agentSeed, type CodingLaunchPrefs } from "@/lib/coding-launch-prefs"
import { byCreatedAtDesc } from "@/lib/ordering"
import {
  ATTACHMENT_REJECTED_TOAST,
  dropPendingImage,
  FILE_CAP_TOAST,
  IMAGE_CAP_TOAST,
  markPendingImageUploaded,
  stagePendingImages,
  type PendingAttachment,
} from "@/lib/pending-images"
import {
  deviceAgentLaunchDefaults,
  deviceHasRunnableAgent,
  deviceIsMine,
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import { uploadTeamSessionImageFile } from "@/lib/storage/issue-image-upload"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"
import { trpc } from "@/lib/trpc-client"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import {
  useLaunchOptions,
  type LaunchOptions,
} from "@/components/launch-dialog/use-launch-options"
import type { McpServerList } from "@/lib/mcp-servers"
import type { LaunchSeed } from "@/lib/launch-seed"

// EXP-825: the ONE launcher's state: the Agent page composer (and the same
// composer in the launch dialog). What a session is ABOUT is a SUBJECT picked
// into the composer (issue chips, or one PROMPT chip off the `/` menu, NEW-2;
// picking the other kind swaps), the free text is the first message (instructions beside a subject),
// files ride as attachments, and the options line under the card carries
// device/account/model/plan with the rest behind the "+".
//
// EXP-1293 (Threads v2): a start is ONE `sessions.send` in the ACTIVE team: a
// new root (a thread) on the picked device, with the subject as its facets
// (`issueIds` | `promptId` (+ `repositoryId`) | `repositoryId`). The server writes a
// `queued` row the device picks up; the composer awaits that row's sync
// (bounded) and opens the thread. No builtins, no resume, no start ids and
// no landing watch: the row exists the moment the mutation returns.

export type { LaunchSeed }

/** One prompt as the composer lists it: the synced, body-less row. */
export type ComposerPrompt = SyncedPrompt
/** NEW-3: one agent as the "+" menu lists it (`listAgents`: icon, colour, name). */
export type ComposerAgent = AgentRow

export type LaunchSubject =
  | { kind: `issues`; ids: string[] }
  | { kind: `prompt`; id: string }
  /** NEW-3: a message into the caller's DM with a team agent. */
  | { kind: `agent`; id: string }
  | null

/** Hard cap per session: parity with `sessions.send`'s zod cap. */
export const MAX_ISSUES_PER_RUN = SESSIONS_SEND_MAX_ISSUES
// Above this, a batch gets a soft token-cost note (matches the native sheets).
export const BATCH_COST_HINT_THRESHOLD = 6
/** How long a start waits for its row to sync before it opens the thread
 *  anyway (the page renders its own skeleton until the row lands). */
export const START_SYNC_WAIT_MS = 4_000

// Only issues in a state worth coding are offered (mirrors the desktop
// picker). EXP-314: keyed on the dual-written ANCHOR enum, so custom
// statuses anchor into this set automatically.
const CODEABLE_STATUSES = new Set<string>([`backlog`, `in_progress`, `in_review`])

/** The submit's label per subject (its aria-label + tooltip). */
export function submitLabelFor(subject: LaunchSubject): string {
  if (subject?.kind === `agent`) return `Send`
  if (subject === null || subject.kind === `prompt`) return `Start session`
  return subject.ids.length >= 2
    ? `Start batch · ${subject.ids.length}`
    : `Start coding`
}

export type SessionsSendInput = z.input<typeof sessionsSendInputSchema>

/**
 * The `sessions.send` payload for a NEW root, pure so the mapping is a test:
 * the runner picks off `buildOptions` (`agent` → `cli`; blank model/effort
 * omitted = the CLI default), the text and attachments, then the subject's
 * facets (`issueIds`, or `promptId` beside the picked repository, or the
 * chat's `repositoryId`; repo-less = none).
 */
export function buildSendInput(args: {
  teamId: string
  deviceId: string
  options: CodingLaunchPrefs
  text: string
  attachmentIds: readonly string[]
  subject: LaunchSubject
  repoId: string
}): SessionsSendInput {
  const { options, subject } = args
  const text = args.text.trim()
  // NEW-3: an agent subject = a message into the caller's DM with it. The
  // DM runs on the agent's own device and login: no runner picks ride.
  if (subject?.kind === `agent`) {
    return {
      teamId: args.teamId,
      agentId: subject.id,
      ...(text ? { text: args.text } : {}),
      ...(args.attachmentIds.length > 0 ? { attachmentIds: [...args.attachmentIds] } : {}),
    }
  }
  return {
    teamId: args.teamId,
    deviceId: args.deviceId,
    cli: options.agent,
    ...(options.account ? { account: options.account } : {}),
    ...(options.model ? { model: options.model } : {}),
    ...(options.effort ? { effort: options.effort } : {}),
    planMode: options.planMode,
    ultracode: options.ultracode,
    ...(options.computerUse !== undefined ? { computerUse: options.computerUse } : {}),
    ...(options.mcpServerIds && options.mcpServerIds.length > 0
      ? { mcpServerIds: [...options.mcpServerIds] }
      : {}),
    ...(text ? { text: args.text } : {}),
    ...(args.attachmentIds.length > 0 ? { attachmentIds: [...args.attachmentIds] } : {}),
    ...(subject?.kind === `issues`
      ? { issueIds: [...subject.ids] }
      : {
          ...(subject?.kind === `prompt` ? { promptId: subject.id } : {}),
          ...(args.repoId ? { repositoryId: args.repoId } : {}),
        }),
  }
}

/** The failure toast, `Couldn't start the session` (FEED-79), with a
 *  "Sign in" action when the refusal is one a remote sign-in can fix (a
 *  PRECONDITION on one of the caller's OWN devices); everything else offers Retry. */
function startFailureToast(
  error: unknown,
  device: SteerDevice,
  agent: string,
  retry: () => void
): void {
  const canSignIn =
    trpcErrorCode(error) === `PRECONDITION_FAILED` &&
    deviceIsMine(device)
  if (!canSignIn) {
    failedAction(`start`, `the session`, error, { retry })
    return
  }
  toast.error(actionErrorTitle(`start`, `the session`), {
    description: trpcErrorMessage(error, `The session could not be started`),
    action: { label: `Sign in`, onClick: () => requestAgentLogin({ device, agent }) },
  })
}

/** Wait for `txid` to sync, at most `ms`; never throws. */
async function awaitSynced(txid: number, ms: number): Promise<void> {
  let timer: ReturnType<typeof setTimeout> | undefined
  await Promise.race([
    sessionCollection.utils.awaitTxId(txid).catch(() => undefined),
    new Promise<void>((resolve) => {
      timer = setTimeout(resolve, ms)
    }),
  ])
  clearTimeout(timer)
}

export interface LaunchComposerModel {
  teamId: string
  subject: LaunchSubject
  /** The subject prompt's row (null while it is not synced, or no prompt). */
  selectedPrompt: ComposerPrompt | null
  /** The team's prompts in server order (the `/` menu's rows). */
  prompts: ComposerPrompt[]
  /** Codeable issues on repo-backed boards, newest first. */
  eligibleIssues: Issue[]
  /** The checked issues' rows (a checked id not synced yet is absent). */
  checkedIssues: Issue[]
  toggleIssue: (issueId: string) => void
  /** The `/` menu's pick: the prompt becomes the subject. */
  pickPrompt: (promptId: string) => void
  clearPrompt: () => void
  /** NEW-3: the team's agents (the "+" menu's Message an agent › rows). */
  agents: ComposerAgent[]
  /** The subject agent's row (null while it is not synced, or no agent). */
  selectedAgent: ComposerAgent | null
  /** The "+" menu's pick: the agent becomes the subject (a DM). */
  pickAgent: (agentId: string) => void
  clearAgent: () => void

  text: string
  setText: (text: string) => void
  images: PendingAttachment[]
  /** Stages files and drops their markers at `caret`; returns the caret
   * behind the last marker. */
  addFiles: (files: File[], caret: number) => number
  removeImage: (url: string) => void

  /** The team's connected repositories (null until fetched). */
  repos: ChatRepoOption[] | null
  /** No subject or a prompt: the repository the session works in. */
  repoId: string
  setRepoId: (repoId: string) => void
  repoOptions: { value: string; label: string }[]

  launch: LaunchOptions
  /** Online machines with a runnable agent. */
  candidateDevices: SteerDevice[]
  /** EXP-836: why the machine a play button named is NOT the selected one. */
  deviceRequestNote: string | null
  mcpServers: McpServerList | null
  /** EXP-792: where a server the caller has not connected yet sends them. */
  mcpConnectHref: (serverId: string) => string | null

  submitLabel: string
  /** Something stops the submit; the notes say what. */
  blocked: boolean
  overCap: boolean
  spansRepos: boolean
  spansBranches: boolean
  costHint: boolean
  /** Uploading, or the start is in flight (the submit's pending spinner). */
  busy: boolean
  submit: () => Promise<void>
}

export function useLaunchComposer({
  teamId,
  devices,
  seed,
  onSeedConsumed,
  onStarted,
}: {
  teamId: string
  /** The caller's machines (`useSteerDevices`); null while syncing. */
  devices: SteerDevice[] | null
  /** The route's one-shot preselection; consumed once, then reported. */
  seed: LaunchSeed | null
  onSeedConsumed: () => void
  /** The session started (its row synced, or the wait gave up); called
   *  right before the composer opens its thread. */
  onStarted?: (sessionId: string) => void
}): LaunchComposerModel {
  const navigate = useNavigate()
  const [subject, setSubject] = useState<LaunchSubject>(null)
  const [text, setText] = useState(``)
  const [images, setImages] = useState<PendingAttachment[]>([])
  const [repoId, setRepoId] = useState(``)
  const [sending, setSending] = useState(false)
  const imagesRef = useRef(images)
  imagesRef.current = images

  // ── Data ──────────────────────────────────────────────────────────────────

  const repos = useTeamRepos(teamId)
  const seededRepo = useRef(false)
  useEffect(() => {
    if (!repos || seededRepo.current) return
    seededRepo.current = true
    setRepoId(defaultChatRepoId(repos))
  }, [repos])
  const repoOptions = useMemo(() => chatRepoOptions(repos ?? []), [repos])

  const { prompts } = usePrompts(teamId)
  const selectedPrompt =
    subject?.kind !== `prompt`
      ? null
      : (prompts.find((prompt) => prompt.id === subject.id) ?? null)
  // NEW-3: the team's agents, the "+" menu's Message an agent › rows.
  // (no viewer: the rows need no busy / needs-you state here)
  const { agents } = useAgents(teamId, null)
  const selectedAgent =
    subject?.kind !== `agent` ? null : (agents.find((agent) => agent.agentId === subject.id) ?? null)

  // Codeable issues live on boards that HAVE a repo (coding gates on repo
  // presence). Sorted ids keep the dep string stable.
  const boards = useTeamBoards(teamId)
  const repoBoardIds = useMemo(() => {
    const ids = boards.filter((board) => board.repositoryId).map((board) => board.id)
    ids.sort()
    return ids
  }, [boards])
  const { data: issueRows } = useLiveQuery(
    (query) =>
      repoBoardIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.boardId, repoBoardIds))
        : undefined,
    [repoBoardIds.join(`,`)]
  )
  const allById = useMemo(
    () => new Map(((issueRows ?? []) as Issue[]).map((issue) => [issue.id, issue])),
    [issueRows]
  )
  const eligibleIssues = useMemo(
    () =>
      ((issueRows ?? []) as Issue[])
        .filter((issue) => CODEABLE_STATUSES.has(issue.status))
        .sort(byCreatedAtDesc),
    [issueRows]
  )
  const checkedIds = subject?.kind === `issues` ? subject.ids : []
  const checkedIssues = useMemo(
    () =>
      checkedIds
        .map((id) => allById.get(id))
        .filter((issue): issue is Issue => Boolean(issue)),
    [checkedIds, allById]
  )

  // A batch is ONE repository on ONE base branch (the server enforces both):
  // block a cross-repo / cross-branch selection client-side. A board with no
  // branch override rides the repo's default, which only the server knows,
  // so the branch guard fires only on two DIFFERENT overrides.
  const boardById = useMemo(
    () => new Map(boards.map((board) => [board.id, board as Board])),
    [boards]
  )
  const { spansRepos, spansBranches } = useMemo(() => {
    const repoIds = new Set<string>()
    const branches = new Set<string>()
    for (const issue of checkedIssues) {
      const board = boardById.get(issue.boardId)
      if (board?.repositoryId) repoIds.add(board.repositoryId)
      if (board?.defaultBranch) branches.add(board.defaultBranch)
    }
    return { spansRepos: repoIds.size > 1, spansBranches: branches.size > 1 }
  }, [checkedIssues, boardById])

  // ── Options cluster ───────────────────────────────────────────────────────

  // EXP-403: the registry lists offline machines too; only online ones with
  // a runnable agent are startable.
  const candidateDevices = useMemo(
    () => (devices ?? []).filter(deviceIsOnline).filter(deviceHasRunnableAgent),
    [devices]
  )
  const mcp = useMcpServers(teamId)
  const team = useTeamById(teamId)
  const teamSlug = team?.slug ?? null
  const mcpConnectHref = useCallback(
    (serverId: string) =>
      teamSlug
        ? `/t/${encodeURIComponent(teamSlug)}/settings/mcp-servers?connect=${encodeURIComponent(serverId)}`
        : null,
    [teamSlug]
  )
  const hasSubject = subject !== null
  // EXP-772: a subjectless session starts in build mode; a subject seeds
  // plan mode from the device's defaults.
  const launch = useLaunchOptions({
    open: true,
    devices: candidateDevices,
    initialDeviceId: seed?.deviceId,
    planModeOff: !hasSubject,
    teamId,
    mcpServers: mcp.servers,
  })
  const { device, agent } = launch
  // EXP-836: a play button can name a machine the composer cannot start on.
  const deviceRequestNote = useMemo(() => {
    const requested = launch.unavailableRequestId
    if (!requested) return null
    if (devices === null) return null
    const row = devices.find((candidate) => candidate.deviceId === requested)
    if (!row) return `That machine is no longer in your registry.`
    const label = row.deviceLabel || row.deviceId
    if (!deviceIsOnline(row)) return `${label} is offline.`
    if (!deviceHasRunnableAgent(row)) return `No agent is signed in on ${label}.`
    return null
  }, [launch.unavailableRequestId, devices])
  const hadSubjectRef = useRef(hasSubject)
  useEffect(() => {
    if (hadSubjectRef.current === hasSubject) return
    hadSubjectRef.current = hasSubject
    launch.setPlanMode(
      hasSubject ? agentSeed(agent, deviceAgentLaunchDefaults(device, agent)).planMode : false
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasSubject])

  // ── Seed ──────────────────────────────────────────────────────────────────

  useEffect(() => {
    if (!seed) return
    if (seed.issueIds.length > 0) {
      setSubject({ kind: `issues`, ids: [...new Set(seed.issueIds)] })
    }
    // EXP-836: an explicit machine is a REQUEST that outranks the default.
    if (seed.deviceId) launch.requestDevice(seed.deviceId)
    // The text lands only in an EMPTY draft: a seed never stomps typing.
    if (seed.text) {
      const seeded = seed.text
      setText((current) => (current.trim() ? current : seeded))
    }
    onSeedConsumed()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seed])

  // ── Subject actions ───────────────────────────────────────────────────────

  const toggleIssue = useCallback((issueId: string) => {
    setSubject((current) => {
      // Picking an issue while a prompt is the subject SWAPS.
      if (current?.kind !== `issues`) return { kind: `issues`, ids: [issueId] }
      const ids = current.ids.includes(issueId)
        ? current.ids.filter((id) => id !== issueId)
        : [...current.ids, issueId]
      return ids.length === 0 ? null : { kind: `issues`, ids }
    })
  }, [])

  // NEW-2: the `/` menu's pick. The prompt replaces whatever subject is up
  // (issues included: picking the other kind swaps).
  const pickPrompt = useCallback((promptId: string) => {
    setSubject({ kind: `prompt`, id: promptId })
  }, [])

  const clearPrompt = useCallback(() => {
    setSubject((current) => (current?.kind === `prompt` ? null : current))
  }, [])

  // NEW-3: the "+" menu's Message an agent › pick. The agent replaces
  // whatever subject is up (issues and prompts included: picking the other
  // kind swaps).
  const pickAgent = useCallback((agentId: string) => {
    setSubject({ kind: `agent`, id: agentId })
  }, [])

  const clearAgent = useCallback(() => {
    setSubject((current) => (current?.kind === `agent` ? null : current))
  }, [])

  // ── Images and files ──────────────────────────────────────────────────────

  const addFiles = useCallback(
    (files: File[], caret: number) => {
      const staged = stagePendingImages(imagesRef.current, files, text, caret)
      if (staged.added > 0) {
        setImages(staged.images)
        setText(staged.text)
      }
      if (staged.rejected > 0) toast.error(ATTACHMENT_REJECTED_TOAST)
      if (staged.overflow > 0) toast.error(IMAGE_CAP_TOAST)
      if (staged.fileOverflow > 0) toast.error(FILE_CAP_TOAST)
      return staged.caret
    },
    [text]
  )

  const removeImage = useCallback(
    (url: string) => {
      const dropped = dropPendingImage(imagesRef.current, url, text)
      if (dropped.images !== imagesRef.current) URL.revokeObjectURL(url)
      setImages(dropped.images)
      setText(dropped.text)
    },
    [text]
  )

  useEffect(
    () => () => {
      for (const image of imagesRef.current) URL.revokeObjectURL(image.url)
    },
    []
  )

  // ── Gate ──────────────────────────────────────────────────────────────────

  const count = checkedIds.length
  const overCap = count > MAX_ISSUES_PER_RUN
  const hasText = text.trim().length > 0
  const busy = sending
  const subjectBlocked =
    subject === null
      ? // A subjectless session needs SOMETHING to say: text or a file.
        !hasText && images.length === 0
      : subject.kind === `issues`
        ? overCap || spansRepos || spansBranches
        : subject.kind === `agent`
          ? // NEW-3: a DM needs the agent's row and something to say.
            !selectedAgent || (!hasText && images.length === 0)
          : !selectedPrompt
  // NEW-3: a DM runs on the agent's device: the composer's machine and its
  // readiness never gate it.
  const toAgent = subject?.kind === `agent`
  const blocked =
    busy ||
    (!toAgent && !device) ||
    // EXP-773: the picked agent is not ready on the picked machine.
    (!toAgent && launch.agentNotReady) ||
    subjectBlocked

  // ── Submit ────────────────────────────────────────────────────────────────

  const submitRef = useRef<() => Promise<void>>(async () => {})

  const submit = async () => {
    if (blocked || (!device && !toAgent)) return
    setSending(true)
    const retry = () => void submitRef.current()
    try {
      // Upload sequentially, keeping each id as it lands: a mid-batch
      // failure keeps the composer intact and a retry uploads the rest.
      let current = imagesRef.current
      try {
        for (const image of current) {
          if (image.uploadedId) continue
          const uploaded = await uploadTeamSessionImageFile(teamId, image.file)
          current = markPendingImageUploaded(current, image.url, uploaded.id, uploaded.filename)
          setImages(current)
        }
      } catch (error) {
        failedAction(`upload`, `the attachment`, error, { retry })
        return
      }
      const attachmentIds = current
        .map((image) => image.uploadedId)
        .filter((id): id is string => Boolean(id))
      const options = launch.buildOptions()
      const input = buildSendInput({
        teamId,
        deviceId: device?.deviceId ?? ``,
        options,
        text,
        attachmentIds,
        subject,
        repoId,
      })
      let result: { sessionId: string; txid: number }
      try {
        result = await trpc.sessions.send.mutate(input, SKIP_ERROR_TOAST)
      } catch (error) {
        if (toAgent) failedAction(`send`, `the message`, error, { retry })
        else startFailureToast(error, device!, options.agent, retry)
        return
      }
      await awaitSynced(result.txid, START_SYNC_WAIT_MS)
      const agentId = subject?.kind === `agent` ? subject.id : null
      clearDraft()
      onStartedRef.current?.(result.sessionId)
      if (teamSlug) {
        // NEW-3: a DM opens the agent's conversation, a start its thread.
        void navigate(
          agentId
            ? { to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId } }
            : {
                to: `/t/$teamSlug/threads/$threadId`,
                params: { teamSlug, threadId: result.sessionId },
              }
        )
      }
    } finally {
      setSending(false)
    }
  }

  const clearDraft = () => {
    for (const image of imagesRef.current) URL.revokeObjectURL(image.url)
    setImages([])
    setText(``)
    setSubject(null)
  }

  const onStartedRef = useRef(onStarted)
  onStartedRef.current = onStarted

  submitRef.current = submit

  return {
    teamId,
    subject,
    selectedPrompt,
    prompts,
    eligibleIssues,
    checkedIssues,
    toggleIssue,
    pickPrompt,
    clearPrompt,
    agents,
    selectedAgent,
    pickAgent,
    clearAgent,
    text,
    setText,
    images,
    addFiles,
    removeImage,
    repos,
    repoId,
    setRepoId,
    repoOptions,
    launch,
    candidateDevices,
    deviceRequestNote,
    mcpServers: mcp.servers,
    mcpConnectHref,
    submitLabel: submitLabelFor(subject),
    blocked,
    overCap,
    spansRepos,
    spansBranches,
    costHint: !overCap && !spansRepos && count > BATCH_COST_HINT_THRESHOLD,
    busy,
    submit,
  }
}
